//! Windows.Media.Ocr on an image of known text: rendered by the annotation
//! engine in Inter, the overlay's bundled font. Ignored by default: it
//! needs an OCR language pack installed, which a build machine may lack.
//!
//! ```text
//! cargo test -p phramer-ocr --test live -- --ignored --nocapture
//! ```

#![cfg(windows)]

use phramer_annotate::{
    BaseImage, Color, Command, Document, Editor, FontLibrary, Point, RenderOptions, Shape, Size,
    TextAlign, TextFont, render,
};
use phramer_ocr::{
    OcrEffort, OcrStatus, OcrTextLayout, RgbaImage, assemble_text, available_languages,
    order_lines, read_text, recognize, recognize_capture, run_pipeline,
};

/// Inter Variable, as the overlay bundles it.
const INTER: &[u8] = include_bytes!("../../phramer-overlay/assets/InterVariable.ttf");

/// `lines` drawn black on white in 20 pt Inter.
fn rendered(lines: &str) -> RgbaImage {
    rendered_at(lines, 900, 260, 12)
}

/// `lines` drawn black on white in Inter at `size` + 8 pt on a `width` x
/// `height` page.
fn rendered_at(lines: &str, width: u32, height: u32, size: u32) -> RgbaImage {
    let mut fonts = FontLibrary::new();
    fonts
        .add("Inter", false, false, INTER.to_vec())
        .expect("Inter loads");
    let mut editor = Editor::new(Document::new(Size::new(width, height)));
    editor
        .apply(Command::Add {
            color: Color::BLACK,
            size,
            shape: Shape::Text {
                top_left: Point::new(20, 20),
                text: lines.into(),
                font: TextFont {
                    family: "Inter".into(),
                    ..TextFont::default()
                },
                align: TextAlign::Left,
            },
        })
        .expect("text placed");
    let white = [255u8, 255, 255, 255].repeat((width * height) as usize);
    let base = BaseImage::new(width, height, &white).expect("base image");
    let image =
        render(editor.document(), base, &fonts, &RenderOptions::default()).expect("rendered");
    RgbaImage {
        width: image.bounds.width,
        height: image.bounds.height,
        rgba: image.rgba,
    }
}

#[test]
#[ignore = "live: needs an OCR language pack"]
fn reads_rendered_text() {
    let languages = available_languages();
    println!("OCR languages: {languages:?}");
    if languages.is_empty() {
        println!("skipped: no OCR language is installed");
        return;
    }
    let image = rendered("The quick brown fox\njumps over 42 lazy dogs");
    let text = read_text(&image, "", OcrTextLayout::Plain).expect("text found");
    println!("recognised:\n{text}");
    assert_eq!(text, "The quick brown fox\njumps over 42 lazy dogs");

    let joined = read_text(&image, "", OcrTextLayout::Join).expect("text found");
    assert_eq!(joined, "The quick brown fox jumps over 42 lazy dogs");
}

#[test]
#[ignore = "live: needs an OCR language pack"]
fn a_blank_image_has_no_text() {
    if available_languages().is_empty() {
        println!("skipped: no OCR language is installed");
        return;
    }
    let blank = RgbaImage {
        width: 300,
        height: 100,
        rgba: [255u8, 255, 255, 255].repeat(300 * 100),
    };
    assert_eq!(
        read_text(&blank, "", OcrTextLayout::Preserve),
        Err(phramer_ocr::OcrStatus::NoTextFound)
    );
}

