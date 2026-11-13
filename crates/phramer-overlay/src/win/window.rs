//! The overlay windows: one topmost, borderless popup per monitor, sized in
//! physical pixels, drawn with GDI from a BGRA buffer.
//!
//! Presentation is `SetDIBitsToDevice` from memory the renderer writes
//! directly: one copy into the window surface per frame, no swap chain and
//! no extra dependency. A 4K frame is 33 MB; GDI moves that in a few
//! milliseconds, and paints are coalesced by `WM_PAINT`, so fast pointer
//! movement costs one render per displayed frame.
//!
//! Everything but grabbing the other monitors while following the cursor
//! runs on the calling thread, which must not own other windows:
//! [`run_overlay`] pumps that thread's messages until the state machine
//! closes, then destroys every window before it returns, on every path
//! (error, early return or panic) through the windows' guards.

use std::cell::{Cell, RefCell};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock, mpsc};
use std::time::Instant;

use phramer_annotate::Document;
use phramer_capture::{
    Frame, LocalRect, LogicalPoint, Monitor, PhysicalRect, cursor_position, grab_monitor,
    visible_windows,
};
use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows_sys::Win32::Graphics::Dwm::DwmFlush;
use windows_sys::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BeginPaint, DIB_RGB_COLORS, EndPaint, InvalidateRect,
    PAINTSTRUCT, SetDIBitsToDevice, UpdateWindow,
};
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemServices::{MK_CONTROL, MK_LBUTTON, MK_SHIFT};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, MAPVK_VK_TO_CHAR, MapVirtualKeyW, ReleaseCapture, SetCapture, VK_BACK, VK_CONTROL,
    VK_DELETE, VK_DOWN, VK_END, VK_ESCAPE, VK_F1, VK_F24, VK_HOME, VK_INSERT, VK_LEFT, VK_LWIN,
    VK_MENU, VK_NEXT, VK_NUMPAD0, VK_NUMPAD9, VK_PRIOR, VK_RETURN, VK_RIGHT, VK_RWIN, VK_SHIFT,
    VK_SPACE, VK_TAB, VK_UP,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetForegroundWindow,
    GetMessageW, GetWindowThreadProcessId, HTCLIENT, IDC_ARROW, IDC_CROSS, IDC_SIZEALL,
    IDC_SIZENESW, IDC_SIZENS, IDC_SIZENWSE, IDC_SIZEWE, KillTimer, LoadCursorW, MSG,
    PostQuitMessage, PostThreadMessageW, RegisterClassExW, SPI_GETCLIENTAREAANIMATION, SW_HIDE,
    SW_SHOW, SW_SHOWNOACTIVATE, SetCursor, SetForegroundWindow, SetTimer, ShowWindow,
    SystemParametersInfoW, TranslateMessage, WM_ACTIVATEAPP, WM_APP, WM_CHAR, WM_CLOSE,
    WM_ERASEBKGND, WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MBUTTONDOWN, WM_MBUTTONUP,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_RBUTTONDOWN, WM_RBUTTONUP, WM_SETCURSOR, WM_SYSCHAR,
    WM_SYSKEYDOWN, WM_TIMER, WNDCLASSEXW, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_POPUP,
};

use super::last_error;
use crate::error::OverlayError;
use crate::export::CapturedImage;
use crate::input::{Key, Modifiers, PointerButton};
use crate::render::text::Font;
use crate::render::{FrameView, Painter, Theme};
use crate::state::{
    CursorShape, Effect, Event, Millis, Outcome, OverlayConfig, OverlayState, Pixels, SettingChange,
};

const CLASS_NAME: &str = "PhramerCaptureOverlay";
const TIMER_ID: usize = 1;

/// What the overlay needs to start.
pub struct OverlaySession<'a> {
    /// Every monitor of the desktop, as enumerated for this capture.
    pub monitors: Vec<Monitor>,
    /// The monitor to open on.
    pub active: usize,
    pub config: OverlayConfig,
    pub theme: Theme,
    /// TrueType or OpenType bytes for the labels and the help message.
    pub font: &'a [u8],
    /// List the windows for window snap (not when a region is given).
    pub snap_windows: bool,
    /// When the capture was asked for; the first frame is timed from here.
    pub requested_at: Instant,
    /// The settings file the colour and tool sizes chosen on the overlay
    /// are written back to, as they are chosen; `None` keeps them to this
    /// capture.
    pub settings_path: Option<PathBuf>,
}

/// How a capture ended, with the pixels of an export.
pub struct Finished {
    pub outcome: Outcome,
    /// The exported region's pixels, and where they were on the desktop.
    pub capture: Option<(CapturedImage, PhysicalRect)>,
}

