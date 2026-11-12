//! Scripted event sequences through the overlay state machine, one per
//! interaction of 15.0.0's capture overlay, checking the selection and the
//! effects that come out.

use phramer_capture::{LocalRect, LogicalPoint, LogicalRect, Monitor, MonitorId, PhysicalRect};
use phramer_core::config::Settings;
use phramer_overlay::{
    CursorShape, Effect, Event, Export, ExportAction, GeometryLabel, Key, Modifiers, Outcome,
    OverlayConfig, OverlayState, PointerButton,
};

fn monitor(id: &str, (x, y, width, height): (i32, i32, i32, i32), dpi: u32) -> Monitor {
    Monitor::new(
        MonitorId::new(id),
        PhysicalRect::new(x, y, width, height),
        dpi,
        x == 0 && y == 0,
    )
}

fn single() -> Vec<Monitor> {
    vec![monitor("only", (0, 0, 1920, 1080), 96)]
}

/// A 125% primary, a 100% monitor left of it and lowered, a 150% 4K
/// monitor above: negative coordinates and three scales.
fn mixed() -> Vec<Monitor> {
    vec![
        monitor("primary", (0, 0, 1920, 1200), 120),
        monitor("left", (-1920, 200, 1920, 1080), 96),
        monitor("above", (-960, -2160, 3840, 2160), 144),
    ]
}

fn p(x: i32, y: i32) -> LogicalPoint {
    LogicalPoint::new(x, y)
}

fn config() -> OverlayConfig {
    OverlayConfig::default()
}

struct Overlay {
    state: OverlayState,
    now: u64,
    effects: Vec<Effect>,
}

impl Overlay {
    fn new(
        monitors: Vec<Monitor>,
        active: usize,
        windows: Vec<PhysicalRect>,
        config: OverlayConfig,
    ) -> Self {
        let mut state =
            OverlayState::new(monitors, active, windows, config).expect("valid monitor");
        let effects = state.start();
        Self {
            state,
            now: 0,
            effects,
        }
    }

    fn on(monitors: Vec<Monitor>) -> Self {
        Self::new(monitors, 0, Vec::new(), config())
    }

    fn send(&mut self, event: Event) -> &[Effect] {
        self.effects = self.state.handle(event, self.now);
        &self.effects
    }

    fn hover_at(&mut self, monitor: usize, at: LogicalPoint) -> &[Effect] {
        self.send(Event::PointerMove {
            monitor,
            at,
            left_down: false,
            modifiers: Modifiers::NONE,
        })
    }

    fn press(&mut self, monitor: usize, at: LogicalPoint, button: PointerButton) -> &[Effect] {
        self.send(Event::PointerDown {
            monitor,
            at,
            button,
            modifiers: Modifiers::NONE,
        })
    }

    fn drag(&mut self, monitor: usize, at: LogicalPoint, modifiers: Modifiers) -> &[Effect] {
        self.send(Event::PointerMove {
            monitor,
            at,
            left_down: true,
            modifiers,
        })
    }

    fn release(&mut self, monitor: usize, at: LogicalPoint, button: PointerButton) -> &[Effect] {
        self.send(Event::PointerUp {
            monitor,
            at,
            button,
            modifiers: Modifiers::NONE,
        })
    }

    /// A left-button drag from `from` to `to` in a few steps; the effects
    /// of all of them.
    fn select(&mut self, from: LogicalPoint, to: LogicalPoint) -> &[Effect] {
        let monitor = self.state.active_monitor();
        let mid = p((from.x + to.x) / 2, (from.y + to.y) / 2);
        let mut all = self.press(monitor, from, PointerButton::Left).to_vec();
        all.extend_from_slice(self.drag(monitor, mid, Modifiers::NONE));
        all.extend_from_slice(self.drag(monitor, to, Modifiers::NONE));
        all.extend_from_slice(self.release(monitor, to, PointerButton::Left));
        self.effects = all;
        &self.effects
    }

    fn key(&mut self, key: Key, modifiers: Modifiers) -> &[Effect] {
        self.send(Event::Key { key, modifiers })
    }

    fn closed_with(&self) -> Option<Outcome> {
        self.effects.iter().find_map(|effect| match effect {
            Effect::Close(outcome) => Some(*outcome),
            _ => None,
        })
    }
}

