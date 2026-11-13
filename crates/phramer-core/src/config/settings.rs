//! Typed access to the settings file shared with the C++ builds.
//!
//! Every key, default and validation rule mirrors `confighandler.cpp` and
//! `valuehandler.cpp` of v15.0.0 for the Windows build. A stored value that
//! fails its check reads as the default; it is never rewritten or removed,
//! and keys this module does not know are carried through untouched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use super::buttons::{self, ButtonType};
use super::check::Rule;
use super::color::Color;
use super::error::ConfigError;
use super::ini::IniDocument;
use super::keyseq;
use super::path;
use super::variant::Variant;

const SHORTCUTS_GROUP: &str = "Shortcuts";
const FILENAME_PATTERN_DEFAULT: &str = "%F_%H-%M";
/// Entries a colour palette may hold, the picker included.
const USER_COLORS_RANGE: std::ops::RangeInclusive<usize> = 3..=17;

/// `QImageWriter::supportedImageFormats()` of the shipped Windows build
/// (Qt 6 with the `qtimageformats` module).
const SAVE_EXTENSIONS: &[&str] = &[
    "bmp", "cur", "icns", "ico", "jpeg", "jpg", "pbm", "pgm", "png", "ppm", "tif", "tiff", "wbmp",
    "webp", "xbm", "xpm",
];

/// `ColorPickerWidget::defaultSmallColorPalette`; `None` is the picker.
const SMALL_PALETTE: &[Option<Color>] = &[
    None,
    Some(Color::rgb(0xff, 0xff, 0xff)),
    Some(Color::rgb(0x00, 0x00, 0x00)),
    Some(Color::rgb(0x80, 0x00, 0x00)),
    Some(Color::rgb(0xff, 0x00, 0x00)),
    Some(Color::rgb(0xff, 0xff, 0x00)),
    Some(Color::rgb(0x00, 0xff, 0x00)),
    Some(Color::rgb(0x00, 0x80, 0x00)),
    Some(Color::rgb(0x00, 0xff, 0xff)),
    Some(Color::rgb(0x00, 0x00, 0xff)),
    Some(Color::rgb(0xff, 0x00, 0xff)),
    Some(Color::rgb(0x80, 0x00, 0x80)),
];

/// `ColorPickerWidget::defaultLargeColorPalette`.
const LARGE_PALETTE: &[Option<Color>] = &[
    None,
    Some(Color::rgb(0xff, 0xff, 0xff)),
    Some(Color::rgb(0xff, 0x00, 0x00)),
    Some(Color::rgb(0x00, 0xff, 0x00)),
    Some(Color::rgb(0x00, 0x00, 0xff)),
    Some(Color::rgb(0x00, 0x00, 0x00)),
    Some(Color::rgb(0x80, 0x00, 0x00)),
    Some(Color::rgb(0x00, 0x80, 0x00)),
    Some(Color::rgb(0x00, 0x00, 0x80)),
    Some(Color::rgb(0x80, 0x80, 0x80)),
    Some(Color::rgb(0x00, 0xff, 0xff)),
    Some(Color::rgb(0xff, 0x00, 0xff)),
    Some(Color::rgb(0xff, 0xff, 0x00)),
    Some(Color::rgb(0xc0, 0xc0, 0xc0)),
    Some(Color::rgb(0x00, 0x80, 0x80)),
    Some(Color::rgb(0x80, 0x00, 0x80)),
    Some(Color::rgb(0x80, 0x80, 0x00)),
];

/// Shortcut names and their default key sequences (`recognizedShortcuts`).
/// The C++ spells the last default `Delete`; `QKeySequence` renders it `Del`.
pub(super) const SHORTCUTS: &[(&str, &str)] = &[
    ("TYPE_PENCIL", "P"),
    ("TYPE_DRAWER", "D"),
    ("TYPE_ARROW", "A"),
    ("TYPE_SELECTION", "S"),
    ("TYPE_RECTANGLE", "R"),
    ("TYPE_CIRCLE", "C"),
    ("TYPE_MARKER", "M"),
    ("TYPE_MOVESELECTION", "Ctrl+M"),
    ("TYPE_UNDO", "Ctrl+Z"),
    ("TYPE_COPY", "Ctrl+C"),
    ("TYPE_COPY_FILE", "Ctrl+Shift+C"),
    ("TYPE_SAVE", "Ctrl+S"),
    ("TYPE_ACCEPT", "Return"),
    ("TYPE_EXIT", "Ctrl+Q"),
    ("TYPE_CANCEL", "Ctrl+Backspace"),
    ("TYPE_OPEN_APP", "Ctrl+O"),
    ("TYPE_PIXELATE", "B"),
    ("TYPE_INVERT", "I"),
    ("TYPE_REDO", "Ctrl+Shift+Z"),
    ("TYPE_TEXT", "T"),
    ("TYPE_TOGGLE_PANEL", "Space"),
    ("TYPE_GRAB_COLOR", "Alt+G"),
    ("TYPE_MOVE_OBJECT", "G"),
    ("TYPE_RESIZE_LEFT", "Shift+Left"),
    ("TYPE_RESIZE_RIGHT", "Shift+Right"),
    ("TYPE_RESIZE_UP", "Shift+Up"),
    ("TYPE_RESIZE_DOWN", "Shift+Down"),
    ("TYPE_SYM_RESIZE_LEFT", "Ctrl+Shift+Left"),
    ("TYPE_SYM_RESIZE_RIGHT", "Ctrl+Shift+Right"),
    ("TYPE_SYM_RESIZE_UP", "Ctrl+Shift+Up"),
    ("TYPE_SYM_RESIZE_DOWN", "Ctrl+Shift+Down"),
    ("TYPE_SELECT_ALL", "Ctrl+A"),
    ("TYPE_MOVE_LEFT", "Left"),
    ("TYPE_MOVE_RIGHT", "Right"),
    ("TYPE_MOVE_UP", "Up"),
    ("TYPE_MOVE_DOWN", "Down"),
    ("TYPE_COMMIT_CURRENT_TOOL", "Ctrl+Return"),
    ("TAKE_SCREENSHOT", "Meta+Shift+x"),
    ("TYPE_OCR", "O"),
    ("RECORD_VIDEO", ""),
    ("CAPTURE_MODE_SCREENSHOT", "1"),
    ("CAPTURE_MODE_VIDEO", "2"),
    ("TYPE_DELETE_CURRENT_TOOL", "Del"),
    ("TYPE_PIN", ""),
    ("TYPE_OPEN_IN_EDITOR", "E"),
    ("TYPE_SHAPE", ""),
    ("TYPE_SIZEINCREASE", ""),
    ("TYPE_SIZEDECREASE", ""),
    ("TYPE_CIRCLECOUNT", ""),
];

