//! Global hotkeys, the ones 15.0.0's daemon registered on Windows:
//!
//! - `[Shortcuts] TAKE_SCREENSHOT` (default `Meta+Shift+x`, Win+Shift+X):
//!   capture. A `QHotkey` in `Flameshot::Flameshot()`.
//! - `[Shortcuts] RECORD_VIDEO` (no default): hand off to Snipping Tool's
//!   recorder. Also a `QHotkey`.
//! - Print Screen, fixed: capture. Raw `RegisterHotKey` in
//!   `GlobalShortcutFilter`.
//!
//! All go through `tauri-plugin-global-shortcut` (`RegisterHotKey`
//! underneath), which expresses Print Screen alone and Win-key combinations.
//! The fixed Shift+Print Screen registration of 15.0.0 is left out: it served
//! only the Imgur history, which the Windows build compiled out, so it did
//! nothing but swallow the key.

use std::sync::Mutex;

use phramer_core::config::Settings;
use tauri::{AppHandle, Manager, Runtime};
use tauri_plugin_global_shortcut::{Code, GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};

use crate::{actions, cli::GuiRequest, notify};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Action {
    Capture,
    RecordVideo,
}

/// One hotkey to register.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Binding {
    action: Action,
    /// The key sequence as the settings file spells it, for messages.
    text: String,
    shortcut: Shortcut,
}

/// A configured key sequence that cannot be a global hotkey.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{text} cannot be used as a global hotkey")]
pub(crate) struct Unsupported {
    text: String,
}

/// What is registered now, and what could not be at the last attempt (so
/// the same failure is not announced again at every settings change).
#[derive(Default)]
pub(crate) struct Registered(Mutex<Vec<Binding>>, Mutex<Vec<String>>);

fn action_label(action: Action) -> &'static str {
    match action {
        Action::Capture => "Capture screen",
        Action::RecordVideo => "Record video (Snipping Tool)",
    }
}

/// The hotkeys the settings ask for, in 15.0.0's registration order. A key
/// already taken by an earlier entry is skipped, as the second
/// `RegisterHotKey` for it failed silently in 15.0.0.
pub(crate) fn wanted(settings: &Settings) -> (Vec<Binding>, Vec<Unsupported>) {
    let configured = [
        (Action::Capture, settings.shortcut("TAKE_SCREENSHOT")),
        (Action::RecordVideo, settings.shortcut("RECORD_VIDEO")),
        (Action::Capture, Some("Print".to_owned())),
    ];
    let mut bindings: Vec<Binding> = Vec::new();
    let mut unsupported = Vec::new();
    for (action, text) in configured {
        let Some(text) = text.filter(|text| !text.is_empty()) else {
            continue;
        };
        match parse_sequence(&text) {
            Some(shortcut) if bindings.iter().any(|b| b.shortcut == shortcut) => {}
            Some(shortcut) => bindings.push(Binding {
                action,
                text,
                shortcut,
            }),
            None => unsupported.push(Unsupported { text }),
        }
    }
    (bindings, unsupported)
}

/// A Qt key sequence (`QKeySequence::PortableText`) as a global hotkey. As
/// with `QHotkey`, only the first chord of a multi-chord sequence counts.
pub(crate) fn parse_sequence(text: &str) -> Option<Shortcut> {
    let chord = first_chord(text);
    let (modifier_part, key) = match chord.strip_suffix("++") {
        // The plus key itself
        Some(rest) => (rest, "+"),
        None => match chord.rsplit_once('+') {
            Some((rest, key)) if !key.is_empty() => (rest, key),
            _ => ("", chord),
        },
    };
    let mut modifiers = Modifiers::empty();
    let mut keypad = false;
    for name in modifier_part.split('+').filter(|name| !name.is_empty()) {
        match name.trim().to_ascii_lowercase().as_str() {
            "ctrl" => modifiers |= Modifiers::CONTROL,
            "shift" => modifiers |= Modifiers::SHIFT,
            "alt" => modifiers |= Modifiers::ALT,
            // Qt's Meta is the Windows key on Windows
            "meta" => modifiers |= Modifiers::SUPER,
            "num" => keypad = true,
            _ => return None,
        }
    }
    let code = if keypad {
        keypad_code(key)?
    } else {
        key_code(key)?
    };
    let modifiers = (!modifiers.is_empty()).then_some(modifiers);
    Some(Shortcut::new(modifiers, code))
}

