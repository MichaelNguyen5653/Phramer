//! Pinned captures on the desktop: one layered, topmost, borderless window
//! per pin, each on a thread of its own pumping its messages until the pin
//! closes, so any number can be open and none waits on another.
//!
//! The window is drawn with `UpdateLayeredWindow` from premultiplied pixels
//! sized in physical pixels, so it is sharp under per-monitor DPI and the
//! glow and the opacity need no compositing of our own. Dragging anywhere
//! moves it (the whole window is its caption).

use std::cell::RefCell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::OnceLock;
use std::sync::atomic::{AtomicUsize, Ordering};

use phramer_capture::{PhysicalPoint, PhysicalRect, enumerate_monitors, monitor_at_or_primary};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM};
use windows_sys::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, GetDC, ReleaseDC,
    SelectObject,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemServices::MK_CONTROL;
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, TME_LEAVE, TME_NONCLIENT, TRACKMOUSEEVENT, TrackMouseEvent, VK_CONTROL, VK_ESCAPE,
    VK_NUMPAD0, VK_NUMPAD9,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    DispatchMessageW, GetCursorPos, GetMessageW, GetWindowRect, HTCAPTION, IDC_ARROW, LoadCursorW,
    MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage, RegisterClassExW, SW_SHOW,
    SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER, SetForegroundWindow, SetWindowPos, ShowWindow,
    TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu, TranslateMessage, ULW_ALPHA,
    UpdateLayeredWindow, WM_CONTEXTMENU, WM_DESTROY, WM_DPICHANGED, WM_KEYDOWN, WM_MOUSEWHEEL,
    WM_NCHITTEST, WM_NCLBUTTONDBLCLK, WM_NCMOUSELEAVE, WM_NCMOUSEMOVE, WM_NULL, WNDCLASSEXW,
    WS_EX_LAYERED, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::last_error;
use crate::error::OverlayError;
use crate::export::CapturedImage;
use crate::pin::{
    PIN_MENU, PinEffect, PinEvent, PinKey, PinMenuItem, PinRequest, PinState, render_pin,
};

const CLASS_NAME: &str = "PhramerPin";
/// 15.0.0's window title.
const TITLE: &str = "phramer-pin";
/// The scale of 96 DPI.
const BASE_DPI: f64 = 96.0;

/// Pins open now, across threads.
static OPEN_PINS: AtomicUsize = AtomicUsize::new(0);

/// What a pin hands back: a request and the pixels as the pin shows them
/// (rotations included). Runs on the pin's thread.
pub type PinHandler = Box<dyn FnMut(PinRequest, &CapturedImage) + Send>;

/// How a pin looks.
#[derive(Debug, Clone, Copy)]
pub struct PinOptions {
    /// `uiColor`: the glow.
    pub accent: (u8, u8, u8),
    /// `contrastUiColor`: the glow while the pointer is over the pin.
    pub hover_accent: (u8, u8, u8),
    /// `antialiasingPinZoom`.
    pub smooth: bool,
}

/// How many pins are open.
pub fn open_pins() -> usize {
    OPEN_PINS.load(Ordering::SeqCst)
}

/// Pins `image`, which was at `geometry` on the desktop (physical), over
/// that spot. Returns once the pin's thread runs; the pin lives until the
/// user closes it.
pub fn spawn_pin(
    image: CapturedImage,
    geometry: PhysicalRect,
    options: PinOptions,
    on_request: PinHandler,
) -> Result<(), OverlayError> {
    let scale = enumerate_monitors()
        .ok()
        .and_then(|monitors| {
            monitor_at_or_primary(&monitors, PhysicalPoint::new(geometry.x, geometry.y))
                .map(phramer_capture::Monitor::scale)
        })
        .unwrap_or(1.0);
    std::thread::Builder::new()
        .name("pin".to_owned())
        .spawn(move || {
            OPEN_PINS.fetch_add(1, Ordering::SeqCst);
            let state = PinState::new(image, scale, options.smooth);
            if let Err(error) = run_pin(state, geometry, options, on_request) {
                log::error!("pin: {error}");
            }
            OPEN_PINS.fetch_sub(1, Ordering::SeqCst);
        })
        .map(drop)
        .map_err(|_| OverlayError::Win32 {
            call: "spawn",
            code: 0,
        })
}