/// Grabs the active monitor, shows the overlay, runs it until it closes and
/// returns the outcome. Blocks the calling thread for the whole capture.
pub fn run_overlay(session: OverlaySession<'_>) -> Result<Finished, OverlayError> {
    let OverlaySession {
        monitors,
        active,
        config,
        theme,
        font,
        snap_windows,
        requested_at,
        settings_path,
    } = session;
    let mut config = config;
    config.reduced_motion = !client_area_animation();
    let monitor = monitors
        .get(active)
        .cloned()
        .ok_or(OverlayError::NoSuchMonitor(active))?;
    // Parsed now, so a bad font fails before any window exists
    Font::parse(font)?;

    let started = Instant::now();
    let frame = grab_monitor(&monitor)?;
    let grabbed = started.elapsed();
    // Listed before any window of ours exists, so they match the frame
    let windows = if snap_windows {
        visible_windows().unwrap_or_else(|error| {
            log::warn!("window snap unavailable: {error}");
            Vec::new()
        })
    } else {
        Vec::new()
    };

    let listed = started.elapsed();
    let mut state = OverlayState::new(monitors.clone(), active, windows, config)?;
    let class = window_class()?;
    let mut frames: Vec<Option<Frame>> = monitors.iter().map(|_| None).collect();
    frames[active] = Some(frame);

    let _shell_guard = ShellGuard;
    let mut windows: Vec<OverlayWindow> = Vec::new();
    let first = OverlayWindow::create(class, &monitor)?;
    let first_hwnd = first.hwnd;
    windows.push(first);
    PANICKED.with(|flag| flag.set(false));
    SHELL.with(|cell| {
        *cell.borrow_mut() = Some(Shell::new(
            &monitors,
            font,
            theme,
            first_hwnd,
            settings_path,
            requested_at,
        ));
    });
    with_shell(|shell| {
        shell.attach(active, first_hwnd, frames[active].take(), &theme);
    });

    // First frame: rendered before the window shows, presented by the
    // synchronous paint that follows. It assumes the keyboard will come
    // (no "lost focus" notice flashing up); checked once the window shows.
    let mut effects = state.start();
    effects.extend(state.handle(Event::Focus(true), 0));
    if let Ok(cursor) = cursor_position()
        && let Some(local) = monitor.rect_to_logical(PhysicalRect::new(cursor.x, cursor.y, 1, 1))
    {
        effects.extend(state.handle(
            Event::PointerMove {
                monitor: active,
                at: LogicalPoint::new(local.x, local.y),
                left_down: false,
                modifiers: Modifiers::NONE,
            },
            0,
        ));
    }
    with_shell(|shell| {
        shell.state = Some(state);
        shell.share_pixels(active);
    });
    run_effects(effects);
    let painted = with_shell(|shell| shell.paint_pending(active)).unwrap_or(Ok(()));
    painted?;
    let rendered = started.elapsed();
    // SAFETY: `first_hwnd` is a window this thread just created.
    unsafe {
        ShowWindow(first_hwnd, SW_SHOW);
        // Takes the keyboard when Windows allows it: the daemon holds the
        // foreground right of the hotkey press or of the forwarding CLI
        SetForegroundWindow(first_hwnd);
        UpdateWindow(first_hwnd);
        DwmFlush();
    }
    // SAFETY: a plain query.
    if unsafe { GetForegroundWindow() } != first_hwnd {
        log::warn!("overlay: Windows did not give it the keyboard");
        let effects = with_shell(|shell| shell.feed(Event::Focus(false))).unwrap_or_default();
        run_effects(effects);
    }
    let shown = requested_at.elapsed();
    let presented = started.elapsed();
    log::info!(
        "overlay: first frame on screen {} ms after the request ({} ms to start; grab {} ms, \
         window list {} ms, render {} ms, show and present {} ms; {}x{})",
        shown.as_millis(),
        shown.saturating_sub(presented).as_millis(),
        grabbed.as_millis(),
        listed.saturating_sub(grabbed).as_millis(),
        rendered.saturating_sub(listed).as_millis(),
        presented.saturating_sub(rendered).as_millis(),
        monitor.bounds().width,
        monitor.bounds().height
    );

    // Snip across all monitors: the rest are grabbed after the first frame
    // is up, so the user never waits for them, then dimmed
    if is_following() {
        open_other_monitors(&monitors, active, class, &theme, &mut windows, first_hwnd)?;
        if is_following() {
            log::info!(
                "overlay: every monitor on screen {} ms after the request ({} monitors)",
                requested_at.elapsed().as_millis(),
                windows.len()
            );
        }
    }

    pump_until_closed()?;

    let shell = SHELL.with(|cell| cell.borrow_mut().take());
    // The windows go first, whatever happens next
    drop(windows);
    let Some(mut shell) = shell else {
        return Err(OverlayError::Panicked);
    };
    let outcome = shell.outcome.take().unwrap_or(Outcome::Cancelled);
    let exported = match outcome {
        Outcome::Export(export) => Some((export.monitor, export.region)),
        Outcome::Deferred(deferred) => Some((deferred.monitor, deferred.region)),
        Outcome::Cancelled => None,
    };
    let capture = match (exported, shell.state.as_ref()) {
        (Some((index, region)), Some(state)) => {
            let frame = shell
                .slots
                .get_mut(index)
                .and_then(|slot| slot.as_mut())
                .and_then(|slot| slot.frame.take());
            match (frame, state.monitors().get(index)) {
                (Some(frame), Some(monitor)) => Some((
                    capture_region(state.document(), &frame.0, region, font)?,
                    monitor.rect_to_physical(region),
                )),
                _ => None,
            }
        }
        _ => None,
    };
    Ok(Finished { outcome, capture })
}

fn is_following() -> bool {
    with_shell(|shell| shell.state.as_ref().is_some_and(OverlayState::is_following))
        .unwrap_or(false)
}

/// Gives every monitor but `active` its window while the overlay follows
/// the cursor. The monitors are grabbed on another thread and get their
/// windows here as their pictures come in, between this thread's messages:
/// input on the windows already up is handled meanwhile, so a selection
/// made at once (which ends following) and the toolbar it brings never wait
/// for the other monitors. Returns when every monitor has its window, when
/// following ends or when the overlay closes.
fn open_other_monitors(
    monitors: &[Monitor],
    active: usize,
    class: u16,
    theme: &Theme,
    windows: &mut Vec<OverlayWindow>,
    first_hwnd: HWND,
) -> Result<(), OverlayError> {
    let others: Vec<(usize, Monitor)> = monitors
        .iter()
        .cloned()
        .enumerate()
        .filter(|(index, _)| *index != active)
        .collect();
    let mut waiting = others.len();
    // SAFETY: `first_hwnd` is a live window of this thread; a null process
    // id pointer is allowed.
    let thread = unsafe { GetWindowThreadProcessId(first_hwnd, std::ptr::null_mut()) };
    let (sender, frames) = mpsc::channel();
    let stop = StopFlag(Arc::new(AtomicBool::new(false)));
    let grabber_stop = Arc::clone(&stop.0);
    let spawned = std::thread::Builder::new()
        .name("overlay-grab".to_owned())
        .spawn(move || {
            for (index, monitor) in others {
                if grabber_stop.load(Ordering::Relaxed) {
                    return;
                }
                let frame = grab_monitor(&monitor);
                if grabber_stop.load(Ordering::Relaxed) || sender.send((index, frame)).is_err() {
                    return;
                }
                // SAFETY: posts a message without pointers to the overlay
                // thread's queue, which it is still pumping (it sets the
                // flag before it stops); it only wakes the pump up.
                unsafe { PostThreadMessageW(thread, WM_APP, 0, 0) };
            }
        });
    if let Err(error) = spawned {
        log::warn!("cannot grab the other monitors: {error}");
        return Ok(());
    }
    while waiting > 0 && is_following() {
        match frames.try_recv() {
            Ok((index, frame)) => {
                waiting -= 1;
                let frame = match frame {
                    Ok(frame) => frame,
                    Err(error) => {
                        log::warn!("cannot grab monitor {index}: {error}");
                        continue;
                    }
                };
                let Some(other) = monitors.get(index) else {
                    continue;
                };
                let window = OverlayWindow::create(class, other)?;
                let hwnd = window.hwnd;
                windows.push(window);
                with_shell(|shell| shell.attach(index, hwnd, Some(frame), theme));
                with_shell(|shell| shell.paint_pending(index)).unwrap_or(Ok(()))?;
                // SAFETY: a window this thread just created.
                unsafe {
                    ShowWindow(hwnd, SW_SHOWNOACTIVATE);
                    UpdateWindow(hwnd);
                }
            }
            Err(mpsc::TryRecvError::Disconnected) => break,
            // Waits for input, a paint or the next picture
            Err(mpsc::TryRecvError::Empty) => {
                if !pump_one()? {
                    break;
                }
            }
        }
    }
    Ok(())
}

