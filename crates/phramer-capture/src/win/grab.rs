//! Frozen screenshots through GDI.
//!
//! The screen DC of a per-monitor-DPI-aware process addresses the virtual
//! desktop in physical pixels, so a `BitBlt` from it is a 1:1 copy of what
//! is on screen: no scaling, and negative coordinates reach the monitors
//! left of or above the primary. This is what the old application did
//! through `QScreen::grabWindow(0)`.

use std::ffi::c_void;
use std::mem::{size_of, zeroed};
use std::ptr::null_mut;

use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLACKNESS, BitBlt, CAPTUREBLT, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GdiFlush, GetDC, HBITMAP, HDC,
    HGDIOBJ, PatBlt, ReleaseDC, SRCCOPY, SelectObject,
};

use super::dpi::require_per_monitor_v2;
use super::last_error;
use crate::error::CaptureError;
use crate::frame::{BYTES_PER_PIXEL, Frame};
use crate::geometry::PhysicalRect;
use crate::monitor::{Monitor, virtual_bounds};

/// Grabs one monitor. The frame is exactly the monitor's physical size.
pub fn grab_monitor(monitor: &Monitor) -> Result<Frame, CaptureError> {
    grab(monitor.bounds(), &[monitor.bounds()])
}

/// Grabs every monitor into one frame the size of the virtual desktop (the
/// union of the monitors' bounds). Areas of that rectangle that no monitor
/// covers are opaque black.
pub fn grab_virtual_desktop(monitors: &[Monitor]) -> Result<Frame, CaptureError> {
    let bounds = virtual_bounds(monitors).ok_or(CaptureError::NoMonitors)?;
    let sources: Vec<PhysicalRect> = monitors.iter().map(Monitor::bounds).collect();
    grab(bounds, &sources)
}

/// Copies each of `sources` from the screen into a frame covering `bounds`.
fn grab(bounds: PhysicalRect, sources: &[PhysicalRect]) -> Result<Frame, CaptureError> {
    require_per_monitor_v2()?;
    let length = buffer_length(bounds)?;

    // Declared in this order so they are released in reverse: the bitmap is
    // deselected before it is deleted, and both DCs outlive it
    let screen = ScreenDc::acquire()?;
    let memory = MemoryDc::compatible_with(&screen)?;
    let bitmap = DibSection::create(&screen, bounds, length)?;
    let _selected = Selection::select(&memory, &bitmap)?;

    if sources.len() > 1 {
        // Monitors need not tile the desktop; gaps must not show whatever
        // the allocation held
        // SAFETY: `memory` is a live DC with `bitmap` selected, and the
        // rectangle is the bitmap's own extent.
        unsafe { PatBlt(memory.0, 0, 0, bounds.width, bounds.height, BLACKNESS) };
    }
    for source in sources {
        // CAPTUREBLT includes layered (translucent, tooltip, menu) windows,
        // which are composed above the windows a plain SRCCOPY reads
        // SAFETY: both DCs are live for the whole call; GDI clips the copy
        // to the bitmap and to the screen, so no coordinates are invalid.
        let copied = unsafe {
            BitBlt(
                memory.0,
                source.x - bounds.x,
                source.y - bounds.y,
                source.width,
                source.height,
                screen.0,
                source.x,
                source.y,
                SRCCOPY | CAPTUREBLT,
            )
        };
        if copied == 0 {
            return Err(last_error("BitBlt"));
        }
    }

    let mut pixels = bitmap.to_vec();
    // The screen has no alpha channel: BitBlt leaves that byte 0, which
    // would make the whole grab transparent to anything that honours it
    for pixel in pixels.as_chunks_mut::<BYTES_PER_PIXEL>().0 {
        pixel[3] = u8::MAX;
    }
    Ok(Frame::new(bounds, pixels))
}

/// Byte length of a frame covering `bounds`, or an error when the area is
/// empty or its buffer would not be addressable.
fn buffer_length(bounds: PhysicalRect) -> Result<usize, CaptureError> {
    let invalid = CaptureError::InvalidSize {
        width: bounds.width,
        height: bounds.height,
    };
    if bounds.is_empty() {
        return Err(invalid);
    }
    usize::try_from(bounds.width)
        .ok()
        .zip(usize::try_from(bounds.height).ok())
        .and_then(|(width, height)| width.checked_mul(height))
        .and_then(|pixels| pixels.checked_mul(BYTES_PER_PIXEL))
        .filter(|&length| isize::try_from(length).is_ok())
        .ok_or(invalid)
}

/// The DC of the whole screen. Released on drop.
struct ScreenDc(HDC);

impl ScreenDc {
    fn acquire() -> Result<Self, CaptureError> {
        // SAFETY: a null window handle asks for the DC of the entire screen;
        // there are no other preconditions.
        let dc = unsafe { GetDC(null_mut()) };
        if dc.is_null() {
            Err(last_error("GetDC"))
        } else {
            Ok(Self(dc))
        }
    }
}