#[test]
#[ignore = "live: needs an OCR language pack"]
fn an_oversized_image_is_read_in_tiles() {
    if available_languages().is_empty() {
        println!("skipped: no OCR language is installed");
        return;
    }
    // Sixty short lines of small (12 pt) text: the glyphs want about a
    // 2.5x upscale, which makes the page taller than the 1600 pixel limit
    // set here, so passes B and C are split into tiles
    let expected: Vec<String> = (1..=60)
        .map(|n| format!("Line {n:02} alpha bravo {}", n * 7))
        .collect();
    let image = rendered_at(&expected.join("
"), 300, 1300, 4);
    let result = run_pipeline(&image, "", 1600, OcrEffort::Normal, recognize);
    println!("{}", result.diagnostics);
    assert_eq!(result.status, OcrStatus::Ok);
    assert!(
        result.diagnostics.lines().any(|line| line.contains("pass B")
            && !line.contains(" 1 tile(s)")),
        "pass B was not tiled:
{}",
        result.diagnostics
    );
    let text = assemble_text(&order_lines(&result.lines), OcrTextLayout::Plain);
    println!("recognised:
{text}");
    let found: Vec<&str> = text.lines().collect();
    let read = expected
        .iter()
        .filter(|line| found.contains(&line.as_str()))
        .count();
    println!("{read} of {} lines read exactly", expected.len());
    assert!(read >= 55, "only {read} lines read exactly");
    // Seams stitched: no line twice
    for line in &expected {
        assert!(
            found.iter().filter(|seen| **seen == line.as_str()).count() <= 1,
            "{line} read twice"
        );
    }
}

#[test]
#[ignore = "live: needs an OCR language pack"]
fn trying_harder_runs_every_pass() {
    if available_languages().is_empty() {
        println!("skipped: no OCR language is installed");
        return;
    }
    let image = rendered("The quick brown fox
jumps over 42 lazy dogs");
    let result = recognize_capture(&image, "", OcrEffort::High, None);
    println!("{}", result.diagnostics);
    assert_eq!(result.status, OcrStatus::Ok);
    assert!(result.diagnostics.contains("pass B"));
    assert!(result.diagnostics.contains("pass C (binarized)"));
    assert_eq!(
        assemble_text(&result.lines, OcrTextLayout::Plain),
        "The quick brown fox
jumps over 42 lazy dogs"
    );
}

/// Title the test's console window carries, to find it by.
const CONSOLE_TITLE: &str = "PhramerUiaTest";

/// The visible console window whose title holds [`CONSOLE_TITLE`].
fn test_console() -> Option<windows::Win32::Foundation::HWND> {
    use windows::Win32::Foundation::{HWND, LPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowTextW, IsWindowVisible,
    };
    use windows::core::BOOL;

    unsafe extern "system" fn collect(window: HWND, found: LPARAM) -> BOOL {
        // SAFETY: `found` is the `&mut Vec` passed below, alive for the
        // whole enumeration.
        unsafe { (*(found.0 as *mut Vec<HWND>)).push(window) };
        true.into()
    }
    let mut all: Vec<HWND> = Vec::new();
    // SAFETY: `collect` only pushes into `all`, which outlives the call.
    let _ = unsafe { EnumWindows(Some(collect), LPARAM(&raw mut all as isize)) };
    all.into_iter().find(|window| {
        let mut class = [0u16; 64];
        let mut title = [0u16; 256];
        // SAFETY: plain queries; the buffers hold the lengths given.
        let (class_length, title_length, visible) = unsafe {
            (
                GetClassNameW(*window, &mut class),
                GetWindowTextW(*window, &mut title),
                IsWindowVisible(*window).as_bool(),
            )
        };
        visible
            && String::from_utf16_lossy(&class[..class_length.max(0) as usize]) == "ConsoleWindowClass"
            && String::from_utf16_lossy(&title[..title_length.max(0) as usize]).contains(CONSOLE_TITLE)
    })
}

#[test]
#[ignore = "live: opens a console window and reads its text through UI Automation"]
fn the_exact_text_path_reads_a_window() {
    use std::os::windows::process::CommandExt;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::UI::WindowsAndMessaging::GetWindowRect;
    /// `CREATE_NEW_CONSOLE`: a console window of its own.
    const CREATE_NEW_CONSOLE: u32 = 0x10;

    // The console has a UI Automation text provider (Windows Sandbox has no
    // Notepad)
    let file = std::env::temp_dir().join(format!("phramer-uia-{}.txt", std::process::id()));
    std::fs::write(&file, "The quick brown fox\r\njumps over 42 lazy dogs\r\n").unwrap();
    // Written to CON, not the inherited (redirected) output; ping waits,
    // since pause and timeout need the console's input
    let mut console = std::process::Command::new("cmd.exe")
        .raw_arg(format!(
            "/c \"title {CONSOLE_TITLE} & type \"{}\" > CON & ping -n 60 127.0.0.1 > nul\"",
            file.display()
        ))
        .creation_flags(CREATE_NEW_CONSOLE)
        .stdin(std::process::Stdio::null())
        .spawn()
        .expect("start a console");
    let mut window = None;
    for _ in 0..50 {
        window = test_console();
        if window.is_some() {
            break;
        }
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    let window = window.expect("the console's window");
    std::thread::sleep(std::time::Duration::from_secs(1));
    let mut rect = RECT::default();
    // SAFETY: `rect` is a valid out-parameter; `window` is a live window.
    unsafe { GetWindowRect(window, &mut rect) }.expect("its rectangle");
    let selection = phramer_ocr::Rect::new(
        f64::from(rect.left),
        f64::from(rect.top),
        f64::from(rect.right - rect.left),
        f64::from(rect.bottom - rect.top),
    );
    let result = phramer_ocr::uia::read_window_text(selection);
    let _ = std::process::Command::new("taskkill")
        .args(["/F", "/T", "/PID", &console.id().to_string()])
        .output();
    let _ = console.wait();
    let _ = std::fs::remove_file(&file);
    println!("{:?}: {:?}", result.status, result.full_text());
    assert_eq!(result.status, OcrStatus::Ok);
    // Console rows are padded with spaces to the window's width
    let lines: Vec<&str> = result.lines.iter().map(|line| line.text.trim_end()).collect();
    assert_eq!(lines, ["The quick brown fox", "jumps over 42 lazy dogs"]);
    // And it agrees with what recognition would read
    assert!(phramer_ocr::texts_agree(
        "The quick brown fox jumps over 42 lazy dogs",
        &result.full_text()
    ));
}
