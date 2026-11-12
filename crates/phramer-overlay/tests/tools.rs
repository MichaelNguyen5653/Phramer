//! Scripted event sequences through the annotation tools on the overlay:
//! drawing, undo and redo, tool sizes, the colour wheel, text entry, the
//! toolbar's layout and buttons, and the settings written back.

use phramer_annotate::{Color, Shape};
use phramer_capture::{LogicalPoint, LogicalRect, Monitor, MonitorId, PhysicalRect};
use phramer_core::config::{ButtonType, Settings};
use phramer_overlay::{
    DeferredAction, Effect, Event, ExportAction, Key, Modifiers, Outcome, OverlayConfig,
    OverlayState, PointerButton, SettingChange, ShapeKind, TOOLBAR_ORDER, button_info, has_icon,
};

fn monitor(width: i32, height: i32, dpi: u32) -> Monitor {
    Monitor::new(
        MonitorId::new("only"),
        PhysicalRect::new(0, 0, width, height),
        dpi,
        true,
    )
}

fn p(x: i32, y: i32) -> LogicalPoint {
    LogicalPoint::new(x, y)
}

struct Overlay {
    state: OverlayState,
    now: u64,
    effects: Vec<Effect>,
}

impl Overlay {
    fn with(config: OverlayConfig, dpi: u32) -> Self {
        let mut state = OverlayState::new(vec![monitor(1920, 1080, dpi)], 0, Vec::new(), config)
            .expect("state");
        let effects = state.start();
        Self {
            state,
            now: 0,
            effects,
        }
    }

    fn new() -> Self {
        Self::with(OverlayConfig::default(), 96)
    }

    /// An overlay with a selection of (200,200)-(1000,700).
    fn selected() -> Self {
        let mut overlay = Self::new();
        overlay.drag_from(p(200, 200), p(1000, 700), Modifiers::NONE);
        overlay
    }

    fn send(&mut self, event: Event) -> Vec<Effect> {
        self.effects = self.state.handle(event, self.now);
        self.effects.clone()
    }

    fn press(&mut self, at: LogicalPoint, button: PointerButton) -> Vec<Effect> {
        self.send(Event::PointerDown {
            monitor: 0,
            at,
            button,
            modifiers: Modifiers::NONE,
        })
    }

    fn release(&mut self, at: LogicalPoint, button: PointerButton) -> Vec<Effect> {
        self.send(Event::PointerUp {
            monitor: 0,
            at,
            button,
            modifiers: Modifiers::NONE,
        })
    }

    fn hover(&mut self, at: LogicalPoint) -> Vec<Effect> {
        self.send(Event::PointerMove {
            monitor: 0,
            at,
            left_down: false,
            modifiers: Modifiers::NONE,
        })
    }

    fn drag_from(&mut self, from: LogicalPoint, to: LogicalPoint, modifiers: Modifiers) {
        let mut all = self.press(from, PointerButton::Left);
        let mid = p((from.x + to.x) / 2, (from.y + to.y) / 2);
        for at in [mid, to] {
            all.extend(self.send(Event::PointerMove {
                monitor: 0,
                at,
                left_down: true,
                modifiers,
            }));
        }
        all.extend(self.release(to, PointerButton::Left));
        self.effects = all;
    }

    fn click(&mut self, at: LogicalPoint) -> Vec<Effect> {
        let mut all = self.press(at, PointerButton::Left);
        all.extend(self.release(at, PointerButton::Left));
        self.effects = all.clone();
        all
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) -> Vec<Effect> {
        self.send(Event::Key { key, modifiers })
    }

    fn char(&mut self, c: char) -> Vec<Effect> {
        self.key(Key::Char(c), Modifiers::NONE)
    }

    fn type_text(&mut self, text: &str) {
        for c in text.chars() {
            self.send(Event::Text(c));
        }
    }

    fn wheel(&mut self, delta: i32, modifiers: Modifiers) -> Vec<Effect> {
        self.send(Event::Wheel {
            monitor: 0,
            at: p(600, 400),
            delta,
            modifiers,
        })
    }

    fn button_rect(&self, button: ButtonType) -> LogicalRect {
        self.state
            .toolbar()
            .iter()
            .find(|entry| entry.button == button)
            .map(|entry| entry.rect)
            .unwrap_or_else(|| panic!("{button:?} is not on the toolbar"))
    }

    fn click_button(&mut self, button: ButtonType) -> Vec<Effect> {
        let rect = self.button_rect(button);
        self.click(p(rect.x + rect.width / 2, rect.y + rect.height / 2))
    }

    fn objects(&self) -> usize {
        self.state.document().objects().len()
    }

