//! Scripted event sequences for the 15.0.0 behaviours brought to the
//! overlay: the Screenshot/Video mode bar, Move up/Move down in the side
//! panel's object list, the text style toggles, editing a placed text again
//! on double-click, `copyOnDoubleClick` and the names under the buttons.

use phramer_annotate::Shape;
use phramer_capture::{LogicalPoint, LogicalRect, Monitor, MonitorId, PhysicalRect};
use phramer_overlay::{
    Effect, Event, ExportAction, Key, Modifiers, Outcome, OverlayConfig, OverlayState,
    PointerButton,
};

fn p(x: i32, y: i32) -> LogicalPoint {
    LogicalPoint::new(x, y)
}

fn center(rect: LogicalRect) -> LogicalPoint {
    p(rect.x + rect.width / 2, rect.y + rect.height / 2)
}

struct Overlay {
    state: OverlayState,
    now: u64,
}

impl Overlay {
    fn with(config: OverlayConfig) -> Self {
        let monitor = Monitor::new(
            MonitorId::new("primary"),
            PhysicalRect::new(0, 0, 1920, 1080),
            96,
            true,
        );
        let mut state = OverlayState::new(vec![monitor], 0, Vec::new(), config).expect("state");
        state.start();
        state.handle(Event::Focus(true), 0);
        Self { state, now: 1_000 }
    }

    /// Every event a second after the last, so no two clicks make a
    /// double-click unless [`Overlay::double_click`] says so.
    fn send(&mut self, event: Event) -> Vec<Effect> {
        self.now += 1_000;
        self.state.handle(event, self.now)
    }

    fn press(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.send(Event::PointerDown {
            monitor: 0,
            at,
            button: PointerButton::Left,
            modifiers: Modifiers::NONE,
        })
    }

    fn release(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.send(Event::PointerUp {
            monitor: 0,
            at,
            button: PointerButton::Left,
            modifiers: Modifiers::NONE,
        })
    }

    fn click(&mut self, at: LogicalPoint) -> Vec<Effect> {
        let mut all = self.press(at);
        all.extend(self.release(at));
        all
    }

    /// Two clicks 100 ms apart.
    fn double_click(&mut self, at: LogicalPoint) -> Vec<Effect> {
        let mut all = self.click(at);
        self.now -= 1_900;
        all.extend(self.click(at));
        all
    }

    fn drag(&mut self, from: LogicalPoint, to: LogicalPoint) {
        self.press(from);
        self.send(Event::PointerMove {
            monitor: 0,
            at: to,
            left_down: true,
            modifiers: Modifiers::NONE,
        });
        self.release(to);
    }

    fn key(&mut self, key: Key) -> Vec<Effect> {
        self.send(Event::Key {
            key,
            modifiers: Modifiers::NONE,
        })
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send(Event::Text(c));
        }
    }

    fn texts(&self) -> Vec<(String, bool, bool)> {
        self.state
            .document()
            .objects()
            .iter()
            .filter_map(|object| match &object.shape {
                Shape::Text { text, font, .. } => Some((text.clone(), font.bold, font.italic)),
                _ => None,
            })
            .collect()
    }
}

fn closed_with(effects: &[Effect]) -> Option<Outcome> {
    effects.iter().find_map(|effect| match effect {
        Effect::Close(outcome) => Some(*outcome),
        _ => None,
    })
}

fn video() -> OverlayConfig {
    OverlayConfig {
        video_mode: true,
        ..OverlayConfig::default()
    }
}

// --- The mode bar ---------------------------------------------------------------

#[test]
fn the_mode_bar_shows_only_with_video_capture_on_and_before_a_selection() {
    assert!(
        Overlay::with(OverlayConfig::default())
            .state
            .mode_bar()
            .is_none()
    );
    let overlay = Overlay::with(video());
    let bar = overlay.state.mode_bar().expect("mode bar");
    // Centred at the top
    assert_eq!(bar.layout.bar.y, 12);
    assert_eq!(bar.layout.bar.x * 2 + bar.layout.bar.width, 1920);
    assert_eq!(bar.screenshot_key.as_deref(), Some("1"));
    assert_eq!(bar.video_key.as_deref(), Some("2"));
    // A region given up front is already a screenshot
    let given = Overlay::with(OverlayConfig {
        initial_selection: Some(phramer_capture::LocalRect::new(10, 10, 200, 100)),
        ..video()
    });
    assert!(given.state.mode_bar().is_none());
}

#[test]
fn video_by_key_or_click_closes_for_snipping_tool_and_exports_nothing() {
    let mut overlay = Overlay::with(video());
    assert_eq!(
        closed_with(&overlay.key(Key::Char('2'))),
        Some(Outcome::RecordVideo)
    );

    let mut overlay = Overlay::with(video());
    let segment = overlay.state.mode_bar().expect("bar").layout.video;
    assert_eq!(
        closed_with(&overlay.press(center(segment))),
        Some(Outcome::RecordVideo)
    );
}

