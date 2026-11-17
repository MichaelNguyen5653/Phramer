//! Qt's INI escaping rules, ported from `QSettingsPrivate::iniEscaped*` and
//! `iniUnescaped*` (Qt 6, where INI files are UTF-8).

/// Right-hand side of an entry after unescaping: Qt yields a list as soon as
/// it meets a comma outside quotes.
#[derive(Debug, PartialEq)]
pub(super) enum Unescaped {
    Single(String),
    List(Vec<String>),
}

fn hex_digit(byte: u8) -> Option<u16> {
    (byte as char).to_digit(16).map(|d| d as u16)
}

fn oct_digit(byte: u8) -> Option<u16> {
    (byte as char).to_digit(8).map(|d| d as u16)
}

/// A UTF-16 code unit as Qt appends it; lone surrogates cannot be kept.
fn code_unit(unit: u16) -> char {
    char::from_u32(u32::from(unit)).unwrap_or(char::REPLACEMENT_CHARACTER)
}

fn simple_escape(byte: u8) -> Option<char> {
    Some(match byte {
        b'a' => '\x07',
        b'b' => '\x08',
        b'f' => '\x0c',
        b'n' => '\n',
        b'r' => '\r',
        b't' => '\t',
        b'v' => '\x0b',
        b'"' => '"',
        b'?' => '?',
        b'\'' => '\'',
        b'\\' => '\\',
        _ => return None,
    })
}

fn chop_trailing_spaces(text: &mut String, limit: usize) {
    while text.len() > limit && text.ends_with([' ', '\t']) {
        text.pop();
    }
}

/// Reads digits into a wrapping 16-bit value, as Qt's `StHexEscape` and
/// `StOctEscape` states do: they take every digit available.
fn read_escape_digits(
    raw: &[u8],
    i: &mut usize,
    shift: u32,
    digit: fn(u8) -> Option<u16>,
    mut value: u16,
) -> char {
    while let Some(d) = raw.get(*i).copied().and_then(digit) {
        value = (value << shift).wrapping_add(d);
        *i += 1;
    }
    code_unit(value)
}

/// `QSettingsPrivate::iniUnescapedStringList`.
pub(super) fn unescape_value(raw: &[u8]) -> Unescaped {
    let to = raw.len();
    let mut items: Vec<String> = Vec::new();
    let mut is_list = false;
    let mut current = String::new();
    let mut in_quotes = false;
    let mut current_is_quoted = false;
    let mut skip_spaces = true;
    let mut i = 0;

    // Each pass of this loop is one entry into Qt's `StNormal` state, which
    // re-reads the chop limit; `skip_spaces` is the `StSkipSpaces` prelude.
    'normal: loop {
        if skip_spaces {
            while i < to && (raw[i] == b' ' || raw[i] == b'\t') {
                i += 1;
            }
            skip_spaces = false;
        }
        let chop_limit = current.len();
        while i < to {
            match raw[i] {
                b'\\' => {
                    i += 1;
                    if i >= to {
                        break 'normal;
                    }
                    let ch = raw[i];
                    i += 1;
                    if let Some(c) = simple_escape(ch) {
                        current.push(c);
                        continue 'normal;
                    }
                    if ch == b'x' {
                        if i >= to {
                            break 'normal;
                        }
                        if hex_digit(raw[i]).is_some() {
                            current.push(read_escape_digits(raw, &mut i, 4, hex_digit, 0));
                            if i >= to {
                                break 'normal;
                            }
                            continue 'normal;
                        }
                    } else if let Some(first) = oct_digit(ch) {
                        current.push(read_escape_digits(raw, &mut i, 3, oct_digit, first));
                        if i >= to {
                            break 'normal;
                        }
                        continue 'normal;
                    } else if (ch == b'\n' || ch == b'\r') && i < to {
                        // Backslash-newline continues the line; swallow the
                        // second half of a two-byte line terminator.
                        let next = raw[i];
                        if (next == b'\n' || next == b'\r') && next != ch {
                            i += 1;
                        }
                    }
                    // Any other escaped character is dropped.
                }
                b'"' => {
                    i += 1;
                    current_is_quoted = true;
                    in_quotes = !in_quotes;
                    if !in_quotes {
                        skip_spaces = true;
                        continue 'normal;
                    }
                }
                b',' if !in_quotes => {
                    if !current_is_quoted {
                        chop_trailing_spaces(&mut current, chop_limit);
                    }
                    is_list = true;
                    items.push(std::mem::take(&mut current));
                    current_is_quoted = false;
                    i += 1;
                    skip_spaces = true;
                    continue 'normal;
                }
                _ => {
                    let mut j = i + 1;
                    while j < to && !matches!(raw[j], b'\\' | b'"' | b',') {
                        j += 1;
                    }
                    current.push_str(&String::from_utf8_lossy(&raw[i..j]));
                    i = j;
                }
            }
        }
        if !current_is_quoted {
            chop_trailing_spaces(&mut current, chop_limit);
        }
        break;
    }

    if is_list {
        items.push(current);
        Unescaped::List(items)
    } else {
        Unescaped::Single(current)
    }
}