    fn closed_with(&self) -> Option<Outcome> {
        self.effects.iter().find_map(|effect| match effect {
            Effect::Close(outcome) => Some(*outcome),
            _ => None,
        })
    }
}

fn persisted(effects: &[Effect]) -> Vec<SettingChange> {
    effects
        .iter()
        .filter_map(|effect| match effect {
            Effect::Persist(change) => Some(*change),
            _ => None,
        })
        .collect()
}

// --- Toolbar ---------------------------------------------------------------

#[test]
fn the_toolbar_shows_only_with_a_settled_selection() {
    let mut overlay = Overlay::new();
    assert!(overlay.state.toolbar().is_empty());
    overlay.press(p(200, 200), PointerButton::Left);
    overlay.send(Event::PointerMove {
        monitor: 0,
        at: p(600, 500),
        left_down: true,
        modifiers: Modifiers::NONE,
    });
    // Hidden while the selection is dragged out
    assert!(overlay.state.toolbar().is_empty());
    overlay.release(p(600, 500), PointerButton::Left);
    assert!(!overlay.state.toolbar().is_empty());
}

#[test]
fn the_default_toolbar_is_15_0_0s_in_its_order() {
    let overlay = Overlay::selected();
    let buttons: Vec<ButtonType> = overlay.state.toolbar().iter().map(|b| b.button).collect();
    use ButtonType::*;
    assert_eq!(
        buttons,
        [
            Pencil,
            Line,
            Arrow,
            Shape,
            Marker,
            Text,
            CircleCount,
            Pixelate,
            Invert,
            MoveObject,
            MoveSelection,
            Undo,
            Redo,
            Copy,
            CopyFile,
            Save,
            Exit,
            OpenApp,
            Ocr,
            Pin,
            OpenInEditor
        ]
    );
}

#[test]
fn the_button_list_setting_picks_buttons_and_tasks_swap_exports_for_accept() {
    let mut settings = Settings::default();
    settings.set_buttons(&[
        ButtonType::Copy,
        ButtonType::Accept,
        ButtonType::Undo,
        ButtonType::Pencil,
    ]);
    let config = OverlayConfig::from_settings(&settings);
    let mut plain = Overlay::with(config.clone(), 96);
    plain.drag_from(p(200, 200), p(1000, 700), Modifiers::NONE);
    let buttons: Vec<ButtonType> = plain.state.toolbar().iter().map(|b| b.button).collect();
    // Toolbar order, not the stored one; Accept only with tasks
    assert_eq!(
        buttons,
        [ButtonType::Pencil, ButtonType::Undo, ButtonType::Copy]
    );

    let mut tasks = Overlay::with(
        OverlayConfig {
            has_tasks: true,
            ..config
        },
        96,
    );
    tasks.drag_from(p(200, 200), p(1000, 700), Modifiers::NONE);
    let buttons: Vec<ButtonType> = tasks.state.toolbar().iter().map(|b| b.button).collect();
    assert_eq!(
        buttons,
        [ButtonType::Pencil, ButtonType::Undo, ButtonType::Accept]
    );
}

#[test]
fn buttons_sit_outside_the_selection_and_flip_away_from_screen_edges() {
    let bottom_edge = LogicalRect::new(400, 900, 800, 180);
    let mut overlay = Overlay::new();
    overlay.drag_from(
        p(bottom_edge.x, bottom_edge.y),
        p(bottom_edge.right(), 1079),
        Modifiers::NONE,
    );
    let selection = overlay.state.selection().expect("selection");
    let toolbar = overlay.state.toolbar();
    assert_eq!(toolbar.len(), 21);
    for button in &toolbar {
        // On the monitor, never over the selection
        assert!(
            button.rect.x >= 0 && button.rect.right() <= 1920,
            "{button:?}"
        );
        assert!(
            button.rect.y >= 0 && button.rect.bottom() <= 1080,
            "{button:?}"
        );
        assert!(selection.intersection(&button.rect).is_none(), "{button:?}");
    }
    // Nothing fits below: the first buttons go to the right, then the top
    assert!(toolbar[0].rect.x >= selection.right());
    assert!(toolbar.iter().any(|b| b.rect.bottom() <= selection.y));

    // A selection in the open has its first row centred under it
    let overlay = Overlay::selected();
    let first = overlay.state.toolbar()[0].rect;
    assert!(first.y > 700 && first.y < 720, "{first:?}");
}

#[test]
fn a_whole_monitor_selection_puts_the_buttons_inside() {
    let mut overlay = Overlay::new();
    overlay.key(Key::Char('a'), Modifiers::CTRL);
    let toolbar = overlay.state.toolbar();
    assert_eq!(toolbar.len(), 21);
    assert!(
        toolbar
            .iter()
            .all(|b| b.rect.bottom() <= 1080 && b.rect.y > 900)
    );
}