fn exported(effects: &[Effect]) -> Option<Export> {
    effects.iter().find_map(|effect| match effect {
        Effect::Close(Outcome::Export(export)) => Some(*export),
        _ => None,
    })
}

// --- Opening -----------------------------------------------------------

#[test]
fn opening_draws_the_window_and_shows_the_help() {
    let overlay = Overlay::on(single());
    assert_eq!(
        overlay.effects,
        [
            Effect::Redraw { monitor: 0 },
            Effect::SetCursor(CursorShape::Cross)
        ]
    );
    assert!(overlay.state.help_visible());
    assert_eq!(overlay.state.selection(), None);
    assert!(!overlay.state.is_following());
}

#[test]
fn the_show_help_setting_turns_the_help_off() {
    let mut settings = Settings::default();
    settings.set_show_help(false);
    let overlay = Overlay::new(
        single(),
        0,
        Vec::new(),
        OverlayConfig::from_settings(&settings),
    );
    assert!(!overlay.state.help_visible());
}

#[test]
fn help_lists_copy_and_save_for_a_plain_capture_and_enter_with_tasks() {
    let plain = Overlay::on(single());
    let keys: Vec<String> = plain
        .state
        .help_rows()
        .into_iter()
        .map(|(keys, _)| keys)
        .collect();
    assert_eq!(keys, ["Mouse", "Click", "Ctrl+S", "Ctrl+C", "Esc"]);

    let tasks = Overlay::new(
        single(),
        0,
        Vec::new(),
        OverlayConfig {
            has_tasks: true,
            ..config()
        },
    );
    let rows = tasks.state.help_rows();
    assert_eq!(
        rows[2],
        ("Enter".to_owned(), "Accept the capture".to_owned())
    );
    assert_eq!(rows.len(), 4);
}

#[test]
fn an_unknown_monitor_index_is_an_error() {
    assert!(OverlayState::new(single(), 3, Vec::new(), config()).is_err());
}

// --- Window snap -------------------------------------------------------

#[test]
fn hovering_highlights_the_topmost_window_under_the_pointer() {
    let windows = vec![
        PhysicalRect::new(200, 100, 400, 300),
        PhysicalRect::new(0, 0, 1920, 1040),
    ];
    let mut overlay = Overlay::new(single(), 0, windows, config());
    assert_eq!(
        overlay.hover_at(0, p(250, 150)),
        [Effect::Redraw { monitor: 0 }]
    );
    assert_eq!(
        overlay.state.hover_window(),
        Some(LogicalRect::new(200, 100, 400, 300))
    );
    // Same window: nothing to redraw
    assert!(overlay.hover_at(0, p(260, 160)).is_empty());
    overlay.hover_at(0, p(1000, 900));
    assert_eq!(
        overlay.state.hover_window(),
        Some(LogicalRect::new(0, 0, 1920, 1040))
    );
    // The taskbar strip: no window
    overlay.hover_at(0, p(1000, 1060));
    assert_eq!(overlay.state.hover_window(), None);
}

#[test]
fn a_click_without_a_drag_selects_the_highlighted_window() {
    let windows = vec![PhysicalRect::new(200, 100, 400, 300)];
    let mut overlay = Overlay::new(single(), 0, windows, config());
    overlay.hover_at(0, p(300, 200));
    overlay.press(0, p(300, 200), PointerButton::Left);
    // A twitch within 3 px is still a click
    overlay.drag(0, p(301, 202), Modifiers::NONE);
    assert_eq!(overlay.state.selection(), None);
    overlay.release(0, p(301, 202), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(200, 100, 400, 300))
    );
    assert_eq!(overlay.state.hover_window(), None);
    assert!(!overlay.state.help_visible());
}

#[test]
fn a_click_picks_the_window_under_the_pointer_even_without_a_move() {
    let windows = vec![PhysicalRect::new(200, 100, 400, 300)];
    let mut overlay = Overlay::new(single(), 0, windows, config());
    overlay.press(0, p(300, 200), PointerButton::Left);
    overlay.release(0, p(300, 200), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(200, 100, 400, 300))
    );
}

