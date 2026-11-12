//! Keyboard shortcut text, validated the way `QKeySequence` parses it and
//! normalized the way the C++ `KeySequence` value handler does.

/// Chords `QKeySequence` keeps from one string.
const MAX_CHORDS: usize = 4;
const MODIFIERS: [&str; 5] = ["ctrl+", "shift+", "alt+", "meta+", "num+"];

/// Whether the C++ `KeySequence::check` accepts `text`: empty, or something
/// `QKeySequence` can render back to a non-empty string.
pub(super) fn is_valid(text: &str) -> bool {
    let chords = split_chords(text);
    // Qt joins the chords with ", ", so only a lone unknown chord renders as
    // the empty string.
    chords.len() != 1 || is_known_chord(&chords[0])
}

/// The C++ `KeySequence::process`: `Enter` means the Return key, and the
/// final character is lower-cased.
pub(super) fn normalize(text: &str) -> String {
    if text == "Enter" {
        return "Return".to_owned();
    }
    let mut chars = text.chars();
    match chars.next_back() {
        Some(last) => chars.chain(last.to_lowercase()).collect(),
        None => String::new(),
    }
}

/// Whether `text` is exactly the unmodified key `name` (one of Qt's names
/// for it), ignoring case as `QKeySequence` does.
pub(super) fn is_key(text: &str, names: &[&str]) -> bool {
    names.iter().any(|name| text.eq_ignore_ascii_case(name))
}

/// `QKeySequence::assign`: chords are separated by a comma, optionally
/// followed by one space; a comma that ends the text or follows another
/// comma is the comma key itself.
fn split_chords(text: &str) -> Vec<String> {
    let mut rest: Vec<char> = text.chars().collect();
    let mut chords = Vec::new();
    while !rest.is_empty() && chords.len() < MAX_CHORDS {
        let mut cut = rest.iter().position(|&c| c == ',');
        let mut skipped_space = 0;
        if let Some(mut at) = cut {
            if at == rest.len() - 1 {
                cut = None;
            } else {
                if rest[at + 1] == ',' {
                    at += 1;
                }
                if rest.get(at + 1) == Some(&' ') {
                    skipped_space = 1;
                    at += 1;
                }
                cut = Some(at);
            }
        }
        match cut {
            Some(at) => {
                chords.push(rest[..at - skipped_space].iter().collect());
                rest.drain(..=at);
            }
            None => {
                chords.push(rest.iter().collect());
                rest.clear();
            }
        }
    }
    chords
}

/// `QKeySequencePrivate::decodeString`, reduced to "is the key known".
fn is_known_chord(chord: &str) -> bool {
    let accel: Vec<char> = chord.to_lowercase().chars().collect();
    let last_plus = accel.iter().rposition(|&c| c == '+');

    // Everything up to each '+' must be a modifier; the one exception is a
    // lone '+' that ends the chord (the plus key).
    let mut piece_start = 0;
    let mut search_from = 1;
    while let Some(offset) = accel
        .get(search_from..)
        .and_then(|tail| tail.iter().position(|&c| c == '+'))
    {
        let plus = search_from + offset;
        let piece: String = accel[piece_start..=plus].iter().collect();
        if piece.len() == 1 {
            if last_plus != Some(accel.len() - 1) {
                return false;
            }
        } else if !MODIFIERS.contains(&piece.as_str()) {
            return false;
        }
        piece_start = plus + 1;
        search_from = plus + 1;
    }

    // The key is what follows the last '+', not counting a final '+'.
    let key_start = match accel
        .len()
        .checked_sub(1)
        .and_then(|end| accel[..end].iter().rposition(|&c| c == '+'))
    {
        Some(plus) if plus > 0 => plus + 1,
        _ => 0,
    };
    let key: String = accel[key_start..].iter().collect();

    if key.chars().count() == 1 {
        return true;
    }
    let function_key = key
        .strip_prefix('f')
        .and_then(|number| number.trim().parse::<u32>().ok());
    function_key.is_some_and(|number| (1..=35).contains(&number))
        || KEY_NAMES.iter().any(|name| name.eq_ignore_ascii_case(&key))
}