#[test]
fn hovering_a_button_highlights_it_with_a_tooltip_and_its_shortcut() {
    let mut overlay = Overlay::selected();
    let pencil = overlay.button_rect(ButtonType::Pencil);
    overlay.now = 1000;
    let effects = overlay.hover(p(pencil.x + 5, pencil.y + 5));
    assert!(effects.contains(&Effect::Redraw { monitor: 0 }));
    // The fade asks for frames until it is done
    assert!(
        effects
            .iter()
            .any(|e| matches!(e, Effect::WakeAt(at) if *at <= 1016))
    );
    assert!(overlay.state.toolbar()[0].hovered);
    assert_eq!(overlay.state.hover_progress(), 0.0);
    overlay.now = 1100;
    overlay.send(Event::Tick);
    assert_eq!(overlay.state.hover_progress(), 1.0);
    let (anchor, text) = overlay.state.tooltip().expect("tooltip");
    assert_eq!(anchor, pencil);
    assert_eq!(text, "Set the Pencil as the paint tool (P)");
    // Return reads as Enter
    let mut settings = Settings::default();
    settings.set_buttons(&[ButtonType::Accept]);
    let mut tasks = Overlay::with(
        OverlayConfig {
            has_tasks: true,
            ..OverlayConfig::from_settings(&settings)
        },
        96,
    );
    tasks.drag_from(p(200, 200), p(1000, 700), Modifiers::NONE);
    let accept = tasks.button_rect(ButtonType::Accept);
    tasks.hover(p(accept.x + 5, accept.y + 5));
    assert_eq!(
        tasks.state.tooltip().map(|(_, text)| text).as_deref(),
        Some("Accept the capture (Enter)")
    );
}

#[test]
fn reduced_motion_makes_the_hover_instant() {
    let mut overlay = Overlay::with(
        OverlayConfig {
            reduced_motion: true,
            ..OverlayConfig::default()
        },
        96,
    );
    overlay.drag_from(p(200, 200), p(1000, 700), Modifiers::NONE);
    let pencil = overlay.button_rect(ButtonType::Pencil);
    let effects = overlay.hover(p(pencil.x + 5, pencil.y + 5));
    assert!(!effects.iter().any(|e| matches!(e, Effect::WakeAt(_))));
    assert_eq!(overlay.state.hover_progress(), 1.0);
}

#[test]
fn clicking_a_tool_selects_it_and_clicking_again_deselects_it() {
    let mut overlay = Overlay::selected();
    overlay.click_button(ButtonType::Arrow);
    assert_eq!(overlay.state.active_tool(), Some(ButtonType::Arrow));
    assert!(
        overlay
            .state
            .toolbar()
            .iter()
            .any(|b| b.active && b.button == ButtonType::Arrow)
    );
    // The press alone shows the button pressed and does nothing yet
    let marker = overlay.button_rect(ButtonType::Marker);
    overlay.press(p(marker.x + 3, marker.y + 3), PointerButton::Left);
    assert!(overlay.state.toolbar().iter().any(|b| b.pressed));
    assert_eq!(overlay.state.active_tool(), Some(ButtonType::Arrow));
    // Released elsewhere: nothing
    overlay.release(p(marker.x - 50, marker.y - 50), PointerButton::Left);
    assert_eq!(overlay.state.active_tool(), Some(ButtonType::Arrow));
    overlay.click_button(ButtonType::Arrow);
    assert_eq!(overlay.state.active_tool(), None);
    // A click on the toolbar never clears the selection
    assert!(overlay.state.selection().is_some());
}

#[test]
fn every_button_has_a_lucide_icon_and_a_name() {
    for button in TOOLBAR_ORDER {
        let info = button_info(button);
        assert!(has_icon(info.icon), "{button:?}: {}", info.icon);
        assert!(!info.description.is_empty());
    }
}

// --- Drawing ---------------------------------------------------------------

#[test]
fn a_drag_with_a_tool_places_its_object_in_physical_pixels() {
    // 150%: logical (300,300)-(500,400) is physical (450,450)-(750,600)
    let mut overlay = Overlay::with(OverlayConfig::default(), 144);
    overlay.drag_from(p(100, 100), p(1000, 600), Modifiers::NONE);
    overlay.char('d');
    assert_eq!(overlay.state.active_tool(), Some(ButtonType::Line));
    overlay.drag_from(p(300, 300), p(500, 400), Modifiers::NONE);
    let objects = overlay.state.document().objects();
    assert_eq!(objects.len(), 1);
    assert_eq!(
        objects[0].shape,
        Shape::Line {
            start: phramer_annotate::Point::new(450, 450),
            end: phramer_annotate::Point::new(750, 600)
        }
    );
    // Size 3 at 150%, in the default red
    assert_eq!(objects[0].size, 5);
    assert_eq!(objects[0].color, Color::rgb(255, 0, 0));
    // Drawing inside does not move or clear the selection
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 100, 900, 500))
    );
}