/// Multi-monitor behaviour of GUI captures (`captureRegionMode`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CaptureRegionMode {
    /// Ask which monitor to capture.
    SelectMonitor = 0,
    /// Capture the monitor under the cursor.
    ActiveMonitor = 1,
    /// Snip across all monitors.
    AllMonitors = 2,
}

/// The user's settings, backed by the INI file the C++ builds read and write.
#[derive(Debug, Clone, Default)]
pub struct Settings {
    pub(super) doc: IniDocument,
}

/// Generates the getter and setter for one key from its C++ value handler.
macro_rules! accessor {
    ($get:ident, $set:ident, $key:literal, Bool($default:expr)) => {
        #[doc = concat!("The `", $key, "` setting.")]
        pub fn $get(&self) -> bool {
            match self.stored_text($key).as_deref() {
                Some("true") => true,
                Some("false") => false,
                _ => $default,
            }
        }

        #[doc = concat!("Sets `", $key, "`.")]
        pub fn $set(&mut self, value: bool) {
            if value != self.$get() {
                self.write_text($key, if value { "true" } else { "false" });
            }
        }
    };
    ($get:ident, $set:ident, $key:literal, Text($default:expr)) => {
        #[doc = concat!("The `", $key, "` setting.")]
        pub fn $get(&self) -> String {
            self.stored_text($key)
                .unwrap_or_else(|| $default.to_owned())
        }

        #[doc = concat!("Sets `", $key, "`.")]
        pub fn $set(&mut self, value: &str) {
            if value != self.$get() {
                self.write_text($key, value);
            }
        }
    };
    ($get:ident, $set:ident, $key:literal, Rgb($r:expr, $g:expr, $b:expr)) => {
        #[doc = concat!("The `", $key, "` setting.")]
        pub fn $get(&self) -> Color {
            self.stored_text($key)
                .and_then(|text| Color::from_setting(&text))
                .unwrap_or(Color::rgb($r, $g, $b))
        }

        #[doc = concat!("Sets `", $key, "`.")]
        pub fn $set(&mut self, value: Color) {
            if value != self.$get() {
                self.write_text($key, &value.to_setting());
            }
        }
    };
    ($get:ident, $set:ident, $key:literal, Bounded($min:expr, $max:expr, $default:expr)) => {
        #[doc = concat!("The `", $key, "` setting (", $min, " to ", $max, ").")]
        pub fn $get(&self) -> i32 {
            self.read_int($key, $min, $max, $default)
        }

        #[doc = concat!("Sets `", $key, "`; fails outside ", $min, " to ", $max, ".")]
        pub fn $set(&mut self, value: i32) -> Result<(), ConfigError> {
            let unchanged = value == self.$get();
            self.write_int($key, $min, $max, value, unchanged)
        }
    };
    ($get:ident, $set:ident, $key:literal, AtLeast($min:expr, $default:expr)) => {
        #[doc = concat!("The `", $key, "` setting (", $min, " or more).")]
        pub fn $get(&self) -> i32 {
            self.read_int($key, $min, i32::MAX, $default)
        }

        #[doc = concat!("Sets `", $key, "`; fails below ", $min, ".")]
        pub fn $set(&mut self, value: i32) -> Result<(), ConfigError> {
            let unchanged = value == self.$get();
            self.write_int($key, $min, i32::MAX, value, unchanged)
        }
    };
    // A tool size that was split off `drawThickness`: while its own key is
    // absent it keeps following the shared value (`ConfigHandler::toolSize`).
    ($get:ident, $set:ident, $key:literal, ToolSize($min:expr, $default:expr)) => {
        #[doc = concat!("The `", $key, "` setting; follows `drawThickness` until it is first set.")]
        pub fn $get(&self) -> i32 {
            if self.doc.get($key).is_none() {
                return self.draw_thickness();
            }
            self.read_int($key, $min, i32::MAX, $default)
        }

        #[doc = concat!("Sets `", $key, "`; fails below ", $min, ".")]
        pub fn $set(&mut self, value: i32) -> Result<(), ConfigError> {
            let unchanged = self.doc.get($key).is_some() && value == self.$get();
            self.write_int($key, $min, i32::MAX, value, unchanged)
        }
    };
    // Accessors written by hand below.
    ($get:ident, $set:ident, $key:literal, Custom($rule:expr)) => {};
}

