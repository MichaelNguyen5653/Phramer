//! Keyboard and pointer input as the state machine sees it, and the
//! `[Shortcuts]` bindings the overlay reacts to.

use phramer_core::config::{ButtonType, Settings};

use crate::state::CaptureMode;

/// A key, independent of the keyboard layout's modifiers. Letters are
/// lower case; digits and punctuation are their unshifted character.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Key {
    Escape,
    /// The main Return key.
    Return,
    /// The keypad's Enter key. Bindings treat it as Return, as 15.0.0 did.
    Enter,
    Backspace,
    Delete,
    Insert,
    Tab,
    Space,
    Left,
    Right,
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
    /// F1 to F24.
    Function(u8),
    Char(char),
}

/// Modifier keys held during an event.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Modifiers {
    pub ctrl: bool,
    pub shift: bool,
    pub alt: bool,
    /// The Windows key (Qt's Meta).
    pub meta: bool,
}

impl Modifiers {
    pub const NONE: Self = Self {
        ctrl: false,
        shift: false,
        alt: false,
        meta: false,
    };
    pub const CTRL: Self = Self {
        ctrl: true,
        ..Self::NONE
    };
    pub const SHIFT: Self = Self {
        shift: true,
        ..Self::NONE
    };
    pub const CTRL_SHIFT: Self = Self {
        ctrl: true,
        shift: true,
        ..Self::NONE
    };
}

/// A mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum PointerButton {
    Left,
    Right,
    Middle,
}

/// One key with its modifiers, parsed from a `[Shortcuts]` value.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Shortcut {
    pub key: Key,
    pub modifiers: Modifiers,
}

impl Shortcut {
    /// Parses one chord as the settings store it (`Ctrl+Shift+c`,
    /// `Return`, `Ctrl+Backspace`). `None` for an empty value, a sequence
    /// of several chords, or a key the overlay cannot receive.
    pub fn parse(text: &str) -> Option<Self> {
        let text = text.trim();
        if text.is_empty() || text.contains(", ") {
            return None;
        }
        let mut modifiers = Modifiers::NONE;
        let mut rest = text;
        loop {
            let lower = rest.to_ascii_lowercase();
            let Some((name, tail)) = lower.split_once('+') else {
                break;
            };
            // A trailing '+' is the plus key itself
            if tail.is_empty() {
                break;
            }
            match name {
                "ctrl" => modifiers.ctrl = true,
                "shift" => modifiers.shift = true,
                "alt" => modifiers.alt = true,
                "meta" => modifiers.meta = true,
                _ => return None,
            }
            rest = &rest[name.len() + 1..];
        }
        Some(Self {
            key: key_named(rest)?,
            modifiers,
        })
    }

    /// Whether a key press triggers this shortcut. Enter and Return are one
    /// key here, as `CaptureWidget::newShortcut` registered both.
    pub fn matches(&self, key: Key, modifiers: Modifiers) -> bool {
        let same_key = self.key == key
            || (matches!(self.key, Key::Return | Key::Enter)
                && matches!(key, Key::Return | Key::Enter));
        same_key && self.modifiers == modifiers
    }

    /// The shortcut as the help message shows it: `Ctrl+S`, `Enter`.
    pub fn label(&self) -> String {
        let mut parts: Vec<String> = Vec::new();
        if self.modifiers.ctrl {
            parts.push("Ctrl".into());
        }
        if self.modifiers.alt {
            parts.push("Alt".into());
        }
        if self.modifiers.shift {
            parts.push("Shift".into());
        }
        if self.modifiers.meta {
            parts.push("Win".into());
        }
        parts.push(match self.key {
            Key::Escape => "Esc".into(),
            // `initHelpMessage` says Enter for the Return key
            Key::Return | Key::Enter => "Enter".into(),
            Key::Backspace => "Backspace".into(),
            Key::Delete => "Del".into(),
            Key::Insert => "Ins".into(),
            Key::Tab => "Tab".into(),
            Key::Space => "Space".into(),
            Key::Left => "Left".into(),
            Key::Right => "Right".into(),
            Key::Up => "Up".into(),
            Key::Down => "Down".into(),
            Key::Home => "Home".into(),
            Key::End => "End".into(),
            Key::PageUp => "PgUp".into(),
            Key::PageDown => "PgDown".into(),
            Key::Function(n) => format!("F{n}"),
            Key::Char(c) => c.to_uppercase().collect(),
        });
        parts.join("+")
    }
}

