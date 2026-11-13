//! Phramer's capture overlay: the fullscreen window shown after the capture
//! hotkey, from the frozen screenshot to the exported selection.
//!
//! Three layers, each usable without the next:
//!
//! - [`OverlayState`]: the behaviour, a pure state machine. [`Event`]s in
//!   (pointer, wheel, keys, text, time, focus) in the logical coordinates of
//!   one monitor, [`Effect`]s out (redraw, cursor, switch monitor, settings
//!   to save, close). It owns the selection, the toolbar and the annotation
//!   [`Document`](phramer_annotate::Document) its tools edit.
//! - [`render`]: the pixels of one monitor's window for a state, drawn with
//!   `tiny-skia` and the caller's font bytes ([`BUNDLED_FONT`] is Inter
//!   Variable).
//! - The Windows shell (`run_overlay`, Windows only): one topmost borderless window per
//!   monitor, input translated into events, effects carried out, and the
//!   [`Outcome`] handed back once every window is gone. Exporting it is
//!   [`export`]'s job, called exactly once by the owner of the capture.
//!
//! Coordinates follow `phramer-capture`: logical inside the state, physical
//! in frames and exported regions, one monitor at a time.

mod error;
mod export;
mod icons;
mod input;
mod pin;
mod render;
mod state;
#[cfg(windows)]
mod win;

pub use error::OverlayError;
#[cfg(windows)]
pub use export::LocalTime;
pub use export::{
    CaptureTasks, CapturedImage, ExportContext, ExportReport, Handoff, ImageFormat, Notice,
    SaveTarget, TimeFormat, encode, expand_pattern, export, screenshot_path,
};
pub use icons::{ICONS, has_icon};
pub use input::{Bindings, Key, Modifiers, PointerButton};
pub use pin::{
    PIN_MENU, PinEffect, PinEvent, PinKey, PinMenuItem, PinRequest, PinState, render_pin,
    rotate_left, rotate_right, scale_image,
};
pub use render::{FrameView, RgbaImage, Theme, render};
pub use state::tools::{ButtonInfo, MAX_TOOL_SIZE, TOOLBAR_ORDER, button_info, is_tool};
pub use state::{
    CursorShape, Deferred, DeferredAction, Effect, Event, Export, ExportAction, GRAB_KEYS,
    GeometryLabel, GrabView, MagnifierShape, MagnifierView, Millis, Outcome, OverlayConfig,
    OverlayState, PanelControl, PanelLayout, PanelRow, Pixels, SettingChange, ShapeKind,
    ShapeOption, SidePanelView, ToolSettings, ToolbarButton, WheelView, Zoom, disc_position,
    side_panel_layout, side_panel_tab,
};
#[cfg(windows)]
pub use win::{
    Finished, OverlaySession, PinHandler, PinOptions, SystemClipboard, open_pins, run_overlay,
    show_in_folder, spawn_pin,
};

/// Inter Variable 4.1 (SIL Open Font License 1.1; see
/// `assets/Inter-LICENSE.txt`), the overlay's UI font.
pub const BUNDLED_FONT: &[u8] = include_bytes!("../assets/InterVariable.ttf");