/// The `config --check` rule of one key's value handler.
macro_rules! rule {
    (Bool($default:expr)) => { Rule::Bool };
    (Text($default:expr)) => { Rule::Any };
    (Rgb($r:expr, $g:expr, $b:expr)) => { Rule::Color };
    (Bounded($min:expr, $max:expr, $default:expr)) => { Rule::Bounded($min, $max) };
    (AtLeast($min:expr, $default:expr)) => { Rule::AtLeast($min) };
    (ToolSize($min:expr, $default:expr)) => { Rule::AtLeast($min) };
    (Custom($rule:expr)) => { $rule };
}

/// Declares every key of the General section (`recognizedGeneralOptions`).
macro_rules! general_options {
    ($($get:ident / $set:ident: $key:literal => $kind:ident($($arg:expr),*);)*) => {
        #[cfg(test)]
        const GENERAL_KEYS: &[&str] = &[$($key),*];

        /// Each key with the rule `config --check` applies to its value.
        pub(super) const GENERAL_RULES: &[(&str, Rule)] = &[$(($key, rule!($kind($($arg),*)))),*];

        impl Settings {
            $(accessor!($get, $set, $key, $kind($($arg),*));)*
        }
    };
}

general_options! {
    show_help / set_show_help: "showHelp" => Bool(true);
    show_side_panel_button / set_show_side_panel_button: "showSidePanelButton" => Bool(true);
    show_desktop_notification / set_show_desktop_notification: "showDesktopNotification" => Bool(false);
    show_abort_notification / set_show_abort_notification: "showAbortNotification" => Bool(false);
    disabled_tray_icon / set_disabled_tray_icon: "disabledTrayIcon" => Bool(false);
    history_confirmation_to_delete / set_history_confirmation_to_delete: "historyConfirmationToDelete" => Bool(true);
    check_for_updates / set_check_for_updates: "checkForUpdates" => Bool(true);
    allow_multiple_gui_instances / set_allow_multiple_gui_instances: "allowMultipleGuiInstances" => Bool(false);
    show_magnifier / set_show_magnifier: "showMagnifier" => Bool(false);
    square_magnifier / set_square_magnifier: "squareMagnifier" => Bool(false);
    auto_close_idle_daemon / set_auto_close_idle_daemon: "autoCloseIdleDaemon" => Bool(false);
    startup_launch / set_startup_launch: "startupLaunch" => Bool(true);
    show_startup_launch_message / set_show_startup_launch_message: "showStartupLaunchMessage" => Bool(false);
    show_quit_prompt / set_show_quit_prompt: "showQuitPrompt" => Bool(false);
    show_editor_hint / set_show_editor_hint: "showEditorHint" => Bool(true);
    show_tool_labels / set_show_tool_labels: "showToolLabels" => Bool(true);
    copy_url_after_upload / set_copy_url_after_upload: "copyURLAfterUpload" => Bool(true);
    copy_path_after_save / set_copy_path_after_save: "copyPathAfterSave" => Bool(false);
    antialiasing_pin_zoom / set_antialiasing_pin_zoom: "antialiasingPinZoom" => Bool(true);
    use_jpg_for_clipboard / set_use_jpg_for_clipboard: "useJpgForClipboard" => Bool(false);
    upload_without_confirmation / set_upload_without_confirmation: "uploadWithoutConfirmation" => Bool(false);
    save_after_copy / set_save_after_copy: "saveAfterCopy" => Bool(false);
    save_path / set_save_path: "savePath" => Custom(Rule::ExistingDir);
    save_path_fixed / set_save_path_fixed: "savePathFixed" => Bool(true);
    save_as_file_extension / set_save_as_file_extension: "saveAsFileExtension" => Custom(Rule::SaveFileExtension);
    save_last_region / set_save_last_region: "saveLastRegion" => Bool(false);
    upload_history_max / set_upload_history_max: "uploadHistoryMax" => AtLeast(0, 25);
    undo_limit / set_undo_limit: "undoLimit" => Bounded(0, 999, 100);
    ui_language / set_ui_language: "uiLanguage" => Text("auto");
    ui_color / set_ui_color: "uiColor" => Rgb(56, 153, 194);
    contrast_ui_color / set_contrast_ui_color: "contrastUiColor" => Rgb(17, 60, 79);
    contrast_opacity / set_contrast_opacity: "contrastOpacity" => Bounded(0, 255, 190);
    buttons / set_buttons: "buttons" => Custom(Rule::ButtonList);
    filename_pattern / set_filename_pattern: "filenamePattern" => Custom(Rule::Any);
    draw_thickness / set_draw_thickness: "drawThickness" => AtLeast(1, 3);
    draw_pencil_size / set_draw_pencil_size: "drawPencilSize" => ToolSize(1, 3);
    draw_line_size / set_draw_line_size: "drawLineSize" => ToolSize(1, 3);
    draw_arrow_size / set_draw_arrow_size: "drawArrowSize" => ToolSize(1, 3);
    draw_circle_size / set_draw_circle_size: "drawCircleSize" => ToolSize(1, 3);
    draw_selection_size / set_draw_selection_size: "drawSelectionSize" => ToolSize(1, 3);
    highlight_outline_width / set_highlight_outline_width: "highlightOutlineWidth" => Bounded(0, 20, 2);
    draw_font_size / set_draw_font_size: "drawFontSize" => AtLeast(1, 8);
    draw_circle_counter_size / set_draw_circle_counter_size: "drawCircleCounterSize" => AtLeast(1, 1);
    draw_pixelate_size / set_draw_pixelate_size: "drawPixelateSize" => AtLeast(1, 2);
    draw_rectangle_size / set_draw_rectangle_size: "drawRectangleSize" => AtLeast(1, 1);
    draw_marker_size / set_draw_marker_size: "drawMarkerSize" => AtLeast(1, 5);
    draw_color / set_draw_color: "drawColor" => Rgb(255, 0, 0);
    user_colors / set_user_colors: "userColors" => Custom(Rule::UserColors(3, 17));
    ignore_update_to_version / set_ignore_update_to_version: "ignoreUpdateToVersion" => Text("");
    keep_open_app_launcher / set_keep_open_app_launcher: "keepOpenAppLauncher" => Bool(false);
    font_family / set_font_family: "fontFamily" => Text("");
    predefined_color_palette_large / set_predefined_color_palette_large: "predefinedColorPaletteLarge" => Bool(false);
    copy_on_double_click / set_copy_on_double_click: "copyOnDoubleClick" => Bool(false);
    upload_client_secret / set_upload_client_secret: "uploadClientSecret" => Text("313baf0c7b4d3ff");
    show_selection_geometry / set_show_selection_geometry: "showSelectionGeometry" => Bounded(0, 5, 4);
    show_selection_geometry_hide_time / set_show_selection_geometry_hide_time: "showSelectionGeometryHideTime" => AtLeast(0, 3000);
    jpeg_quality / set_jpeg_quality: "jpegQuality" => Bounded(0, 100, 75);
    reverse_arrow / set_reverse_arrow: "reverseArrow" => Bool(false);
    arrow_style / set_arrow_style: "arrowStyle" => Bounded(0, 1, 0);
    rectangle_fill_mode / set_rectangle_fill_mode: "rectangleFillMode" => Bounded(0, 1, 0);
    insecure_pixelate / set_insecure_pixelate: "insecurePixelate" => Bool(false);
    auto_open_in_editor / set_auto_open_in_editor: "autoOpenInEditor" => Bool(false);
    welcome_tour_shown_for / set_welcome_tour_shown_for: "welcomeTourShownFor" => Text("");
    welcome_tour_disabled / set_welcome_tour_disabled: "welcomeTourDisabled" => Bool(false);
    shape_kind / set_shape_kind: "shapeKind" => Bounded(0, 1, 0);
    shape_style / set_shape_style: "shapeStyle" => Bounded(0, 2, 1);
    ocr_join_wrapped_lines / set_ocr_join_wrapped_lines: "ocrJoinWrappedLines" => Bool(false);
    ocr_text_layout / set_ocr_text_layout: "ocrTextLayout" => Bounded(0, 2, 0);
    ocr_language / set_ocr_language: "ocrLanguage" => Text("");
    ignore_prnt_scr_forces_snipping / set_ignore_prnt_scr_forces_snipping: "ignorePrntScrForcesSnipping" => Bool(false);
    show_welcome_message / set_show_welcome_message: "showWelcomeMessage" => Bool(true);
    video_capture_enabled / set_video_capture_enabled: "videoCaptureEnabled" => Bool(false);
    capture_active_monitor / set_capture_active_monitor: "captureActiveMonitor" => Bool(false);
    capture_region_mode / set_capture_region_mode: "captureRegionMode" => Custom(Rule::Bounded(0, 2));
}

