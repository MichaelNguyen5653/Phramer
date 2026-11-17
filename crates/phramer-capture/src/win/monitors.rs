//! Live monitor enumeration and the cursor position.

use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Foundation::{LPARAM, POINT, RECT, TRUE};
use windows_sys::Win32::Graphics::Gdi::{
    DISPLAY_DEVICEW, EnumDisplayDevicesW, EnumDisplayMonitors, GetMonitorInfoW, HDC, HMONITOR,
    MONITORINFO, MONITORINFOEXW,
};
use windows_sys::Win32::UI::HiDpi::{GetDpiForMonitor, MDT_EFFECTIVE_DPI};
use windows_sys::Win32::UI::WindowsAndMessaging::GetCursorPos;
use windows_sys::core::BOOL;

use super::dpi::require_per_monitor_v2;
use super::last_error;
use crate::error::CaptureError;
use crate::geometry::{PhysicalPoint, PhysicalRect};
use crate::monitor::{Monitor, MonitorId};

/// `MONITORINFO::dwFlags` bit of the primary monitor.
const MONITORINFOF_PRIMARY: u32 = 1;
/// `EnumDisplayDevicesW` flag: put the device interface path in `DeviceID`.
const EDD_GET_DEVICE_INTERFACE_NAME: u32 = 1;

/// Every monitor of the desktop: the primary first, then left to right, top
/// to bottom. Fails with [`CaptureError::NoMonitors`] when there is none,
/// which is what a session without an interactive desktop looks like.
pub fn enumerate_monitors() -> Result<Vec<Monitor>, CaptureError> {
    require_per_monitor_v2()?;

    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: `collect_monitor` matches MONITORENUMPROC and treats the
    // LPARAM as the `Vec<HMONITOR>` passed here. The enumeration is
    // synchronous, so `handles` outlives every callback and nothing else
    // touches it meanwhile. A null DC and clip rectangle enumerate the whole
    // virtual desktop.
    let enumerated = unsafe {
        EnumDisplayMonitors(
            null_mut(),
            std::ptr::null(),
            Some(collect_monitor),
            &raw mut handles as LPARAM,
        )
    };
    if enumerated == 0 {
        return Err(last_error("EnumDisplayMonitors"));
    }

    let mut monitors = handles
        .into_iter()
        .map(describe)
        .collect::<Result<Vec<_>, _>>()?;
    if monitors.is_empty() {
        return Err(CaptureError::NoMonitors);
    }
    monitors.sort_by_key(|monitor| {
        (
            !monitor.is_primary(),
            monitor.bounds().x,
            monitor.bounds().y,
        )
    });
    Ok(monitors)
}

/// The cursor position on the virtual desktop.
pub fn cursor_position() -> Result<PhysicalPoint, CaptureError> {
    require_per_monitor_v2()?;
    let mut point = POINT { x: 0, y: 0 };
    // SAFETY: `point` is a valid, writable POINT for the duration of the call.
    if unsafe { GetCursorPos(&raw mut point) } == 0 {
        return Err(last_error("GetCursorPos"));
    }
    Ok(PhysicalPoint::new(point.x, point.y))
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _dc: HDC,
    _clip: *mut RECT,
    data: LPARAM,
) -> BOOL {
    // SAFETY: `data` is the `&mut Vec<HMONITOR>` that `enumerate_monitors`
    // handed to EnumDisplayMonitors; it is alive and exclusively ours for
    // the whole synchronous enumeration.
    let handles = unsafe { &mut *(data as *mut Vec<HMONITOR>) };
    handles.push(monitor);
    TRUE
}

fn describe(handle: HMONITOR) -> Result<Monitor, CaptureError> {
    // SAFETY: MONITORINFOEXW is plain integers and arrays, for which all
    // zeroes is a valid value.
    let mut info: MONITORINFOEXW = unsafe { zeroed() };
    info.monitorInfo.cbSize = size_of::<MONITORINFOEXW>() as u32;
    // SAFETY: `handle` came from EnumDisplayMonitors. `info` is a writable
    // MONITORINFOEXW whose cbSize says so, which is what lets the API fill
    // the extended structure through a MONITORINFO pointer.
    let found = unsafe { GetMonitorInfoW(handle, (&raw mut info).cast::<MONITORINFO>()) };
    if found == 0 {
        return Err(last_error("GetMonitorInfoW"));
    }

    let (mut dpi_x, mut dpi_y) = (0u32, 0u32);
    // SAFETY: `handle` is a monitor handle and both out pointers are valid,
    // writable u32s for the duration of the call.
    let result =
        unsafe { GetDpiForMonitor(handle, MDT_EFFECTIVE_DPI, &raw mut dpi_x, &raw mut dpi_y) };
    if result < 0 {
        return Err(CaptureError::Win32 {
            call: "GetDpiForMonitor",
            code: result as u32,
        });
    }

    let area = info.monitorInfo.rcMonitor;
    let device = wide_to_string(&info.szDevice);
    let id = interface_path(&info.szDevice).unwrap_or(device);
    Ok(Monitor::new(
        MonitorId::new(id),
        PhysicalRect::from_edges(area.left, area.top, area.right, area.bottom),
        dpi_x,
        info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
    ))
}

/// The device interface path of the monitor attached to a display device
/// (`\\.\DISPLAY1`), e.g. `\\?\DISPLAY#DEL4187#5&2c4f...#{e6f07b5f-...}`.
/// Unlike the display device name it follows the panel, not the output.
fn interface_path(device: &[u16; 32]) -> Option<String> {
    // SAFETY: DISPLAY_DEVICEW is plain integers and arrays, for which all
    // zeroes is a valid value.
    let mut monitor: DISPLAY_DEVICEW = unsafe { zeroed() };
    monitor.cb = size_of::<DISPLAY_DEVICEW>() as u32;
    // SAFETY: `device` is the NUL-terminated name GetMonitorInfoW wrote into
    // a 32-element buffer, and `monitor` is a writable DISPLAY_DEVICEW with
    // its cb set.
    let found = unsafe {
        EnumDisplayDevicesW(
            device.as_ptr(),
            0,
            &raw mut monitor,
            EDD_GET_DEVICE_INTERFACE_NAME,
        )
    };
    let path = wide_to_string(&monitor.DeviceID);
    (found != 0 && !path.is_empty()).then_some(path)
}

fn wide_to_string(wide: &[u16]) -> String {
    let length = wide
        .iter()
        .position(|&unit| unit == 0)
        .unwrap_or(wide.len());
    String::from_utf16_lossy(&wide[..length])
}