/// Qt's key names, as `QKeySequence` spells them on input.
fn key_named(name: &str) -> Option<Key> {
    let mut chars = name.chars();
    if let (Some(c), None) = (chars.next(), chars.next()) {
        return Some(Key::Char(c.to_lowercase().next().unwrap_or(c)));
    }
    let lower = name.to_ascii_lowercase();
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return (1..=24).contains(&number).then_some(Key::Function(number));
    }
    Some(match lower.as_str() {
        "esc" | "escape" => Key::Escape,
        "return" => Key::Return,
        "enter" => Key::Enter,
        "backspace" => Key::Backspace,
        "del" | "delete" => Key::Delete,
        "ins" | "insert" => Key::Insert,
        "tab" => Key::Tab,
        "space" => Key::Space,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "home" => Key::Home,
        "end" => Key::End,
        "pgup" | "pageup" => Key::PageUp,
        "pgdown" | "pagedown" => Key::PageDown,
        _ => return None,
    })
}

/// What a bound key does on the overlay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum KeyAction {
    Accept,
    Copy,
    Save,
    CopyFile,
    Exit,
    Cancel,
    SelectAll,
    Move(Direction),
    Resize(Direction),
    SymmetricResize(Direction),
    /// A toolbar button's shortcut (tools, undo, redo and the actions that
    /// are not exports).
    Button(ButtonType),
    /// Finishes the text being typed (`TYPE_COMMIT_CURRENT_TOOL`).
    CommitTool,
    /// Deletes the selected object (`TYPE_DELETE_CURRENT_TOOL`).
    DeleteTool,
    /// Shows or hides the side panel (`TYPE_TOGGLE_PANEL`).
    TogglePanel,
    /// Picks the draw colour from the screen (`TYPE_GRAB_COLOR`).
    GrabColor,
    /// A segment of the mode bar (`CAPTURE_MODE_SCREENSHOT`,
    /// `CAPTURE_MODE_VIDEO`); only while the bar shows.
    CaptureMode(CaptureMode),
}

