//! Offering Phramer as a handler of the `ms-screenclip:` protocol (15.0.0's
//! `ScreenClipProtocol`).
//!
//! Windows launches `ms-screenclip:` for its own screen capture (the Print
//! Screen key, and apps that ask for a snip). Which app answers is the
//! user's per-user choice, kept in a hash-protected `UserChoice` key no
//! program can write. So Phramer only puts itself on the list of candidates,
//! the Default Programs way, and opens Windows Settings where the user picks
//! it.
//!
//! Being listed means writing under HKLM, which needs administrator rights,
//! so the writes happen in a second, elevated copy of Phramer started with
//! [`REGISTER_ARGUMENT`] or [`UNREGISTER_ARGUMENT`]: it changes the keys and
//! exits; nothing else ever runs elevated.
//!
//! Every name is the one 14.x and 15.0.0 used, so a choice made for them
//! stays valid. The Store edition declares the handler in its package
//! manifest instead: nothing is written, nothing elevates.

use std::path::Path;

use serde::Serialize;

/// The elevated copy's arguments, each the one and only argument.
pub(crate) const REGISTER_ARGUMENT: &str = "--register-screenclip";
pub(crate) const UNREGISTER_ARGUMENT: &str = "--unregister-screenclip";

const APP_NAME: &str = "Phramer";
const PROG_ID: &str = "Phramer";
const CAPABILITIES_PATH: &str = r"SOFTWARE\Phramer\Capabilities";
const URL_ASSOCIATIONS: &str = r"SOFTWARE\Phramer\Capabilities\URLAssociations";
const MACHINE_CLASSES: &str = r"SOFTWARE\Classes";
const USER_CLASSES: &str = r"Software\Classes";
const REGISTERED_APPS: &str = r"SOFTWARE\RegisteredApplications";
const USER_CHOICE: &str =
    r"Software\Microsoft\Windows\Shell\Associations\UrlAssociations\ms-screenclip\UserChoice";
const COMMAND_KEY: &str = r"shell\open\command";
/// Test builds of 15.0 wrote onto Windows' own key, then a ProgId of another
/// name; both are still Phramer's to clean up.
const LEGACY_KEY: &str = r"SOFTWARE\Classes\ms-screenclip";
const LEGACY_PROG_ID: &str = "Phramer.ScreenClip";
/// The UAC prompt can sit unanswered; past this the caller stops waiting.
const ELEVATED_WAIT_MS: u32 = 120_000;

/// The command registered for the copy of Phramer at `exe`: starts a
/// capture, with the link passed so a recording request can be told apart.
pub(crate) fn command(exe: &Path) -> String {
    format!("\"{}\" --screenclip \"%1\"", exe.display())
}

/// Exactly the shape [`command`] produces, for any install location, or the
/// `gui` one earlier 15.0 builds registered. Only such a command is ever
/// removed.
fn is_phramer_command(command: &str) -> bool {
    let lower = command.to_lowercase();
    let Some(rest) = lower.strip_prefix('"') else {
        return false;
    };
    let Some((exe, tail)) = rest.split_once('"') else {
        return false;
    };
    let named = exe
        .rsplit_once('\\')
        .is_some_and(|(_, name)| name == "phramer.exe");
    named && (tail == " gui" || tail == " --screenclip \"%1\"")
}

/// What the Settings row and the welcome show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub(crate) struct Status {
    /// This copy of Phramer is listed.
    pub(crate) registered: bool,
    /// Some copy of Phramer is listed, this one or another install.
    pub(crate) registered_by_phramer: bool,
    /// The user chose Phramer for ms-screenclip in Windows Settings.
    pub(crate) chosen: bool,
    /// The Store edition: registered by its package, nothing to toggle.
    pub(crate) store: bool,
}

/// How a request through UAC went.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Outcome {
    Succeeded,
    Cancelled,
    Failed,
}