#[test]
fn every_drawing_tool_draws_its_own_shape() {
    let cases = [
        ('p', "pencil"),
        ('d', "line"),
        ('a', "arrow"),
        ('s', "rectangle"),
        ('r', "rectangle"),
        ('c', "ellipse"),
        ('m', "marker"),
        ('b', "pixelate"),
        ('i', "invert"),
    ];
    for (key, kind) in cases {
        let mut overlay = Overlay::selected();
        overlay.char(key);
        overlay.drag_from(p(300, 300), p(450, 380), Modifiers::NONE);
        let objects = overlay.state.document().objects();
        assert_eq!(objects.len(), 1, "{key}");
        let json = serde_json_shape(&objects[0].shape);
        assert!(json.contains(kind), "{key}: {json}");
    }
    // Shape, from its button: a filled square by default
    let mut overlay = Overlay::selected();
    overlay.click_button(ButtonType::Shape);
    overlay.drag_from(p(300, 300), p(450, 380), Modifiers::NONE);
    assert!(matches!(
        overlay.state.document().objects()[0].shape,
        Shape::Rectangle {
            style: phramer_annotate::ShapeStyle::Filled,
            ..
        }
    ));
    // The counter places a numbered bubble with a click
    overlay.click_button(ButtonType::CircleCount);
    overlay.click(p(600, 500));
    overlay.click(p(700, 500));
    let counts: Vec<u32> = overlay
        .state
        .document()
        .objects()
        .iter()
        .filter_map(|o| match o.shape {
            Shape::CircleCounter { count, .. } => Some(count),
            _ => None,
        })
        .collect();
    assert_eq!(counts, [1, 2]);
}

fn serde_json_shape(shape: &Shape) -> String {
    format!("{shape:?}").to_lowercase()
}

#[test]
fn a_click_that_draws_nothing_places_nothing() {
    let mut overlay = Overlay::selected();
    overlay.char('r');
    overlay.click(p(400, 400));
    assert_eq!(overlay.objects(), 0);
    assert!(!overlay.state.can_undo());
}

#[test]
fn shift_keeps_lines_at_45_degrees() {
    let mut overlay = Overlay::selected();
    overlay.char('d');
    overlay.drag_from(p(300, 300), p(500, 310), Modifiers::SHIFT);
    let Shape::Line { start, end } = overlay.state.document().objects()[0].shape.clone() else {
        panic!("not a line");
    };
    assert_eq!(start.y, end.y);
}

#[test]
fn the_shape_menu_picks_kind_and_style_and_saves_them() {
    let mut overlay = Overlay::selected();
    overlay.click_button(ButtonType::Shape);
    assert!(overlay.state.shape_menu().is_empty());
    // Clicking the active Shape button opens its menu
    overlay.click_button(ButtonType::Shape);
    let menu = overlay.state.shape_menu();
    assert_eq!(menu.len(), 6);
    assert!(menu.iter().any(|o| o.active && o.kind == ShapeKind::Square));
    let hollow_circle = menu[3];
    let effects = overlay.click(p(hollow_circle.rect.x + 4, hollow_circle.rect.y + 4));
    assert_eq!(
        persisted(&effects),
        [SettingChange::Shape {
            kind: ShapeKind::Circle,
            style: phramer_annotate::ShapeStyle::Outline
        }]
    );
    assert!(overlay.state.shape_menu().is_empty());
    overlay.drag_from(p(300, 300), p(450, 380), Modifiers::NONE);
    assert!(matches!(
        overlay.state.document().objects()[0].shape,
        Shape::Ellipse {
            style: phramer_annotate::ShapeStyle::Outline,
            ..
        }
    ));
}

// --- Undo and redo ---------------------------------------------------------