/// `QSettingsPrivate::iniEscapedString`, appended to `out`.
pub(super) fn escape_string(text: &str, out: &mut String) {
    // Serialized binary payloads are Latin-1 bytes, never UTF-8 text.
    let binary = text.starts_with("@ByteArray(")
        || text.starts_with("@Variant(")
        || text.starts_with("@DateTime(");
    let start = out.len();
    let mut needs_quotes = false;
    let mut escape_next_if_digit = false;

    for ch in text.chars() {
        if matches!(ch, ';' | ',' | '=') {
            needs_quotes = true;
        }
        if escape_next_if_digit && ch.is_ascii_hexdigit() {
            // Otherwise the digit would be read as part of the escape before it.
            out.push_str(&format!("\\x{:x}", ch as u32));
            continue;
        }
        escape_next_if_digit = false;
        match ch {
            '\0' => {
                out.push_str("\\0");
                escape_next_if_digit = true;
            }
            '\x07' => out.push_str("\\a"),
            '\x08' => out.push_str("\\b"),
            '\x0c' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            '\x0b' => out.push_str("\\v"),
            '"' | '\\' => {
                out.push('\\');
                out.push(ch);
            }
            _ if ch <= '\x1f' || (ch >= '\x7f' && binary) => {
                out.push_str(&format!("\\x{:x}", ch as u32));
                escape_next_if_digit = true;
            }
            _ => out.push(ch),
        }
    }

    let written = &out[start..];
    if needs_quotes || written.starts_with(' ') || written.ends_with(' ') {
        out.insert(start, '"');
        out.push('"');
    }
}

/// `QSettingsPrivate::iniEscapedStringList`.
pub(super) fn escape_string_list(items: &[String]) -> String {
    if items.is_empty() {
        // Qt has no empty-list marker; an invalid variant reads back as one.
        return "@Invalid()".to_owned();
    }
    let mut out = String::new();
    for (index, item) in items.iter().enumerate() {
        if index != 0 {
            out.push_str(", ");
        }
        escape_string(item, &mut out);
    }
    out
}

/// `QSettingsPrivate::iniUnescapedKey`: `%XX`, `%UXXXX` and `\` for `/`.
pub(super) fn unescape_key(raw: &[u8]) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut i = 0;
    while i < raw.len() {
        let ch = raw[i];
        if ch == b'\\' {
            out.push('/');
            i += 1;
            continue;
        }
        if ch != b'%' || i == raw.len() - 1 {
            // Key bytes outside escapes are Latin-1.
            out.push(ch as char);
            i += 1;
            continue;
        }
        let (first_digit, digits) = if raw[i + 1] == b'U' {
            (i + 2, 4)
        } else {
            (i + 1, 2)
        };
        let unit = raw
            .get(first_digit..first_digit + digits)
            .and_then(|hex| std::str::from_utf8(hex).ok())
            .filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()))
            .and_then(|hex| u16::from_str_radix(hex, 16).ok());
        match unit {
            Some(unit) => {
                out.push(code_unit(unit));
                i = first_digit + digits;
            }
            None => {
                out.push('%');
                i += 1;
            }
        }
    }
    out
}