#[cfg(not(feature = "store"))]
pub(crate) fn status() -> Status {
    let exe = std::env::current_exe().unwrap_or_default();
    let prog_id_command = registry::read(registry::Root::Classes, &prog_id_command_key(PROG_ID));
    let association =
        registry::read_value(registry::Root::Machine, URL_ASSOCIATIONS, "ms-screenclip");
    let listed = registry::read_value(registry::Root::Machine, REGISTERED_APPS, APP_NAME);
    let ours = |text: &Option<String>, expected: &str| {
        text.as_deref()
            .is_some_and(|text| text.eq_ignore_ascii_case(expected))
    };
    Status {
        registered: ours(&prog_id_command, &command(&exe))
            && ours(&association, PROG_ID)
            && ours(&listed, CAPABILITIES_PATH),
        registered_by_phramer: prog_id_command.as_deref().is_some_and(is_phramer_command),
        chosen: is_chosen(),
        store: false,
    }
}

#[cfg(feature = "store")]
pub(crate) fn status() -> Status {
    let declared = manifest_declares_protocol();
    Status {
        registered: declared,
        registered_by_phramer: declared,
        chosen: is_chosen(),
        store: true,
    }
}

/// The package manifest beside the exe declares the handler; the
/// `noscreenclip` variant does not, and must not offer it.
#[cfg(feature = "store")]
fn manifest_declares_protocol() -> bool {
    std::env::current_exe()
        .ok()
        .and_then(|exe| std::fs::read_to_string(exe.with_file_name("AppxManifest.xml")).ok())
        .is_some_and(|manifest| manifest.contains(r#"Name="ms-screenclip""#))
}

fn is_chosen() -> bool {
    registry::read_value(registry::Root::User, USER_CHOICE, "ProgId")
        .is_some_and(|prog_id| prog_id.eq_ignore_ascii_case(PROG_ID))
}

fn prog_id_command_key(prog_id: &str) -> String {
    format!(r"{prog_id}\{COMMAND_KEY}")
}

/// The elevated copy's work; the exit code tells the copy that started it.
pub(crate) fn register_here() -> i32 {
    #[cfg(not(feature = "store"))]
    if write_registration() {
        return 0;
    }
    1
}

pub(crate) fn unregister_here() -> i32 {
    #[cfg(not(feature = "store"))]
    if remove_registration() {
        return 0;
    }
    1
}

/// The elevated half of Register: writes the ProgId and the Default
/// Programs listing. True when this copy is listed afterwards.
#[cfg(not(feature = "store"))]
pub(crate) fn write_registration() -> bool {
    use registry::Root;
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    remove_legacy_entry();
    // A per-user ProgId from 14.x would shadow the machine-wide one
    remove_prog_id(Root::User, USER_CLASSES, PROG_ID);
    let icon = format!("\"{}\",0", exe.display());
    let class = format!(r"{MACHINE_CLASSES}\{PROG_ID}");
    let written = [
        registry::write(Root::Machine, &class, None, "Phramer screen capture"),
        registry::write(Root::Machine, &format!(r"{class}\DefaultIcon"), None, &icon),
        registry::write(
            Root::Machine,
            &format!(r"{class}\{COMMAND_KEY}"),
            None,
            &command(&exe),
        ),
        registry::write(
            Root::Machine,
            CAPABILITIES_PATH,
            Some("ApplicationName"),
            APP_NAME,
        ),
        registry::write(
            Root::Machine,
            CAPABILITIES_PATH,
            Some("ApplicationDescription"),
            "Screenshot and annotation tool",
        ),
        registry::write(
            Root::Machine,
            CAPABILITIES_PATH,
            Some("ApplicationIcon"),
            &icon,
        ),
        registry::write(
            Root::Machine,
            URL_ASSOCIATIONS,
            Some("ms-screenclip"),
            PROG_ID,
        ),
        registry::write(
            Root::Machine,
            REGISTERED_APPS,
            Some(APP_NAME),
            CAPABILITIES_PATH,
        ),
    ];
    written.iter().all(|ok| *ok) && status().registered
}

/// The elevated half of Unregister: removes each part, only when it is
/// Phramer's. True when no copy is listed afterwards.
#[cfg(not(feature = "store"))]
pub(crate) fn remove_registration() -> bool {
    use registry::Root;
    remove_legacy_entry();
    remove_prog_id(Root::User, USER_CLASSES, PROG_ID);
    remove_prog_id(Root::Machine, MACHINE_CLASSES, PROG_ID);
    let ours = registry::read_value(Root::Machine, URL_ASSOCIATIONS, "ms-screenclip")
        .is_some_and(|prog_id| prog_id.eq_ignore_ascii_case(PROG_ID));
    if ours {
        registry::delete_tree(Root::Machine, CAPABILITIES_PATH);
        registry::delete_if_empty(Root::Machine, r"SOFTWARE\Phramer");
    }
    let listed = registry::read_value(Root::Machine, REGISTERED_APPS, APP_NAME)
        .is_some_and(|path| path.eq_ignore_ascii_case(CAPABILITIES_PATH));
    if listed {
        registry::delete_value(Root::Machine, REGISTERED_APPS, APP_NAME);
    }
    !status().registered_by_phramer
}

/// Removes a ProgId from a classes root, but only one of Phramer's.
fn remove_prog_id(root: registry::Root, classes: &str, prog_id: &str) {
    let key = format!(r"{classes}\{prog_id}");
    let command = registry::read(root, &format!(r"{key}\{COMMAND_KEY}"));
    if command.as_deref().is_some_and(is_phramer_command) {
        registry::delete_tree(root, &key);
    }
}

fn remove_legacy_entry() {
    use registry::Root;
    remove_prog_id(Root::Machine, MACHINE_CLASSES, LEGACY_PROG_ID);
    let command = registry::read(Root::Machine, &format!(r"{LEGACY_KEY}\{COMMAND_KEY}"));
    if !command.as_deref().is_some_and(is_phramer_command) {
        return;
    }
    registry::delete_tree(Root::Machine, &format!(r"{LEGACY_KEY}\{COMMAND_KEY}"));
    // The parents go too, but only once nothing else lives under them
    registry::delete_if_empty(Root::Machine, &format!(r"{LEGACY_KEY}\shell\open"));
    registry::delete_if_empty(Root::Machine, &format!(r"{LEGACY_KEY}\shell"));
}

/// Register or Unregister through UAC: an elevated copy of this exe with
/// `argument`. Blocks while the prompt is up.
#[cfg(not(feature = "store"))]
pub(crate) fn run_elevated(argument: &str) -> Outcome {
    use windows_sys::Win32::Foundation::{
        CloseHandle, ERROR_CANCELLED, GetLastError, WAIT_OBJECT_0,
    };
    use windows_sys::Win32::System::Threading::{GetExitCodeProcess, WaitForSingleObject};
    use windows_sys::Win32::UI::Shell::{
        SEE_MASK_NOASYNC, SEE_MASK_NOCLOSEPROCESS, SHELLEXECUTEINFOW, ShellExecuteExW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_HIDE;

    let Ok(exe) = std::env::current_exe() else {
        return Outcome::Failed;
    };
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let (verb, file, parameters) = (
        wide("runas"),
        wide(&exe.display().to_string()),
        wide(argument),
    );
    let mut info = SHELLEXECUTEINFOW {
        cbSize: size_of::<SHELLEXECUTEINFOW>() as u32,
        fMask: SEE_MASK_NOCLOSEPROCESS | SEE_MASK_NOASYNC,
        lpVerb: verb.as_ptr(),
        lpFile: file.as_ptr(),
        lpParameters: parameters.as_ptr(),
        nShow: SW_HIDE,
        ..Default::default()
    };
    // SAFETY: `info` is initialised with its size; the strings outlive the
    // call.
    if unsafe { ShellExecuteExW(&mut info) } == 0 {
        // SAFETY: reads this thread's last error.
        return if unsafe { GetLastError() } == ERROR_CANCELLED {
            Outcome::Cancelled
        } else {
            Outcome::Failed
        };
    }
    if info.hProcess.is_null() {
        return Outcome::Failed;
    }
    let mut code = 1u32;
    // SAFETY: a process handle this call owns, closed below.
    let finished = unsafe { WaitForSingleObject(info.hProcess, ELEVATED_WAIT_MS) } == WAIT_OBJECT_0;
    if finished {
        // SAFETY: as above; `code` is a valid out-parameter.
        unsafe { GetExitCodeProcess(info.hProcess, &mut code) };
    }
    // SAFETY: closes the handle ShellExecuteEx gave this call.
    unsafe { CloseHandle(info.hProcess) };
    if finished && code == 0 {
        // The elevated copy did this too, unless UAC was answered with
        // another account's credentials: then its HKCU was not this user's
        remove_prog_id(registry::Root::User, USER_CLASSES, PROG_ID);
        Outcome::Succeeded
    } else {
        Outcome::Failed
    }
}

/// What a Register or Unregister did, and where things stand now.
#[derive(Debug, Serialize)]
pub(crate) struct Toggled {
    outcome: Outcome,
    /// It was Unregister.
    unregistered: bool,
    status: Status,
}

#[tauri::command]
pub(crate) fn screenclip_status() -> Status {
    status()
}

/// Register, or Unregister when some copy of Phramer is listed (replacing
/// another copy's registration needs removing it first). Waits for UAC off
/// the main thread. Opens Windows Settings when only the user's choice
/// there is left to make: after Register, unless Phramer is chosen already;
/// after Unregister while Windows still sends Print Screen to Phramer.
#[tauri::command]
pub(crate) async fn screenclip_toggle() -> Result<Toggled, String> {
    tauri::async_runtime::spawn_blocking(|| {
        let unregistered = status().registered_by_phramer;
        #[cfg(not(feature = "store"))]
        let outcome = run_elevated(if unregistered {
            UNREGISTER_ARGUMENT
        } else {
            REGISTER_ARGUMENT
        });
        // The package declares it or not; nothing to do here
        #[cfg(feature = "store")]
        let outcome = Outcome::Failed;
        let status = status();
        let choice_left = if unregistered {
            status.chosen
        } else {
            !status.chosen
        };
        if outcome == Outcome::Succeeded && choice_left {
            open_default_apps_settings();
        }
        Toggled {
            outcome,
            unregistered,
            status,
        }
    })
    .await
    .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn screenclip_open_settings() {
    open_default_apps_settings();
}

/// Phramer's page in Settings > Default apps while it is listed, where it is
/// chosen; the general page otherwise, where another app is.
pub(crate) fn open_default_apps_settings() {
    let page = if status().registered_by_phramer && !cfg!(feature = "store") {
        format!("ms-settings:defaultapps?registeredAppMachine={APP_NAME}")
    } else {
        "ms-settings:defaultapps".to_owned()
    };
    open(&page);
}

fn open(target: &str) {
    use windows_sys::Win32::UI::Shell::ShellExecuteW;
    use windows_sys::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
    let wide = |text: &str| -> Vec<u16> { text.encode_utf16().chain(Some(0)).collect() };
    let (verb, file) = (wide("open"), wide(target));
    // SAFETY: NUL-terminated strings that outlive the call; the rest null.
    let result = unsafe {
        ShellExecuteW(
            std::ptr::null_mut(),
            verb.as_ptr(),
            file.as_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            SW_SHOWNORMAL,
        )
    };
    if result as usize <= 32 {
        log::error!("cannot open {target} (code {})", result as usize);
    }
}

/// The few registry calls the registration needs, on string values.
mod registry {
    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY, HKEY_CLASSES_ROOT, HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE, KEY_READ, REG_SZ,
        RRF_RT_REG_SZ, RegCloseKey, RegDeleteKeyValueW, RegDeleteKeyW, RegDeleteTreeW,
        RegGetValueW, RegOpenKeyExW, RegQueryInfoKeyW, RegSetKeyValueW,
    };

    #[derive(Debug, Clone, Copy)]
    pub(super) enum Root {
        Machine,
        User,
        /// The merged view Windows resolves a ProgId through.
        Classes,
    }

    fn hkey(root: Root) -> HKEY {
        match root {
            Root::Machine => HKEY_LOCAL_MACHINE,
            Root::User => HKEY_CURRENT_USER,
            Root::Classes => HKEY_CLASSES_ROOT,
        }
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    /// A key's default value.
    pub(super) fn read(root: Root, key: &str) -> Option<String> {
        read_named(root, key, None)
    }

    pub(super) fn read_value(root: Root, key: &str, value: &str) -> Option<String> {
        read_named(root, key, Some(value))
    }

    fn read_named(root: Root, key: &str, value: Option<&str>) -> Option<String> {
        let key = wide(key);
        let value = value.map(wide);
        let value_ptr = value
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let mut size = 0u32;
        // SAFETY: the names are NUL-terminated; a null buffer asks the size.
        let status = unsafe {
            RegGetValueW(
                hkey(root),
                key.as_ptr(),
                value_ptr,
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let mut buffer = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: `buffer` holds `size` bytes.
        let status = unsafe {
            RegGetValueW(
                hkey(root),
                key.as_ptr(),
                value_ptr,
                RRF_RT_REG_SZ,
                std::ptr::null_mut(),
                buffer.as_mut_ptr().cast(),
                &mut size,
            )
        };
        if status != ERROR_SUCCESS {
            return None;
        }
        let end = buffer
            .iter()
            .position(|&unit| unit == 0)
            .unwrap_or(buffer.len());
        Some(String::from_utf16_lossy(&buffer[..end]))
    }

    /// Writes a string; `value` `None` is the key's default value.
    pub(super) fn write(root: Root, key: &str, value: Option<&str>, data: &str) -> bool {
        let (key, value, data) = (wide(key), value.map(wide), wide(data));
        let value_ptr = value
            .as_ref()
            .map_or(std::ptr::null(), |value| value.as_ptr());
        let bytes = u32::try_from(data.len() * 2).unwrap_or(u32::MAX);
        // SAFETY: NUL-terminated names; `data` holds `bytes` bytes,
        // terminator included, as REG_SZ wants.
        let status = unsafe {
            RegSetKeyValueW(
                hkey(root),
                key.as_ptr(),
                value_ptr,
                REG_SZ,
                data.as_ptr().cast(),
                bytes,
            )
        };
        if status != ERROR_SUCCESS {
            log::error!(
                "screenclip: cannot write {}: error {status}",
                String::from_utf16_lossy(&key)
            );
        }
        status == ERROR_SUCCESS
    }

    pub(super) fn delete_tree(root: Root, key: &str) {
        let key = wide(key);
        // SAFETY: a NUL-terminated subkey of a predefined root; deletes it
        // and everything under it.
        unsafe {
            RegDeleteTreeW(hkey(root), key.as_ptr());
            RegDeleteKeyW(hkey(root), key.as_ptr());
        }
    }

    pub(super) fn delete_value(root: Root, key: &str, value: &str) {
        let (key, value) = (wide(key), wide(value));
        // SAFETY: NUL-terminated names.
        unsafe { RegDeleteKeyValueW(hkey(root), key.as_ptr(), value.as_ptr()) };
    }

    /// Deletes `key` when it has neither subkeys nor values.
    pub(super) fn delete_if_empty(root: Root, key: &str) {
        let name = wide(key);
        let mut handle: HKEY = std::ptr::null_mut();
        // SAFETY: opens a subkey for reading into `handle`.
        if unsafe { RegOpenKeyExW(hkey(root), name.as_ptr(), 0, KEY_READ, &mut handle) }
            != ERROR_SUCCESS
        {
            return;
        }
        let (mut subkeys, mut values) = (0u32, 0u32);
        // SAFETY: an open key; only the two counts are asked for.
        let status = unsafe {
            RegQueryInfoKeyW(
                handle,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
                &mut subkeys,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                &mut values,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            )
        };
        // SAFETY: closes the key opened above.
        unsafe { RegCloseKey(handle) };
        if status == ERROR_SUCCESS && subkeys == 0 && values == 0 {
            // SAFETY: a NUL-terminated subkey without subkeys.
            unsafe { RegDeleteKeyW(hkey(root), name.as_ptr()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_command_starts_a_capture_with_the_link() {
        assert_eq!(
            command(Path::new(r"C:\Program Files\Phramer\phramer.exe")),
            r#""C:\Program Files\Phramer\phramer.exe" --screenclip "%1""#
        );
    }

    #[test]
    fn only_phramer_commands_are_ever_removed() {
        for ours in [
            r#""C:\Program Files\Phramer\phramer.exe" --screenclip "%1""#,
            r#""D:\Tools\PHRAMER.EXE" --screenclip "%1""#,
            // Earlier 15.0 builds
            r#""C:\Program Files\Phramer\phramer.exe" gui"#,
        ] {
            assert!(is_phramer_command(ours), "{ours}");
        }
        for theirs in [
            r#""C:\Windows\SystemApps\snip.exe" --screenclip "%1""#,
            r#""C:\Tools\notphramer.exe" gui"#,
            r#"C:\Program Files\Phramer\phramer.exe gui"#,
            r#""C:\Program Files\Phramer\phramer.exe" --screenclip "%1" --evil"#,
            "",
        ] {
            assert!(!is_phramer_command(theirs), "{theirs}");
        }
    }
}
