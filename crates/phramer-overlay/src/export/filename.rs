//! File names for saved captures: the `filenamePattern` expansion of
//! `strfparse.cpp` and the path rules of `FileNameHandler`.

use std::path::{Path, PathBuf};

/// The `strftime` conversions a pattern may use (`create_specifier_list`).
/// Anything else after a `%` is left as typed.
const SPECIFIERS: &[char] = &[
    'a', 'A', 'b', 'B', 'c', 'C', 'd', 'D', 'e', 'F', 'g', 'G', 'h', 'H', 'I', 'j', 'm', 'M', 'n',
    'p', 'r', 'R', 'S', 't', 'T', 'u', 'U', 'V', 'w', 'W', 'x', 'X', 'y', 'Y', 'z', 'Z',
];

/// Formats one `strftime` conversion (`%` + the character) for the time
/// the capture is saved.
pub trait TimeFormat {
    fn format(&self, specifier: char) -> String;
}

/// `FileNameHandler::parseFilename`: every known `%x` replaced by its value,
/// then `/` and `:` made safe for a file name.
///
/// Each conversion found in the pattern is replaced everywhere in it, in
/// the order of the conversion characters, exactly as `format_time_string`
/// does; the replacements never contain `%`, so the order cannot matter.
pub fn expand_pattern(pattern: &str, time: &dyn TimeFormat) -> String {
    let mut found: Vec<char> = pattern
        .split('%')
        .skip(1)
        .filter_map(|after| after.chars().next())
        .filter(|c| SPECIFIERS.contains(c))
        .collect();
    found.sort_unstable();
    found.dedup();

    let mut expanded = pattern.to_owned();
    for specifier in found {
        expanded = expanded.replace(&format!("%{specifier}"), &time.format(specifier));
    }
    // A slash would start a folder and a colon is not allowed in a name;
    // the slash becomes U+2044 FRACTION SLASH, which looks the same
    expanded.replace('/', "\u{2044}").replace(':', "-")
}

/// `FileNameHandler::properScreenshotPath`: where a capture saved to `path`
/// goes.
///
/// A folder gets a name from the pattern (already expanded in `name`); a
/// file keeps its name. The extension becomes `format` when one is set,
/// otherwise the file's own or `png`. An existing file is never replaced:
/// `_1`, `_2`, ... is added to the name until it is free.
pub fn screenshot_path(path: &Path, format: &str, name: &str) -> PathBuf {
    let (stem_path, own_extension) = if path.is_dir() {
        (path.join(name), None)
    } else {
        let extension = path
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned());
        (path.with_extension(""), extension)
    };
    let extension = if !format.is_empty() {
        format.to_owned()
    } else {
        own_extension
            .filter(|extension| !extension.is_empty())
            .unwrap_or_else(|| "png".to_owned())
    };
    let candidate = append_extension(&stem_path, &extension);
    if candidate.exists() {
        numbered_free_path(&stem_path, &extension)
    } else {
        candidate
    }
}

/// `path` + `.` + `extension`, without touching a dot already in the name
/// (a pattern like `%F_%H.%M` keeps its dot).
fn append_extension(path: &Path, extension: &str) -> PathBuf {
    let mut text = path.as_os_str().to_owned();
    text.push(".");
    text.push(extension);
    PathBuf::from(text)
}

/// `autoNumerateDuplicate`: the first `<name>_<n>.<extension>` that does not
/// exist, counting from 1.
fn numbered_free_path(stem_path: &Path, extension: &str) -> PathBuf {
    let mut number: u64 = 1;
    loop {
        let mut text = stem_path.as_os_str().to_owned();
        text.push(format!("_{number}"));
        let candidate = append_extension(Path::new(&text), extension);
        if !candidate.exists() {
            return candidate;
        }
        number += 1;
    }
}

/// The local time through the C runtime's `strftime`, the same function and
/// "C" locale 15.0.0 formatted with, so names come out identical.
#[cfg(windows)]
pub use crt::LocalTime;

#[cfg(windows)]
mod crt {
    use std::ffi::{c_char, c_int};

    use super::TimeFormat;

    /// `struct tm` of the Microsoft C runtime.
    #[repr(C)]
    #[derive(Debug, Clone, Copy, Default)]
    struct Tm {
        tm_sec: c_int,
        tm_min: c_int,
        tm_hour: c_int,
        tm_mday: c_int,
        tm_mon: c_int,
        tm_year: c_int,
        tm_wday: c_int,
        tm_yday: c_int,
        tm_isdst: c_int,
    }

