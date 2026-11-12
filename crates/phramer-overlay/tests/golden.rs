//! Golden images of the overlay renderer, compared byte for byte with the
//! PNGs under `tests/golden/`. `PHRAMER_UPDATE_GOLDEN=1` rewrites them; a
//! mismatch writes the actual image and a diff under `target/overlay-golden/`
//! and names them in the failure.

use std::path::{Path, PathBuf};

use phramer_capture::{LogicalPoint, Monitor, MonitorId, PhysicalRect};
use phramer_overlay::{
    BUNDLED_FONT, Event, FrameView, Key, Modifiers, OverlayConfig, OverlayState, PointerButton,
    RgbaImage, Theme, render,
};

const UPDATE_ENV: &str = "PHRAMER_UPDATE_GOLDEN";

/// A made-up desktop at `scale`: a gradient wallpaper, two windows with
/// title bars and text-like lines, a taskbar. Returns BGRA pixels and the
/// windows' rectangles (physical).
fn desktop(scale: f64) -> (u32, u32, Vec<u8>, Vec<PhysicalRect>) {
    let s = |v: i32| (f64::from(v) * scale).round() as i32;
    let (width, height) = (s(640) as u32, s(400) as u32);
    let mut bgra = Vec::with_capacity(width as usize * height as usize * 4);
    for y in 0..height {
        for x in 0..width {
            let t = y as f64 / height as f64;
            let wave = (((x as f64 / scale) * 0.05).sin() * 8.0) as i32;
            let r = (40.0 + 60.0 * t) as i32 + wave;
            let g = (70.0 - 30.0 * t) as i32;
            let b = (120.0 + 20.0 * t) as i32 - wave;
            bgra.extend_from_slice(&[b as u8, g as u8, r as u8, 255]);
        }
    }
    let mut fill = |x: i32, y: i32, w: i32, h: i32, (r, g, b): (u8, u8, u8)| {
        for row in s(y)..s(y + h) {
            for column in s(x)..s(x + w) {
                let at = (row as usize * width as usize + column as usize) * 4;
                bgra[at..at + 4].copy_from_slice(&[b, g, r, 255]);
            }
        }
    };
    // Window A: a document
    fill(60, 50, 260, 170, (238, 238, 242));
    fill(60, 50, 260, 24, (214, 214, 224));
    for line in 0..6 {
        fill(
            76,
            90 + line * 18,
            180 + (line % 3) * 30,
            6,
            (150, 150, 160),
        );
    }
    // Window B: a chat, above A
    fill(280, 140, 230, 190, (255, 255, 255));
    fill(280, 140, 230, 28, (70, 110, 220));
    for line in 0..5 {
        fill(
            296,
            184 + line * 26,
            120 + (line % 2) * 60,
            14,
            (225, 232, 250),
        );
    }
    // Taskbar
    fill(0, 370, 640, 30, (24, 26, 32));
    for icon in 0..5 {
        fill(240 + icon * 34, 376, 18, 18, (120, 160, 230));
    }
    let windows = vec![
        PhysicalRect::new(s(280), s(140), s(230), s(190)),
        PhysicalRect::new(s(60), s(50), s(260), s(170)),
        PhysicalRect::new(0, s(370), s(640), s(30)),
    ];
    (width, height, bgra, windows)
}

fn monitor(width: u32, height: u32, dpi: u32) -> Monitor {
    Monitor::new(
        MonitorId::new("golden"),
        PhysicalRect::new(0, 0, width as i32, height as i32),
        dpi,
        true,
    )
}

fn draw(state: &OverlayState, monitor: usize, width: u32, height: u32, bgra: &[u8]) -> RgbaImage {
    let frame = FrameView::new(width, height, bgra).expect("frame size");
    render(state, monitor, frame, &Theme::default(), BUNDLED_FONT).expect("render")
}

fn golden_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("golden")
}

fn failure_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("target")
        .join("overlay-golden")
}

fn write_png(path: &Path, image: &RgbaImage) {
    let file = std::fs::File::create(path).expect("create png");
    let mut encoder = png::Encoder::new(std::io::BufWriter::new(file), image.width, image.height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().expect("png header");
    writer.write_image_data(&image.pixels).expect("png data");
}

fn read_png(path: &Path) -> Option<RgbaImage> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .ok()?;
    let mut pixels = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut pixels).ok()?;
    pixels.truncate(info.buffer_size());
    Some(RgbaImage {
        width: info.width,
        height: info.height,
        pixels,
    })
}

fn check(name: &str, actual: &RgbaImage) {
    let golden = golden_dir().join(format!("{name}.png"));
    if std::env::var_os(UPDATE_ENV).is_some_and(|value| value == "1") {
        std::fs::create_dir_all(golden_dir()).expect("golden dir");
        write_png(&golden, actual);
        return;
    }
    let expected = read_png(&golden);
    if expected.as_ref() == Some(actual) {
        return;
    }
    let out = failure_dir();
    std::fs::create_dir_all(&out).expect("failure dir");
    let actual_path = out.join(format!("{name}.actual.png"));
    write_png(&actual_path, actual);
    let mut message = format!(
        "{name} differs from {}; actual written to {}",
        golden.display(),
        actual_path.display()
    );
    if let Some(expected) = expected
        && (expected.width, expected.height) == (actual.width, actual.height)
    {
        let mut diff = actual.clone();
        let mut differing = 0usize;
        for (pixel, other) in diff
            .pixels
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(expected.pixels.as_chunks::<4>().0)
        {
            let same = *pixel == *other;
            differing += usize::from(!same);
            *pixel = if same {
                [0, 0, 0, 255]
            } else {
                [255, 0, 255, 255]
            };
        }
        let diff_path = out.join(format!("{name}.diff.png"));
        write_png(&diff_path, &diff);
        message.push_str(&format!(
            ", {differing} pixels differ (diff: {})",
            diff_path.display()
        ));
    }
    panic!("{message}");
}