#[test]
fn a_drag_that_returns_to_its_start_is_not_a_window_click() {
    let windows = vec![PhysicalRect::new(200, 100, 400, 300)];
    let mut overlay = Overlay::new(single(), 0, windows, config());
    overlay.hover_at(0, p(300, 200));
    overlay.press(0, p(300, 200), PointerButton::Left);
    overlay.drag(0, p(400, 300), Modifiers::NONE);
    overlay.drag(0, p(301, 201), Modifiers::NONE);
    overlay.release(0, p(301, 201), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(300, 200, 1, 1))
    );
}

#[test]
fn window_snap_maps_through_each_monitors_scale_and_offset() {
    // A window straddling "left" (100%, negative x) and "primary" (125%)
    let windows = vec![PhysicalRect::new(-300, 300, 800, 500)];
    let mut on_primary = Overlay::new(mixed(), 0, windows.clone(), config());
    on_primary.hover_at(0, p(100, 300));
    assert_eq!(
        on_primary.state.hover_window(),
        Some(LogicalRect::new(0, 240, 400, 400))
    );
    on_primary.press(0, p(100, 300), PointerButton::Left);
    on_primary.release(0, p(100, 300), PointerButton::Left);
    let export = exported(on_primary.key(Key::Char('c'), Modifiers::CTRL)).expect("export");
    // Only the part on this monitor, in its physical pixels
    assert_eq!(export.region, LocalRect::new(0, 300, 500, 500));

    let mut on_left = Overlay::new(mixed(), 1, windows, config());
    on_left.hover_at(1, p(1700, 200));
    assert_eq!(
        on_left.state.hover_window(),
        Some(LogicalRect::new(1620, 100, 300, 500))
    );
}

// --- Drawing a selection -----------------------------------------------

#[test]
fn dragging_draws_a_selection_and_hides_the_help() {
    let mut overlay = Overlay::on(single());
    overlay.press(0, p(100, 100), PointerButton::Left);
    // Not yet: within the click distance
    overlay.drag(0, p(102, 101), Modifiers::NONE);
    assert_eq!(overlay.state.selection(), None);
    let effects = overlay.drag(0, p(300, 250), Modifiers::NONE).to_vec();
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 100, 200, 150))
    );
    assert!(effects.contains(&Effect::Redraw { monitor: 0 }));
    assert!(effects.contains(&Effect::SetCursor(CursorShape::ResizeNwSe)));
    assert!(!overlay.state.help_visible());
    overlay.release(0, p(300, 250), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 100, 200, 150))
    );
}

#[test]
fn a_selection_can_be_drawn_in_any_direction() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(300, 300), p(100, 50));
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 50, 200, 250))
    );
}

#[test]
fn shift_draws_around_the_start_and_ctrl_draws_a_square() {
    let mut overlay = Overlay::on(single());
    overlay.press(0, p(500, 500), PointerButton::Left);
    overlay.drag(0, p(510, 510), Modifiers::NONE);
    overlay.drag(0, p(600, 550), Modifiers::SHIFT);
    // The centre stays where it was when Shift came in: (505, 505)
    let selection = overlay.state.selection().expect("selection");
    assert_eq!(
        (
            selection.x + selection.right(),
            selection.y + selection.bottom()
        ),
        (1010, 1010)
    );

    let mut overlay = Overlay::on(single());
    overlay.press(0, p(100, 100), PointerButton::Left);
    overlay.drag(0, p(110, 110), Modifiers::NONE);
    overlay.drag(0, p(400, 200), Modifiers::CTRL);
    let selection = overlay.state.selection().expect("selection");
    assert_eq!(selection.width, selection.height);
}

#[test]
fn pressing_outside_a_selection_starts_a_new_one() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.select(p(800, 600), p(900, 700));
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(800, 600, 100, 100))
    );
}

#[test]
fn a_lost_button_release_ends_the_drag() {
    let mut overlay = Overlay::on(single());
    overlay.press(0, p(100, 100), PointerButton::Left);
    overlay.drag(0, p(200, 200), Modifiers::NONE);
    // The next move arrives with the button already up, over the corner
    overlay.hover_at(0, p(205, 205));
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 100, 100, 100))
    );
    // No longer dragging: moving on changes nothing
    overlay.hover_at(0, p(400, 400));
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 100, 100, 100))
    );
    assert_eq!(overlay.state.cursor(), CursorShape::Cross);
}