struct PinShell {
    state: PinState,
    options: PinOptions,
    on_request: PinHandler,
    /// Leaving the window is being watched for.
    tracking: bool,
}

thread_local! {
    static PIN: RefCell<Option<PinShell>> = const { RefCell::new(None) };
}

fn with_pin<T>(f: impl FnOnce(&mut PinShell) -> T) -> Option<T> {
    PIN.with(|cell| {
        let mut shell = cell.try_borrow_mut().ok()?;
        shell.as_mut().map(f)
    })
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

fn window_class() -> Result<u16, OverlayError> {
    static CLASS: OnceLock<u16> = OnceLock::new();
    if let Some(atom) = CLASS.get() {
        return Ok(*atom);
    }
    let name = wide(CLASS_NAME);
    let class = WNDCLASSEXW {
        cbSize: size_of::<WNDCLASSEXW>() as u32,
        lpfnWndProc: Some(window_proc),
        // SAFETY: a null name asks for this executable's own module handle.
        hInstance: unsafe { GetModuleHandleW(std::ptr::null()) },
        // SAFETY: a system cursor; no module handle is needed.
        hCursor: unsafe { LoadCursorW(std::ptr::null_mut(), IDC_ARROW) },
        lpszClassName: name.as_ptr(),
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and `name` outlives the call;
    // the system copies the class name.
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 {
        // Another pin's thread may have registered it first
        return CLASS
            .get()
            .copied()
            .ok_or_else(|| last_error("RegisterClassExW"));
    }
    Ok(*CLASS.get_or_init(|| atom))
}

/// Destroys the window if the pin ends some other way than by closing it.
struct WindowGuard(HWND);

impl Drop for WindowGuard {
    fn drop(&mut self) {
        PIN.with(|cell| {
            if let Ok(mut shell) = cell.try_borrow_mut() {
                shell.take();
            }
        });
        // SAFETY: a window this thread created; destroying one already gone
        // just fails.
        unsafe { DestroyWindow(self.0) };
    }
}

fn run_pin(
    state: PinState,
    geometry: PhysicalRect,
    options: PinOptions,
    on_request: PinHandler,
) -> Result<(), OverlayError> {
    let class = window_class()?;
    let margin = state.margin();
    let (width, height) = state.window_size();
    let title = wide(TITLE);
    // SAFETY: the class atom is registered, `title` outlives the call, and
    // every other argument is a plain value or null.
    let hwnd = unsafe {
        CreateWindowExW(
            WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
            class as usize as *const u16,
            title.as_ptr(),
            WS_POPUP,
            geometry.x - margin,
            geometry.y - margin,
            width,
            height,
            std::ptr::null_mut(),
            std::ptr::null_mut(),
            // SAFETY: as in `window_class`.
            GetModuleHandleW(std::ptr::null()),
            std::ptr::null(),
        )
    };
    if hwnd.is_null() {
        return Err(last_error("CreateWindowExW"));
    }
    let _guard = WindowGuard(hwnd);
    PIN.with(|cell| {
        *cell.borrow_mut() = Some(PinShell {
            state,
            options,
            on_request,
            tracking: false,
        });
    });
    present(hwnd)?;
    // SAFETY: a window this thread just created.
    unsafe {
        ShowWindow(hwnd, SW_SHOW);
        // 15.0.0 activated the pin so its keys work at once
        SetForegroundWindow(hwnd);
    }

    let mut message = MSG::default();
    loop {
        // SAFETY: `message` is a valid out-parameter; messages for every
        // window of this thread.
        let got = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
        if got <= 0 {
            break;
        }
        // SAFETY: `message` was just filled by GetMessageW.
        unsafe {
            TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
    Ok(())
}

/// Draws the pin into its window at its current top-left corner.
fn present(hwnd: HWND) -> Result<(), OverlayError> {
    let Some((pixels, (width, height), alpha)) = with_pin(|shell| {
        (
            render_pin(
                &shell.state,
                shell.options.accent,
                shell.options.hover_accent,
            ),
            shell.state.window_size(),
            shell.state.alpha(),
        )
    }) else {
        return Ok(());
    };
    let mut rect = RECT::default();
    // SAFETY: `rect` is a valid out-parameter for a window of this thread.
    unsafe { GetWindowRect(hwnd, &mut rect) };

    let info = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            // Negative: top row first, as the pixels are
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        },
        ..Default::default()
    };
    // SAFETY: the screen DC, released below.
    let screen = unsafe { GetDC(std::ptr::null_mut()) };
    // SAFETY: a memory DC compatible with the screen, deleted below.
    let memory = unsafe { CreateCompatibleDC(screen) };
    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `info` describes a 32-bit top-down bitmap; `bits` receives
    // the pointer to its pixels.
    let bitmap = unsafe {
        CreateDIBSection(
            memory,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        )
    };
    let result = if bitmap.is_null() || bits.is_null() {
        Err(last_error("CreateDIBSection"))
    } else {
        let length = (width.max(0) as usize) * (height.max(0) as usize) * 4;
        // SAFETY: the DIB section holds `width * height` 32-bit pixels,
        // which is `length` bytes, and `pixels` is exactly that long.
        unsafe {
            std::ptr::copy_nonoverlapping(
                pixels.as_ptr(),
                bits.cast::<u8>(),
                length.min(pixels.len()),
            );
        }
        // SAFETY: selects the bitmap into the memory DC; the previous object
        // is put back before the bitmap is deleted.
        let previous = unsafe { SelectObject(memory, bitmap) };
        let position = POINT {
            x: rect.left,
            y: rect.top,
        };
        let size = SIZE {
            cx: width,
            cy: height,
        };
        let origin = POINT { x: 0, y: 0 };
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        // SAFETY: every pointer is to a local that outlives the call; the
        // memory DC holds a premultiplied 32-bit bitmap of `size`.
        let updated = unsafe {
            UpdateLayeredWindow(
                hwnd, screen, &position, &size, memory, &origin, 0, &blend, ULW_ALPHA,
            )
        };
        // SAFETY: restores the DC's original bitmap.
        unsafe { SelectObject(memory, previous) };
        if updated == 0 {
            Err(last_error("UpdateLayeredWindow"))
        } else {
            Ok(())
        }
    };
    // SAFETY: each object was created above and is released once.
    unsafe {
        if !bitmap.is_null() {
            DeleteObject(bitmap);
        }
        DeleteDC(memory);
        ReleaseDC(std::ptr::null_mut(), screen);
    }
    result
}

/// Feeds an event in and carries out the effects.
fn feed(hwnd: HWND, event: PinEvent) {
    let Some(effects) = with_pin(|shell| shell.state.handle(event)) else {
        return;
    };
    for effect in effects {
        match effect {
            PinEffect::Redraw => {
                if let Err(error) = present(hwnd) {
                    log::error!("pin: {error}");
                }
            }
            PinEffect::Request(request) => {
                with_pin(|shell| {
                    let image = shell.state.image().clone();
                    (shell.on_request)(request, &image);
                });
            }
            PinEffect::Close => {
                // SAFETY: a window of this thread; WM_DESTROY ends the loop.
                unsafe { DestroyWindow(hwnd) };
                return;
            }
        }
    }
}

fn show_menu(hwnd: HWND, lparam: LPARAM) {
    // SAFETY: a new, empty menu, destroyed below.
    let menu = unsafe { CreatePopupMenu() };
    if menu.is_null() {
        return;
    }
    for entry in PIN_MENU {
        match entry {
            Some(item) => {
                let label = wide(item.label());
                // SAFETY: `label` is NUL-terminated and outlives the call.
                unsafe { AppendMenuW(menu, MF_STRING, item.id(), label.as_ptr()) };
            }
            // SAFETY: a separator takes no text.
            None => unsafe {
                AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
            },
        }
    }
    // From the keyboard (Shift+F10) the position is -1, -1
    let mut at = POINT {
        x: i32::from((lparam & 0xffff) as u16 as i16),
        y: i32::from(((lparam >> 16) & 0xffff) as u16 as i16),
    };
    if at.x == -1 && at.y == -1 {
        // SAFETY: `at` is a valid out-parameter.
        unsafe { GetCursorPos(&mut at) };
    }
    // SAFETY: `menu` and `hwnd` are this thread's; the foreground change and
    // the WM_NULL afterwards are what make the menu close when clicked away.
    let chosen = unsafe {
        SetForegroundWindow(hwnd);
        let chosen = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON,
            at.x,
            at.y,
            0,
            hwnd,
            std::ptr::null(),
        );
        PostMessageW(hwnd, WM_NULL, 0, 0);
        DestroyMenu(menu);
        chosen
    };
    if let Some(item) = usize::try_from(chosen).ok().and_then(PinMenuItem::from_id) {
        feed(hwnd, PinEvent::Menu(item));
    }
}

fn key_event(key: u32) -> Option<PinKey> {
    // SAFETY: reads this thread's keyboard state.
    let ctrl = unsafe { GetKeyState(i32::from(VK_CONTROL)) } < 0;
    match key {
        k if k == u32::from(VK_ESCAPE) => Some(PinKey::Escape),
        k if k == u32::from(b'Q') && ctrl => Some(PinKey::Quit),
        k if (u32::from(b'0')..=u32::from(b'9')).contains(&k) => {
            u8::try_from(k - u32::from(b'0')).ok().map(PinKey::Digit)
        }
        k if (u32::from(VK_NUMPAD0)..=u32::from(VK_NUMPAD9)).contains(&k) => {
            u8::try_from(k - u32::from(VK_NUMPAD0))
                .ok()
                .map(PinKey::Digit)
        }
        _ => None,
    }
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    let handled = catch_unwind(AssertUnwindSafe(|| {
        handle_message(hwnd, message, wparam, lparam)
    }));
    match handled {
        Ok(Some(result)) => result,
        // SAFETY: the arguments are exactly those this procedure received.
        Ok(None) => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
        Err(_) => {
            // SAFETY: a window of this thread; its destruction ends the loop.
            unsafe { DestroyWindow(hwnd) };
            0
        }
    }
}

/// `Some(result)` for a message the pin handled.
fn handle_message(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match message {
        // The whole pin is its caption: a drag moves it, and Windows does
        // the moving
        WM_NCHITTEST => Some(HTCAPTION as LRESULT),
        // Instead of maximizing, as a caption would
        WM_NCLBUTTONDBLCLK => {
            feed(hwnd, PinEvent::DoubleClick);
            Some(0)
        }
        // Instead of the system menu
        WM_CONTEXTMENU => {
            show_menu(hwnd, lparam);
            Some(0)
        }
        WM_NCMOUSEMOVE => {
            let start = with_pin(|shell| !std::mem::replace(&mut shell.tracking, true));
            if start == Some(true) {
                let mut track = TRACKMOUSEEVENT {
                    cbSize: size_of::<TRACKMOUSEEVENT>() as u32,
                    dwFlags: TME_LEAVE | TME_NONCLIENT,
                    hwndTrack: hwnd,
                    dwHoverTime: 0,
                };
                // SAFETY: `track` is initialised and names this window.
                unsafe { TrackMouseEvent(&mut track) };
                feed(hwnd, PinEvent::Hover(true));
            }
            None
        }
        WM_NCMOUSELEAVE => {
            with_pin(|shell| shell.tracking = false);
            feed(hwnd, PinEvent::Hover(false));
            Some(0)
        }
        WM_MOUSEWHEEL => {
            let delta = i32::from(((wparam >> 16) & 0xffff) as u16 as i16);
            let ctrl = (wparam & 0xffff) as u32 & MK_CONTROL != 0;
            feed(hwnd, PinEvent::Wheel { delta, ctrl });
            Some(0)
        }
        WM_KEYDOWN => {
            if let Some(key) = u32::try_from(wparam).ok().and_then(key_event) {
                feed(hwnd, PinEvent::Key(key));
                return Some(0);
            }
            None
        }
        WM_DPICHANGED => {
            let dpi = (wparam & 0xffff) as u16;
            // SAFETY: for WM_DPICHANGED `lparam` points to the suggested
            // window rectangle, valid for this message.
            let suggested = unsafe { (lparam as *const RECT).as_ref() }.copied();
            if let Some(suggested) = suggested {
                // SAFETY: moves a window of this thread; the size follows
                // from the pin's own state in `present`.
                unsafe {
                    SetWindowPos(
                        hwnd,
                        std::ptr::null_mut(),
                        suggested.left,
                        suggested.top,
                        0,
                        0,
                        SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
                    )
                };
            }
            feed(hwnd, PinEvent::ScaleChanged(f64::from(dpi) / BASE_DPI));
            Some(0)
        }
        WM_DESTROY => {
            // SAFETY: ends this thread's message loop.
            unsafe { PostQuitMessage(0) };
            Some(0)
        }
        _ => None,
    }
}
