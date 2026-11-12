//! The export into a temporary folder: file names from the pattern, the
//! collision rule, formats, and image sizes equal to the selection in
//! physical pixels. The clipboard is a recording fake here; the real one is
//! tested in `win::clipboard`.

use std::path::{Path, PathBuf};

use phramer_capture::{LogicalPoint, Monitor, MonitorId, PhysicalRect};
use phramer_core::config::Settings;
use phramer_overlay::{
    CaptureTasks, CapturedImage, Effect, Event, ExportAction, ExportContext, Handoff, Key,
    Modifiers, Outcome, OverlayConfig, OverlayError, OverlayState, PointerButton, SaveTarget,
    TimeFormat, export, screenshot_path,
};

/// 2026-11-11 20:26:57, as the default pattern needs it.
struct Fixed;

impl TimeFormat for Fixed {
    fn format(&self, specifier: char) -> String {
        match specifier {
            'F' => "2026-11-11".into(),
            'H' => "20".into(),
            'M' => "26".into(),
            'S' => "57".into(),
            other => format!("{other}"),
        }
    }
}

#[derive(Default)]
struct Recorded {
    images: Vec<(u32, u32, Option<u8>)>,
    texts: Vec<String>,
    files: Vec<PathBuf>,
}

impl Handoff for Recorded {
    fn copy_image(&mut self, image: &CapturedImage, jpeg: Option<u8>) -> Result<(), OverlayError> {
        self.images.push((image.width, image.height, jpeg));
        Ok(())
    }

    fn copy_text(&mut self, text: &str) -> Result<(), OverlayError> {
        self.texts.push(text.to_owned());
        Ok(())
    }

    fn copy_file(&mut self, path: &Path) -> Result<(), OverlayError> {
        self.files.push(path.to_owned());
        Ok(())
    }
}

