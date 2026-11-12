//! Compatibility of the settings module with files written by the C++
//! builds (14.1.x and 15.0.0).

use std::fs;
use std::path::PathBuf;

use phramer_core::config::{ButtonType, CaptureRegionMode, Color, Settings};

fn manifest_path(relative: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative)
}

fn read(relative: &str) -> Vec<u8> {
    let path = manifest_path(relative);
    fs::read(&path).unwrap_or_else(|error| panic!("cannot read {}: {error}", path.display()))
}

fn fixture(name: &str) -> Vec<u8> {
    read(&format!("tests/fixtures/{name}"))
}

fn text(settings: &Settings) -> String {
    String::from_utf8(settings.to_bytes()).expect("fixtures are UTF-8")
}

/// The sample file with its entries switched on: the shipped file comments
/// every line out with a single `;` (explanations use `;;`).
fn example_uncommented() -> String {
    let example =
        String::from_utf8(read("../../phramer.example.ini")).expect("the example is UTF-8");
    example
        .lines()
        .map(|line| match line.strip_prefix(';') {
            Some(entry) if !entry.starts_with(';') => entry,
            _ => line,
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn opaque(r: u8, g: u8, b: u8) -> Option<Color> {
    Some(Color::rgb(r, g, b))
}

#[test]
fn example_file_as_shipped_is_all_comments() {
    let bytes = read("../../phramer.example.ini");
    let settings = Settings::parse(&bytes);
    assert_eq!(settings.to_bytes(), bytes);
    // Nothing is set, so everything reads as the default.
    assert_eq!(settings.ui_color(), Color::rgb(56, 153, 194));
    assert!(settings.save_path_fixed());
    assert_eq!(
        settings.shortcut("TYPE_GRAB_COLOR").as_deref(),
        Some("Alt+g")
    );
}

#[test]
fn example_file_values_load() {
    let source = example_uncommented();
    let settings = Settings::parse(source.as_bytes());
    assert_eq!(text(&settings), source);

    assert_eq!(settings.ui_color(), Color::rgb(0x74, 0x00, 0x96));
    assert_eq!(settings.contrast_ui_color(), Color::rgb(0x27, 0x00, 0x32));
    assert_eq!(settings.draw_color(), Color::rgb(0xff, 0x00, 0x00));
    assert!(!settings.save_path_fixed());
    assert_eq!(settings.save_as_file_extension(), "png");
    assert_eq!(settings.ui_language(), "auto");
    assert_eq!(settings.ignore_update_to_version(), "");
    assert_eq!(settings.filename_pattern(), "%F_%H-%M");
    assert_eq!(settings.draw_thickness(), 3);
    assert_eq!(settings.draw_font_size(), 8);
    assert_eq!(settings.draw_marker_size(), 5);
    assert_eq!(settings.contrast_opacity(), 190);
    assert_eq!(settings.jpeg_quality(), 75);
    assert!(settings.show_desktop_notification());
    assert!(settings.show_startup_launch_message());
    assert!(!settings.predefined_color_palette_large());
    assert_eq!(
        settings.user_colors(),
        [
            None,
            opaque(0x80, 0x00, 0x00),
            opaque(0xff, 0x00, 0x00),
            opaque(0xff, 0xff, 0x00),
            opaque(0x00, 0xff, 0x00),
            opaque(0x00, 0x80, 0x00),
            opaque(0x00, 0xff, 0xff),
            opaque(0x00, 0x00, 0xff),
            opaque(0xff, 0x00, 0xff),
            opaque(0x80, 0x00, 0x80),
        ]
    );
    // The sample toolbar names buttons 7 and 13, which the Windows build does
    // not have, so the default toolbar applies.
    assert_eq!(settings.buttons(), Settings::default().buttons());

    assert_eq!(
        settings.shortcut("TYPE_REDO").as_deref(),
        Some("Ctrl+Shift+z")
    );
    assert_eq!(
        settings.shortcut("TYPE_COMMIT_CURRENT_TOOL").as_deref(),
        Some("Ctrl+Return")
    );
    assert_eq!(settings.shortcut("TYPE_GRAB_COLOR").as_deref(), Some("g"));
    assert_eq!(settings.shortcut("TYPE_PIN").as_deref(), Some(""));
}

#[test]
fn example_file_keeps_unknown_keys_when_a_value_changes() {
    let source = example_uncommented();
    // Keys this build does not know: none of them is an error.
    for unknown in [
        "copyAndCloseAfterUpload=true",
        "useX11LegacyScreenshot=false",
        "TYPE_IMAGEUPLOADER=Return",
    ] {
        assert!(source.contains(unknown), "{unknown} is in the sample");
    }
    let mut settings = Settings::parse(source.as_bytes());
    settings.set_show_help(false);
    assert_eq!(
        text(&settings),
        source.replace("showHelp=true", "showHelp=false")
    );
}

#[test]
fn v14_1_style_file_loads() {
    let bytes = fixture("v14.1-style.ini");
    let settings = Settings::parse(&bytes);
    assert_eq!(settings.to_bytes(), bytes);

    assert!(settings.antialiasing_pin_zoom());
    assert!(!settings.copy_url_after_upload());
    assert!(settings.show_desktop_notification());
    assert!(!settings.show_help());
    assert!(!settings.startup_launch());
    assert_eq!(settings.contrast_opacity(), 120);
    assert_eq!(settings.jpeg_quality(), 90);
    assert_eq!(settings.undo_limit(), 250);
    assert_eq!(settings.upload_history_max(), 0);
    assert_eq!(settings.draw_font_size(), 12);
    assert_eq!(settings.ui_language(), "de");
    assert_eq!(settings.welcome_tour_shown_for(), "14.1.7");
    assert_eq!(settings.filename_pattern(), "shot_%Y-%m-%d_%H.%M.%S");
    assert_eq!(settings.font_family(), "Consolas, Courier New;fallback");
    assert_eq!(settings.save_as_file_extension(), "jpg");
    assert_eq!(settings.contrast_ui_color(), Color::rgb(0x27, 0x00, 0x32));
    assert_eq!(settings.ui_color(), Color::rgb(0x48, 0x3d, 0x8b));
    // Eight hex digits are RRGGBBAA.
    assert_eq!(
        settings.draw_color(),
        Color {
            r: 0xff,
            g: 0x00,
            b: 0x00,
            a: 0x80
        }
    );
    assert_eq!(
        settings.user_colors(),
        [
            None,
            opaque(0x80, 0, 0),
            opaque(0xff, 0, 0),
            opaque(0xff, 0xff, 0),
            opaque(0, 0xff, 0)
        ]
    );
    assert_eq!(
        settings.buttons(),
        [
            ButtonType::Pencil,
            ButtonType::Arrow,
            ButtonType::Rectangle,
            ButtonType::Text,
            ButtonType::Copy,
            ButtonType::Save
        ]
    );

    // Keys added after 14.1.x are absent and take the current defaults.
    assert!(settings.show_tool_labels());
    assert!(!settings.video_capture_enabled());
    // Per-tool sizes did not exist yet: they follow the shared thickness.
    assert_eq!(settings.draw_thickness(), 5);
    assert_eq!(settings.draw_pencil_size(), 5);
    assert_eq!(settings.draw_selection_size(), 5);

    assert_eq!(
        settings.shortcut("TAKE_SCREENSHOT").as_deref(),
        Some("Ctrl+Print")
    );
    assert_eq!(settings.shortcut("TYPE_ACCEPT").as_deref(), Some("Return"));
    assert_eq!(
        settings.shortcut("TYPE_COPY").as_deref(),
        Some("Ctrl+Shift+c")
    );
    assert_eq!(settings.shortcut("TYPE_PENCIL").as_deref(), Some(""));
    assert_eq!(
        settings.shortcut("TYPE_SAVE").as_deref(),
        Some("Ctrl+Alt+s")
    );
    // TYPE_COPY_FILE arrived in 15.0.0 with the default this user already
    // gave TYPE_COPY; the explicit binding wins.
    assert_eq!(settings.shortcut("TYPE_COPY_FILE").as_deref(), Some(""));
}

#[test]
fn v14_1_style_file_derives_capture_region_mode_from_the_legacy_flag() {
    let settings = Settings::parse(&fixture("v14.1-style.ini"));
    assert!(settings.capture_active_monitor());
    assert_eq!(
        settings.capture_region_mode(),
        CaptureRegionMode::ActiveMonitor
    );

    let unticked = Settings::parse(b"[General]\ncaptureActiveMonitor=false\n");
    assert_eq!(
        unticked.capture_region_mode(),
        CaptureRegionMode::SelectMonitor
    );

    // A user who never chose gets the current default.
    let never_chose = Settings::parse(b"[General]\nshowHelp=false\n");
    assert_eq!(
        never_chose.capture_region_mode(),
        CaptureRegionMode::AllMonitors
    );

    // Once the new key exists it decides, whatever the legacy flag says.
    let both = Settings::parse(b"[General]\ncaptureActiveMonitor=true\ncaptureRegionMode=0\n");
    assert_eq!(both.capture_region_mode(), CaptureRegionMode::SelectMonitor);
}

#[test]
fn v15_0_style_file_loads() {
    let bytes = fixture("v15.0-style.ini");
    assert!(
        bytes.windows(2).any(|pair| pair == b"\r\n"),
        "the fixture keeps Windows line endings"
    );
    let settings = Settings::parse(&bytes);
    assert_eq!(settings.to_bytes(), bytes);

    assert!(settings.auto_open_in_editor());
    assert!(!settings.check_for_updates());
    assert!(!settings.show_tool_labels());
    assert!(settings.video_capture_enabled());
    assert!(settings.predefined_color_palette_large());
    assert_eq!(
        settings.capture_region_mode(),
        CaptureRegionMode::ActiveMonitor
    );
    assert_eq!(settings.ocr_language(), "ja-JP");
    assert_eq!(settings.ocr_text_layout(), 2);
    assert_eq!(settings.shape_style(), 2);
    assert_eq!(settings.save_as_file_extension(), "webp");
    assert_eq!(settings.welcome_tour_shown_for(), "15.0.0");
    assert_eq!(settings.ui_color(), Color::rgb(0x38, 0x99, 0xc2));
    assert_eq!(settings.contrast_ui_color(), Color::rgb(0x11, 0x3c, 0x4f));
    // `@@` escapes a leading `@`; `@Invalid()` is a present key with no value.
    assert_eq!(settings.upload_client_secret(), "@secret");
    assert_eq!(settings.ignore_update_to_version(), "");
    assert_eq!(
        settings.buttons(),
        [
            ButtonType::Shape,
            ButtonType::MoveObject,
            ButtonType::CopyFile,
            ButtonType::Ocr
        ]
    );
    // No palette stored: the large default applies because of the flag above.
    assert_eq!(settings.user_colors().len(), 17);
    assert_eq!(settings.draw_arrow_size(), 9);
    assert_eq!(settings.draw_pencil_size(), 4);

    assert_eq!(
        settings.shortcut("CAPTURE_MODE_VIDEO").as_deref(),
        Some("3")
    );
    assert_eq!(
        settings.shortcut("RECORD_VIDEO").as_deref(),
        Some("Ctrl+Alt+r")
    );
    assert_eq!(
        settings.shortcut("TYPE_COPY_FILE").as_deref(),
        Some("Ctrl+Alt+c")
    );
    assert_eq!(settings.shortcut("TYPE_MOVE_OBJECT").as_deref(), Some("g"));
}

#[test]
fn corrupt_values_fall_back_to_defaults_and_stay_in_the_file() {
    let bytes = fixture("corrupt-values.ini");
    let settings = Settings::parse(&bytes);
    let defaults = Settings::default();

    // Bool
    assert_eq!(settings.show_help(), defaults.show_help());
    assert_eq!(
        settings.show_desktop_notification(),
        defaults.show_desktop_notification()
    );
    // String
    assert_eq!(settings.ui_language(), "auto");
    // Color
    assert_eq!(settings.ui_color(), defaults.ui_color());
    assert_eq!(settings.contrast_ui_color(), defaults.contrast_ui_color());
    assert_eq!(settings.draw_color(), defaults.draw_color());
    // BoundedInt
    assert_eq!(settings.undo_limit(), 100);
    assert_eq!(settings.contrast_opacity(), 190);
    assert_eq!(settings.jpeg_quality(), 75);
    assert_eq!(
        settings.capture_region_mode(),
        CaptureRegionMode::AllMonitors
    );
    // LowerBoundedInt
    assert_eq!(settings.draw_thickness(), 3);
    assert_eq!(settings.draw_font_size(), 8);
    // ExistingDir
    assert_eq!(settings.save_path(), defaults.save_path());
    assert!(settings.save_path().is_some_and(|dir| dir.is_dir()));
    // FilenamePattern
    assert_eq!(settings.filename_pattern(), "%F_%H-%M");
    // ButtonList
    assert_eq!(settings.buttons(), defaults.buttons());
    // UserColors
    assert_eq!(settings.user_colors(), defaults.user_colors());
    // SaveFileExtension
    assert_eq!(settings.save_as_file_extension(), "");
    // KeySequence
    assert_eq!(settings.shortcut("TYPE_COPY").as_deref(), Some("Ctrl+c"));
    assert_eq!(settings.shortcut("TYPE_SAVE").as_deref(), Some("Ctrl+s"));

    // Reading never repairs or drops anything.
    assert_eq!(settings.to_bytes(), bytes);
}

#[test]
fn more_corrupt_palettes_and_toolbars() {
    let defaults = Settings::default();
    let bad_colour = Settings::parse(b"userColors=picker, #ff0000, nope\n");
    assert_eq!(bad_colour.user_colors(), defaults.user_colors());

    // Not a serialized list at all: the C++ reads this as an empty toolbar.
    let not_a_list = Settings::parse(b"buttons=garbage\n");
    assert!(not_a_list.buttons().is_empty());
}

#[test]
fn unknown_keys_survive_edits_to_known_ones() {
    let bytes = fixture("v15.0-style.ini");
    let original = String::from_utf8(bytes.clone()).expect("fixtures are UTF-8");
    let mut settings = Settings::parse(&bytes);

    settings.set_show_tool_labels(true);
    settings.set_ui_color(Color::rgb(1, 2, 3));
    settings.set_jpeg_quality(60).expect("in range");
    settings.set_buttons(&[ButtonType::Pencil]);
    settings
        .set_shortcut("TYPE_TEXT", "Ctrl+T")
        .expect("free sequence");
    settings.set_capture_region_mode(CaptureRegionMode::AllMonitors);

    let written = text(&settings);
    let reread = Settings::parse(written.as_bytes());
    assert!(reread.show_tool_labels());
    assert_eq!(reread.ui_color(), Color::rgb(1, 2, 3));
    assert_eq!(reread.jpeg_quality(), 60);
    assert_eq!(reread.buttons(), [ButtonType::Pencil]);
    assert_eq!(reread.shortcut("TYPE_TEXT").as_deref(), Some("Ctrl+t"));
    assert_eq!(reread.capture_region_mode(), CaptureRegionMode::AllMonitors);

    // Every line that was not assigned is still there, byte for byte, in the
    // same order; nothing was removed or renamed.
    let assigned = [
        "showToolLabels=",
        "uiColor=",
        "buttons=",
        "captureRegionMode=",
    ];
    let untouched: Vec<&str> = original
        .lines()
        .filter(|line| !assigned.iter().any(|key| line.starts_with(key)))
        .collect();
    let mut remaining = written.lines();
    for line in &untouched {
        assert!(
            remaining.any(|candidate| candidate == *line),
            "line `{line}` was lost or moved"
        );
    }
    for key in original
        .lines()
        .filter_map(|line| line.split_once('='))
        .map(|(key, _)| key)
    {
        assert!(
            written
                .lines()
                .any(|line| line.split_once('=').is_some_and(|(name, _)| name == key)),
            "{key}"
        );
    }
    for unknown in [
        "futureSetting=@Variant(\\0\\0\\0\\x43\\x1\\xff\\xff\\0\\0\\0\\0\\0\\0\\0\\0)",
        "windowTitleNote=Größe 日本語",
        "[WindowState]",
        "blob=@ByteArray(\\x1\\xd9\\xd0\\xcb\\0\\x3\\0\\0)",
        "lastPos=@Point(-1920 40)",
        "lastRect=@Rect(100 120 800 600)",
        "lastSize=@Size(1280 720)",
        "nothing=@Invalid()",
        "recent=one, \"two, three\", four",
    ] {
        assert!(written.contains(unknown), "{unknown}");
    }
    // New keys went into their own sections, not after the unknown one.
    let general_end = written
        .find("[Shortcuts]")
        .expect("the Shortcuts section is kept");
    assert!(written[..general_end].contains("jpegQuality=60"));
    let shortcuts_end = written
        .find("[WindowState]")
        .expect("the unknown section is kept");
    assert!(written[general_end..shortcuts_end].contains("TYPE_TEXT=Ctrl+t"));
}