// --- Moving and resizing -------------------------------------------------

#[test]
fn dragging_the_inside_moves_the_selection() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.hover_at(0, p(200, 200));
    assert_eq!(overlay.state.cursor(), CursorShape::Arrow);
    overlay.press(0, p(200, 200), PointerButton::Left);
    assert_eq!(overlay.state.cursor(), CursorShape::Grabbing);
    overlay.drag(0, p(250, 260), Modifiers::NONE);
    overlay.drag(0, p(260, 280), Modifiers::NONE);
    overlay.release(0, p(260, 280), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(160, 180, 200, 200))
    );
}

#[test]
fn a_selection_moved_off_the_monitor_exports_its_visible_part() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.press(0, p(200, 200), PointerButton::Left);
    overlay.drag(0, p(50, 200), Modifiers::NONE);
    overlay.release(0, p(50, 200), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(-50, 100, 200, 200))
    );
    let export = exported(overlay.key(Key::Char('c'), Modifiers::CTRL)).expect("export");
    assert_eq!(export.region, LocalRect::new(0, 100, 150, 200));
}

#[test]
fn each_of_the_eight_handles_moves_its_edges() {
    // Selection (100,100)-(300,300); each handle dragged 20 px outward
    let cases = [
        (
            p(100, 100),
            p(80, 80),
            (80, 80, 300, 300),
            CursorShape::ResizeNwSe,
        ),
        (
            p(200, 100),
            p(200, 80),
            (100, 80, 300, 300),
            CursorShape::ResizeVertical,
        ),
        (
            p(300, 100),
            p(320, 80),
            (100, 80, 320, 300),
            CursorShape::ResizeNeSw,
        ),
        (
            p(300, 200),
            p(320, 200),
            (100, 100, 320, 300),
            CursorShape::ResizeHorizontal,
        ),
        (
            p(300, 300),
            p(320, 320),
            (100, 100, 320, 320),
            CursorShape::ResizeNwSe,
        ),
        (
            p(200, 300),
            p(200, 320),
            (100, 100, 300, 320),
            CursorShape::ResizeVertical,
        ),
        (
            p(100, 300),
            p(80, 320),
            (80, 100, 300, 320),
            CursorShape::ResizeNeSw,
        ),
        (
            p(100, 200),
            p(80, 200),
            (80, 100, 300, 300),
            CursorShape::ResizeHorizontal,
        ),
    ];
    for (handle, to, (left, top, right, bottom), cursor) in cases {
        let mut overlay = Overlay::on(single());
        overlay.select(p(100, 100), p(300, 300));
        overlay.hover_at(0, handle);
        assert_eq!(overlay.state.cursor(), cursor, "hovering {handle:?}");
        overlay.press(0, handle, PointerButton::Left);
        overlay.drag(0, to, Modifiers::NONE);
        overlay.release(0, to, PointerButton::Left);
        assert_eq!(
            overlay.state.selection(),
            Some(LogicalRect::from_edges(left, top, right, bottom)),
            "handle at {handle:?}"
        );
    }
}

#[test]
fn the_grab_area_reaches_past_the_edge() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    // 8 px outside the left edge still holds it
    overlay.press(0, p(92, 200), PointerButton::Left);
    overlay.drag(0, p(50, 200), Modifiers::NONE);
    overlay.release(0, p(50, 200), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::from_edges(50, 100, 300, 300))
    );
}

#[test]
fn an_edge_dragged_past_its_opposite_turns_into_it() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.press(0, p(100, 200), PointerButton::Left);
    overlay.drag(0, p(400, 200), Modifiers::NONE);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::from_edges(300, 100, 400, 300))
    );
    // Still held: now as the right edge
    overlay.drag(0, p(500, 200), Modifiers::NONE);
    overlay.release(0, p(500, 200), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::from_edges(300, 100, 500, 300))
    );
}

#[test]
fn shift_on_a_handle_resizes_around_the_centre() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.press(0, p(300, 200), PointerButton::Left);
    overlay.drag(0, p(320, 200), Modifiers::SHIFT);
    overlay.release(0, p(320, 200), PointerButton::Left);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::from_edges(80, 100, 320, 300))
    );
}