/// Tells the grabbing thread to stop when dropped.
struct StopFlag(Arc<AtomicBool>);

impl Drop for StopFlag {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

/// The exported pixels: the frame's region, with any annotations drawn in.
fn capture_region(
    document: &Document,
    frame: &Frame,
    region: LocalRect,
    font: &[u8],
) -> Result<CapturedImage, OverlayError> {
    let bounds = (
        u32::try_from(region.x).unwrap_or(0),
        u32::try_from(region.y).unwrap_or(0),
        u32::try_from(region.width).unwrap_or(0),
        u32::try_from(region.height).unwrap_or(0),
    );
    if document.objects().is_empty() {
        return Ok(CapturedImage::crop(frame.bgra(), frame.width(), bounds));
    }
    let mut rgba = frame.bgra().to_vec();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    let base = phramer_annotate::BaseImage::new(frame.width(), frame.height(), &rgba)
        .map_err(|error| OverlayError::Encode(error.to_string()))?;
    let mut fonts = phramer_annotate::FontLibrary::new();
    fonts
        .add("Inter", false, false, font.to_vec())
        .map_err(|error| OverlayError::Font(error.to_string()))?;
    let rendered = phramer_annotate::render(
        document,
        base,
        &fonts,
        &phramer_annotate::RenderOptions::default(),
    )
    .map_err(|error| OverlayError::Encode(error.to_string()))?;
    let mut image = CapturedImage::crop(&rendered.rgba, rendered.bounds.width, {
        let offset_x = u32::try_from(-rendered.bounds.x).unwrap_or(0);
        let offset_y = u32::try_from(-rendered.bounds.y).unwrap_or(0);
        (bounds.0 + offset_x, bounds.1 + offset_y, bounds.2, bounds.3)
    });
    for pixel in image.bgra.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
    Ok(image)
}

/// Whether Windows animates controls (Settings > Accessibility > Visual
/// effects > Animation effects); off means reduced motion.
fn client_area_animation() -> bool {
    let mut enabled: i32 = 1;
    // SAFETY: SPI_GETCLIENTAREAANIMATION writes one BOOL to the pointer,
    // which points at a live i32.
    let ok = unsafe {
        SystemParametersInfoW(SPI_GETCLIENTAREAANIMATION, 0, (&raw mut enabled).cast(), 0)
    };
    ok == 0 || enabled != 0
}

/// Writes a choice made on the overlay to the settings file, as 15.0.0 did
/// when it was made.
fn persist(path: &std::path::Path, change: SettingChange) {
    let mut settings = match phramer_core::config::Settings::load(path) {
        Ok(settings) => settings,
        Err(error) => {
            log::error!("cannot read the settings to save {change:?}: {error}");
            return;
        }
    };
    if let Err(error) = change.apply(&mut settings) {
        log::error!("cannot save {change:?}: {error}");
        return;
    }
    if let Err(error) = settings.save(path) {
        log::error!("cannot save {change:?}: {error}");
    }
}

fn wide(text: &str) -> Vec<u16> {
    text.encode_utf16().chain(Some(0)).collect()
}

/// Registers the window class once per process.
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
        lpszClassName: name.as_ptr(),
        ..Default::default()
    };
    // SAFETY: `class` is fully initialised and `name` outlives the call;
    // the system copies the class name.
    let atom = unsafe { RegisterClassExW(&class) };
    if atom == 0 {
        // Another thread may have registered it first
        return CLASS
            .get()
            .copied()
            .ok_or_else(|| last_error("RegisterClassExW"));
    }
    Ok(*CLASS.get_or_init(|| atom))
}

/// A window of the overlay, destroyed when dropped.
struct OverlayWindow {
    hwnd: HWND,
}

impl OverlayWindow {
    /// A hidden popup exactly covering `monitor`.
    fn create(class: u16, monitor: &Monitor) -> Result<Self, OverlayError> {
        let bounds = monitor.bounds();
        let title = wide("Phramer capture");
        // SAFETY: the class atom is registered, `title` outlives the call,
        // and every other argument is a plain value or null.
        let hwnd = unsafe {
            CreateWindowExW(
                // Topmost; a tool window stays out of the taskbar and Alt+Tab
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class as usize as *const u16,
                title.as_ptr(),
                WS_POPUP,
                bounds.x,
                bounds.y,
                bounds.width,
                bounds.height,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                GetModuleHandleW(std::ptr::null()),
                std::ptr::null(),
            )
        };
        if hwnd.is_null() {
            return Err(last_error("CreateWindowExW"));
        }
        Ok(Self { hwnd })
    }
}

impl Drop for OverlayWindow {
    fn drop(&mut self) {
        // SAFETY: the window was created by this thread and is destroyed
        // once, here.
        unsafe { DestroyWindow(self.hwnd) };
    }
}

/// A monitor's screenshot, shared with the state for colour grab.
struct SharedFrame(Frame);

impl AsRef<[u8]> for SharedFrame {
    fn as_ref(&self) -> &[u8] {
        self.0.bgra()
    }
}

/// One monitor's window and what it shows.
struct Slot {
    hwnd: HWND,
    frame: Option<Arc<SharedFrame>>,
    painter: Option<Painter>,
    buffer: Vec<u8>,
    dirty: bool,
}

/// The running overlay, reachable from the window procedure.
struct Shell {
    state: Option<OverlayState>,
    slots: Vec<Option<Slot>>,
    scales: Vec<f64>,
    font: Vec<u8>,
    theme: Theme,
    timer_window: HWND,
    epoch: Instant,
    cursor: CursorShape,
    outcome: Option<Outcome>,
    settings_path: Option<PathBuf>,
    /// When the capture was asked for; the toolbar is timed from here.
    requested_at: Instant,
    /// When the input that brought the toolbar up was handled.
    toolbar_since: Option<Instant>,
    /// The toolbar's first appearance was logged.
    toolbar_logged: bool,
}

thread_local! {
    static SHELL: RefCell<Option<Shell>> = const { RefCell::new(None) };
    static PANICKED: Cell<bool> = const { Cell::new(false) };
}

/// Clears the thread's shell however `run_overlay` ends.
struct ShellGuard;

impl Drop for ShellGuard {
    fn drop(&mut self) {
        SHELL.with(|cell| {
            if let Ok(mut shell) = cell.try_borrow_mut() {
                shell.take();
            }
        });
    }
}