impl Drop for ScreenDc {
    fn drop(&mut self) {
        // SAFETY: `self.0` came from GetDC(null) and is released exactly
        // once, with the same (null) window it was acquired for.
        unsafe { ReleaseDC(null_mut(), self.0) };
    }
}

/// A memory DC to draw into a bitmap with. Deleted on drop.
struct MemoryDc(HDC);

impl MemoryDc {
    fn compatible_with(screen: &ScreenDc) -> Result<Self, CaptureError> {
        // SAFETY: `screen.0` is a live DC for the duration of the call.
        let dc = unsafe { CreateCompatibleDC(screen.0) };
        if dc.is_null() {
            Err(last_error("CreateCompatibleDC"))
        } else {
            Ok(Self(dc))
        }
    }
}

impl Drop for MemoryDc {
    fn drop(&mut self) {
        // SAFETY: `self.0` was created by CreateCompatibleDC, is owned by
        // this value and is deleted exactly once.
        unsafe { DeleteDC(self.0) };
    }
}

/// A top-down 32-bit bitmap whose pixels GDI keeps in memory this process
/// can read. Deleted on drop.
struct DibSection {
    handle: HBITMAP,
    bits: *const u8,
    length: usize,
}

impl DibSection {
    /// `length` must be `buffer_length(bounds)`.
    fn create(
        screen: &ScreenDc,
        bounds: PhysicalRect,
        length: usize,
    ) -> Result<Self, CaptureError> {
        // SAFETY: BITMAPINFO is plain integers, for which all zeroes is a
        // valid value.
        let mut info: BITMAPINFO = unsafe { zeroed() };
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: bounds.width,
            // Negative: top row first, the order every consumer expects
            biHeight: -bounds.height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..info.bmiHeader
        };

        let mut bits: *mut c_void = null_mut();
        // SAFETY: `screen.0` is a live DC, `info` describes an uncompressed
        // 32-bit bitmap and needs no colour table, `bits` is a valid out
        // pointer, and a null section lets the system allocate the pixels.
        let handle = unsafe {
            CreateDIBSection(
                screen.0,
                &raw const info,
                DIB_RGB_COLORS,
                &raw mut bits,
                null_mut(),
                0,
            )
        };
        if handle.is_null() || bits.is_null() {
            let error = last_error("CreateDIBSection");
            if !handle.is_null() {
                // SAFETY: `handle` is a bitmap this function just created
                // and has not handed out.
                unsafe { DeleteObject(handle) };
            }
            return Err(error);
        }
        Ok(Self {
            handle,
            bits: bits.cast::<u8>(),
            length,
        })
    }

    /// A copy of the pixels as GDI has drawn them so far.
    fn to_vec(&self) -> Vec<u8> {
        // SAFETY: GdiFlush has no preconditions. It makes the calling
        // thread's pending drawing land in the bitmap before it is read.
        unsafe { GdiFlush() };
        // SAFETY: `bits` points at the bitmap's pixels: width x height
        // 32-bit pixels with no row padding, which is `length` bytes (see
        // `create`). They stay allocated until the bitmap is deleted in
        // `drop`, and nothing writes to them during this copy because all
        // drawing happens on this thread and has been flushed.
        unsafe { std::slice::from_raw_parts(self.bits, self.length) }.to_vec()
    }
}

impl Drop for DibSection {
    fn drop(&mut self) {
        // SAFETY: `handle` was created by CreateDIBSection, is owned by this
        // value and is deleted exactly once. Any `Selection` borrowing it
        // has been dropped first, so it is not selected into a DC.
        unsafe { DeleteObject(self.handle) };
    }
}

/// A bitmap selected into a memory DC. Puts the DC's previous bitmap back on
/// drop, which is what makes both safe to delete.
struct Selection<'a> {
    dc: &'a MemoryDc,
    previous: HGDIOBJ,
    _bitmap: &'a DibSection,
}

impl<'a> Selection<'a> {
    fn select(dc: &'a MemoryDc, bitmap: &'a DibSection) -> Result<Self, CaptureError> {
        // SAFETY: both handles are live and owned by the borrowed values.
        let previous = unsafe { SelectObject(dc.0, bitmap.handle) };
        if previous.is_null() {
            return Err(last_error("SelectObject"));
        }
        Ok(Self {
            dc,
            previous,
            _bitmap: bitmap,
        })
    }
}

impl Drop for Selection<'_> {
    fn drop(&mut self) {
        // SAFETY: the DC is alive (borrowed) and `previous` is the object
        // SelectObject returned for it, which GDI keeps valid meanwhile.
        unsafe { SelectObject(self.dc.0, self.previous) };
    }
}