impl Settings {
    /// Reads settings from the bytes of an INI file.
    pub fn parse(bytes: &[u8]) -> Self {
        Self {
            doc: IniDocument::parse(bytes),
        }
    }

    /// Loads the settings file; a missing file yields the defaults.
    pub fn load(path: &Path) -> Result<Self, ConfigError> {
        match fs::read(path) {
            Ok(bytes) => Ok(Self::parse(&bytes)),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(source) => Err(ConfigError::Read {
                path: path.to_owned(),
                source,
            }),
        }
    }

    /// The file contents: the bytes that were loaded, with only the entries
    /// that were assigned since then rewritten.
    pub fn to_bytes(&self) -> Vec<u8> {
        self.doc.to_bytes()
    }

    /// Writes the settings file, replacing it in one step so a crash cannot
    /// leave it truncated.
    pub fn save(&self, path: &Path) -> Result<(), ConfigError> {
        let write_error = |source| ConfigError::Write {
            path: path.to_owned(),
            source,
        };
        if let Some(dir) = path.parent().filter(|dir| !dir.as_os_str().is_empty()) {
            fs::create_dir_all(dir).map_err(write_error)?;
        }
        let mut temp = path.as_os_str().to_owned();
        temp.push(".tmp");
        let temp = PathBuf::from(temp);
        fs::write(&temp, self.to_bytes()).map_err(write_error)?;
        fs::rename(&temp, path).map_err(write_error)
    }