fn with_shell<T>(f: impl FnOnce(&mut Shell) -> T) -> Option<T> {
    SHELL.with(|cell| {
        let mut shell = cell.try_borrow_mut().ok()?;
        shell.as_mut().map(f)
    })
}

impl Shell {
    fn new(
        monitors: &[Monitor],
        font: &[u8],
        theme: Theme,
        timer_window: HWND,
        settings_path: Option<PathBuf>,
        requested_at: Instant,
    ) -> Self {
        Self {
            state: None,
            slots: monitors.iter().map(|_| None).collect(),
            scales: monitors.iter().map(Monitor::scale).collect(),
            font: font.to_vec(),
            theme,
            timer_window,
            epoch: Instant::now(),
            cursor: CursorShape::Cross,
            outcome: None,
            settings_path,
            requested_at,
            toolbar_since: None,
            toolbar_logged: false,
        }
    }

    fn attach(&mut self, monitor: usize, hwnd: HWND, frame: Option<Frame>, theme: &Theme) {
        let painter = frame
            .as_ref()
            .map(|frame| Painter::new(FrameView::from_frame(frame), theme, &self.font));
        let buffer = frame
            .as_ref()
            .map(|frame| vec![0; frame.bgra().len()])
            .unwrap_or_default();
        if let Some(slot) = self.slots.get_mut(monitor) {
            *slot = Some(Slot {
                hwnd,
                frame: frame.map(|frame| Arc::new(SharedFrame(frame))),
                painter,
                buffer,
                dirty: true,
            });
        }
        self.share_pixels(monitor);
    }

    /// Hands a monitor's screenshot to the state, for colour grab.
    fn share_pixels(&mut self, monitor: usize) {
        let Some(Some(slot)) = self.slots.get(monitor) else {
            return;
        };
        let (Some(frame), Some(state)) = (slot.frame.as_ref(), self.state.as_mut()) else {
            return;
        };
        let shared: Arc<dyn AsRef<[u8]> + Send + Sync> = frame.clone();
        match Pixels::new(frame.0.width(), frame.0.height(), shared) {
            Ok(pixels) => state.set_pixels(monitor, pixels),
            Err(error) => log::warn!("colour grab unavailable on monitor {monitor}: {error}"),
        }
    }

    fn now(&self) -> Millis {
        Millis::try_from(self.epoch.elapsed().as_millis()).unwrap_or(Millis::MAX)
    }

    fn monitor_of(&self, hwnd: HWND) -> Option<usize> {
        self.slots
            .iter()
            .position(|slot| slot.as_ref().is_some_and(|slot| slot.hwnd == hwnd))
    }

    /// Renders a window's buffer if its picture is out of date.
    fn paint_pending(&mut self, monitor: usize) -> Result<(), OverlayError> {
        let Some(state) = self.state.as_ref() else {
            return Ok(());
        };
        let Some(Some(slot)) = self.slots.get_mut(monitor) else {
            return Ok(());
        };
        if !slot.dirty {
            return Ok(());
        }
        let (Some(frame), Some(painter)) = (slot.frame.as_ref(), slot.painter.as_mut()) else {
            return Ok(());
        };
        let mut font = Font::parse(&self.font)?;
        painter.paint(
            state,
            monitor,
            FrameView::from_frame(&frame.0),
            &self.theme,
            &mut font,
            &mut slot.buffer,
        )?;
        slot.dirty = false;
        Ok(())
    }

    /// Feeds an event in and turns the effects into window work, which runs
    /// after the shell is released (it can send messages back here).
    fn feed(&mut self, event: Event) -> Vec<Effect> {
        let now = self.now();
        let Some(state) = self.state.as_mut() else {
            return Vec::new();
        };
        let hidden = !state.toolbar_visible();
        let effects = state.handle(event, now);
        if hidden && state.toolbar_visible() && self.toolbar_since.is_none() {
            self.toolbar_since = Some(Instant::now());
        }
        effects
    }

    /// Logs the first frame with the toolbar in it, once it is in the
    /// window: from the request, and from the input that brought it up.
    fn toolbar_painted(&mut self, monitor: usize) {
        if self.toolbar_logged {
            return;
        }
        let Some(state) = self.state.as_ref() else {
            return;
        };
        if monitor != state.active_monitor() || state.toolbar().is_empty() {
            return;
        }
        self.toolbar_logged = true;
        let since_input = self
            .toolbar_since
            .map(|at| format!("{} ms after the input", at.elapsed().as_millis()))
            .unwrap_or_else(|| "with the first frame".to_owned());
        log::info!(
            "overlay: toolbar on screen {} ms after the request ({since_input})",
            self.requested_at.elapsed().as_millis()
        );
    }
}

/// Window work an effect needs, done outside the shell's borrow.
enum Work {
    Invalidate(HWND),
    Cursor(CursorShape),
    Hide(HWND),
    Timer(HWND, u32),
}

/// Applies effects: bookkeeping inside the shell, window calls after.
fn run_effects(effects: Vec<Effect>) {
    if effects.is_empty() {
        return;
    }
    let work = with_shell(|shell| {
        let mut work = Vec::new();
        for effect in effects {
            match effect {
                Effect::Redraw { monitor } => {
                    if let Some(Some(slot)) = shell.slots.get_mut(monitor) {
                        slot.dirty = true;
                        work.push(Work::Invalidate(slot.hwnd));
                    }
                }
                Effect::SetCursor(shape) => {
                    shell.cursor = shape;
                    work.push(Work::Cursor(shape));
                }
                // Every monitor already has its window while following
                Effect::SwitchMonitor { .. } => {}
                Effect::ReleaseMonitors => {
                    let active = shell.state.as_ref().map(OverlayState::active_monitor);
                    for (index, slot) in shell.slots.iter().enumerate() {
                        if let Some(slot) = slot
                            && Some(index) != active
                        {
                            work.push(Work::Hide(slot.hwnd));
                        }
                    }
                }
                Effect::WakeAt(at) => {
                    let delay = at.saturating_sub(shell.now());
                    let delay = u32::try_from(delay).unwrap_or(u32::MAX).max(1);
                    work.push(Work::Timer(shell.timer_window, delay));
                }
                Effect::Persist(change) => match &shell.settings_path {
                    Some(path) => persist(path, change),
                    None => log::debug!("not saved: {change:?}"),
                },
                Effect::Close(outcome) => shell.outcome = Some(outcome),
            }
        }
        work
    })
    .unwrap_or_default();
    for item in work {
        // SAFETY: every handle is a window of this thread that is still
        // alive (windows are destroyed only after the loop ends); the calls
        // take plain values.
        unsafe {
            match item {
                Work::Invalidate(hwnd) => {
                    InvalidateRect(hwnd, std::ptr::null(), 0);
                }
                Work::Cursor(shape) => {
                    SetCursor(load_cursor(shape));
                }
                Work::Hide(hwnd) => {
                    ShowWindow(hwnd, SW_HIDE);
                }
                Work::Timer(hwnd, delay) => {
                    SetTimer(hwnd, TIMER_ID, delay, None);
                }
            }
        }
    }
}