/// The text before the first chord separator: a comma that is not itself
/// the key of the chord.
fn first_chord(text: &str) -> &str {
    let bytes = text.as_bytes();
    let separator = bytes
        .iter()
        .enumerate()
        .position(|(at, &byte)| byte == b',' && at > 0 && bytes[at - 1] != b'+');
    match separator {
        Some(at) => text[..at].trim(),
        None => text.trim(),
    }
}

fn key_code(key: &str) -> Option<Code> {
    if let [byte] = key.as_bytes() {
        return single_key_code(byte.to_ascii_lowercase());
    }
    let lower = key.to_ascii_lowercase();
    if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()) {
        return function_key(number);
    }
    Some(match lower.as_str() {
        "space" => Code::Space,
        "esc" | "escape" => Code::Escape,
        "tab" => Code::Tab,
        "backspace" => Code::Backspace,
        "return" | "enter" => Code::Enter,
        "ins" | "insert" => Code::Insert,
        "del" | "delete" => Code::Delete,
        "pause" => Code::Pause,
        "print" => Code::PrintScreen,
        "home" => Code::Home,
        "end" => Code::End,
        "left" => Code::ArrowLeft,
        "up" => Code::ArrowUp,
        "right" => Code::ArrowRight,
        "down" => Code::ArrowDown,
        "pgup" | "page up" => Code::PageUp,
        "pgdown" | "page down" => Code::PageDown,
        "capslock" | "caps lock" => Code::CapsLock,
        "numlock" | "num lock" => Code::NumLock,
        "scrolllock" | "scroll lock" => Code::ScrollLock,
        "volume down" => Code::AudioVolumeDown,
        "volume mute" => Code::AudioVolumeMute,
        "volume up" => Code::AudioVolumeUp,
        "media play" => Code::MediaPlay,
        "media stop" => Code::MediaStop,
        "media previous" => Code::MediaTrackPrevious,
        "media next" => Code::MediaTrackNext,
        "media pause" => Code::MediaPause,
        "toggle media play/pause" => Code::MediaPlayPause,
        _ => return None,
    })
}

fn single_key_code(byte: u8) -> Option<Code> {
    const LETTERS: [Code; 26] = [
        Code::KeyA,
        Code::KeyB,
        Code::KeyC,
        Code::KeyD,
        Code::KeyE,
        Code::KeyF,
        Code::KeyG,
        Code::KeyH,
        Code::KeyI,
        Code::KeyJ,
        Code::KeyK,
        Code::KeyL,
        Code::KeyM,
        Code::KeyN,
        Code::KeyO,
        Code::KeyP,
        Code::KeyQ,
        Code::KeyR,
        Code::KeyS,
        Code::KeyT,
        Code::KeyU,
        Code::KeyV,
        Code::KeyW,
        Code::KeyX,
        Code::KeyY,
        Code::KeyZ,
    ];
    const DIGITS: [Code; 10] = [
        Code::Digit0,
        Code::Digit1,
        Code::Digit2,
        Code::Digit3,
        Code::Digit4,
        Code::Digit5,
        Code::Digit6,
        Code::Digit7,
        Code::Digit8,
        Code::Digit9,
    ];
    Some(match byte {
        b'a'..=b'z' => LETTERS[usize::from(byte - b'a')],
        b'0'..=b'9' => DIGITS[usize::from(byte - b'0')],
        b'`' => Code::Backquote,
        b'-' => Code::Minus,
        b'=' => Code::Equal,
        b'[' => Code::BracketLeft,
        b']' => Code::BracketRight,
        b'\\' => Code::Backslash,
        b';' => Code::Semicolon,
        b'\'' => Code::Quote,
        b',' => Code::Comma,
        b'.' => Code::Period,
        b'/' => Code::Slash,
        _ => return None,
    })
}

fn function_key(number: u8) -> Option<Code> {
    const KEYS: [Code; 24] = [
        Code::F1,
        Code::F2,
        Code::F3,
        Code::F4,
        Code::F5,
        Code::F6,
        Code::F7,
        Code::F8,
        Code::F9,
        Code::F10,
        Code::F11,
        Code::F12,
        Code::F13,
        Code::F14,
        Code::F15,
        Code::F16,
        Code::F17,
        Code::F18,
        Code::F19,
        Code::F20,
        Code::F21,
        Code::F22,
        Code::F23,
        Code::F24,
    ];
    KEYS.get(usize::from(number).checked_sub(1)?).copied()
}

