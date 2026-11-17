//! 15.0.0's "Quit Capture" question (`CaptureWidget::promptQuit`), drawn in
//! the overlay as a card instead of a system dialog. While it shows it takes
//! every key and press: Enter or Quit ends the capture, Esc or Cancel goes
//! back to it, Space or a click ticks "Do not show this again", which is
//! saved whichever way the question is answered, as 15.0.0's check box was.

use phramer_capture::{LogicalPoint, LogicalRect};

use super::{CursorShape, Effect, Effects, Event, Outcome, OverlayState, SettingChange};
use crate::input::{Key, PointerButton};

const CARD_WIDTH: i32 = 360;
/// Around the card's content (`--space-16`).
const PAD: i32 = 16;
/// From the card's top to the check box row, under the title and question.
const CHECK_TOP: i32 = 74;
const CHECK_HEIGHT: i32 = 20;
/// From the card's top to the buttons.
const BUTTONS_TOP: i32 = 110;
const BUTTON_WIDTH: i32 = 104;
const BUTTON_HEIGHT: i32 = 28;
/// Between the buttons (`--space-8`).
const GAP: i32 = 8;

/// A part of the prompt that takes a click.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum QuitControl {
    StopAsking,
    Cancel,
    Quit,
}

/// Where the prompt's parts are, in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitPromptLayout {
    pub card: LogicalRect,
    /// The check box and its label: the whole row takes the click.
    pub stop_asking: LogicalRect,
    pub cancel: LogicalRect,
    pub quit: LogicalRect,
}

/// The prompt centred on a monitor of logical size `screen`.
pub fn quit_prompt_layout(screen: LogicalRect) -> QuitPromptLayout {
    let height = BUTTONS_TOP + BUTTON_HEIGHT + PAD;
    let card = LogicalRect::new(
        screen.x + (screen.width - CARD_WIDTH) / 2,
        screen.y + (screen.height - height) / 2,
        CARD_WIDTH,
        height,
    );
    let quit = LogicalRect::new(
        card.right() - PAD - BUTTON_WIDTH,
        card.y + BUTTONS_TOP,
        BUTTON_WIDTH,
        BUTTON_HEIGHT,
    );
    QuitPromptLayout {
        card,
        stop_asking: LogicalRect::new(
            card.x + PAD,
            card.y + CHECK_TOP,
            CARD_WIDTH - 2 * PAD,
            CHECK_HEIGHT,
        ),
        cancel: LogicalRect::new(
            quit.x - GAP - BUTTON_WIDTH,
            quit.y,
            BUTTON_WIDTH,
            BUTTON_HEIGHT,
        ),
        quit,
    }
}

/// The prompt as drawn.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct QuitPromptView {
    pub layout: QuitPromptLayout,
    /// "Do not show this again" is ticked.
    pub stop_asking: bool,
    /// The part under the pointer.
    pub hovered: Option<QuitControl>,
}

/// The open prompt.
#[derive(Debug, Clone, Copy, Default)]
pub(crate) struct QuitPrompt {
    stop_asking: bool,
    hovered: Option<QuitControl>,
}

impl OverlayState {
    /// The quit prompt, while it shows.
    pub fn quit_prompt(&self) -> Option<QuitPromptView> {
        let prompt = self.quit_prompt?;
        Some(QuitPromptView {
            layout: quit_prompt_layout(self.monitor_rect()),
            stop_asking: prompt.stop_asking,
            hovered: prompt.hovered,
        })
    }

    pub(crate) fn open_quit_prompt(&mut self, effects: &mut Effects) {
        let hovered = self.pointer.and_then(|at| self.quit_control_at(at));
        self.quit_prompt = Some(QuitPrompt {
            stop_asking: false,
            hovered,
        });
        if self.pointer.is_some() {
            self.prompt_cursor(effects);
        }
        effects.redraw(self.active);
    }

    /// An event while the prompt shows; false for those handled as usual:
    /// time, focus, and a release, so a drag held through Esc still ends.
    pub(crate) fn quit_prompt_event(&mut self, event: Event, effects: &mut Effects) -> bool {
        match event {
            Event::PointerMove { monitor, at, .. } if monitor == self.active => {
                self.pointer = Some(at);
                let hovered = self.quit_control_at(at);
                if let Some(prompt) = &mut self.quit_prompt
                    && prompt.hovered != hovered
                {
                    prompt.hovered = hovered;
                    effects.redraw(self.active);
                }
                self.prompt_cursor(effects);
            }
            Event::PointerDown {
                monitor,
                at,
                button: PointerButton::Left,
                ..
            } if monitor == self.active => match self.quit_control_at(at) {
                Some(QuitControl::StopAsking) => self.toggle_stop_asking(effects),
                Some(QuitControl::Cancel) => self.answer_quit(false, effects),
                Some(QuitControl::Quit) => self.answer_quit(true, effects),
                None => {}
            },
            Event::Key { key, .. } => match key {
                Key::Escape => self.answer_quit(false, effects),
                Key::Return | Key::Enter => self.answer_quit(true, effects),
                Key::Space => self.toggle_stop_asking(effects),
                _ => {}
            },
            Event::Tick | Event::Focus(_) | Event::PointerUp { .. } => return false,
            Event::PointerMove { .. }
            | Event::PointerDown { .. }
            | Event::Text(_)
            | Event::Wheel { .. } => {}
        }
        true
    }

    fn quit_control_at(&self, at: LogicalPoint) -> Option<QuitControl> {
        let layout = quit_prompt_layout(self.monitor_rect());
        [
            (layout.stop_asking, QuitControl::StopAsking),
            (layout.cancel, QuitControl::Cancel),
            (layout.quit, QuitControl::Quit),
        ]
        .into_iter()
        .find_map(|(rect, control)| rect.contains(at).then_some(control))
    }

    /// The arrow everywhere: nothing under the prompt can be selected.
    fn prompt_cursor(&mut self, effects: &mut Effects) {
        if self.cursor != CursorShape::Arrow {
            self.cursor = CursorShape::Arrow;
            effects.push(Effect::SetCursor(CursorShape::Arrow));
        }
    }

    fn toggle_stop_asking(&mut self, effects: &mut Effects) {
        if let Some(prompt) = &mut self.quit_prompt {
            prompt.stop_asking = !prompt.stop_asking;
            effects.redraw(self.active);
        }
    }

    fn answer_quit(&mut self, quit: bool, effects: &mut Effects) {
        let Some(prompt) = self.quit_prompt.take() else {
            return;
        };
        if prompt.stop_asking {
            self.config.quit_prompt = false;
            effects.push(Effect::Persist(SettingChange::ShowQuitPrompt(false)));
        }
        if quit {
            self.close(Outcome::Cancelled, effects);
        } else {
            effects.redraw(self.active);
            self.update_cursor(self.pointer, effects);
        }
    }
}