// --- Clearing --------------------------------------------------------------

#[test]
fn a_click_outside_the_selection_clears_it_and_brings_the_help_back() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    assert!(!overlay.state.help_visible());
    overlay.press(0, p(800, 800), PointerButton::Left);
    overlay.release(0, p(800, 800), PointerButton::Left);
    assert_eq!(overlay.state.selection(), None);
    assert!(overlay.state.help_visible());
    // A click inside keeps it
    overlay.select(p(100, 100), p(300, 300));
    overlay.press(0, p(200, 200), PointerButton::Left);
    overlay.release(0, p(200, 200), PointerButton::Left);
    assert!(overlay.state.selection().is_some());
}

#[test]
fn a_right_click_clears_outside_the_selection_and_keeps_inside() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.press(0, p(200, 200), PointerButton::Right);
    overlay.release(0, p(200, 200), PointerButton::Right);
    assert!(overlay.state.selection().is_some());
    overlay.press(0, p(900, 900), PointerButton::Right);
    overlay.release(0, p(900, 900), PointerButton::Right);
    assert_eq!(overlay.state.selection(), None);
}

#[test]
fn ctrl_backspace_clears_the_selection() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.key(Key::Backspace, Modifiers::CTRL);
    assert_eq!(overlay.state.selection(), None);
    assert!(overlay.state.help_visible());
    assert_eq!(overlay.closed_with(), None);
}

// --- Closing ------------------------------------------------------------

#[test]
fn esc_cancels_and_nothing_happens_after_closing() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    assert_eq!(
        overlay.key(Key::Escape, Modifiers::NONE),
        [Effect::Close(Outcome::Cancelled)]
    );
    assert!(overlay.state.is_closed());
    assert!(overlay.key(Key::Char('c'), Modifiers::CTRL).is_empty());
    assert!(overlay.hover_at(0, p(10, 10)).is_empty());
}

#[test]
fn ctrl_q_exits_without_exporting() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.key(Key::Char('q'), Modifiers::CTRL);
    assert_eq!(overlay.closed_with(), Some(Outcome::Cancelled));
}

#[test]
fn copy_save_and_copy_file_keys_export_the_selection() {
    for (key, modifiers, action) in [
        (Key::Char('c'), Modifiers::CTRL, ExportAction::Copy),
        (Key::Char('s'), Modifiers::CTRL, ExportAction::Save),
        (
            Key::Char('c'),
            Modifiers::CTRL_SHIFT,
            ExportAction::CopyFile,
        ),
    ] {
        let mut overlay = Overlay::on(single());
        overlay.select(p(100, 100), p(300, 250));
        let effects = overlay.key(key, modifiers).to_vec();
        assert_eq!(
            effects,
            [Effect::Close(Outcome::Export(Export {
                monitor: 0,
                region: LocalRect::new(100, 100, 200, 150),
                action,
            }))]
        );
    }
}

#[test]
fn without_a_selection_the_whole_monitor_is_exported() {
    let mut overlay = Overlay::new(mixed(), 0, Vec::new(), config());
    let export = exported(overlay.key(Key::Char('s'), Modifiers::CTRL)).expect("export");
    assert_eq!(export.region, LocalRect::new(0, 0, 1920, 1200));
}

#[test]
fn enter_accepts_only_when_the_request_has_tasks() {
    let mut plain = Overlay::on(single());
    plain.select(p(10, 10), p(20, 20));
    assert!(plain.key(Key::Return, Modifiers::NONE).is_empty());

    let mut tasks = Overlay::new(
        single(),
        0,
        Vec::new(),
        OverlayConfig {
            has_tasks: true,
            ..config()
        },
    );
    tasks.select(p(10, 10), p(20, 20));
    // Copy is not offered when the request decides
    assert!(tasks.key(Key::Char('c'), Modifiers::CTRL).is_empty());
    // The keypad's Enter is Return
    let export = exported(tasks.key(Key::Enter, Modifiers::NONE)).expect("export");
    assert_eq!(export.action, ExportAction::Accept);
    assert_eq!(export.region, LocalRect::new(10, 10, 10, 10));
}