/// The key action a toolbar button shares its shortcut with.
pub(crate) fn button_action(button: ButtonType) -> KeyAction {
    match button {
        ButtonType::Copy => KeyAction::Copy,
        ButtonType::Save => KeyAction::Save,
        ButtonType::CopyFile => KeyAction::CopyFile,
        ButtonType::Accept => KeyAction::Accept,
        ButtonType::Exit => KeyAction::Exit,
        other => KeyAction::Button(other),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum Direction {
    Left,
    Right,
    Up,
    Down,
}

/// The `[Shortcuts]` entries the overlay core handles, by the names
/// `confighandler.cpp` registered them under.
const BOUND_ACTIONS: &[(&str, KeyAction)] = &[
    ("TYPE_ACCEPT", KeyAction::Accept),
    ("TYPE_COPY", KeyAction::Copy),
    ("TYPE_SAVE", KeyAction::Save),
    ("TYPE_COPY_FILE", KeyAction::CopyFile),
    ("TYPE_EXIT", KeyAction::Exit),
    ("TYPE_CANCEL", KeyAction::Cancel),
    ("TYPE_SELECT_ALL", KeyAction::SelectAll),
    ("TYPE_MOVE_LEFT", KeyAction::Move(Direction::Left)),
    ("TYPE_MOVE_RIGHT", KeyAction::Move(Direction::Right)),
    ("TYPE_MOVE_UP", KeyAction::Move(Direction::Up)),
    ("TYPE_MOVE_DOWN", KeyAction::Move(Direction::Down)),
    ("TYPE_RESIZE_LEFT", KeyAction::Resize(Direction::Left)),
    ("TYPE_RESIZE_RIGHT", KeyAction::Resize(Direction::Right)),
    ("TYPE_RESIZE_UP", KeyAction::Resize(Direction::Up)),
    ("TYPE_RESIZE_DOWN", KeyAction::Resize(Direction::Down)),
    (
        "TYPE_SYM_RESIZE_LEFT",
        KeyAction::SymmetricResize(Direction::Left),
    ),
    (
        "TYPE_SYM_RESIZE_RIGHT",
        KeyAction::SymmetricResize(Direction::Right),
    ),
    (
        "TYPE_SYM_RESIZE_UP",
        KeyAction::SymmetricResize(Direction::Up),
    ),
    (
        "TYPE_SYM_RESIZE_DOWN",
        KeyAction::SymmetricResize(Direction::Down),
    ),
    ("TYPE_PENCIL", KeyAction::Button(ButtonType::Pencil)),
    ("TYPE_DRAWER", KeyAction::Button(ButtonType::Line)),
    ("TYPE_ARROW", KeyAction::Button(ButtonType::Arrow)),
    ("TYPE_SELECTION", KeyAction::Button(ButtonType::Selection)),
    ("TYPE_RECTANGLE", KeyAction::Button(ButtonType::Rectangle)),
    ("TYPE_CIRCLE", KeyAction::Button(ButtonType::Circle)),
    ("TYPE_MARKER", KeyAction::Button(ButtonType::Marker)),
    ("TYPE_TEXT", KeyAction::Button(ButtonType::Text)),
    ("TYPE_PIXELATE", KeyAction::Button(ButtonType::Pixelate)),
    ("TYPE_INVERT", KeyAction::Button(ButtonType::Invert)),
    (
        "TYPE_CIRCLECOUNT",
        KeyAction::Button(ButtonType::CircleCount),
    ),
    ("TYPE_SHAPE", KeyAction::Button(ButtonType::Shape)),
    (
        "TYPE_MOVE_OBJECT",
        KeyAction::Button(ButtonType::MoveObject),
    ),
    (
        "TYPE_MOVESELECTION",
        KeyAction::Button(ButtonType::MoveSelection),
    ),
    ("TYPE_UNDO", KeyAction::Button(ButtonType::Undo)),
    ("TYPE_REDO", KeyAction::Button(ButtonType::Redo)),
    (
        "TYPE_SIZEINCREASE",
        KeyAction::Button(ButtonType::SizeIncrease),
    ),
    (
        "TYPE_SIZEDECREASE",
        KeyAction::Button(ButtonType::SizeDecrease),
    ),
    ("TYPE_OPEN_APP", KeyAction::Button(ButtonType::OpenApp)),
    ("TYPE_OCR", KeyAction::Button(ButtonType::Ocr)),
    ("TYPE_PIN", KeyAction::Button(ButtonType::Pin)),
    (
        "TYPE_OPEN_IN_EDITOR",
        KeyAction::Button(ButtonType::OpenInEditor),
    ),
    ("TYPE_COMMIT_CURRENT_TOOL", KeyAction::CommitTool),
    ("TYPE_DELETE_CURRENT_TOOL", KeyAction::DeleteTool),
    ("TYPE_TOGGLE_PANEL", KeyAction::TogglePanel),
    ("TYPE_GRAB_COLOR", KeyAction::GrabColor),
    // Last: on keys the other actions do not take while the bar is gone
    (
        "CAPTURE_MODE_SCREENSHOT",
        KeyAction::CaptureMode(CaptureMode::Screenshot),
    ),
    (
        "CAPTURE_MODE_VIDEO",
        KeyAction::CaptureMode(CaptureMode::Video),
    ),
];

/// The overlay's key bindings, read from `[Shortcuts]`. Esc is fixed and
/// not listed: it always closes the overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bindings {
    entries: Vec<(Shortcut, KeyAction)>,
}

impl Bindings {
    /// The bindings of `settings`; an action whose shortcut was cleared or
    /// cannot be parsed has none.
    pub fn from_settings(settings: &Settings) -> Self {
        Self::from_lookup(|name| settings.shortcut(name))
    }

    /// The 15.0.0 defaults.
    pub fn defaults() -> Self {
        Self::from_settings(&Settings::default())
    }

    fn from_lookup(lookup: impl Fn(&str) -> Option<String>) -> Self {
        let entries = BOUND_ACTIONS
            .iter()
            .filter_map(|(name, action)| {
                let text = lookup(name)?;
                Some((Shortcut::parse(&text)?, *action))
            })
            .collect();
        Self { entries }
    }

    /// The action bound to a key press, if any. The first binding wins when
    /// two actions share a key.
    pub(crate) fn action(&self, key: Key, modifiers: Modifiers) -> Option<KeyAction> {
        self.entries
            .iter()
            .find(|(shortcut, _)| shortcut.matches(key, modifiers))
            .map(|(_, action)| *action)
    }

    /// The mode bar segment a key press picks, if any; it goes before any
    /// other binding while the bar shows.
    pub(crate) fn capture_mode(&self, key: Key, modifiers: Modifiers) -> Option<CaptureMode> {
        self.entries
            .iter()
            .find_map(|(shortcut, action)| match action {
                KeyAction::CaptureMode(mode) if shortcut.matches(key, modifiers) => Some(*mode),
                _ => None,
            })
    }

    /// The shortcut bound to an action, for the help message.
    pub(crate) fn shortcut(&self, action: KeyAction) -> Option<Shortcut> {
        self.entries
            .iter()
            .find(|(_, bound)| *bound == action)
            .map(|(shortcut, _)| *shortcut)
    }
}

impl Default for Bindings {
    fn default() -> Self {
        Self::defaults()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_the_stored_spellings() {
        assert_eq!(
            Shortcut::parse("Ctrl+Shift+c"),
            Some(Shortcut {
                key: Key::Char('c'),
                modifiers: Modifiers::CTRL_SHIFT
            })
        );
        assert_eq!(Shortcut::parse("Return").map(|s| s.key), Some(Key::Return));
        assert_eq!(
            Shortcut::parse("Ctrl+Backspace").map(|s| s.key),
            Some(Key::Backspace)
        );
        assert_eq!(Shortcut::parse("Del").map(|s| s.key), Some(Key::Delete));
        assert_eq!(
            Shortcut::parse("Ctrl++").map(|s| s.key),
            Some(Key::Char('+'))
        );
        assert_eq!(
            Shortcut::parse("F12").map(|s| s.key),
            Some(Key::Function(12))
        );
        assert_eq!(Shortcut::parse(""), None);
        assert_eq!(Shortcut::parse("Ctrl+K, Ctrl+C"), None);
        assert_eq!(Shortcut::parse("Hyper+X"), None);
    }

    #[test]
    fn enter_and_return_are_one_key() {
        let accept = Shortcut::parse("Return").unwrap_or_else(|| unreachable!());
        assert!(accept.matches(Key::Enter, Modifiers::NONE));
        assert!(accept.matches(Key::Return, Modifiers::NONE));
        assert!(!accept.matches(Key::Return, Modifiers::CTRL));
    }

    #[test]
    fn labels_read_like_the_help_message() {
        let label = |text| Shortcut::parse(text).map(|s| s.label());
        assert_eq!(label("Ctrl+s").as_deref(), Some("Ctrl+S"));
        assert_eq!(label("Return").as_deref(), Some("Enter"));
        assert_eq!(label("Ctrl+Shift+c").as_deref(), Some("Ctrl+Shift+C"));
    }

    #[test]
    fn default_bindings_cover_the_15_0_0_keys() {
        let bindings = Bindings::defaults();
        let act = |key, modifiers| bindings.action(key, modifiers);
        assert_eq!(act(Key::Return, Modifiers::NONE), Some(KeyAction::Accept));
        assert_eq!(act(Key::Char('c'), Modifiers::CTRL), Some(KeyAction::Copy));
        assert_eq!(act(Key::Char('s'), Modifiers::CTRL), Some(KeyAction::Save));
        assert_eq!(
            act(Key::Char('c'), Modifiers::CTRL_SHIFT),
            Some(KeyAction::CopyFile)
        );
        assert_eq!(act(Key::Char('q'), Modifiers::CTRL), Some(KeyAction::Exit));
        assert_eq!(
            act(Key::Backspace, Modifiers::CTRL),
            Some(KeyAction::Cancel)
        );
        assert_eq!(
            act(Key::Char('a'), Modifiers::CTRL),
            Some(KeyAction::SelectAll)
        );
        assert_eq!(
            act(Key::Left, Modifiers::NONE),
            Some(KeyAction::Move(Direction::Left))
        );
        assert_eq!(
            act(Key::Left, Modifiers::SHIFT),
            Some(KeyAction::Resize(Direction::Left))
        );
        assert_eq!(
            act(Key::Down, Modifiers::CTRL_SHIFT),
            Some(KeyAction::SymmetricResize(Direction::Down))
        );
        assert_eq!(act(Key::Escape, Modifiers::NONE), None);
    }

    #[test]
    fn a_cleared_or_rebound_shortcut_follows_the_settings() {
        let mut settings = Settings::default();
        settings
            .set_shortcut("TYPE_COPY", "Ctrl+Alt+K")
            .unwrap_or_else(|error| panic!("{error}"));
        settings
            .set_shortcut("TYPE_SAVE", "")
            .unwrap_or_else(|error| panic!("{error}"));
        let bindings = Bindings::from_settings(&settings);
        let ctrl_alt = Modifiers {
            ctrl: true,
            alt: true,
            ..Modifiers::NONE
        };
        assert_eq!(
            bindings.action(Key::Char('k'), ctrl_alt),
            Some(KeyAction::Copy)
        );
        assert_eq!(bindings.action(Key::Char('c'), Modifiers::CTRL), None);
        assert_eq!(bindings.action(Key::Char('s'), Modifiers::CTRL), None);
    }
}