#[test]
fn screenshot_keeps_the_overlay_and_the_digits_go_back_to_tool_sizes() {
    let mut overlay = Overlay::with(video());
    let size = overlay.state.tool_size();
    let segment = overlay.state.mode_bar().expect("bar").layout.screenshot;
    assert!(closed_with(&overlay.click(center(segment))).is_none());
    // 1 is the Screenshot key while the bar shows, not a tool size
    assert!(closed_with(&overlay.key(Key::Char('1'))).is_none());
    assert_eq!(overlay.state.tool_size(), size);
    assert!(overlay.state.mode_bar().is_some());
    // The first press elsewhere starts the capture; the bar goes for good
    overlay.drag(p(300, 300), p(700, 600));
    assert!(overlay.state.selection().is_some());
    assert!(overlay.state.mode_bar().is_none());
    overlay.key(Key::Char('2'));
    assert!(!overlay.state.is_closed());
}

// --- Move up / Move down ------------------------------------------------------------

fn three_boxes() -> Overlay {
    let mut overlay = Overlay::with(OverlayConfig::default());
    overlay.drag(p(200, 200), p(1000, 700));
    overlay.key(Key::Char('r'));
    for x in [300, 500, 700] {
        overlay.drag(p(x, 300), p(x + 100, 400));
    }
    overlay.key(Key::Escape);
    overlay.key(Key::Space);
    overlay
}

fn ids(overlay: &Overlay) -> Vec<u64> {
    overlay
        .state
        .document()
        .objects()
        .iter()
        .map(|object| object.id.0)
        .collect()
}

#[test]
fn move_up_and_down_reorder_the_selected_object_in_one_undo_step() {
    let mut overlay = three_boxes();
    let before = ids(&overlay);
    let layout = overlay.state.side_panel().expect("panel").layout;
    // Row 0 is <Empty>; row 2 is the second box
    overlay.click(center(layout.rows[2]));
    let view = overlay.state.side_panel().expect("panel");
    assert!(view.can_move_up && view.can_move_down);
    overlay.click(center(layout.move_down));
    assert_eq!(ids(&overlay), [before[0], before[2], before[1]]);
    // Still selected, now last: nothing further down
    let view = overlay.state.side_panel().expect("panel");
    assert!(view.can_move_up && !view.can_move_down);
    overlay.click(center(layout.move_down));
    assert_eq!(ids(&overlay), [before[0], before[2], before[1]]);
    overlay.click(center(layout.move_up));
    overlay.click(center(layout.move_up));
    assert_eq!(ids(&overlay), [before[1], before[0], before[2]]);
    assert!(!overlay.state.side_panel().expect("panel").can_move_up);
    // Undo walks back one move at a time
    overlay.send(Event::Key {
        key: Key::Char('z'),
        modifiers: Modifiers::CTRL,
    });
    assert_eq!(ids(&overlay), [before[0], before[1], before[2]]);
}

#[test]
fn move_buttons_do_nothing_without_a_selected_object() {
    let mut overlay = three_boxes();
    let before = ids(&overlay);
    let layout = overlay.state.side_panel().expect("panel").layout;
    let view = overlay.state.side_panel().expect("panel");
    assert!(!view.can_move_up && !view.can_move_down);
    overlay.click(center(layout.move_up));
    overlay.click(center(layout.move_down));
    assert_eq!(ids(&overlay), before);
}

// --- Text: styles, editing again ------------------------------------------------------

fn with_text(text: &str) -> Overlay {
    let mut overlay = Overlay::with(OverlayConfig::default());
    overlay.drag(p(200, 200), p(1000, 700));
    overlay.key(Key::Char('t'));
    overlay.click(p(300, 300));
    overlay.type_text(text);
    overlay.key(Key::Escape);
    // Off with the tool
    overlay.key(Key::Escape);
    overlay
}