/// Keys after Qt's `Num+` modifier.
fn keypad_code(key: &str) -> Option<Code> {
    const DIGITS: [Code; 10] = [
        Code::Numpad0,
        Code::Numpad1,
        Code::Numpad2,
        Code::Numpad3,
        Code::Numpad4,
        Code::Numpad5,
        Code::Numpad6,
        Code::Numpad7,
        Code::Numpad8,
        Code::Numpad9,
    ];
    Some(match key.to_ascii_lowercase().as_str() {
        "+" => Code::NumpadAdd,
        "-" => Code::NumpadSubtract,
        "*" => Code::NumpadMultiply,
        "/" => Code::NumpadDivide,
        "." => Code::NumpadDecimal,
        "=" => Code::NumpadEqual,
        "enter" | "return" => Code::NumpadEnter,
        digit if digit.len() == 1 => {
            let byte = digit.as_bytes()[0];
            *DIGITS.get(usize::from(byte.checked_sub(b'0')?))?
        }
        _ => return None,
    })
}

/// Registers what the settings ask for, replacing what was registered
/// before, and reports the hotkeys that could not be had in a tray
/// notification. Does nothing when the wanted set is unchanged, so an
/// unrelated settings change does not repeat the notification.
pub(crate) fn apply<R: Runtime>(app: &AppHandle<R>, settings: &Settings) {
    let (bindings, unsupported) = wanted(settings);
    let state = app.state::<Registered>();
    let Ok(mut registered) = state.0.lock() else {
        log::error!("hotkey state poisoned; hotkeys left as they were");
        return;
    };
    if *registered == bindings && unsupported.is_empty() {
        return;
    }

    let manager = app.global_shortcut();
    if let Err(error) = manager.unregister_all() {
        log::warn!("could not unregister the previous hotkeys: {error}");
    }
    let mut failures: Vec<String> = unsupported
        .iter()
        .map(|item| {
            log::warn!("{item}");
            item.text.clone()
        })
        .collect();
    registered.clear();
    for binding in bindings {
        let action = binding.action;
        let result = manager.on_shortcut(binding.shortcut, move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                run(app, action);
            }
        });
        match result {
            Ok(()) => {
                log::info!(
                    "hotkey {} registered ({})",
                    binding.text,
                    action_label(action)
                );
                registered.push(binding);
            }
            Err(error) => {
                log::warn!(
                    "hotkey {} ({}) could not be registered: {error}",
                    binding.text,
                    action_label(action)
                );
                failures.push(format!("{} ({})", binding.text, action_label(action)));
            }
        }
    }
    drop(registered);

    let announced = state
        .1
        .lock()
        .map(|mut last| std::mem::replace(&mut *last, failures.clone()) == failures)
        .unwrap_or(false);
    if !failures.is_empty() && !announced {
        let text = format!(
            "These hotkeys could not be registered; another application may be using them:\n{}",
            failures.join("\n")
        );
        notify::tray_message(app, "Phramer", &text);
    }
}

/// The key sequences `settings` asks for that are not registered now: taken
/// by another application, or not expressible as a hotkey. For the settings
/// window to show next to the shortcut.
pub(crate) fn failures<R: Runtime>(app: &AppHandle<R>, settings: &Settings) -> Vec<String> {
    let (bindings, unsupported) = wanted(settings);
    let state = app.state::<Registered>();
    let Ok(registered) = state.0.lock() else {
        return Vec::new();
    };
    bindings
        .into_iter()
        .filter(|binding| !registered.contains(binding))
        .map(|binding| binding.text)
        .chain(unsupported.into_iter().map(|item| item.text))
        .collect()
}

/// Lets go of every hotkey until the next [`apply`], so the settings
/// window's shortcut recorder sees the keys instead of a capture starting.
pub(crate) fn suspend<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<Registered>();
    let Ok(mut registered) = state.0.lock() else {
        return;
    };
    if let Err(error) = app.global_shortcut().unregister_all() {
        log::warn!("could not release the hotkeys: {error}");
    }
    registered.clear();
    log::info!("hotkeys suspended");
}