/// Qt's `keyname` table (`qkeysequence.cpp`): every key with a spelled-out
/// name, including the alternative spellings accepted on input.
const KEY_NAMES: &[&str] = &[
    "Space",
    "Esc",
    "Tab",
    "Backtab",
    "Backspace",
    "Return",
    "Enter",
    "Ins",
    "Del",
    "Pause",
    "Print",
    "SysReq",
    "Home",
    "End",
    "Left",
    "Up",
    "Right",
    "Down",
    "PgUp",
    "PgDown",
    "CapsLock",
    "NumLock",
    "ScrollLock",
    "Menu",
    "Help",
    "Back",
    "Forward",
    "Stop",
    "Refresh",
    "Volume Down",
    "Volume Mute",
    "Volume Up",
    "Bass Boost",
    "Bass Up",
    "Bass Down",
    "Treble Up",
    "Treble Down",
    "Media Play",
    "Media Stop",
    "Media Previous",
    "Media Next",
    "Media Record",
    "Media Pause",
    "Toggle Media Play/Pause",
    "Home Page",
    "Favorites",
    "Search",
    "Standby",
    "Open URL",
    "Launch Mail",
    "Launch Media",
    "Launch (0)",
    "Launch (1)",
    "Launch (2)",
    "Launch (3)",
    "Launch (4)",
    "Launch (5)",
    "Launch (6)",
    "Launch (7)",
    "Launch (8)",
    "Launch (9)",
    "Launch (A)",
    "Launch (B)",
    "Launch (C)",
    "Launch (D)",
    "Launch (E)",
    "Launch (F)",
    "Launch (G)",
    "Launch (H)",
    "Monitor Brightness Up",
    "Monitor Brightness Down",
    "Keyboard Light On/Off",
    "Keyboard Brightness Up",
    "Keyboard Brightness Down",
    "Power Off",
    "Wake Up",
    "Eject",
    "Screensaver",
    "WWW",
    "Sleep",
    "LightBulb",
    "Shop",
    "History",
    "Add Favorite",
    "Hot Links",
    "Adjust Brightness",
    "Finance",
    "Community",
    "Media Rewind",
    "Back Forward",
    "Application Left",
    "Application Right",
    "Book",
    "CD",
    "Calculator",
    "Calendar",
    "Clear",
    "Clear Grab",
    "Close",
    "Adjust contrast",
    "Copy",
    "Cut",
    "Display",
    "DOS",
    "Documents",
    "Spreadsheet",
    "Browser",
    "Game",
    "Go",
    "iTouch",
    "Logoff",
    "Market",
    "Meeting",
    "Memo",
    "Keyboard Menu",
    "Menu PB",
    "My Sites",
    "News",
    "Home Office",
    "Option",
    "Paste",
    "Phone",
    "Reply",
    "Reload",
    "Rotate Windows",
    "Rotation PB",
    "Rotation KB",
    "Save",
    "Send",
    "Spellchecker",
    "Split Screen",
    "Support",
    "Task Panel",
    "Terminal",
    "To-do list",
    "Tools",
    "Travel",
    "Video",
    "Word Processor",
    "XFer",
    "Zoom In",
    "Zoom Out",
    "Away",
    "Messenger",
    "WebCam",
    "Mail Forward",
    "Pictures",
    "Music",
    "Battery",
    "Bluetooth",
    "Wireless",
    "Ultra Wide Band",
    "Media Fast Forward",
    "Audio Repeat",
    "Audio Random Play",
    "Subtitle",
    "Audio Cycle Track",
    "Time",
    "Hibernate",
    "View",
    "Top Menu",
    "Power Down",
    "Suspend",
    "Microphone Mute",
    "Red",
    "Green",
    "Yellow",
    "Blue",
    "Channel Up",
    "Channel Down",
    "Guide",
    "Info",
    "Settings",
    "Microphone Volume Up",
    "Microphone Volume Down",
    "New",
    "Open",
    "Find",
    "Undo",
    "Redo",
    "Print Screen",
    "Page Up",
    "Page Down",
    "Caps Lock",
    "Num Lock",
    "Number Lock",
    "Scroll Lock",
    "Insert",
    "Delete",
    "Escape",
    "System Request",
    "Select",
    "Yes",
    "No",
    "Context1",
    "Context2",
    "Context3",
    "Context4",
    "Call",
    "Hangup",
    "Toggle Call/Hangup",
    "Flip",
    "Voice Dial",
    "Last Number Redial",
    "Camera Shutter",
    "Camera Focus",
    "Kanji",
    "Muhenkan",
    "Henkan",
    "Romaji",
    "Hiragana",
    "Katakana",
    "Hiragana Katakana",
    "Zenkaku",
    "Hankaku",
    "Zenkaku Hankaku",
    "Touroku",
    "Massyo",
    "Kana Lock",
    "Kana Shift",
    "Eisu Shift",
    "Eisu toggle",
    "Code input",
    "Multiple Candidate",
    "Previous Candidate",
    "Hangul",
    "Hangul Start",
    "Hangul End",
    "Hangul Hanja",
    "Hangul Jamo",
    "Hangul Romaja",
    "Hangul Jeonja",
    "Hangul Banja",
    "Hangul PreHanja",
    "Hangul PostHanja",
    "Hangul Special",
    "Cancel",
    "Printer",
    "Execute",
    "Play",
    "Zoom",
    "Exit",
    "Touchpad Toggle",
    "Touchpad On",
    "Touchpad Off",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shipped_defaults_are_valid() {
        for text in [
            "P",
            "Ctrl+M",
            "Ctrl+Shift+Z",
            "Return",
            "Ctrl+Backspace",
            "Space",
            "Alt+G",
            "Shift+Left",
        ] {
            assert!(is_valid(text), "{text}");
        }
        for text in [
            "Meta+Shift+x",
            "Ctrl+Return",
            "Delete",
            "Del",
            "1",
            "",
            "Enter",
            "F5",
            "f35",
            "PgUp",
        ] {
            assert!(is_valid(text), "{text}");
        }
    }

    #[test]
    fn plus_and_comma_keys_are_valid() {
        for text in [
            "+",
            "Ctrl++",
            "Ctrl+,",
            ",",
            "Ctrl+K, Ctrl+C",
            "ctrl+shift+page up",
        ] {
            assert!(is_valid(text), "{text}");
        }
    }

    #[test]
    fn unknown_keys_and_modifiers_are_invalid() {
        for text in [
            "Ctrl+Nonsense",
            "garbage",
            "Super+A",
            "Ctrl+",
            "F36",
            "Ctrl+Shift",
            "A+B",
        ] {
            assert!(!is_valid(text), "{text}");
        }
    }

    #[test]
    fn normalization_matches_the_value_handler() {
        assert_eq!(normalize("Enter"), "Return");
        assert_eq!(normalize("Ctrl+Shift+Z"), "Ctrl+Shift+z");
        assert_eq!(normalize("P"), "p");
        assert_eq!(normalize("Return"), "Return");
        assert_eq!(normalize(""), "");
    }

    #[test]
    fn unmodified_keys_are_recognized_by_any_name() {
        assert!(is_key("escape", &["Esc", "Escape"]));
        assert!(!is_key("Ctrl+Esc", &["Esc", "Escape"]));
    }
}