#[test]
fn ctrl_z_undoes_and_ctrl_shift_z_or_ctrl_y_redo() {
    let mut overlay = Overlay::selected();
    overlay.char('r');
    overlay.drag_from(p(300, 300), p(400, 400), Modifiers::NONE);
    overlay.drag_from(p(500, 300), p(600, 400), Modifiers::NONE);
    assert_eq!(overlay.objects(), 2);
    overlay.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(overlay.objects(), 1);
    overlay.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(overlay.objects(), 0);
    assert!(!overlay.state.can_undo());
    overlay.key(Key::Char('z'), Modifiers::CTRL_SHIFT);
    assert_eq!(overlay.objects(), 1);
    overlay.key(Key::Char('y'), Modifiers::CTRL);
    assert_eq!(overlay.objects(), 2);
    assert!(!overlay.state.can_redo());
    // The buttons do the same, and show whether they can
    overlay.click_button(ButtonType::Undo);
    assert_eq!(overlay.objects(), 1);
    let redo = overlay
        .state
        .toolbar()
        .into_iter()
        .find(|b| b.button == ButtonType::Redo)
        .expect("redo");
    assert!(redo.enabled);
    overlay.click_button(ButtonType::Redo);
    assert_eq!(overlay.objects(), 2);
}

// --- Selecting objects -----------------------------------------------------

#[test]
fn without_a_tool_an_object_is_picked_moved_and_deleted() {
    let mut overlay = Overlay::selected();
    overlay.char('r');
    overlay.drag_from(p(300, 300), p(400, 400), Modifiers::NONE);
    overlay.char('r');
    assert_eq!(overlay.state.active_tool(), None);
    // A press on the object picks it; dragging moves it, not the selection
    let before = overlay.state.selection();
    overlay.drag_from(p(350, 350), p(450, 380), Modifiers::NONE);
    assert_eq!(overlay.state.selection(), before);
    let id = overlay.state.selected_object().expect("picked");
    let Shape::Rectangle { start, .. } = overlay.state.document().objects()[0].shape.clone() else {
        panic!("not a rectangle");
    };
    assert_eq!((start.x, start.y), (400, 330));
    // Undo puts it back
    overlay.key(Key::Char('z'), Modifiers::CTRL);
    let Shape::Rectangle { start, .. } = overlay.state.document().objects()[0].shape.clone() else {
        panic!("not a rectangle");
    };
    assert_eq!((start.x, start.y), (300, 300));
    assert_eq!(overlay.state.selected_object(), Some(id));
    overlay.key(Key::Delete, Modifiers::NONE);
    assert_eq!(overlay.objects(), 0);
    assert_eq!(overlay.state.selected_object(), None);
}

// --- Sizes -----------------------------------------------------------------

#[test]
fn the_wheel_steps_the_size_and_saves_it_when_the_notice_goes() {
    let mut overlay = Overlay::selected();
    overlay.char('p');
    assert_eq!(overlay.state.tool_size(), 3);
    overlay.now = 100;
    let effects = overlay.wheel(120, Modifiers::NONE);
    assert_eq!(overlay.state.tool_size(), 4);
    assert_eq!(overlay.state.size_notice(), Some(4));
    assert!(effects.contains(&Effect::WakeAt(700)));
    overlay.wheel(-120, Modifiers::NONE);
    overlay.wheel(-120, Modifiers::NONE);
    assert_eq!(overlay.state.tool_size(), 2);
    overlay.now = 700;
    let effects = overlay.send(Event::Tick);
    assert_eq!(
        persisted(&effects),
        [SettingChange::ToolSize {
            button: ButtonType::Pencil,
            size: 2
        }]
    );
    assert_eq!(overlay.state.size_notice(), None);
    // Never below 1 or above 50
    for _ in 0..5 {
        overlay.wheel(-120, Modifiers::NONE);
    }
    assert_eq!(overlay.state.tool_size(), 1);
    for _ in 0..60 {
        overlay.wheel(120, Modifiers::NONE);
    }
    assert_eq!(overlay.state.tool_size(), 50);
}

#[test]
fn a_touchpad_steps_at_most_every_200_ms() {
    let mut overlay = Overlay::selected();
    overlay.char('p');
    overlay.now = 1000;
    overlay.wheel(15, Modifiers::NONE);
    overlay.now = 1100;
    overlay.wheel(15, Modifiers::NONE);
    assert_eq!(overlay.state.tool_size(), 4);
    overlay.now = 1200;
    overlay.wheel(15, Modifiers::NONE);
    assert_eq!(overlay.state.tool_size(), 5);
}

#[test]
fn digits_type_a_size_and_an_out_of_range_one_starts_over() {
    let mut overlay = Overlay::selected();
    overlay.char('m');
    overlay.char('1');
    assert_eq!(overlay.state.tool_size(), 1);
    overlay.char('2');
    assert_eq!(overlay.state.tool_size(), 12);
    // 129 is clamped to 50, and the next digit starts a new number
    overlay.char('9');
    assert_eq!(overlay.state.tool_size(), 50);
    overlay.char('7');
    assert_eq!(overlay.state.tool_size(), 7);
    // Any other key starts over too
    overlay.key(Key::Right, Modifiers::NONE);
    overlay.char('3');
    assert_eq!(overlay.state.tool_size(), 3);
}