fn run<R: Runtime>(app: &AppHandle<R>, action: Action) {
    match action {
        Action::Capture => actions::capture_gui(app, GuiRequest::default()),
        Action::RecordVideo => actions::record_video(app),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shortcut(modifiers: Modifiers, code: Code) -> Option<Shortcut> {
        Some(Shortcut::new(
            (!modifiers.is_empty()).then_some(modifiers),
            code,
        ))
    }

    #[test]
    fn the_default_capture_shortcut_is_win_shift_x() {
        assert_eq!(
            parse_sequence("Meta+Shift+x"),
            shortcut(Modifiers::SUPER | Modifiers::SHIFT, Code::KeyX)
        );
    }

    #[test]
    fn print_screen_alone() {
        assert_eq!(
            parse_sequence("Print"),
            shortcut(Modifiers::empty(), Code::PrintScreen)
        );
        assert_eq!(
            parse_sequence("Shift+Print"),
            shortcut(Modifiers::SHIFT, Code::PrintScreen)
        );
    }

    #[test]
    fn modifiers_and_keys_ignore_case() {
        assert_eq!(
            parse_sequence("Alt+Shift+s"),
            shortcut(Modifiers::ALT | Modifiers::SHIFT, Code::KeyS)
        );
        assert_eq!(
            parse_sequence("ctrl+ALT+Del"),
            shortcut(Modifiers::CONTROL | Modifiers::ALT, Code::Delete)
        );
        assert_eq!(
            parse_sequence("Ctrl+F12"),
            shortcut(Modifiers::CONTROL, Code::F12)
        );
        assert_eq!(
            parse_sequence("Ctrl+PgDown"),
            shortcut(Modifiers::CONTROL, Code::PageDown)
        );
    }

    #[test]
    fn punctuation_keypad_and_plus() {
        assert_eq!(
            parse_sequence("Ctrl+,"),
            shortcut(Modifiers::CONTROL, Code::Comma)
        );
        // The plus key has no key of its own on a US layout
        assert_eq!(parse_sequence("Ctrl++"), None);
        assert_eq!(
            parse_sequence("Num+5"),
            shortcut(Modifiers::empty(), Code::Numpad5)
        );
        assert_eq!(
            parse_sequence("Ctrl+Num++"),
            shortcut(Modifiers::CONTROL, Code::NumpadAdd)
        );
    }

    #[test]
    fn only_the_first_chord_counts() {
        assert_eq!(
            parse_sequence("Ctrl+K, Ctrl+C"),
            shortcut(Modifiers::CONTROL, Code::KeyK)
        );
        assert_eq!(
            parse_sequence("Ctrl+,, Ctrl+C"),
            shortcut(Modifiers::CONTROL, Code::Comma)
        );
    }

    #[test]
    fn keys_a_hotkey_cannot_express() {
        assert_eq!(parse_sequence("Launch (0)"), None);
        assert_eq!(parse_sequence("Ctrl+F25"), None);
        assert_eq!(parse_sequence("Ctrl+!"), None);
        assert_eq!(parse_sequence("Hyper+X"), None);
    }

    #[test]
    fn defaults_register_win_shift_x_and_print_screen() {
        let (bindings, unsupported) = wanted(&Settings::default());
        assert!(unsupported.is_empty());
        let wanted: Vec<_> = bindings.iter().map(|b| (b.action, b.text.as_str())).collect();
        assert_eq!(
            wanted,
            [(Action::Capture, "Meta+Shift+x"), (Action::Capture, "Print")]
        );
    }

    #[test]
    fn configured_shortcuts_are_used() {
        let settings = Settings::parse(
            b"[Shortcuts]\nTAKE_SCREENSHOT=Alt+Shift+s\nRECORD_VIDEO=Ctrl+Shift+R\n",
        );
        let (bindings, _) = wanted(&settings);
        let wanted: Vec<_> = bindings.iter().map(|b| (b.action, b.text.as_str())).collect();
        assert_eq!(
            wanted,
            [
                (Action::Capture, "Alt+Shift+s"),
                (Action::RecordVideo, "Ctrl+Shift+r"),
                (Action::Capture, "Print"),
            ]
        );
    }

    #[test]
    fn a_cleared_capture_shortcut_leaves_print_screen() {
        let settings = Settings::parse(b"[Shortcuts]\nTAKE_SCREENSHOT=\n");
        let (bindings, _) = wanted(&settings);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].text, "Print");
    }

    #[test]
    fn print_screen_is_registered_once() {
        let settings = Settings::parse(b"[Shortcuts]\nTAKE_SCREENSHOT=Print\n");
        let (bindings, _) = wanted(&settings);
        assert_eq!(bindings.len(), 1);
        assert_eq!(bindings[0].action, Action::Capture);
    }

    #[test]
    fn unsupported_sequences_are_reported() {
        let settings = Settings::parse(b"[Shortcuts]\nRECORD_VIDEO=Launch (0)\n");
        let (_, unsupported) = wanted(&settings);
        assert_eq!(
            unsupported,
            [Unsupported {
                text: "Launch (0)".into()
            }]
        );
    }
}