fn load_cursor(shape: CursorShape) -> windows_sys::Win32::UI::WindowsAndMessaging::HCURSOR {
    let id = match shape {
        CursorShape::Cross => IDC_CROSS,
        CursorShape::Arrow => IDC_ARROW,
        CursorShape::ResizeNwSe => IDC_SIZENWSE,
        CursorShape::ResizeNeSw => IDC_SIZENESW,
        CursorShape::ResizeHorizontal => IDC_SIZEWE,
        CursorShape::ResizeVertical => IDC_SIZENS,
        CursorShape::Grabbing => IDC_SIZEALL,
    };
    // SAFETY: a system cursor id with a null module is the documented way
    // to load a shared system cursor.
    unsafe { LoadCursorW(std::ptr::null_mut(), id) }
}

/// Pumps this thread's messages until the overlay closes or a handler
/// panics.
fn pump_until_closed() -> Result<(), OverlayError> {
    while pump_one()? {}
    Ok(())
}

/// Waits for one message of this thread and handles it; `false` once the
/// overlay closed (or the loop must end), an error when a handler panicked.
fn pump_one() -> Result<bool, OverlayError> {
    if PANICKED.with(Cell::get) {
        return Err(OverlayError::Panicked);
    }
    if with_shell(|shell| shell.outcome.is_some()).unwrap_or(true) {
        return Ok(false);
    }
    let mut message = MSG::default();
    // SAFETY: `message` is a valid out-parameter; a null window reads
    // every message of this thread.
    let got = unsafe { GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) };
    if got == 0 {
        return Ok(false);
    }
    if got == -1 {
        return Err(last_error("GetMessageW"));
    }
    // SAFETY: `message` was filled in by GetMessageW; translating key
    // presses posts their WM_CHAR to this same queue.
    unsafe {
        TranslateMessage(&message);
        DispatchMessageW(&message);
    }
    Ok(true)
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
            // Ends the loop; `run_overlay` then destroys every window
            PANICKED.with(|flag| flag.set(true));
            // SAFETY: posts WM_QUIT to this thread's own queue.
            unsafe { PostQuitMessage(1) };
            0
        }
    }
}

/// `Some(result)` for a message the overlay handled.
fn handle_message(hwnd: HWND, message: u32, wparam: WPARAM, lparam: LPARAM) -> Option<LRESULT> {
    match message {
        WM_PAINT => {
            paint(hwnd);
            Some(0)
        }
        // The whole client area is painted from the buffer
        WM_ERASEBKGND => Some(1),
        WM_SETCURSOR if (lparam & 0xffff) as u32 == HTCLIENT => {
            let shape = with_shell(|shell| shell.cursor)?;
            // SAFETY: sets a shared system cursor.
            unsafe { SetCursor(load_cursor(shape)) };
            Some(1)
        }
        WM_MOUSEMOVE => {
            let monitor = with_shell(|shell| shell.monitor_of(hwnd))??;
            pointer(monitor, lparam, |at| Event::PointerMove {
                monitor,
                at,
                left_down: wparam & MK_LBUTTON as usize != 0,
                modifiers: pointer_modifiers(wparam),
            });
            Some(0)
        }
        WM_MOUSEWHEEL => {
            let monitor = with_shell(|shell| shell.monitor_of(hwnd))??;
            let delta = i32::from((wparam >> 16) as u16 as i16);
            // Wheel positions are on the screen; the window covers its
            // monitor, so the monitor's corner makes them client positions
            let origin = with_shell(|shell| {
                shell
                    .state
                    .as_ref()
                    .and_then(|state| state.monitors().get(monitor))
                    .map(|monitor| monitor.bounds())
            })??;
            let x = (lparam & 0xffff) as u16 as i16 as i32 - origin.x;
            let y = ((lparam >> 16) & 0xffff) as u16 as i16 as i32 - origin.y;
            let client = ((y as u32 & 0xffff) << 16 | (x as u32 & 0xffff)) as LPARAM;
            pointer(monitor, client, |at| Event::Wheel {
                monitor,
                at,
                delta,
                modifiers: pointer_modifiers(wparam),
            });
            Some(0)
        }
        WM_CHAR => {
            let c = char::from_u32(u32::try_from(wparam).ok()?)?;
            let effects = with_shell(|shell| shell.feed(Event::Text(c)))?;
            run_effects(effects);
            Some(0)
        }
        WM_LBUTTONDOWN | WM_RBUTTONDOWN | WM_MBUTTONDOWN => {
            let monitor = with_shell(|shell| shell.monitor_of(hwnd))??;
            // SAFETY: captures the pointer for a window of this thread.
            unsafe { SetCapture(hwnd) };
            pointer(monitor, lparam, |at| Event::PointerDown {
                monitor,
                at,
                button: button_of(message),
                modifiers: pointer_modifiers(wparam),
            });
            Some(0)
        }
        WM_LBUTTONUP | WM_RBUTTONUP | WM_MBUTTONUP => {
            let monitor = with_shell(|shell| shell.monitor_of(hwnd))??;
            // SAFETY: releases this thread's pointer capture.
            unsafe { ReleaseCapture() };
            pointer(monitor, lparam, |at| Event::PointerUp {
                monitor,
                at,
                button: button_of(message),
                modifiers: pointer_modifiers(wparam),
            });
            Some(0)
        }
        WM_KEYDOWN | WM_SYSKEYDOWN => {
            let key = key_of(wparam, lparam)?;
            let effects = with_shell(|shell| {
                shell.feed(Event::Key {
                    key,
                    modifiers: keyboard_modifiers(),
                })
            })?;
            run_effects(effects);
            // System keys go on to the default handling, so Alt+F4 becomes
            // WM_CLOSE (a cancel, below)
            (message == WM_KEYDOWN).then_some(0)
        }
        // Alt+G and the like were handled as keys: no menu, no beep
        WM_SYSCHAR => Some(0),
        // Alt+F4 and the like: a cancel, never a destroyed window
        WM_CLOSE => {
            let effects = with_shell(|shell| {
                shell.feed(Event::Key {
                    key: Key::Escape,
                    modifiers: Modifiers::NONE,
                })
            })?;
            run_effects(effects);
            Some(0)
        }
        WM_ACTIVATEAPP => {
            let effects = with_shell(|shell| shell.feed(Event::Focus(wparam != 0)))?;
            run_effects(effects);
            Some(0)
        }
        WM_TIMER if wparam == TIMER_ID => {
            // SAFETY: stops this window's own one-shot timer.
            unsafe { KillTimer(hwnd, TIMER_ID) };
            let effects = with_shell(|shell| shell.feed(Event::Tick))?;
            run_effects(effects);
            Some(0)
        }
        _ => None,
    }
}