/// `QSettingsPrivate::iniEscapedKey`.
pub(super) fn escape_key(key: &str) -> String {
    let mut out = String::with_capacity(key.len());
    let mut units = [0u16; 2];
    for ch in key.chars() {
        match ch {
            '/' => out.push('\\'),
            _ if ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-' | '.') => out.push(ch),
            _ if ch <= '\u{ff}' => out.push_str(&format!("%{:02X}", ch as u32)),
            _ => {
                for unit in ch.encode_utf16(&mut units) {
                    out.push_str(&format!("%U{unit:04X}"));
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn single(raw: &str) -> String {
        match unescape_value(raw.as_bytes()) {
            Unescaped::Single(text) => text,
            Unescaped::List(items) => panic!("expected a single value, got {items:?}"),
        }
    }

    fn list(raw: &str) -> Vec<String> {
        match unescape_value(raw.as_bytes()) {
            Unescaped::List(items) => items,
            Unescaped::Single(text) => panic!("expected a list, got {text:?}"),
        }
    }

    fn escaped(text: &str) -> String {
        let mut out = String::new();
        escape_string(text, &mut out);
        out
    }

    #[test]
    fn plain_values_lose_surrounding_blanks() {
        assert_eq!(single("  hello world \t"), "hello world");
        assert_eq!(single(""), "");
    }

    #[test]
    fn quoted_values_keep_blanks_and_separators() {
        assert_eq!(single("\" a, b;c=d \""), " a, b;c=d ");
        assert_eq!(single("\"a\"   "), "a");
    }

    #[test]
    fn escapes_are_decoded() {
        assert_eq!(single(r"a\tb\n\\\x41\x7f\0\101"), "a\tb\n\\A\u{7f}\0A");
        // A hex escape takes every hex digit that follows it.
        assert_eq!(single(r"\x41\x62"), "Ab");
        assert_eq!(single(r"\x4142"), "\u{4142}");
        // Unknown escapes drop the character; a trailing backslash is ignored.
        assert_eq!(single(r"a\qb\"), "ab");
    }

    #[test]
    fn escaped_trailing_blank_is_not_chopped() {
        assert_eq!(single(r"a\x20"), "a ");
    }

    #[test]
    fn commas_split_into_a_list() {
        assert_eq!(
            list("picker, #800000,#ff0000 "),
            ["picker", "#800000", "#ff0000"]
        );
        assert_eq!(list("a,"), ["a", ""]);
        assert_eq!(list("\"a,b\", c"), ["a,b", "c"]);
    }

    #[test]
    fn utf8_text_is_read_and_written_verbatim() {
        assert_eq!(single("Größe 日本"), "Größe 日本");
        assert_eq!(escaped("Größe 日本"), "Größe 日本");
    }

    #[test]
    fn strings_are_escaped_like_qt() {
        assert_eq!(escaped("plain"), "plain");
        assert_eq!(escaped("a;b"), "\"a;b\"");
        assert_eq!(escaped("a,b=c"), "\"a,b=c\"");
        assert_eq!(escaped(" lead"), "\" lead\"");
        assert_eq!(escaped("tab\there \"q\" \\"), "tab\\there \\\"q\\\" \\\\");
        // A hex digit after an escape must be escaped too.
        assert_eq!(escaped("\u{1}1g"), "\\x1\\x31g");
        assert_eq!(escaped("\0a"), "\\0\\x61");
    }

    #[test]
    fn binary_payloads_escape_high_bytes() {
        assert_eq!(
            escaped("@Variant(\0\u{7f}\u{ff})"),
            "@Variant(\\0\\x7f\\xff)"
        );
    }

    #[test]
    fn escaped_strings_read_back() {
        for text in [
            "a;b, c",
            " x ",
            "\0\u{1}1\u{7f}",
            "q\"uote\\",
            "日本,語",
            "@Variant(\0\u{b}QList<int>\0)",
        ] {
            assert_eq!(single(&escaped(text)), text, "round-trip of {text:?}");
        }
    }

    #[test]
    fn list_escaping_marks_the_empty_list() {
        assert_eq!(escape_string_list(&[]), "@Invalid()");
        assert_eq!(
            escape_string_list(&["a".into(), "b;c".into()]),
            "a, \"b;c\""
        );
    }

    #[test]
    fn keys_escape_and_unescape() {
        assert_eq!(escape_key("a b/c_d-e.f"), "a%20b\\c_d-e.f");
        assert_eq!(escape_key("é日"), "%E9%U65E5");
        assert_eq!(unescape_key(b"a%20b\\c"), "a b/c");
        assert_eq!(unescape_key(b"%E9%U65E5"), "é日");
        assert_eq!(unescape_key(b"100%"), "100%");
        assert_eq!(unescape_key(b"%zz"), "%zz");
    }
}