#[test]
fn a_size_change_resizes_the_selected_object_with_undo() {
    let mut overlay = Overlay::selected();
    overlay.char('d');
    overlay.drag_from(p(300, 300), p(500, 300), Modifiers::NONE);
    overlay.char('d');
    overlay.click(p(400, 300));
    assert!(overlay.state.selected_object().is_some());
    overlay.wheel(120, Modifiers::NONE);
    assert_eq!(overlay.state.document().objects()[0].size, 4);
    overlay.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(overlay.state.document().objects()[0].size, 3);
}

#[test]
fn ctrl_wheel_with_the_counter_picks_the_next_number() {
    let mut overlay = Overlay::selected();
    overlay.click_button(ButtonType::CircleCount);
    overlay.wheel(120, Modifiers::CTRL);
    overlay.wheel(120, Modifiers::CTRL);
    assert_eq!(overlay.state.tool_size(), 1);
    overlay.click(p(500, 500));
    assert!(matches!(
        overlay.state.document().objects()[0].shape,
        Shape::CircleCounter { count: 3, .. }
    ));
}

#[test]
fn the_size_buttons_step_by_one() {
    let mut settings = Settings::default();
    settings.set_buttons(&[ButtonType::SizeIncrease, ButtonType::SizeDecrease]);
    let mut overlay = Overlay::with(OverlayConfig::from_settings(&settings), 96);
    overlay.drag_from(p(200, 200), p(1000, 700), Modifiers::NONE);
    overlay.char('a');
    overlay.click_button(ButtonType::SizeIncrease);
    overlay.click_button(ButtonType::SizeIncrease);
    overlay.click_button(ButtonType::SizeDecrease);
    assert_eq!(overlay.state.tool_size(), 4);
}

#[test]
fn each_tool_starts_at_its_saved_size() {
    let mut settings = Settings::default();
    settings.set_draw_marker_size(9).expect("valid");
    settings.set_draw_font_size(20).expect("valid");
    let mut overlay = Overlay::with(OverlayConfig::from_settings(&settings), 96);
    overlay.drag_from(p(200, 200), p(1000, 700), Modifiers::NONE);
    overlay.char('m');
    assert_eq!(overlay.state.tool_size(), 9);
    overlay.char('t');
    assert_eq!(overlay.state.tool_size(), 20);
}

#[test]
fn setting_changes_write_15_0_0s_keys() {
    let mut settings = Settings::default();
    for change in [
        SettingChange::DrawColor(Color::rgb(0, 128, 255)),
        SettingChange::ToolSize {
            button: ButtonType::Text,
            size: 14,
        },
        SettingChange::ToolSize {
            button: ButtonType::Shape,
            size: 6,
        },
        SettingChange::ToolSize {
            button: ButtonType::MoveObject,
            size: 7,
        },
        SettingChange::Shape {
            kind: ShapeKind::Circle,
            style: phramer_annotate::ShapeStyle::Highlight,
        },
    ] {
        change.apply(&mut settings).expect("valid");
    }
    let text = String::from_utf8(settings.to_bytes()).expect("utf-8");
    assert_eq!(
        text,
        "[General]\r\ndrawColor=#0080ff\r\ndrawFontSize=14\r\ndrawRectangleSize=6\r\n\
         drawThickness=7\r\nshapeKind=1\r\nshapeStyle=2\r\n"
    );
}

// --- Colour wheel ----------------------------------------------------------

#[test]
fn the_colour_wheel_applies_on_hover_and_saves_on_release() {
    let mut overlay = Overlay::selected();
    overlay.press(p(600, 450), PointerButton::Right);
    let wheel = overlay.state.wheel().expect("open");
    assert_eq!(wheel.center, p(600, 450));
    // The default palette: picker, white, black, dark red, red...
    assert_eq!(wheel.swatches.len(), 12);
    assert_eq!(wheel.swatches[0].1, None);
    // Red is current
    assert_eq!(wheel.highlighted, Some(4));
    let (black, _) = wheel.swatches[2];
    overlay.hover(black);
    // Live: before any release
    assert_eq!(overlay.state.draw_color(), Color::rgb(0, 0, 0));
    let effects = overlay.release(black, PointerButton::Right);
    assert_eq!(
        persisted(&effects),
        [SettingChange::DrawColor(Color::rgb(0, 0, 0))]
    );
    assert!(overlay.state.wheel().is_none());
    // The release over the wheel is no click: the selection stays
    assert!(overlay.state.selection().is_some());
}