/// Client coordinates (physical) to the monitor's logical ones, then the
/// event.
fn pointer(monitor: usize, lparam: LPARAM, event: impl FnOnce(LogicalPoint) -> Event) {
    let x = f64::from((lparam & 0xffff) as u16 as i16);
    let y = f64::from(((lparam >> 16) & 0xffff) as u16 as i16);
    let effects = with_shell(|shell| {
        let scale = shell.scales.get(monitor).copied().unwrap_or(1.0);
        let at = LogicalPoint::new((x / scale).round() as i32, (y / scale).round() as i32);
        shell.feed(event(at))
    })
    .unwrap_or_default();
    run_effects(effects);
}

fn button_of(message: u32) -> PointerButton {
    match message {
        WM_RBUTTONDOWN | WM_RBUTTONUP => PointerButton::Right,
        WM_MBUTTONDOWN | WM_MBUTTONUP => PointerButton::Middle,
        _ => PointerButton::Left,
    }
}

fn key_down(key: u16) -> bool {
    // SAFETY: reads this thread's keyboard state for one virtual key.
    unsafe { GetKeyState(i32::from(key)) < 0 }
}

fn keyboard_modifiers() -> Modifiers {
    Modifiers {
        ctrl: key_down(VK_CONTROL),
        shift: key_down(VK_SHIFT),
        alt: key_down(VK_MENU),
        meta: key_down(VK_LWIN) || key_down(VK_RWIN),
    }
}

fn pointer_modifiers(wparam: WPARAM) -> Modifiers {
    Modifiers {
        ctrl: wparam & MK_CONTROL as usize != 0,
        shift: wparam & MK_SHIFT as usize != 0,
        ..keyboard_modifiers()
    }
}

/// A virtual key as a [`Key`]; `None` for modifier keys and keys the
/// overlay has no use for.
fn key_of(wparam: WPARAM, lparam: LPARAM) -> Option<Key> {
    let vk = u16::try_from(wparam).ok()?;
    // Bit 24: an extended key, which for Return means the keypad's Enter
    let extended = (lparam >> 24) & 1 == 1;
    Some(match vk {
        VK_ESCAPE => Key::Escape,
        VK_RETURN if extended => Key::Enter,
        VK_RETURN => Key::Return,
        VK_BACK => Key::Backspace,
        VK_DELETE => Key::Delete,
        VK_INSERT => Key::Insert,
        VK_TAB => Key::Tab,
        VK_SPACE => Key::Space,
        VK_LEFT => Key::Left,
        VK_RIGHT => Key::Right,
        VK_UP => Key::Up,
        VK_DOWN => Key::Down,
        VK_HOME => Key::Home,
        VK_END => Key::End,
        VK_PRIOR => Key::PageUp,
        VK_NEXT => Key::PageDown,
        f if (VK_F1..=VK_F24).contains(&f) => Key::Function(u8::try_from(f - VK_F1 + 1).ok()?),
        n if (VK_NUMPAD0..=VK_NUMPAD9).contains(&n) => {
            Key::Char(char::from(b'0' + u8::try_from(n - VK_NUMPAD0).ok()?))
        }
        letter @ 0x41..=0x5a => {
            Key::Char(char::from(u8::try_from(letter).ok()?).to_ascii_lowercase())
        }
        digit @ 0x30..=0x39 => Key::Char(char::from(u8::try_from(digit).ok()?)),
        other => {
            // Punctuation: the character the layout puts on the key
            // SAFETY: a pure lookup in the active keyboard layout.
            let mapped = unsafe { MapVirtualKeyW(u32::from(other), MAPVK_VK_TO_CHAR) } & 0x7fff;
            let c = char::from_u32(mapped).filter(|c| !c.is_control())?;
            Key::Char(c.to_lowercase().next().unwrap_or(c))
        }
    })
}