#[test]
fn style_toggles_set_new_text_and_restyle_a_selected_text() {
    let mut overlay = Overlay::with(OverlayConfig::default());
    overlay.drag(p(200, 200), p(1000, 700));
    overlay.key(Key::Space);
    // Nothing to style yet
    assert!(!overlay.state.side_panel().expect("panel").text_enabled);
    overlay.key(Key::Char('t'));
    let view = overlay.state.side_panel().expect("panel");
    assert!(view.text_enabled);
    overlay.click(center(view.layout.text_styles[0]));
    assert_eq!(
        overlay.state.side_panel().expect("panel").text_styles,
        [true, false, false, false]
    );
    overlay.click(p(500, 300));
    overlay.type_text("Bold");
    overlay.key(Key::Escape);
    assert_eq!(overlay.texts(), [("Bold".to_owned(), true, false)]);

    // Select it with the tool off, then italic: one undoable restyle
    overlay.key(Key::Escape);
    overlay.click(p(510, 300));
    assert!(overlay.state.selected_object().is_some());
    let italic = overlay
        .state
        .side_panel()
        .expect("panel")
        .layout
        .text_styles[1];
    overlay.click(center(italic));
    assert_eq!(overlay.texts(), [("Bold".to_owned(), true, true)]);
    overlay.send(Event::Key {
        key: Key::Char('z'),
        modifiers: Modifiers::CTRL,
    });
    assert_eq!(overlay.texts(), [("Bold".to_owned(), true, false)]);
}

#[test]
fn double_click_on_a_placed_text_types_into_it_again() {
    let mut overlay = with_text("Phramer");
    assert_eq!(overlay.texts().len(), 1);
    overlay.double_click(p(310, 300));
    assert_eq!(overlay.state.text_being_edited(), Some("Phramer"));
    // The placed text is drawn as the edit while it is typed into
    assert!(overlay.state.excluded_object().is_some());
    overlay.type_text(" 16");
    overlay.key(Key::Escape);
    assert_eq!(overlay.texts(), [("Phramer 16".to_owned(), false, false)]);
    assert_eq!(overlay.state.document().objects().len(), 1);
    // One step back to the text as it was
    overlay.send(Event::Key {
        key: Key::Char('z'),
        modifiers: Modifiers::CTRL,
    });
    assert_eq!(overlay.texts(), [("Phramer".to_owned(), false, false)]);
}

#[test]
fn a_text_emptied_while_edited_again_is_removed() {
    let mut overlay = with_text("ab");
    overlay.double_click(p(305, 300));
    overlay.key(Key::Backspace);
    overlay.key(Key::Backspace);
    overlay.key(Key::Escape);
    assert!(overlay.texts().is_empty());
    assert!(overlay.state.selected_object().is_none());
}

#[test]
fn double_click_copies_the_selection_only_with_copy_on_double_click() {
    let mut overlay = Overlay::with(OverlayConfig::default());
    overlay.drag(p(200, 200), p(1000, 700));
    assert!(closed_with(&overlay.double_click(p(600, 450))).is_none());

    let mut overlay = Overlay::with(OverlayConfig {
        copy_on_double_click: true,
        ..OverlayConfig::default()
    });
    overlay.drag(p(200, 200), p(1000, 700));
    // Two clicks a second apart are no double-click
    assert!(closed_with(&overlay.click(p(600, 450))).is_none());
    assert!(closed_with(&overlay.click(p(600, 450))).is_none());
    match closed_with(&overlay.double_click(p(600, 450))) {
        Some(Outcome::Export(export)) => assert_eq!(export.action, ExportAction::Copy),
        other => panic!("not a copy: {other:?}"),
    }
}

// --- Names under the buttons ------------------------------------------------------------

#[test]
fn tool_names_sit_under_their_buttons_and_space_the_toolbar_out() {
    let labelled = {
        let mut overlay = Overlay::with(OverlayConfig::default());
        overlay.drag(p(200, 200), p(1000, 700));
        overlay
    };
    let plain = {
        let mut overlay = Overlay::with(OverlayConfig {
            tool_labels: false,
            ..OverlayConfig::default()
        });
        overlay.drag(p(200, 200), p(1000, 700));
        overlay
    };
    assert!(plain.state.toolbar_labels().is_empty());
    let labels = labelled.state.toolbar_labels();
    let buttons = labelled.state.toolbar();
    assert_eq!(labels.len(), buttons.len());
    assert!(labels.iter().any(|(_, text)| text == "Pencil (P)"));
    assert!(labels.iter().any(|(_, text)| text == "Undo"));
    for ((label, _), button) in labels.iter().zip(&buttons) {
        // Centred under its button
        assert_eq!(label.y, button.rect.bottom() + 2);
        assert!((label.x + label.width / 2 - (button.rect.x + 16)).abs() <= 1);
    }
    for (index, (a, _)) in labels.iter().enumerate() {
        for (b, _) in &labels[index + 1..] {
            assert!(a.intersection(b).is_none(), "{a:?} {b:?}");
        }
        for button in &buttons {
            assert!(
                a.intersection(&button.rect).is_none(),
                "{a:?} on {button:?}"
            );
        }
    }
    // The cells grew: neighbours are further apart than without names
    let pitch = |buttons: &[phramer_overlay::ToolbarButton]| buttons[1].rect.x - buttons[0].rect.x;
    assert!(pitch(&buttons).abs() > pitch(&plain.state.toolbar()).abs());
}