    /// The `savePath` setting: the stored folder if it exists, otherwise the
    /// first of the Pictures, home and temporary folders that does.
    pub fn save_path(&self) -> Option<PathBuf> {
        self.stored_text("savePath")
            .filter(|text| !text.is_empty() && Path::new(text).is_dir())
            .map(PathBuf::from)
            .or_else(path::default_save_dir)
    }

    /// Sets `savePath`.
    pub fn set_save_path(&mut self, value: &Path) {
        if self.save_path().as_deref() != Some(value) {
            self.write_text("savePath", &value.to_string_lossy());
        }
    }

    /// The `saveAsFileExtension` setting without its leading dot; empty when
    /// unset or not a writable image format.
    pub fn save_as_file_extension(&self) -> String {
        self.stored_text("saveAsFileExtension")
            .and_then(|text| checked_extension(&text))
            .unwrap_or_default()
    }

    /// Sets `saveAsFileExtension` (`png` or `.png`); fails for a format the
    /// application cannot write.
    pub fn set_save_as_file_extension(&mut self, value: &str) -> Result<(), ConfigError> {
        let extension =
            checked_extension(value).ok_or_else(|| invalid("saveAsFileExtension", value))?;
        if extension != self.save_as_file_extension() {
            self.write_text("saveAsFileExtension", value);
        }
        Ok(())
    }