fn paint(hwnd: HWND) {
    let mut paint = PAINTSTRUCT::default();
    // SAFETY: `hwnd` is a window of this thread receiving WM_PAINT;
    // `paint` is a valid out-parameter, ended below.
    let dc = unsafe { BeginPaint(hwnd, &mut paint) };
    let drawn = with_shell(|shell| {
        let monitor = shell.monitor_of(hwnd)?;
        if let Err(error) = shell.paint_pending(monitor) {
            log::error!("overlay paint failed: {error}");
        }
        let slot = shell.slots.get(monitor)?.as_ref()?;
        let frame = &slot.frame.as_ref()?.0;
        let (width, height) = (frame.width(), frame.height());
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: i32::try_from(width).ok()?,
                // Negative: rows top-down, as the buffer holds them
                biHeight: -i32::try_from(height).ok()?,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: `dc` is the paint DC; `buffer` holds `width * height`
        // 32-bit pixels described by `info`, and both outlive the call.
        unsafe {
            SetDIBitsToDevice(
                dc,
                0,
                0,
                width,
                height,
                0,
                0,
                0,
                height,
                slot.buffer.as_ptr().cast(),
                &info,
                DIB_RGB_COLORS,
            );
        }
        shell.toolbar_painted(monitor);
        Some(())
    });
    if drawn.is_none() {
        log::debug!("overlay paint skipped");
    }
    // SAFETY: pairs the BeginPaint above.
    unsafe { EndPaint(hwnd, &paint) };
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use phramer_capture::{MonitorId, PhysicalRect};
    use windows_sys::Win32::Graphics::Gdi::{
        CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, SelectObject,
    };

    use super::*;

    const ROUNDS: u32 = 10;

    fn monitor_4k() -> Monitor {
        Monitor::new(
            MonitorId::new("4k"),
            PhysicalRect::new(0, 0, 3840, 2160),
            144,
            true,
        )
    }

    fn frame_4k() -> Vec<u8> {
        (0..3840u32 * 2160)
            .flat_map(|i| [(i % 251) as u8, (i % 241) as u8, (i % 239) as u8, 255])
            .collect()
    }

    fn pointer(state: &mut OverlayState, kind: &str, x: i32, y: i32) {
        let at = LogicalPoint::new(x, y);
        let event = match kind {
            "down" => Event::PointerDown {
                monitor: 0,
                at,
                button: PointerButton::Left,
                modifiers: Modifiers::NONE,
            },
            "up" => Event::PointerUp {
                monitor: 0,
                at,
                button: PointerButton::Left,
                modifiers: Modifiers::NONE,
            },
            _ => Event::PointerMove {
                monitor: 0,
                at,
                left_down: kind == "drag",
                modifiers: Modifiers::NONE,
            },
        };
        state.handle(event, 0);
    }

    fn key(state: &mut OverlayState, c: char) {
        state.handle(
            Event::Key {
                key: Key::Char(c),
                modifiers: Modifiers::NONE,
            },
            0,
        );
    }

    /// The time `SetDIBitsToDevice` takes to copy `buffer` into a DIB
    /// section of its size, standing in for the window's surface.
    fn present_time(buffer: &[u8], width: u32, height: u32) -> Duration {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width as i32,
                biHeight: -(height as i32),
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB,
                ..Default::default()
            },
            ..Default::default()
        };
        // SAFETY: a memory DC with a DIB section of the buffer's size
        // selected into it, all released below; the buffer matches `info`.
        unsafe {
            let dc = CreateCompatibleDC(std::ptr::null_mut());
            let mut bits = std::ptr::null_mut();
            let bitmap = CreateDIBSection(
                dc,
                &info,
                DIB_RGB_COLORS,
                &mut bits,
                std::ptr::null_mut(),
                0,
            );
            let previous = SelectObject(dc, bitmap);
            let started = Instant::now();
            for _ in 0..ROUNDS {
                SetDIBitsToDevice(
                    dc,
                    0,
                    0,
                    width,
                    height,
                    0,
                    0,
                    0,
                    height,
                    buffer.as_ptr().cast(),
                    &info,
                    DIB_RGB_COLORS,
                );
            }
            let present = started.elapsed() / ROUNDS;
            SelectObject(dc, previous);
            DeleteObject(bitmap);
            DeleteDC(dc);
            present
        }
    }

    fn ms(duration: Duration) -> f64 {
        duration.as_secs_f64() * 1000.0
    }

    /// What one interactive redraw costs at 4K: the render, then the copy
    /// GDI makes of the buffer.
    #[test]
    #[ignore = "timing; run with --release"]
    fn render_and_present_a_4k_frame() {
        let (width, height) = (3840u32, 2160u32);
        let mut state =
            OverlayState::new(vec![monitor_4k()], 0, Vec::new(), OverlayConfig::default()).unwrap();
        state.handle(Event::Focus(true), 0);
        pointer(&mut state, "down", 300, 200);
        pointer(&mut state, "drag", 900, 700);
        pointer(&mut state, "drag", 1500, 1000);
        let frame = frame_4k();
        let view = FrameView::new(width, height, &frame).unwrap();
        let theme = Theme::default();
        let started = Instant::now();
        let mut painter = Painter::new(view, &theme, crate::BUNDLED_FONT);
        let dimmed = started.elapsed();
        let mut font = Font::parse(crate::BUNDLED_FONT).unwrap();
        let mut buffer = vec![0u8; frame.len()];
        let started = Instant::now();
        for _ in 0..ROUNDS {
            painter
                .paint(&state, 0, view, &theme, &mut font, &mut buffer)
                .unwrap();
        }
        let render = started.elapsed() / ROUNDS;
        let present = present_time(&buffer, width, height);
        println!(
            "4K: dim once {} ms; per redraw: render {:.1} ms, SetDIBitsToDevice {:.1} ms",
            dimmed.as_millis(),
            ms(render),
            ms(present)
        );
    }

    /// A 4K redraw with the toolbar up and 50 objects placed, as a pointer
    /// move over the toolbar causes it: the annotations come from the
    /// painter's cache, so the cost does not grow with the objects. Also
    /// times a redraw while a new rectangle is dragged out (its preview is
    /// rendered again each frame) and the one-off rebuild of the cache.
    #[test]
    #[ignore = "timing; run with --release"]
    fn redraw_a_4k_frame_with_the_toolbar_and_50_objects() {
        let (width, height) = (3840u32, 2160u32);
        let config = OverlayConfig {
            show_help: false,
            ..OverlayConfig::default()
        };
        let mut state = OverlayState::new(vec![monitor_4k()], 0, Vec::new(), config).unwrap();
        state.handle(Event::Focus(true), 0);
        // A selection of most of the monitor (logical, 150%)
        pointer(&mut state, "down", 200, 150);
        pointer(&mut state, "drag", 1200, 800);
        pointer(&mut state, "drag", 2300, 1250);
        pointer(&mut state, "up", 2300, 1250);
        let tools = ['r', 'c', 'a', 'd', 'm', 'p'];
        for index in 0..50usize {
            key(&mut state, tools[index % tools.len()]);
            let (x, y) = (
                260 + (index as i32 % 10) * 200,
                200 + (index as i32 / 10) * 200,
            );
            pointer(&mut state, "down", x, y);
            pointer(&mut state, "drag", x + 60, y + 50);
            pointer(&mut state, "drag", x + 150, y + 120);
            pointer(&mut state, "up", x + 150, y + 120);
        }
        assert_eq!(state.document().objects().len(), 50);
        assert!(!state.toolbar().is_empty());

        let frame = frame_4k();
        let view = FrameView::new(width, height, &frame).unwrap();
        let theme = Theme::default();
        let mut painter = Painter::new(view, &theme, crate::BUNDLED_FONT);
        let mut font = Font::parse(crate::BUNDLED_FONT).unwrap();
        let mut buffer = vec![0u8; frame.len()];
        let started = Instant::now();
        painter
            .paint(&state, 0, view, &theme, &mut font, &mut buffer)
            .unwrap();
        let rebuild = started.elapsed();

        // In and out of the first toolbar button, one paint per move
        let button = state.toolbar()[0].rect;
        let started = Instant::now();
        for round in 0..ROUNDS {
            let (x, y) = if round % 2 == 0 {
                (button.x + 4, button.y + 4)
            } else {
                (button.x - 30, button.y - 30)
            };
            pointer(&mut state, "move", x, y);
            painter
                .paint(&state, 0, view, &theme, &mut font, &mut buffer)
                .unwrap();
        }
        let redraw = started.elapsed() / ROUNDS;

        // A rectangle being dragged out across the selection
        key(&mut state, 'r');
        if state.active_tool().is_none() {
            key(&mut state, 'r');
        }
        pointer(&mut state, "down", 400, 300);
        let started = Instant::now();
        for round in 0..ROUNDS as i32 {
            pointer(&mut state, "drag", 900 + round * 20, 700 + round * 10);
            painter
                .paint(&state, 0, view, &theme, &mut font, &mut buffer)
                .unwrap();
        }
        let drawing = started.elapsed() / ROUNDS;
        let present = present_time(&buffer, width, height);
        println!(
            "4K, toolbar and 50 objects: cache rebuild once {:.1} ms; per redraw: \
             pointer move {:.1} ms (+ SetDIBitsToDevice {:.1} ms = {:.1} ms), \
             dragging a new rectangle {:.1} ms",
            ms(rebuild),
            ms(redraw),
            ms(present),
            ms(redraw + present),
            ms(drawing)
        );
        assert!(
            redraw + present < Duration::from_millis(16),
            "a redraw took {:.1} ms",
            ms(redraw + present)
        );
    }

    /// A 4K pointer move while a selection is dragged out, without and with
    /// the magnifier (round and square), and with the side panel open: the
    /// magnifier draws only its own square, so it adds little.
    #[test]
    #[ignore = "timing; run with --release"]
    fn redraw_a_4k_frame_with_the_magnifier_and_the_side_panel() {
        let (width, height) = (3840u32, 2160u32);
        let frame = frame_4k();
        let view = FrameView::new(width, height, &frame).unwrap();
        let theme = Theme::default();
        let mut font = Font::parse(crate::BUNDLED_FONT).unwrap();
        let mut buffer = vec![0u8; frame.len()];
        let mut timed = |show_magnifier: bool, square_magnifier: bool, panel: bool| {
            let config = OverlayConfig {
                show_help: false,
                show_magnifier,
                square_magnifier,
                ..OverlayConfig::default()
            };
            let mut state = OverlayState::new(vec![monitor_4k()], 0, Vec::new(), config).unwrap();
            state.handle(Event::Focus(true), 0);
            if panel {
                state.handle(
                    Event::Key {
                        key: Key::Space,
                        modifiers: Modifiers::NONE,
                    },
                    0,
                );
            }
            let mut painter = Painter::new(view, &theme, crate::BUNDLED_FONT);
            pointer(&mut state, "down", 300, 200);
            pointer(&mut state, "drag", 900, 700);
            painter
                .paint(&state, 0, view, &theme, &mut font, &mut buffer)
                .unwrap();
            assert_eq!(state.magnifier().is_some(), show_magnifier);
            assert_eq!(state.side_panel().is_some(), panel);
            let started = Instant::now();
            for round in 0..ROUNDS as i32 {
                pointer(&mut state, "drag", 1000 + round * 37, 800 + round * 23);
                painter
                    .paint(&state, 0, view, &theme, &mut font, &mut buffer)
                    .unwrap();
            }
            started.elapsed() / ROUNDS
        };
        let plain = timed(false, false, false);
        let round = timed(true, false, false);
        let square = timed(true, true, false);
        let panel = timed(true, false, true);
        let present = present_time(&buffer, width, height);
        println!(
            "4K pointer move while selecting: render {:.1} ms without the magnifier, {:.1} ms \
             round, {:.1} ms square, {:.1} ms round with the side panel open; \
             + SetDIBitsToDevice {:.1} ms",
            ms(plain),
            ms(round),
            ms(square),
            ms(panel),
            ms(present)
        );
        for redraw in [round, square, panel] {
            assert!(
                redraw + present < Duration::from_millis(16),
                "a redraw took {:.1} ms",
                ms(redraw + present)
            );
        }
    }

    /// While the overlay follows the cursor, the other monitors get their
    /// windows on the overlay thread as their pictures come in; input that
    /// arrives meanwhile waits for at most one of them. Times that step
    /// (dimming the picture, the first paint, the copy into the window) for
    /// three monitors at mixed scales, and the first frame with the toolbar
    /// on the 4K one, which together must stay well inside the 150 ms
    /// feedback rule once the first frame (100 ms budget) is up.
    #[test]
    #[ignore = "timing; run with --release"]
    fn other_monitor_windows_and_the_toolbar_at_mixed_scales() {
        let monitors = vec![
            monitor_4k(),
            Monitor::new(
                MonitorId::new("1200p"),
                PhysicalRect::new(3840, 0, 1920, 1200),
                120,
                false,
            ),
            Monitor::new(
                MonitorId::new("1080p"),
                PhysicalRect::new(5760, 0, 1920, 1080),
                96,
                false,
            ),
        ];
        let config = OverlayConfig {
            show_help: false,
            follow_cursor: true,
            ..OverlayConfig::default()
        };
        let mut state = OverlayState::new(monitors.clone(), 0, Vec::new(), config).unwrap();
        state.handle(Event::Focus(true), 0);
        let theme = Theme::default();
        let mut font = Font::parse(crate::BUNDLED_FONT).unwrap();
        let mut slowest = Duration::ZERO;
        for (index, monitor) in monitors.iter().enumerate().skip(1) {
            let bounds = monitor.bounds();
            let (width, height) = (bounds.width as u32, bounds.height as u32);
            let frame: Vec<u8> = (0..width * height)
                .flat_map(|i| [(i % 251) as u8, (i % 241) as u8, (i % 239) as u8, 255])
                .collect();
            let view = FrameView::new(width, height, &frame).unwrap();
            let started = Instant::now();
            let mut painter = Painter::new(view, &theme, crate::BUNDLED_FONT);
            let mut buffer = vec![0u8; frame.len()];
            painter
                .paint(&state, index, view, &theme, &mut font, &mut buffer)
                .unwrap();
            let opened = started.elapsed() + present_time(&buffer, width, height);
            println!(
                "{width}x{height} at {}x: window picture {:.1} ms",
                monitor.scale(),
                ms(opened)
            );
            slowest = slowest.max(opened);
        }

        // A selection on the 4K monitor ends following; the toolbar shows
        pointer(&mut state, "down", 300, 200);
        pointer(&mut state, "drag", 1500, 1000);
        pointer(&mut state, "up", 1500, 1000);
        assert!(!state.toolbar().is_empty());
        let frame = frame_4k();
        let view = FrameView::new(3840, 2160, &frame).unwrap();
        let mut painter = Painter::new(view, &theme, crate::BUNDLED_FONT);
        let mut buffer = vec![0u8; frame.len()];
        let started = Instant::now();
        painter
            .paint(&state, 0, view, &theme, &mut font, &mut buffer)
            .unwrap();
        let toolbar = started.elapsed() + present_time(&buffer, 3840, 2160);
        println!(
            "slowest other window {:.1} ms; 4K frame with the toolbar {:.1} ms",
            ms(slowest),
            ms(toolbar)
        );
        assert!(
            slowest + toolbar < Duration::from_millis(50),
            "input could wait {:.1} ms for the toolbar",
            ms(slowest + toolbar)
        );
    }
}