    unsafe extern "C" {
        fn _time64(destination: *mut i64) -> i64;
        fn _localtime64_s(result: *mut Tm, time: *const i64) -> c_int;
        fn strftime(
            buffer: *mut c_char,
            size: usize,
            format: *const c_char,
            time: *const Tm,
        ) -> usize;
    }

    /// A moment in local time.
    #[derive(Debug, Clone, Copy)]
    pub struct LocalTime {
        tm: Tm,
    }

    impl LocalTime {
        /// Now.
        pub fn now() -> Self {
            let mut tm = Tm::default();
            // SAFETY: `_time64` accepts a null destination and only returns
            // the time; `_localtime64_s` writes one `Tm` through a valid
            // pointer and reads the time it is given. On failure `tm` stays
            // zeroed, which still formats.
            unsafe {
                let now = _time64(std::ptr::null_mut());
                _localtime64_s(&mut tm, &now);
            }
            Self { tm }
        }

        /// A fixed local date and time (month 1 to 12), for tests.
        pub fn at(year: i32, month: i32, day: i32, hour: i32, minute: i32, second: i32) -> Self {
            let days_before = [0, 31, 59, 90, 120, 151, 181, 212, 243, 273, 304, 334];
            let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
            let month_index = (month - 1).clamp(0, 11) as usize;
            let yday = days_before[month_index] + day - 1 + i32::from(leap && month > 2);
            // Zeller-free weekday: days since 1970-01-01 (a Thursday)
            let years = i64::from(year) - 1970;
            let leaps_before = |y: i64| y / 4 - y / 100 + y / 400;
            let days = years * 365 + leaps_before(i64::from(year) - 1) - leaps_before(1969)
                + i64::from(yday);
            let wday = (days + 4).rem_euclid(7) as i32;
            Self {
                tm: Tm {
                    tm_sec: second,
                    tm_min: minute,
                    tm_hour: hour,
                    tm_mday: day,
                    tm_mon: month - 1,
                    tm_year: year - 1900,
                    tm_wday: wday,
                    tm_yday: yday,
                    tm_isdst: 0,
                },
            }
        }
    }

    impl TimeFormat for LocalTime {
        fn format(&self, specifier: char) -> String {
            let mut format = [0u8; 3];
            format[0] = b'%';
            // Only ASCII conversions from the allowed list reach here; any
            // other character would make the C runtime abort
            if !specifier.is_ascii_alphabetic() {
                return String::new();
            }
            format[1] = specifier as u8;
            let mut buffer = [0u8; 128];
            // SAFETY: `format` is a NUL-terminated conversion the runtime
            // knows, `buffer` holds the length passed, and `tm` is a fully
            // initialised `struct tm`. The result is the number of bytes
            // written, without the NUL.
            let written = unsafe {
                strftime(
                    buffer.as_mut_ptr().cast(),
                    buffer.len(),
                    format.as_ptr().cast(),
                    &self.tm,
                )
            };
            String::from_utf8_lossy(&buffer[..written.min(buffer.len())]).into_owned()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Fixed;

    impl TimeFormat for Fixed {
        fn format(&self, specifier: char) -> String {
            match specifier {
                'F' => "2026-11-11".into(),
                'H' => "20".into(),
                'M' => "26".into(),
                'T' => "20:26:57".into(),
                'D' => "11/11/26".into(),
                other => format!("<{other}>"),
            }
        }
    }

    #[test]
    fn expands_the_default_pattern() {
        assert_eq!(expand_pattern("%F_%H-%M", &Fixed), "2026-11-11_20-26");
    }

    #[test]
    fn unknown_conversions_and_a_trailing_percent_stay() {
        // 15.0.0 meant to drop a trailing '%' but formatted the original
        assert_eq!(expand_pattern("shot %Q %%H %", &Fixed), "shot %Q %20 %");
    }

    #[test]
    fn slashes_and_colons_are_made_safe() {
        assert_eq!(
            expand_pattern("%T on %D", &Fixed),
            "20-26-57 on 11\u{2044}11\u{2044}26"
        );
    }

    #[test]
    fn repeated_conversions_are_all_replaced() {
        assert_eq!(expand_pattern("%H%H", &Fixed), "2020");
    }
}