    /// The `filenamePattern` setting (`strftime` syntax).
    pub fn filename_pattern(&self) -> String {
        self.stored_text("filenamePattern")
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| FILENAME_PATTERN_DEFAULT.to_owned())
    }

    /// Sets `filenamePattern`.
    pub fn set_filename_pattern(&mut self, value: &str) {
        if value != self.filename_pattern() {
            self.write_text("filenamePattern", value);
        }
    }

    /// The `buttons` setting: the toolbar buttons in display order.
    ///
    /// A list naming an unknown button reads as the default toolbar. A value
    /// that is not a serialized list at all reads as an empty toolbar, which
    /// is what the C++ makes of it.
    pub fn buttons(&self) -> Vec<ButtonType> {
        match self.stored("buttons") {
            None => buttons::default_buttons(),
            Some(Variant::IntList(ids)) => {
                match ids
                    .iter()
                    .map(|&id| ButtonType::from_id(id))
                    .collect::<Option<Vec<_>>>()
                {
                    Some(mut list) => {
                        buttons::sort(&mut list);
                        list
                    }
                    None => buttons::default_buttons(),
                }
            }
            Some(_) => Vec::new(),
        }
    }

    /// Sets `buttons`.
    pub fn set_buttons(&mut self, value: &[ButtonType]) {
        let mut sorted = value.to_vec();
        buttons::sort(&mut sorted);
        if sorted != self.buttons() {
            let ids = value.iter().map(|button| button.id()).collect();
            self.doc.set("buttons", Variant::IntList(ids));
        }
    }

    /// The `userColors` setting: the colour palette, where `None` is the
    /// custom colour picker.
    pub fn user_colors(&self) -> Vec<Option<Color>> {
        self.stored("userColors")
            .and_then(Variant::to_string_list)
            .filter(|names| USER_COLORS_RANGE.contains(&names.len()))
            .and_then(|names| names.iter().map(|name| palette_entry(name)).collect())
            .unwrap_or_else(|| {
                let palette = if self.predefined_color_palette_large() {
                    LARGE_PALETTE
                } else {
                    SMALL_PALETTE
                };
                palette.to_vec()
            })
    }

    /// Sets `userColors`; fails unless there are 3 to 17 entries. Alpha is
    /// not stored, as in the C++.
    pub fn set_user_colors(&mut self, value: &[Option<Color>]) -> Result<(), ConfigError> {
        if !USER_COLORS_RANGE.contains(&value.len()) {
            return Err(invalid("userColors", &format!("{} entries", value.len())));
        }
        if value != self.user_colors() {
            let names = value.iter().map(|entry| match entry {
                Some(color) => Variant::String(color.hex_rgb()),
                None => Variant::String("picker".to_owned()),
            });
            self.doc.set("userColors", Variant::List(names.collect()));
        }
        Ok(())
    }

    /// The `captureRegionMode` setting.
    ///
    /// Files written before the key existed only carry the
    /// `captureActiveMonitor` flag; a user who set it keeps that choice.
    pub fn capture_region_mode(&self) -> CaptureRegionMode {
        if self.doc.get("captureRegionMode").is_none()
            && self.doc.get("captureActiveMonitor").is_some()
        {
            return if self.capture_active_monitor() {
                CaptureRegionMode::ActiveMonitor
            } else {
                CaptureRegionMode::SelectMonitor
            };
        }
        match self.read_int("captureRegionMode", 0, 2, 2) {
            0 => CaptureRegionMode::SelectMonitor,
            1 => CaptureRegionMode::ActiveMonitor,
            _ => CaptureRegionMode::AllMonitors,
        }
    }

    /// Sets `captureRegionMode`, keeping the legacy `captureActiveMonitor`
    /// flag in step so older builds behave the same.
    pub fn set_capture_region_mode(&mut self, value: CaptureRegionMode) {
        let number = value as i32;
        if number != self.read_int("captureRegionMode", 0, 2, 2) {
            self.write_text("captureRegionMode", &number.to_string());
        }
        self.set_capture_active_monitor(value == CaptureRegionMode::ActiveMonitor);
    }

    /// The key sequence bound to `action` (a `TYPE_...` name), or `None` for
    /// a name the application does not register. An empty string means the
    /// action has no shortcut.
    ///
    /// An action left at its default yields to another action that was
    /// explicitly given the same sequence.
    pub fn shortcut(&self, action: &str) -> Option<String> {
        let default = keyseq::normalize(default_shortcut(action)?);
        let Some(stored) = self.doc.get(&shortcut_key(action)) else {
            let taken = self
                .doc
                .section_entries(SHORTCUTS_GROUP)
                .any(|(_, value)| matches!(value, Variant::String(text) if *text == default));
            return Some(if taken { String::new() } else { default });
        };
        if *stored == Variant::Invalid {
            return Some(default);
        }
        let text = stored.to_text();
        Some(if keyseq::is_valid(&text) {
            keyseq::normalize(&text)
        } else {
            default
        })
    }

    /// Binds `shortcut` to `action`; an empty string clears the binding.
    ///
    /// Fails for an unknown action, text that is not a key sequence, the
    /// reserved Backspace and Esc keys, and a sequence another action has
    /// configured.
    pub fn set_shortcut(&mut self, action: &str, shortcut: &str) -> Result<(), ConfigError> {
        if default_shortcut(action).is_none() {
            return Err(ConfigError::UnknownShortcut(action.to_owned()));
        }
        let key = shortcut_key(action);
        if shortcut.is_empty() {
            self.write_text(&key, "");
            return Ok(());
        }
        if !keyseq::is_valid(shortcut) {
            return Err(invalid(&key, shortcut));
        }
        if keyseq::is_key(shortcut, &["Backspace"]) || keyseq::is_key(shortcut, &["Esc", "Escape"])
        {
            return Err(ConfigError::ReservedShortcut(shortcut.to_owned()));
        }
        let normalized = keyseq::normalize(shortcut);
        let conflict = self
            .doc
            .section_entries(SHORTCUTS_GROUP)
            .find(|&(other, value)| {
                let text = value.to_text();
                other != action && keyseq::is_valid(&text) && keyseq::normalize(&text) == normalized
            });
        if let Some((other, _)) = conflict {
            return Err(ConfigError::ShortcutConflict {
                shortcut: normalized,
                action: other.to_owned(),
            });
        }
        self.write_text(&key, &normalized);
        Ok(())
    }

    /// The stored value of `key`, unless it is absent or `@Invalid()`.
    fn stored(&self, key: &str) -> Option<&Variant> {
        self.doc
            .get(key)
            .filter(|value| **value != Variant::Invalid)
    }

    fn stored_text(&self, key: &str) -> Option<String> {
        self.stored(key).map(Variant::to_text)
    }

    /// `BoundedInt` / `LowerBoundedInt`: a decimal integer within range.
    fn read_int(&self, key: &str, min: i32, max: i32, default: i32) -> i32 {
        self.stored_text(key)
            .and_then(|text| text.trim().parse::<i32>().ok())
            .filter(|number| (min..=max).contains(number))
            .unwrap_or(default)
    }

    fn write_int(
        &mut self,
        key: &str,
        min: i32,
        max: i32,
        value: i32,
        unchanged: bool,
    ) -> Result<(), ConfigError> {
        if !(min..=max).contains(&value) {
            return Err(invalid(key, &value.to_string()));
        }
        if !unchanged {
            self.write_text(key, &value.to_string());
        }
        Ok(())
    }

    fn write_text(&mut self, key: &str, value: &str) {
        self.doc.set(key, Variant::String(value.to_owned()));
    }
}

fn invalid(key: &str, value: &str) -> ConfigError {
    ConfigError::InvalidValue {
        key: key.to_owned(),
        value: value.to_owned(),
    }
}

fn shortcut_key(action: &str) -> String {
    format!("{SHORTCUTS_GROUP}/{action}")
}

fn default_shortcut(action: &str) -> Option<&'static str> {
    SHORTCUTS
        .iter()
        .find(|(name, _)| *name == action)
        .map(|&(_, default)| default)
}

/// `SaveFileExtension`: the extension without its dot, if it is a format the
/// application can write.
pub(super) fn checked_extension(text: &str) -> Option<String> {
    let extension = text.strip_prefix('.').unwrap_or(text);
    SAVE_EXTENSIONS
        .contains(&extension)
        .then(|| extension.to_owned())
}