#[test]
fn esc_on_the_wheel_puts_the_colour_back() {
    let mut overlay = Overlay::selected();
    overlay.press(p(600, 450), PointerButton::Right);
    let (yellow, _) = overlay.state.wheel().expect("open").swatches[5];
    overlay.hover(yellow);
    assert_eq!(overlay.state.draw_color(), Color::rgb(255, 255, 0));
    let effects = overlay.key(Key::Escape, Modifiers::NONE);
    assert_eq!(overlay.state.draw_color(), Color::rgb(255, 0, 0));
    assert!(overlay.state.wheel().is_none());
    assert_eq!(overlay.closed_with(), None);
    assert!(persisted(&effects).is_empty());
}

#[test]
fn the_wheel_recolours_the_selected_object_live_and_with_undo() {
    let mut overlay = Overlay::selected();
    overlay.char('r');
    overlay.drag_from(p(300, 300), p(400, 400), Modifiers::NONE);
    overlay.char('r');
    overlay.click(p(350, 350));
    let id = overlay.state.selected_object().expect("selected");
    overlay.press(p(700, 450), PointerButton::Right);
    let (green, _) = overlay.state.wheel().expect("open").swatches[6];
    overlay.hover(green);
    // Previewed, not yet in the document
    assert_eq!(overlay.state.excluded_object(), Some(id));
    assert_eq!(overlay.state.previews()[0].color, Color::rgb(0, 255, 0));
    assert_eq!(
        overlay.state.document().objects()[0].color,
        Color::rgb(255, 0, 0)
    );
    overlay.release(green, PointerButton::Right);
    assert_eq!(
        overlay.state.document().objects()[0].color,
        Color::rgb(0, 255, 0)
    );
    overlay.key(Key::Char('z'), Modifiers::CTRL);
    assert_eq!(
        overlay.state.document().objects()[0].color,
        Color::rgb(255, 0, 0)
    );
}

#[test]
fn the_custom_slot_opens_a_disc_of_colours() {
    let mut overlay = Overlay::selected();
    overlay.press(p(600, 450), PointerButton::Right);
    let (picker, _) = overlay.state.wheel().expect("open").swatches[0];
    overlay.hover(picker);
    overlay.release(picker, PointerButton::Right);
    let wheel = overlay.state.wheel().expect("still open");
    let disc = wheel.custom_radius.expect("disc");
    // Straight right of the centre, at the rim: pure red; at the centre:
    // white
    overlay.hover(p(600 + disc - 1, 450));
    assert_eq!(overlay.state.draw_color().r, 255);
    assert!(overlay.state.draw_color().g < 20);
    overlay.hover(p(600, 450));
    assert_eq!(overlay.state.draw_color(), Color::rgb(255, 255, 255));
    let effects = overlay.click(p(600, 450));
    assert_eq!(
        persisted(&effects),
        [SettingChange::DrawColor(Color::rgb(255, 255, 255))]
    );
    assert!(overlay.state.wheel().is_none());
}

#[test]
fn a_right_click_without_a_pick_closes_the_wheel_and_counts_as_a_click() {
    let mut overlay = Overlay::selected();
    overlay.press(p(1500, 900), PointerButton::Right);
    overlay.release(p(1500, 900), PointerButton::Right);
    assert!(overlay.state.wheel().is_none());
    // Outside the selection: it clears, as before the tools
    assert_eq!(overlay.state.selection(), None);
}

// --- Text ------------------------------------------------------------------

#[test]
fn text_is_typed_edited_and_placed_with_esc() {
    let mut overlay = Overlay::selected();
    overlay.char('t');
    overlay.click(p(400, 400));
    assert_eq!(overlay.state.text_being_edited(), Some(""));
    let (caret_at_start, height) = overlay.state.caret().expect("caret");
    assert!(height > 10);
    overlay.type_text("Hellp");
    overlay.key(Key::Backspace, Modifiers::NONE);
    overlay.type_text("o");
    // Shortcut letters are text now
    assert_eq!(overlay.state.active_tool(), Some(ButtonType::Text));
    overlay.key(Key::Return, Modifiers::NONE);
    overlay.type_text("wrld");
    overlay.key(Key::Left, Modifiers::NONE);
    overlay.key(Key::Left, Modifiers::NONE);
    overlay.key(Key::Left, Modifiers::NONE);
    overlay.type_text("o");
    assert_eq!(overlay.state.text_being_edited(), Some("Hello\nworld"));
    let (caret, _) = overlay.state.caret().expect("caret");
    // On the second line, after "wo"
    assert!(caret.y > caret_at_start.y);
    assert!(caret.x > caret_at_start.x);
    assert_eq!(overlay.objects(), 0);
    overlay.key(Key::Escape, Modifiers::NONE);
    assert_eq!(overlay.state.text_being_edited(), None);
    assert_eq!(overlay.closed_with(), None);
    let Shape::Text { text, .. } = &overlay.state.document().objects()[0].shape else {
        panic!("not text");
    };
    assert_eq!(text, "Hello\nworld");
}