#[test]
fn rebound_keys_come_from_the_settings() {
    let mut settings = Settings::default();
    settings
        .set_shortcut("TYPE_COPY", "Ctrl+Alt+K")
        .expect("valid shortcut");
    let mut overlay = Overlay::new(
        single(),
        0,
        Vec::new(),
        OverlayConfig::from_settings(&settings),
    );
    overlay.select(p(10, 10), p(20, 20));
    assert!(overlay.key(Key::Char('c'), Modifiers::CTRL).is_empty());
    let ctrl_alt = Modifiers {
        ctrl: true,
        alt: true,
        ..Modifiers::NONE
    };
    let export = exported(overlay.key(Key::Char('k'), ctrl_alt)).expect("export");
    assert_eq!(export.action, ExportAction::Copy);
}

#[test]
fn accept_on_select_exports_when_a_drag_or_a_window_click_settles() {
    let accept = OverlayConfig {
        accept_on_select: true,
        has_tasks: true,
        ..config()
    };
    let mut overlay = Overlay::new(single(), 0, Vec::new(), accept.clone());
    overlay.press(0, p(100, 100), PointerButton::Left);
    overlay.drag(0, p(200, 200), Modifiers::NONE);
    assert_eq!(overlay.closed_with(), None);
    let export = exported(overlay.release(0, p(200, 200), PointerButton::Left)).expect("export");
    assert_eq!(export.region, LocalRect::new(100, 100, 100, 100));

    let windows = vec![PhysicalRect::new(50, 60, 70, 80)];
    let mut overlay = Overlay::new(single(), 0, windows, accept);
    overlay.press(0, p(60, 70), PointerButton::Left);
    let export = exported(overlay.release(0, p(60, 70), PointerButton::Left)).expect("export");
    assert_eq!(export.region, LocalRect::new(50, 60, 70, 80));
}

#[test]
fn a_given_region_with_accept_on_select_closes_at_once() {
    let mut state = OverlayState::new(
        mixed(),
        0,
        Vec::new(),
        OverlayConfig {
            accept_on_select: true,
            has_tasks: true,
            initial_selection: Some(LocalRect::new(125, 250, 500, 375)),
            ..config()
        },
    )
    .expect("state");
    assert_eq!(
        state.selection(),
        Some(LogicalRect::new(100, 200, 400, 300))
    );
    let export = exported(&state.start()).expect("export");
    assert_eq!(export.region, LocalRect::new(125, 250, 500, 375));
}

// --- Keyboard ---------------------------------------------------------------

#[test]
fn arrows_move_and_shift_arrows_resize_as_in_15_0_0() {
    let mut overlay = Overlay::on(single());
    overlay.select(p(100, 100), p(300, 300));
    overlay.key(Key::Right, Modifiers::NONE);
    overlay.key(Key::Down, Modifiers::NONE);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(101, 101, 200, 200))
    );
    overlay.key(Key::Left, Modifiers::SHIFT);
    overlay.key(Key::Down, Modifiers::SHIFT);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(101, 101, 199, 201))
    );
    overlay.key(Key::Right, Modifiers::CTRL_SHIFT);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 101, 201, 201))
    );
    overlay.key(Key::Up, Modifiers::CTRL_SHIFT);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(100, 100, 201, 203))
    );
}

#[test]
fn nudges_stop_at_the_monitor_edge_and_need_a_selection() {
    let mut overlay = Overlay::on(single());
    assert!(overlay.key(Key::Left, Modifiers::NONE).is_empty());
    overlay.key(Key::Char('a'), Modifiers::CTRL);
    overlay.key(Key::Left, Modifiers::NONE);
    // Moved left off the monitor: the visible part stays
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(0, 0, 1919, 1080))
    );
}

#[test]
fn a_nudged_selection_settles_400_ms_after_the_last_key() {
    let mut overlay = Overlay::new(
        single(),
        0,
        Vec::new(),
        OverlayConfig {
            accept_on_select: true,
            has_tasks: true,
            initial_selection: None,
            geometry_label: GeometryLabel::Hidden,
            ..config()
        },
    );
    // A selection made by keyboard only: Ctrl+A settles at once
    overlay.now = 1000;
    let effects = overlay.key(Key::Char('a'), Modifiers::CTRL).to_vec();
    assert!(exported(&effects).is_some());

    let mut overlay = Overlay::new(
        single(),
        0,
        Vec::new(),
        OverlayConfig {
            geometry_label: GeometryLabel::Hidden,
            ..config()
        },
    );
    overlay.select(p(100, 100), p(200, 200));
    overlay.now = 1000;
    assert!(
        overlay
            .key(Key::Right, Modifiers::NONE)
            .contains(&Effect::WakeAt(1400))
    );
    overlay.now = 1200;
    assert!(
        overlay
            .key(Key::Right, Modifiers::NONE)
            .contains(&Effect::WakeAt(1600))
    );
}

