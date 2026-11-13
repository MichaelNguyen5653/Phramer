//! The annotation engine as a WebAssembly module for the editor window, so
//! the editor draws with the very code the capture overlay draws with and
//! their pixels are identical.
//!
//! Built for `wasm32-unknown-unknown` with no binding generator: the module
//! exports a handful of plain functions (all prefixed `pa_`) and the editor
//! moves bytes through linear memory itself (`app/src/editor/engine.ts`).
//!
//! Inputs never pass as pointers into Rust. The caller asks for the staging
//! buffer with [`pa_stage`], writes its bytes there, then calls the function
//! that consumes them. Answers are left in the session's output buffer,
//! found with [`pa_out_ptr`] and [`pa_out_len`]. A pointer stays valid until
//! the next call into the module.
//!
//! Every function is also ordinary Rust, so the same code is tested natively.

mod session;

use std::sync::{Mutex, MutexGuard};

use phramer_annotate::Color;

pub use session::{BUNDLED_FAMILY, BUNDLED_FONT, Edit, RENDER_HEADER, Request, Session};

/// Sessions by handle (index + 1) and the bytes of the next call.
struct Registry {
    sessions: Vec<Option<Session>>,
    staging: Vec<u8>,
}

static REGISTRY: Mutex<Registry> = Mutex::new(Registry {
    sessions: Vec::new(),
    staging: Vec::new(),
});

/// Call status: done, failed (the output holds the reason), or the handle
/// names no session.
pub const OK: u32 = 0;
pub const FAILED: u32 = 1;
pub const NO_SESSION: u32 = 2;

fn registry() -> MutexGuard<'static, Registry> {
    // Nothing can panic while holding it, and the module is single-threaded
    REGISTRY
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Registry {
    fn session(&mut self, handle: u32) -> Option<&mut Session> {
        let index = usize::try_from(handle).ok()?.checked_sub(1)?;
        self.sessions.get_mut(index)?.as_mut()
    }

    fn take_staging(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.staging)
    }
}

/// Makes the staging buffer `len` bytes long (zeroed) and returns where it
/// starts, for the caller to fill before the next call.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_stage(len: u32) -> *mut u8 {
    let mut registry = registry();
    registry.staging.clear();
    registry.staging.resize(len as usize, 0);
    registry.staging.as_mut_ptr()
}

/// A session over the staged straight RGBA8 pixels of a `width` x `height`
/// image. `bundled_font` (non-zero) registers the overlay's font first.
/// Returns the handle, or 0 when the pixels do not fit the size.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_session_new(width: u32, height: u32, bundled_font: u32) -> u32 {
    let mut registry = registry();
    let rgba = registry.take_staging();
    match Session::new(width, height, rgba, bundled_font != 0) {
        Ok(session) => {
            registry.sessions.push(Some(session));
            u32::try_from(registry.sessions.len()).unwrap_or(0)
        }
        Err(_) => 0,
    }
}

/// Drops a session; its handle is never reused.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_session_free(handle: u32) {
    let mut registry = registry();
    if let Some(slot) = usize::try_from(handle)
        .ok()
        .and_then(|handle| handle.checked_sub(1))
        .and_then(|index| registry.sessions.get_mut(index))
    {
        *slot = None;
    }
}

/// Registers a font: the staging buffer holds the family name's UTF-8
/// (`family_len` bytes) followed by the font file.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_add_font(handle: u32, bold: u32, italic: u32, family_len: u32) -> u32 {
    let mut registry = registry();
    let mut staged = registry.take_staging();
    let Some(session) = registry.session(handle) else {
        return NO_SESSION;
    };
    let split = (family_len as usize).min(staged.len());
    let data = staged.split_off(split);
    let family = String::from_utf8_lossy(&staged).into_owned();
    match session.add_font(&family, bold != 0, italic != 0, data) {
        Ok(()) => OK,
        Err(error) => {
            session.out = error.into_bytes();
            FAILED
        }
    }
}