/// One `userColors` entry: `Some(None)` is the picker, `None` is invalid.
fn palette_entry(name: &str) -> Option<Option<Color>> {
    if name == "picker" {
        Some(None)
    } else {
        Color::from_qt_name(name).map(Some)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(settings: &Settings) -> String {
        String::from_utf8(settings.to_bytes()).expect("settings written by the tests are UTF-8")
    }

    #[test]
    fn key_list_matches_v15_0_0() {
        let expected = include_str!(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/v15.0.0-keys.txt"
        ));
        let expected: Vec<&str> = expected.lines().filter(|line| !line.is_empty()).collect();

        let mut actual: Vec<String> = GENERAL_KEYS
            .iter()
            .map(|key| format!("General/{key}"))
            .collect();
        actual.extend(
            SHORTCUTS
                .iter()
                .map(|(name, _)| format!("Shortcuts/{name}")),
        );
        actual.sort();

        assert_eq!(actual, expected);
    }

    #[test]
    fn empty_settings_give_the_cpp_defaults() {
        let settings = Settings::default();
        assert!(settings.show_help());
        assert!(!settings.show_desktop_notification());
        assert!(settings.startup_launch());
        assert!(settings.save_path_fixed());
        assert_eq!(settings.undo_limit(), 100);
        assert_eq!(settings.upload_history_max(), 25);
        assert_eq!(settings.ui_language(), "auto");
        assert_eq!(settings.ui_color(), Color::rgb(56, 153, 194));
        assert_eq!(settings.contrast_ui_color(), Color::rgb(17, 60, 79));
        assert_eq!(settings.draw_color(), Color::rgb(255, 0, 0));
        assert_eq!(settings.filename_pattern(), "%F_%H-%M");
        assert_eq!(settings.save_as_file_extension(), "");
        assert_eq!(settings.buttons(), buttons::default_buttons());
        assert_eq!(settings.user_colors(), SMALL_PALETTE);
        assert_eq!(
            settings.capture_region_mode(),
            CaptureRegionMode::AllMonitors
        );
        assert_eq!(settings.draw_pencil_size(), 3);
        assert_eq!(settings.shortcut("TYPE_COPY").as_deref(), Some("Ctrl+c"));
        assert_eq!(settings.shortcut("TYPE_ACCEPT").as_deref(), Some("Return"));
        assert_eq!(
            settings.shortcut("TAKE_SCREENSHOT").as_deref(),
            Some("Meta+Shift+x")
        );
        assert_eq!(
            settings.shortcut("TYPE_DELETE_CURRENT_TOOL").as_deref(),
            Some("Del")
        );
        assert_eq!(settings.shortcut("TYPE_PIN").as_deref(), Some(""));
        assert_eq!(settings.shortcut("TYPE_NOPE"), None);
    }

    #[test]
    fn every_default_shortcut_is_a_valid_key_sequence() {
        for (name, default) in SHORTCUTS {
            assert!(keyseq::is_valid(default), "{name}={default}");
        }
    }

    #[test]
    fn setters_write_qt_encodings() {
        let mut settings = Settings::default();
        settings.set_show_help(false);
        settings.set_ui_color(Color::rgb(0x74, 0x00, 0x96));
        settings.set_undo_limit(50).expect("in range");
        settings.set_font_family("Segoe UI, Emoji");
        settings.set_buttons(&[ButtonType::Pencil, ButtonType::Copy]);
        settings
            .set_user_colors(&[
                None,
                Some(Color::rgb(0x80, 0, 0)),
                Some(Color::rgb(0, 0xff, 0)),
            ])
            .expect("three entries");
        settings
            .set_shortcut("TYPE_COPY", "Ctrl+Shift+C")
            .expect("free sequence");

        assert_eq!(
            text(&settings),
            "[General]\r\nshowHelp=false\r\nuiColor=#740096\r\nundoLimit=50\r\n\
             fontFamily=\"Segoe UI, Emoji\"\r\n\
             buttons=@Variant(\\0\\0\\0\\x7f\\0\\0\\0\\vQList<int>\\0\\0\\0\\0\\x2\\0\\0\\0\\0\\0\\0\\0\\n)\r\n\
             userColors=picker, #800000, #00ff00\r\n\r\n[Shortcuts]\r\nTYPE_COPY=Ctrl+Shift+c\r\n"
        );

        let reread = Settings::parse(&settings.to_bytes());
        assert!(!reread.show_help());
        assert_eq!(reread.ui_color(), Color::rgb(0x74, 0x00, 0x96));
        assert_eq!(reread.undo_limit(), 50);
        assert_eq!(reread.font_family(), "Segoe UI, Emoji");
        assert_eq!(reread.buttons(), [ButtonType::Pencil, ButtonType::Copy]);
        assert_eq!(
            reread.user_colors(),
            [
                None,
                Some(Color::rgb(0x80, 0, 0)),
                Some(Color::rgb(0, 0xff, 0))
            ]
        );
        assert_eq!(
            reread.shortcut("TYPE_COPY").as_deref(),
            Some("Ctrl+Shift+c")
        );
    }

    #[test]
    fn setting_the_current_value_writes_nothing() {
        let mut settings = Settings::default();
        settings.set_show_help(true);
        settings.set_undo_limit(100).expect("in range");
        settings.set_ui_language("auto");
        settings.set_ui_color(Color::rgb(56, 153, 194));
        settings.set_buttons(&buttons::default_buttons());
        settings
            .set_user_colors(SMALL_PALETTE)
            .expect("valid palette");
        settings.set_filename_pattern("%F_%H-%M");
        assert_eq!(text(&settings), "");
    }

    #[test]
    fn setters_reject_what_the_cpp_would_flag() {
        let mut settings = Settings::default();
        assert!(matches!(
            settings.set_undo_limit(1000),
            Err(ConfigError::InvalidValue { .. })
        ));
        assert!(matches!(
            settings.set_draw_thickness(0),
            Err(ConfigError::InvalidValue { .. })
        ));
        assert!(matches!(
            settings.set_save_as_file_extension(".exe"),
            Err(ConfigError::InvalidValue { .. })
        ));
        assert!(matches!(
            settings.set_user_colors(&[None, None]),
            Err(ConfigError::InvalidValue { .. })
        ));
        assert!(matches!(
            settings.set_shortcut("TYPE_NOPE", "A"),
            Err(ConfigError::UnknownShortcut(_))
        ));
        assert!(matches!(
            settings.set_shortcut("TYPE_COPY", "Ctrl+Nope"),
            Err(ConfigError::InvalidValue { .. })
        ));
        assert!(matches!(
            settings.set_shortcut("TYPE_COPY", "Esc"),
            Err(ConfigError::ReservedShortcut(_))
        ));
        assert_eq!(text(&settings), "");
    }

    #[test]
    fn shortcut_conflicts_are_refused_and_explicit_bindings_win() {
        let mut settings = Settings::default();
        settings
            .set_shortcut("TYPE_PENCIL", "G")
            .expect("G is only a default elsewhere");
        // TYPE_MOVE_OBJECT defaults to G but was never set: it gives way.
        assert_eq!(settings.shortcut("TYPE_MOVE_OBJECT").as_deref(), Some(""));
        assert_eq!(settings.shortcut("TYPE_PENCIL").as_deref(), Some("g"));

        let error = settings
            .set_shortcut("TYPE_ARROW", "g")
            .expect_err("G is taken by TYPE_PENCIL");
        assert!(
            matches!(error, ConfigError::ShortcutConflict { ref action, .. } if action == "TYPE_PENCIL")
        );

        settings
            .set_shortcut("TYPE_PENCIL", "")
            .expect("clearing always works");
        assert_eq!(settings.shortcut("TYPE_PENCIL").as_deref(), Some(""));
        assert_eq!(settings.shortcut("TYPE_MOVE_OBJECT").as_deref(), Some("g"));
    }

    #[test]
    fn tool_sizes_follow_draw_thickness_until_set() {
        let mut settings = Settings::parse(b"drawThickness=7\n");
        assert_eq!(settings.draw_pencil_size(), 7);
        assert_eq!(settings.draw_arrow_size(), 7);
        // 3 is the key's own default, yet it must still be written.
        settings.set_draw_pencil_size(3).expect("in range");
        assert_eq!(settings.draw_pencil_size(), 3);
        assert_eq!(settings.draw_arrow_size(), 7);
        assert_eq!(text(&settings), "drawThickness=7\ndrawPencilSize=3\n");
    }

    #[test]
    fn capture_region_mode_setter_keeps_the_legacy_flag_in_step() {
        let mut settings = Settings::parse(b"captureActiveMonitor=false\n");
        assert_eq!(
            settings.capture_region_mode(),
            CaptureRegionMode::SelectMonitor
        );
        settings.set_capture_region_mode(CaptureRegionMode::ActiveMonitor);
        assert_eq!(
            text(&settings),
            "captureActiveMonitor=true\ncaptureRegionMode=1\n"
        );
        assert_eq!(
            settings.capture_region_mode(),
            CaptureRegionMode::ActiveMonitor
        );
    }

    #[test]
    fn save_path_accepts_an_existing_folder_only() {
        let temp = std::env::temp_dir();
        let mut settings = Settings::default();
        settings.set_save_path(&temp.join("phramer-no-such-folder"));
        assert_eq!(settings.save_path(), path::default_save_dir());

        settings.set_save_path(&temp);
        assert_eq!(settings.save_path(), Some(temp.clone()));
        let reread = Settings::parse(&settings.to_bytes());
        assert_eq!(reread.save_path(), Some(temp));
    }

    #[test]
    fn save_then_load_round_trips_and_missing_files_are_defaults() {
        let dir = std::env::temp_dir().join(format!("phramer-core-test-{}", std::process::id()));
        let file = dir.join("nested").join("phramer.ini");
        assert!(
            Settings::load(&file)
                .expect("a missing file is not an error")
                .to_bytes()
                .is_empty()
        );

        let mut settings = Settings::parse(b"unknownKey=kept\n");
        settings.set_jpeg_quality(90).expect("in range");
        settings
            .save(&file)
            .expect("the temporary folder is writable");
        let loaded = Settings::load(&file).expect("the file was just written");
        assert_eq!(loaded.to_bytes(), settings.to_bytes());
        assert_eq!(loaded.jpeg_quality(), 90);
        assert_eq!(text(&loaded), "unknownKey=kept\njpegQuality=90\n");

        fs::remove_dir_all(&dir).expect("test folder can be removed");
    }
}