fn focused(state: &mut OverlayState) {
    state.handle(Event::Focus(true), 0);
}

fn pointer(state: &mut OverlayState, event: &str, x: i32, y: i32) {
    let at = LogicalPoint::new(x, y);
    let event = match event {
        "move" => Event::PointerMove {
            monitor: state.active_monitor(),
            at,
            left_down: false,
            modifiers: Modifiers::NONE,
        },
        "drag" => Event::PointerMove {
            monitor: state.active_monitor(),
            at,
            left_down: true,
            modifiers: Modifiers::NONE,
        },
        "down" => Event::PointerDown {
            monitor: state.active_monitor(),
            at,
            button: PointerButton::Left,
            modifiers: Modifiers::NONE,
        },
        _ => Event::PointerUp {
            monitor: state.active_monitor(),
            at,
            button: PointerButton::Left,
            modifiers: Modifiers::NONE,
        },
    };
    state.handle(event, 10);
}

fn no_help() -> OverlayConfig {
    OverlayConfig {
        show_help: false,
        ..OverlayConfig::default()
    }
}

#[test]
fn idle_with_a_window_highlighted() {
    let (width, height, bgra, windows) = desktop(1.0);
    let mut state =
        OverlayState::new(vec![monitor(width, height, 96)], 0, windows, no_help()).expect("state");
    focused(&mut state);
    pointer(&mut state, "move", 400, 250);
    check("idle_snap", &draw(&state, 0, width, height, &bgra));
}

#[test]
fn mid_drag() {
    let (width, height, bgra, windows) = desktop(1.0);
    let mut state = OverlayState::new(
        vec![monitor(width, height, 96)],
        0,
        windows,
        OverlayConfig::default(),
    )
    .expect("state");
    focused(&mut state);
    pointer(&mut state, "down", 120, 80);
    pointer(&mut state, "drag", 300, 200);
    pointer(&mut state, "drag", 452, 286);
    check("drag", &draw(&state, 0, width, height, &bgra));
}

#[test]
fn selection_with_handles_and_size_label_at_150_percent() {
    let (width, height, bgra, windows) = desktop(1.5);
    let mut state = OverlayState::new(
        vec![monitor(width, height, 144)],
        0,
        windows,
        OverlayConfig::default(),
    )
    .expect("state");
    focused(&mut state);
    pointer(&mut state, "down", 200, 100);
    pointer(&mut state, "drag", 400, 200);
    pointer(&mut state, "drag", 520, 300);
    pointer(&mut state, "up", 520, 300);
    pointer(&mut state, "move", 600, 360);
    check("selection", &draw(&state, 0, width, height, &bgra));
}

#[test]
fn help_message() {
    let (width, height, bgra, _) = desktop(1.0);
    let mut state = OverlayState::new(
        vec![monitor(width, height, 96)],
        0,
        Vec::new(),
        OverlayConfig::default(),
    )
    .expect("state");
    focused(&mut state);
    check("help", &draw(&state, 0, width, height, &bgra));
}

#[test]
fn another_monitor_while_following_is_only_dimmed() {
    let (width, height, bgra, windows) = desktop(1.0);
    let monitors = vec![
        Monitor::new(
            MonitorId::new("active"),
            PhysicalRect::new(-(width as i32), 0, width as i32, height as i32),
            96,
            false,
        ),
        monitor(width, height, 96),
    ];
    let mut state = OverlayState::new(
        monitors,
        0,
        windows,
        OverlayConfig {
            follow_cursor: true,
            ..OverlayConfig::default()
        },
    )
    .expect("state");
    focused(&mut state);
    assert!(state.is_following());
    check("dim_follow", &draw(&state, 1, width, height, &bgra));
}

#[test]
fn lost_focus_notice() {
    let (width, height, bgra, _) = desktop(1.0);
    let state = OverlayState::new(vec![monitor(width, height, 96)], 0, Vec::new(), no_help())
        .expect("state");
    check("lost_focus", &draw(&state, 0, width, height, &bgra));
}

#[test]
fn a_whole_monitor_selection_keeps_its_frame_on_screen() {
    let (width, height, bgra, _) = desktop(1.0);
    let mut state = OverlayState::new(vec![monitor(width, height, 96)], 0, Vec::new(), no_help())
        .expect("state");
    focused(&mut state);
    state.handle(
        Event::Key {
            key: Key::Char('a'),
            modifiers: Modifiers::CTRL,
        },
        0,
    );
    check("select_all", &draw(&state, 0, width, height, &bgra));
}

#[test]
fn rendering_is_deterministic() {
    let (width, height, bgra, windows) = desktop(1.25);
    let mut state = OverlayState::new(
        vec![monitor(width, height, 120)],
        0,
        windows,
        OverlayConfig::default(),
    )
    .expect("state");
    focused(&mut state);
    pointer(&mut state, "down", 50, 50);
    pointer(&mut state, "drag", 300, 200);
    let first = draw(&state, 0, width, height, &bgra);
    let second = draw(&state, 0, width, height, &bgra);
    assert_eq!(first, second);
    assert_eq!((first.width, first.height), (800, 500));
}