#[test]
fn ctrl_enter_or_a_click_elsewhere_places_text_and_empty_text_is_dropped() {
    let mut overlay = Overlay::selected();
    overlay.char('t');
    overlay.click(p(400, 400));
    overlay.type_text("one");
    overlay.key(Key::Return, Modifiers::CTRL);
    assert_eq!(overlay.objects(), 1);
    overlay.click(p(400, 500));
    overlay.type_text("two");
    // The click that places it opens no new box
    overlay.click(p(700, 600));
    assert_eq!(overlay.objects(), 2);
    assert_eq!(overlay.state.text_being_edited(), None);
    overlay.click(p(700, 600));
    overlay.key(Key::Escape, Modifiers::NONE);
    assert_eq!(overlay.objects(), 2);
}

#[test]
fn exporting_places_the_text_being_typed() {
    let mut overlay = Overlay::selected();
    overlay.char('t');
    overlay.click(p(400, 400));
    overlay.type_text("note");
    overlay.click_button(ButtonType::Save);
    assert!(matches!(
        overlay.closed_with(),
        Some(Outcome::Export(export)) if export.action == ExportAction::Save
    ));
    assert_eq!(overlay.objects(), 1);
}

// --- Esc and actions -------------------------------------------------------

#[test]
fn esc_drops_the_tool_first_and_closes_after() {
    let mut overlay = Overlay::selected();
    overlay.char('p');
    overlay.key(Key::Escape, Modifiers::NONE);
    assert_eq!(overlay.state.active_tool(), None);
    assert_eq!(overlay.closed_with(), None);
    overlay.key(Key::Escape, Modifiers::NONE);
    assert_eq!(overlay.closed_with(), Some(Outcome::Cancelled));
}

#[test]
fn actions_without_a_backend_close_with_a_deferred_outcome() {
    for (button, action) in [
        (ButtonType::Pin, DeferredAction::Pin),
        (ButtonType::Ocr, DeferredAction::Ocr),
        (ButtonType::OpenInEditor, DeferredAction::OpenInEditor),
        (ButtonType::OpenApp, DeferredAction::OpenWith),
    ] {
        let mut overlay = Overlay::selected();
        overlay.click_button(button);
        match overlay.closed_with() {
            Some(Outcome::Deferred(deferred)) => {
                assert_eq!(deferred.action, action);
                assert_eq!(
                    deferred.region,
                    phramer_capture::LocalRect::new(200, 200, 800, 500)
                );
            }
            other => panic!("{button:?}: {other:?}"),
        }
    }
    // Their shortcuts too: O is OCR, E the editor
    let mut overlay = Overlay::selected();
    overlay.char('o');
    assert!(matches!(
        overlay.closed_with(),
        Some(Outcome::Deferred(deferred)) if deferred.action == DeferredAction::Ocr
    ));
}

#[test]
fn copy_save_and_exit_buttons_end_the_capture() {
    for (button, outcome) in [
        (ButtonType::Copy, Some(ExportAction::Copy)),
        (ButtonType::CopyFile, Some(ExportAction::CopyFile)),
        (ButtonType::Save, Some(ExportAction::Save)),
        (ButtonType::Exit, None),
    ] {
        let mut overlay = Overlay::selected();
        overlay.click_button(button);
        match (overlay.closed_with(), outcome) {
            (Some(Outcome::Export(export)), Some(action)) => assert_eq!(export.action, action),
            (Some(Outcome::Cancelled), None) => {}
            other => panic!("{button:?}: {other:?}"),
        }
    }
}

#[test]
fn a_pending_size_is_saved_before_the_capture_closes() {
    let mut overlay = Overlay::selected();
    overlay.char('a');
    overlay.wheel(120, Modifiers::NONE);
    let effects = overlay.key(Key::Char('c'), Modifiers::CTRL);
    let persist = effects
        .iter()
        .position(|e| matches!(e, Effect::Persist(_)))
        .expect("saved");
    let close = effects
        .iter()
        .position(|e| matches!(e, Effect::Close(_)))
        .expect("closed");
    assert!(persist < close);
    assert_eq!(close, effects.len() - 1);
}