/// Answers the staged JSON request (see [`Request`]); the answer, or the
/// reason it failed, is left as JSON in the output.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_call(handle: u32) -> u32 {
    let mut registry = registry();
    let staged = registry.take_staging();
    let Some(session) = registry.session(handle) else {
        return NO_SESSION;
    };
    let request = String::from_utf8_lossy(&staged);
    match session.call(&request) {
        Ok(value) => {
            session.out = value.to_string().into_bytes();
            OK
        }
        Err(error) => {
            session.out = serde_json::Value::String(error).to_string().into_bytes();
            FAILED
        }
    }
}

/// Renders the document with the staged JSON array of preview edits on
/// top; `workspace` is the 0xRRGGBBAA colour past the image. The output
/// holds [`RENDER_HEADER`] bytes of bounds, then the pixels.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_render(handle: u32, workspace: u32) -> u32 {
    let mut registry = registry();
    let staged = registry.take_staging();
    let Some(session) = registry.session(handle) else {
        return NO_SESSION;
    };
    let preview: Vec<Edit> = if staged.is_empty() {
        Vec::new()
    } else {
        match serde_json::from_slice(&staged) {
            Ok(preview) => preview,
            Err(error) => {
                session.out = error.to_string().into_bytes();
                return FAILED;
            }
        }
    };
    let [r, g, b, a] = workspace.to_be_bytes();
    match session.render(preview, Color::rgba(r, g, b, a)) {
        Ok(()) => OK,
        Err(error) => {
            session.out = error.into_bytes();
            FAILED
        }
    }
}

/// Where the session's output starts; null for an unknown handle.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_out_ptr(handle: u32) -> *const u8 {
    let mut registry = registry();
    registry
        .session(handle)
        .map_or(std::ptr::null(), |session| session.out.as_ptr())
}

/// The session's output length in bytes.
// SAFETY: a unique `pa_` symbol; nothing else in a binary defines it.
#[unsafe(no_mangle)]
pub extern "C" fn pa_out_len(handle: u32) -> u32 {
    let mut registry = registry();
    registry
        .session(handle)
        .map_or(0, |session| u32::try_from(session.out.len()).unwrap_or(0))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The registry is global: these tests take turns.
    static TURN: Mutex<()> = Mutex::new(());

    fn turn() -> MutexGuard<'static, ()> {
        TURN.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn stage(bytes: &[u8]) {
        let mut registry = registry();
        registry.staging = bytes.to_vec();
    }

    fn output(handle: u32) -> Vec<u8> {
        let mut registry = registry();
        registry
            .session(handle)
            .map(|session| session.out.clone())
            .unwrap_or_default()
    }

    #[test]
    fn the_exported_functions_move_bytes_through_the_staging_buffer() {
        let _turn = turn();
        stage(&[200; 2 * 2 * 4]);
        let handle = pa_session_new(2, 2, 0);
        assert_ne!(handle, 0);
        stage(br#"{"op":"state"}"#);
        assert_eq!(pa_call(handle), OK);
        let state: serde_json::Value = serde_json::from_slice(&output(handle)).unwrap();
        assert_eq!(state["objects"], 0);
        assert_eq!(pa_out_len(handle) as usize, output(handle).len());
        assert!(!pa_out_ptr(handle).is_null());

        stage(b"");
        assert_eq!(pa_render(handle, 0xffff_ffff), OK);
        assert_eq!(output(handle).len(), RENDER_HEADER + 16);

        stage(b"{");
        assert_eq!(pa_call(handle), FAILED);
        pa_session_free(handle);
        stage(br#"{"op":"state"}"#);
        assert_eq!(pa_call(handle), NO_SESSION);
        assert!(pa_out_ptr(handle).is_null());
    }

    #[test]
    fn mismatched_pixels_make_no_session() {
        let _turn = turn();
        stage(&[0; 3]);
        assert_eq!(pa_session_new(2, 2, 0), 0);
    }

    #[test]
    fn fonts_are_split_from_their_family_name() {
        let _turn = turn();
        stage(&[0; 4]);
        let handle = pa_session_new(1, 1, 0);
        let mut staged = b"Inter".to_vec();
        staged.extend_from_slice(BUNDLED_FONT);
        stage(&staged);
        assert_eq!(pa_add_font(handle, 0, 0, 5), OK);
        stage(b"Bad");
        assert_eq!(pa_add_font(handle, 0, 0, 3), FAILED);
    }
}
