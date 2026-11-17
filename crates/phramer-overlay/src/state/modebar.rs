//! The Screenshot/Video mode bar (15.0.0's `CaptureModeBar`): at the top
//! of the overlay while video capture is on, until the first press starts
//! a selection. Screenshot is what the overlay already does; Video closes
//! it and hands over to Snipping Tool's recorder. The keys are
//! `CAPTURE_MODE_SCREENSHOT` and `CAPTURE_MODE_VIDEO` (1 and 2), taken
//! before the digits that set the tool size.

use phramer_capture::{LogicalPoint, LogicalRect};

use super::{Effects, Outcome, OverlayState};
use crate::input::KeyAction;

/// The bar's distance from the top of the monitor (`TopMargin`).
const TOP: i32 = 12;
/// Around and between the segments (`--space-4`).
const PAD: i32 = 4;
const SEGMENT_WIDTH: i32 = 128;
const SEGMENT_HEIGHT: i32 = 32;

/// A segment of the mode bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CaptureMode {
    Screenshot,
    Video,
}

/// Where the mode bar's parts are, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ModeBarLayout {
    pub bar: LogicalRect,
    pub screenshot: LogicalRect,
    pub video: LogicalRect,
}

/// The mode bar centred at the top of a monitor of logical size `screen`.
pub fn mode_bar_layout(screen: LogicalRect) -> ModeBarLayout {
    let width = 3 * PAD + 2 * SEGMENT_WIDTH;
    let bar = LogicalRect::new(
        screen.x + (screen.width - width) / 2,
        screen.y + TOP,
        width,
        SEGMENT_HEIGHT + 2 * PAD,
    );
    let screenshot = LogicalRect::new(bar.x + PAD, bar.y + PAD, SEGMENT_WIDTH, SEGMENT_HEIGHT);
    let video = LogicalRect::new(
        screenshot.x + SEGMENT_WIDTH + PAD,
        screenshot.y,
        SEGMENT_WIDTH,
        SEGMENT_HEIGHT,
    );
    ModeBarLayout {
        bar,
        screenshot,
        video,
    }
}

/// The mode bar as drawn.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModeBarView {
    pub layout: ModeBarLayout,
    /// The segment under the pointer.
    pub hovered: Option<CaptureMode>,
    /// Each segment's key, as the help message spells keys.
    pub screenshot_key: Option<String>,
    pub video_key: Option<String>,
}

impl OverlayState {
    /// Whether the mode bar shows.
    pub fn mode_bar_visible(&self) -> bool {
        self.mode_bar && self.selection.is_none() && self.grab.is_none()
    }

    /// The mode bar, while it shows.
    pub fn mode_bar(&self) -> Option<ModeBarView> {
        if !self.mode_bar_visible() {
            return None;
        }
        let key = |action| {
            self.config
                .bindings
                .shortcut(action)
                .map(|shortcut| shortcut.label())
        };
        Some(ModeBarView {
            layout: mode_bar_layout(self.monitor_rect()),
            hovered: self.mode_hover,
            screenshot_key: key(KeyAction::CaptureMode(CaptureMode::Screenshot)),
            video_key: key(KeyAction::CaptureMode(CaptureMode::Video)),
        })
    }

    pub(crate) fn over_mode_bar(&self, at: LogicalPoint) -> bool {
        self.mode_bar_visible() && mode_bar_layout(self.monitor_rect()).bar.contains(at)
    }

    fn mode_at(&self, at: LogicalPoint) -> Option<CaptureMode> {
        if !self.mode_bar_visible() {
            return None;
        }
        let layout = mode_bar_layout(self.monitor_rect());
        if layout.screenshot.contains(at) {
            Some(CaptureMode::Screenshot)
        } else if layout.video.contains(at) {
            Some(CaptureMode::Video)
        } else {
            None
        }
    }

    /// A left press: on the bar it picks a mode; anywhere else it starts
    /// the capture and the bar goes. Returns whether the bar took it.
    pub(crate) fn mode_bar_press(&mut self, at: LogicalPoint, effects: &mut Effects) -> bool {
        if !self.mode_bar_visible() {
            return false;
        }
        if self.over_mode_bar(at) {
            if let Some(mode) = self.mode_at(at) {
                self.choose_mode(mode, effects);
            }
            return true;
        }
        self.mode_bar = false;
        self.mode_hover = None;
        effects.redraw(self.active);
        false
    }

    pub(crate) fn mode_bar_hover(&mut self, at: LogicalPoint, effects: &mut Effects) {
        let hovered = self.mode_at(at);
        if hovered != self.mode_hover {
            self.mode_hover = hovered;
            effects.redraw(self.active);
        }
    }

    /// Screenshot is the mode the overlay is already in; Video ends it.
    pub(crate) fn choose_mode(&mut self, mode: CaptureMode, effects: &mut Effects) {
        if mode == CaptureMode::Video {
            self.close(Outcome::RecordVideo, effects);
        }
    }
}