/// A fresh folder under the system temp folder, removed when dropped.
struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> Self {
        let path =
            std::env::temp_dir().join(format!("phramer-export-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir_all(&path).expect("temp folder");
        Self(path)
    }

    fn files(&self) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(&self.0)
            .expect("list")
            .filter_map(|entry| entry.ok()?.file_name().into_string().ok())
            .collect();
        names.sort();
        names
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn settings_saving_to(folder: &Path) -> Settings {
    let mut settings = Settings::default();
    settings.set_save_path(folder);
    settings
}

/// A gradient frame of `width` x `height`, BGRA.
fn frame(width: u32, height: u32) -> Vec<u8> {
    (0..width * height)
        .flat_map(|i| [(i % 251) as u8, (i % 241) as u8, (i % 239) as u8, 255])
        .collect()
}

fn image(width: u32, height: u32) -> CapturedImage {
    CapturedImage {
        width,
        height,
        bgra: frame(width, height),
    }
}

fn run(
    settings: &Settings,
    image: &CapturedImage,
    action: ExportAction,
    tasks: &CaptureTasks,
    handoff: &mut Recorded,
) -> phramer_overlay::ExportReport {
    let mut context = ExportContext {
        settings,
        time: &Fixed,
        handoff,
    };
    export(
        image,
        PhysicalRect::new(0, 0, image.width as i32, image.height as i32),
        action,
        tasks,
        &mut context,
    )
}

fn png_size(path: &Path) -> (u32, u32) {
    let file = std::fs::File::open(path).expect("open png");
    let reader = png::Decoder::new(std::io::BufReader::new(file))
        .read_info()
        .expect("png header");
    (reader.info().width, reader.info().height)
}

/// Width and height from a baseline JPEG's SOF0 segment.
fn jpeg_size(path: &Path) -> (u32, u32) {
    let bytes = std::fs::read(path).expect("read jpeg");
    assert_eq!(&bytes[..2], &[0xff, 0xd8], "not a JPEG");
    let mut at = 2;
    while at + 9 < bytes.len() {
        let (marker, length) = (
            bytes[at + 1],
            u16::from_be_bytes([bytes[at + 2], bytes[at + 3]]),
        );
        if marker == 0xc0 {
            let height = u16::from_be_bytes([bytes[at + 5], bytes[at + 6]]);
            let width = u16::from_be_bytes([bytes[at + 7], bytes[at + 8]]);
            return (u32::from(width), u32::from(height));
        }
        at += 2 + usize::from(length);
    }
    panic!("no SOF0 in {}", path.display());
}

#[test]
fn saves_with_the_pattern_and_numbers_collisions() {
    let folder = TempDir::new("pattern");
    let settings = settings_saving_to(&folder.0);
    let mut handoff = Recorded::default();
    for _ in 0..3 {
        let report = run(
            &settings,
            &image(8, 6),
            ExportAction::Save,
            &CaptureTasks::default(),
            &mut handoff,
        );
        assert!(
            report.notices.iter().all(|notice| !notice.error),
            "{report:?}"
        );
    }
    assert_eq!(
        folder.files(),
        [
            "2026-11-11_20-26.png",
            "2026-11-11_20-26_1.png",
            "2026-11-11_20-26_2.png"
        ]
    );
}

#[test]
fn the_report_names_the_file_for_show_in_folder() {
    let folder = TempDir::new("report");
    let settings = settings_saving_to(&folder.0);
    let report = run(
        &settings,
        &image(4, 4),
        ExportAction::Save,
        &CaptureTasks::default(),
        &mut Recorded::default(),
    );
    let saved = folder.0.join("2026-11-11_20-26.png");
    assert_eq!(report.saved.as_deref(), Some(saved.as_path()));
    assert_eq!(report.notices.len(), 1);
    assert_eq!(
        report.notices[0].text,
        format!("Capture saved as {}", saved.display())
    );
    assert_eq!(report.notices[0].path.as_deref(), Some(saved.as_path()));
}

#[test]
fn a_custom_pattern_with_slashes_and_colons_stays_one_file() {
    let folder = TempDir::new("custom");
    let mut settings = settings_saving_to(&folder.0);
    settings.set_filename_pattern("shot %H:%M/%S");
    run(
        &settings,
        &image(4, 4),
        ExportAction::Save,
        &CaptureTasks::default(),
        &mut Recorded::default(),
    );
    assert_eq!(folder.files(), ["shot 20-26\u{2044}57.png"]);
}

#[test]
fn the_selection_is_saved_at_its_physical_size_as_png_and_jpg() {
    // A 125% monitor: the selection is drawn in logical pixels and saved
    // in physical ones
    let monitor = Monitor::new(
        MonitorId::new("laptop"),
        PhysicalRect::new(0, 0, 1920, 1200),
        120,
        true,
    );
    let mut state =
        OverlayState::new(vec![monitor], 0, Vec::new(), OverlayConfig::default()).expect("state");
    let at = |x, y| LogicalPoint::new(x, y);
    state.handle(
        Event::PointerDown {
            monitor: 0,
            at: at(100, 100),
            button: PointerButton::Left,
            modifiers: Modifiers::NONE,
        },
        0,
    );
    for point in [at(200, 200), at(300, 250)] {
        state.handle(
            Event::PointerMove {
                monitor: 0,
                at: point,
                left_down: true,
                modifiers: Modifiers::NONE,
            },
            0,
        );
    }
    state.handle(
        Event::PointerUp {
            monitor: 0,
            at: at(300, 250),
            button: PointerButton::Left,
            modifiers: Modifiers::NONE,
        },
        0,
    );
    let effects = state.handle(
        Event::Key {
            key: Key::Char('s'),
            modifiers: Modifiers::CTRL,
        },
        0,
    );
    let Some(Effect::Close(Outcome::Export(done))) = effects.last().copied() else {
        panic!("no export in {effects:?}");
    };
    let region = done.region;
    let full = frame(1920, 1200);
    let cropped = CapturedImage::crop(
        &full,
        1920,
        (
            region.x as u32,
            region.y as u32,
            region.width as u32,
            region.height as u32,
        ),
    );
    assert_eq!((cropped.width, cropped.height), (250, 188));

    let folder = TempDir::new("size");
    let mut settings = settings_saving_to(&folder.0);
    run(
        &settings,
        &cropped,
        done.action,
        &CaptureTasks::default(),
        &mut Recorded::default(),
    );
    assert_eq!(png_size(&folder.0.join("2026-11-11_20-26.png")), (250, 188));

    settings
        .set_save_as_file_extension("jpg")
        .expect("jpg is writable");
    run(
        &settings,
        &cropped,
        done.action,
        &CaptureTasks::default(),
        &mut Recorded::default(),
    );
    assert_eq!(
        jpeg_size(&folder.0.join("2026-11-11_20-26.jpg")),
        (250, 188)
    );
}

#[test]
fn copy_puts_the_image_on_the_clipboard_and_save_after_copy_saves_it() {
    let folder = TempDir::new("copy");
    let mut settings = settings_saving_to(&folder.0);
    let mut handoff = Recorded::default();
    let report = run(
        &settings,
        &image(5, 3),
        ExportAction::Copy,
        &CaptureTasks::default(),
        &mut handoff,
    );
    assert_eq!(handoff.images, [(5, 3, None)]);
    assert!(folder.files().is_empty());
    assert_eq!(report.notices[0].text, "Capture saved to clipboard.");

    settings.set_save_after_copy(true);
    settings.set_use_jpg_for_clipboard(true);
    let mut handoff = Recorded::default();
    let report = run(
        &settings,
        &image(5, 3),
        ExportAction::Copy,
        &CaptureTasks::default(),
        &mut handoff,
    );
    assert_eq!(handoff.images, [(5, 3, Some(75))]);
    assert_eq!(folder.files(), ["2026-11-11_20-26.png"]);
    assert!(
        report.notices[0]
            .text
            .starts_with("Capture saved to clipboard. Capture saved as ")
    );
}

#[test]
fn copy_path_after_save_copies_a_windows_path() {
    let folder = TempDir::new("path");
    let mut settings = settings_saving_to(&folder.0);
    settings.set_copy_path_after_save(true);
    let mut handoff = Recorded::default();
    let report = run(
        &settings,
        &image(2, 2),
        ExportAction::Save,
        &CaptureTasks::default(),
        &mut handoff,
    );
    let saved = report.saved.expect("saved");
    assert_eq!(
        handoff.texts,
        [saved.display().to_string().replace('/', "\\")]
    );
    assert!(!handoff.texts[0].contains('/'));
}

#[test]
fn copy_as_file_saves_then_hands_over_the_file() {
    let folder = TempDir::new("copyfile");
    let settings = settings_saving_to(&folder.0);
    let mut handoff = Recorded::default();
    let report = run(
        &settings,
        &image(3, 3),
        ExportAction::CopyFile,
        &CaptureTasks::default(),
        &mut handoff,
    );
    let saved = folder.0.join("2026-11-11_20-26.png");
    assert_eq!(handoff.files, std::slice::from_ref(&saved));
    assert!(handoff.images.is_empty());
    assert_eq!(
        report.notices[0].text,
        format!("Copied as a file. Capture saved as {}", saved.display())
    );
}

#[test]
fn accept_runs_the_request_tasks() {
    let folder = TempDir::new("accept");
    let named = folder.0.join("picked.jpg");
    let tasks = CaptureTasks {
        copy: true,
        save: Some(SaveTarget::Path(named.clone())),
        ..CaptureTasks::default()
    };
    let mut handoff = Recorded::default();
    run(
        &Settings::default(),
        &image(7, 9),
        ExportAction::Accept,
        &tasks,
        &mut handoff,
    );
    // A file name keeps its own extension when none is configured
    assert_eq!(folder.files(), ["picked.jpg"]);
    assert_eq!(jpeg_size(&named), (7, 9));
    assert_eq!(handoff.images, [(7, 9, None)]);
}

#[test]
fn a_format_this_build_cannot_write_is_saved_as_png() {
    let folder = TempDir::new("webp");
    let mut settings = settings_saving_to(&folder.0);
    settings
        .set_save_as_file_extension("webp")
        .expect("webp was writable in 15.0.0");
    run(
        &settings,
        &image(3, 2),
        ExportAction::Save,
        &CaptureTasks::default(),
        &mut Recorded::default(),
    );
    assert_eq!(folder.files(), ["2026-11-11_20-26.png"]);
    assert_eq!(png_size(&folder.0.join("2026-11-11_20-26.png")), (3, 2));
}

#[test]
fn screenshot_path_follows_file_name_handler() {
    let folder = TempDir::new("paths");
    // A folder: the pattern names the file, png by default
    assert_eq!(
        screenshot_path(&folder.0, "", "name"),
        folder.0.join("name.png")
    );
    // A file: its extension is replaced by the configured one
    assert_eq!(
        screenshot_path(&folder.0.join("a.bmp"), "jpg", "ignored"),
        folder.0.join("a.jpg")
    );
    // A dot in the pattern is not an extension
    assert_eq!(
        screenshot_path(&folder.0, "png", "20.26"),
        folder.0.join("20.26.png")
    );
    std::fs::write(folder.0.join("20.26.png"), b"x").expect("write");
    assert_eq!(
        screenshot_path(&folder.0, "png", "20.26"),
        folder.0.join("20.26_1.png")
    );
}

#[cfg(windows)]
#[test]
fn the_c_runtime_formats_like_15_0_0() {
    use phramer_overlay::{LocalTime, expand_pattern};
    let time = LocalTime::at(2026, 11, 11, 20, 26, 57);
    assert_eq!(expand_pattern("%F_%H-%M", &time), "2026-11-11_20-26");
    assert_eq!(expand_pattern("%a %b %d %j %y", &time), "Wed Nov 11 315 26");
    assert_eq!(expand_pattern("%T", &time), "20-26-57");
    assert_eq!(expand_pattern("%D", &time), "11\u{2044}11\u{2044}26");
    assert_eq!(expand_pattern("%I%p", &time), "08PM");
}