#[test]
fn ctrl_a_selects_the_whole_monitor_at_its_scale() {
    let mut overlay = Overlay::new(mixed(), 0, Vec::new(), config());
    overlay.key(Key::Char('a'), Modifiers::CTRL);
    assert_eq!(
        overlay.state.selection(),
        Some(LogicalRect::new(0, 0, 1536, 960))
    );
    let export = exported(overlay.key(Key::Char('c'), Modifiers::CTRL)).expect("export");
    assert_eq!(export.region, LocalRect::new(0, 0, 1920, 1200));
}

// --- The size label -----------------------------------------------------------

#[test]
fn the_size_label_shows_physical_pixels_and_hides_after_the_timeout() {
    let mut overlay = Overlay::new(mixed(), 0, Vec::new(), config());
    overlay.now = 500;
    overlay.select(p(100, 100), p(300, 250));
    // Asked for once, by the first change
    // 125%: 200x150 logical is 250x188 physical (edges rounded)
    assert_eq!(
        overlay.state.geometry_label(),
        Some((GeometryLabel::BottomRight, "250x188+125+125".to_owned()))
    );
    assert!(overlay.effects.contains(&Effect::WakeAt(3500)));
    overlay.now = 3499;
    overlay.send(Event::Tick);
    assert!(overlay.state.geometry_label().is_some());
    overlay.now = 3500;
    assert_eq!(overlay.send(Event::Tick), [Effect::Redraw { monitor: 0 }]);
    assert_eq!(overlay.state.geometry_label(), None);
}

#[test]
fn a_zero_timeout_keeps_the_label_and_position_follows_the_setting() {
    let mut settings = Settings::default();
    settings
        .set_show_selection_geometry_hide_time(0)
        .expect("valid");
    settings.set_show_selection_geometry(5).expect("valid");
    let mut overlay = Overlay::new(
        single(),
        0,
        Vec::new(),
        OverlayConfig::from_settings(&settings),
    );
    overlay.select(p(10, 10), p(110, 60));
    assert!(
        !overlay
            .effects
            .iter()
            .any(|effect| matches!(effect, Effect::WakeAt(_)))
    );
    assert_eq!(
        overlay.state.geometry_label(),
        Some((GeometryLabel::Center, "100x50+10+10".to_owned()))
    );
}

// --- Snip across monitors (mode 2) -------------------------------------------

fn following() -> OverlayConfig {
    OverlayConfig {
        follow_cursor: true,
        ..config()
    }
}

#[test]
fn mode_2_follows_the_cursor_until_the_first_press() {
    let windows = vec![PhysicalRect::new(-1800, 300, 600, 400)];
    let mut overlay = Overlay::new(mixed(), 0, windows, following());
    assert!(overlay.state.is_following());
    assert_eq!(overlay.state.window_monitors(), [0, 1, 2]);
    assert!(overlay.effects.contains(&Effect::Redraw { monitor: 2 }));

    // The cursor enters the left monitor
    let effects = overlay.hover_at(1, p(200, 200)).to_vec();
    assert!(effects.contains(&Effect::SwitchMonitor { to: 1 }));
    assert!(effects.contains(&Effect::Redraw { monitor: 0 }));
    assert!(effects.contains(&Effect::Redraw { monitor: 1 }));
    assert_eq!(overlay.state.active_monitor(), 1);
    assert_eq!(
        overlay.state.hover_window(),
        Some(LogicalRect::new(120, 100, 600, 400))
    );

    // The press commits to it and lets the others go
    let effects = overlay.press(1, p(500, 500), PointerButton::Left).to_vec();
    assert!(effects.contains(&Effect::ReleaseMonitors));
    assert!(!overlay.state.is_following());
    assert_eq!(overlay.state.window_monitors(), [1]);
    // Other monitors' events are ignored from now on
    assert!(overlay.hover_at(0, p(10, 10)).is_empty());
    assert_eq!(overlay.state.active_monitor(), 1);

    overlay.drag(1, p(700, 600), Modifiers::NONE);
    overlay.release(1, p(700, 600), PointerButton::Left);
    let export = exported(overlay.key(Key::Char('c'), Modifiers::CTRL)).expect("export");
    assert_eq!(export.monitor, 1);
    assert_eq!(export.region, LocalRect::new(500, 500, 200, 100));
}

