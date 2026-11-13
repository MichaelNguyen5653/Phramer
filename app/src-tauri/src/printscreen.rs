//! Windows' own claim on the Print Screen key, as 15.0.0's
//! `PrintScreenKey` read and wrote it: the per-user DWORD
//! `HKCU\Control Panel\Keyboard\PrintScreenKeyForSnippingEnabled`. While it
//! is not 0, Windows opens its snipping tool on Print Screen and Phramer
//! never sees the key. Only HKCU is touched, so no administrator approval is
//! needed, and the change can be undone from Settings (15.0.0 could only
//! turn it off).

use std::io;

const KEY: &str = "Control Panel\\Keyboard";
const VALUE: &str = "PrintScreenKeyForSnippingEnabled";

/// Whether Windows takes Print Screen for its snipping tool. An absent value
/// counts as taking it, as 15.0.0 read it (`value(SnippingValue, 1)`).
pub(crate) fn snipping_claims_key() -> bool {
    registry::read_dword(KEY, VALUE).is_none_or(|value| value != 0)
}

/// Gives Print Screen to Phramer (`claim == false`) or back to Windows.
pub(crate) fn set_snipping_claims_key(claim: bool) -> io::Result<()> {
    registry::write_dword(KEY, VALUE, u32::from(claim))
}

mod registry {
    use std::io;

    use windows_sys::Win32::Foundation::ERROR_SUCCESS;
    use windows_sys::Win32::System::Registry::{
        HKEY_CURRENT_USER, REG_DWORD, RRF_RT_REG_DWORD, RegGetValueW, RegSetKeyValueW,
    };

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(Some(0)).collect()
    }

    pub(super) fn read_dword(key: &str, value: &str) -> Option<u32> {
        let (key, value) = (wide(key), wide(value));
        let mut data: u32 = 0;
        let mut size = u32::try_from(size_of::<u32>()).unwrap_or(4);
        // SAFETY: both names are NUL-terminated and outlive the call; `data`
        // holds `size` bytes, which RRF_RT_REG_DWORD fills exactly.
        let status = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                RRF_RT_REG_DWORD,
                std::ptr::null_mut(),
                (&raw mut data).cast(),
                &mut size,
            )
        };
        (status == ERROR_SUCCESS).then_some(data)
    }

    pub(super) fn write_dword(key: &str, value: &str, data: u32) -> io::Result<()> {
        let (key, value) = (wide(key), wide(value));
        let size = u32::try_from(size_of::<u32>()).unwrap_or(4);
        // SAFETY: both names are NUL-terminated and outlive the call; the
        // data pointer covers the `size` bytes of one DWORD.
        let status = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                key.as_ptr(),
                value.as_ptr(),
                REG_DWORD,
                (&raw const data).cast(),
                size,
            )
        };
        if status == ERROR_SUCCESS {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }

    /// Removes the value; an absent one is not an error. Tests use it to put
    /// back a value that was not there.
    #[cfg(test)]
    pub(super) fn delete(key: &str, value: &str) -> io::Result<()> {
        use windows_sys::Win32::Foundation::ERROR_FILE_NOT_FOUND;
        use windows_sys::Win32::System::Registry::RegDeleteKeyValueW;

        let (key, value) = (wide(key), wide(value));
        // SAFETY: both names are NUL-terminated and outlive the call.
        let status = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, key.as_ptr(), value.as_ptr()) };
        if status == ERROR_SUCCESS || status == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(io::Error::from_raw_os_error(status as i32))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Puts the user's value back however the test ends.
    struct Restore(Option<u32>);

    impl Drop for Restore {
        fn drop(&mut self) {
            let restored = match self.0 {
                Some(value) => registry::write_dword(KEY, VALUE, value),
                None => registry::delete(KEY, VALUE),
            };
            if let Err(error) = restored {
                eprintln!("could not restore {KEY}\\{VALUE}: {error}");
            }
        }
    }

    #[test]
    fn print_screen_goes_to_phramer_and_back() {
        let before = registry::read_dword(KEY, VALUE);
        let _restore = Restore(before);

        set_snipping_claims_key(false).unwrap();
        assert_eq!(registry::read_dword(KEY, VALUE), Some(0));
        assert!(!snipping_claims_key());

        set_snipping_claims_key(true).unwrap();
        assert_eq!(registry::read_dword(KEY, VALUE), Some(1));
        assert!(snipping_claims_key());

        // An absent value is Windows' default: it takes the key
        registry::delete(KEY, VALUE).unwrap();
        assert!(snipping_claims_key());
        drop(_restore);
        assert_eq!(registry::read_dword(KEY, VALUE), before);
    }
}