#[test]
fn a_press_on_another_monitor_moves_there_and_commits() {
    let mut overlay = Overlay::new(mixed(), 0, Vec::new(), following());
    let effects = overlay.press(2, p(100, 100), PointerButton::Left).to_vec();
    assert!(effects.contains(&Effect::SwitchMonitor { to: 2 }));
    assert!(effects.contains(&Effect::ReleaseMonitors));
    overlay.drag(2, p(300, 200), Modifiers::NONE);
    overlay.release(2, p(300, 200), PointerButton::Left);
    let export = exported(overlay.key(Key::Char('s'), Modifiers::CTRL)).expect("export");
    // 150%: 200x100 logical is 300x150 physical
    assert_eq!(export.monitor, 2);
    assert_eq!(export.region, LocalRect::new(150, 150, 300, 150));
}

#[test]
fn a_selection_made_by_keyboard_stops_the_following() {
    let mut overlay = Overlay::new(mixed(), 0, Vec::new(), following());
    overlay.key(Key::Char('a'), Modifiers::CTRL);
    assert!(overlay.hover_at(1, p(10, 10)).is_empty());
    assert_eq!(overlay.state.active_monitor(), 0);
}

#[test]
fn a_given_region_is_ignored_while_following() {
    let overlay = Overlay::new(
        mixed(),
        0,
        Vec::new(),
        OverlayConfig {
            initial_selection: Some(LocalRect::new(0, 0, 100, 100)),
            ..following()
        },
    );
    assert_eq!(overlay.state.selection(), None);
}

#[test]
fn a_single_monitor_never_follows() {
    let overlay = Overlay::new(single(), 0, Vec::new(), following());
    assert!(!overlay.state.is_following());
}

// --- Mixed DPI and negative coordinates -------------------------------------

#[test]
fn selections_on_monitors_left_of_and_above_the_primary_export_in_their_pixels() {
    let mut left = Overlay::new(mixed(), 1, Vec::new(), config());
    left.select(p(10, 20), p(110, 70));
    let export = exported(left.key(Key::Char('c'), Modifiers::CTRL)).expect("export");
    assert_eq!(export.region, LocalRect::new(10, 20, 100, 50));
    // On the desktop that is left of the primary
    let monitor = &mixed()[1];
    assert_eq!(
        monitor.rect_to_physical(export.region),
        PhysicalRect::new(-1910, 220, 100, 50)
    );

    let mut above = Overlay::new(mixed(), 2, Vec::new(), config());
    above.select(p(100, 100), p(201, 151));
    let export = exported(above.key(Key::Char('c'), Modifiers::CTRL)).expect("export");
    // 150%, edges rounded half away from zero: 101x51 -> 152x77
    assert_eq!(export.region, LocalRect::new(150, 150, 152, 77));
}

// --- Focus ---------------------------------------------------------------------

#[test]
fn focus_changes_redraw_once() {
    let mut overlay = Overlay::on(single());
    assert!(!overlay.state.is_focused());
    assert_eq!(
        overlay.send(Event::Focus(true)),
        [Effect::Redraw { monitor: 0 }]
    );
    assert!(overlay.send(Event::Focus(true)).is_empty());
    assert!(overlay.state.is_focused());
}

#[test]
fn the_document_covers_the_active_monitor() {
    let mut overlay = Overlay::new(mixed(), 0, Vec::new(), following());
    assert_eq!(overlay.state.document().image_size().width, 1920);
    overlay.hover_at(2, p(10, 10));
    assert_eq!(overlay.state.document().image_size().width, 3840);
    assert!(overlay.state.document().objects().is_empty());
}
