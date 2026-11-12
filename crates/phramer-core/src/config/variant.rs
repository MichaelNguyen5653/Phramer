//! The value forms `QSettings` stores in an INI file, ported from
//! `QSettingsPrivate::stringToVariant` and `variantToString`.

use super::escape::{self, Unescaped};

/// `QMetaType::User` as written by a `QDataStream` of version `Qt_4_0`,
/// which is what `QSettings` serializes `@Variant(...)` payloads with.
const USER_TYPE: u32 = 127;
/// Type name Qt writes for a `QList<int>`, including its terminating NUL.
const INT_LIST_TYPE_NAME: &[u8] = b"QList<int>\0";

/// A decoded INI value.
#[derive(Debug, Clone, PartialEq)]
pub(super) enum Variant {
    /// `@Invalid()`: the key is present but holds no value.
    Invalid,
    String(String),
    /// A comma-separated list.
    List(Vec<Variant>),
    /// `@ByteArray(...)`.
    ByteArray(Vec<u8>),
    /// `@Rect(x y w h)`.
    Rect {
        x: i32,
        y: i32,
        width: i32,
        height: i32,
    },
    /// `@Size(w h)`.
    Size {
        width: i32,
        height: i32,
    },
    /// `@Point(x y)`.
    Point {
        x: i32,
        y: i32,
    },
    /// `@Variant(...)` holding a `QList<int>`, the only serialized type the
    /// application stores (the `buttons` setting).
    IntList(Vec<i32>),
    /// Any other `@Variant(...)` or `@DateTime(...)`, kept as Qt's string form.
    Opaque(String),
}

impl Variant {
    /// Decodes the raw bytes right of the `=` in an entry.
    pub(super) fn decode(raw: &[u8]) -> Self {
        match escape::unescape_value(raw) {
            Unescaped::Single(text) => Self::from_qt_string(text),
            Unescaped::List(items) => {
                Self::List(items.into_iter().map(Self::from_qt_string).collect())
            }
        }
    }

    /// Encodes the value as Qt writes it right of the `=`.
    pub(super) fn encode(&self) -> String {
        match self {
            Self::List(items) => {
                let strings: Vec<String> = items.iter().map(Self::to_qt_string).collect();
                escape::escape_string_list(&strings)
            }
            other => {
                let mut out = String::new();
                escape::escape_string(&other.to_qt_string(), &mut out);
                out
            }
        }
    }

    /// `QVariant::toString()`: empty for values with no string conversion.
    pub(super) fn to_text(&self) -> String {
        match self {
            Self::String(text) => text.clone(),
            Self::ByteArray(bytes) => String::from_utf8_lossy(bytes).into_owned(),
            _ => String::new(),
        }
    }

    /// `QVariant::toStringList()`, or `None` where `canConvert` is false.
    pub(super) fn to_string_list(&self) -> Option<Vec<String>> {
        match self {
            Self::String(text) => Some(vec![text.clone()]),
            Self::List(items) => Some(items.iter().map(Self::to_text).collect()),
            _ => None,
        }
    }

    /// `QSettingsPrivate::stringToVariant`.
    fn from_qt_string(text: String) -> Self {
        if !text.starts_with('@') {
            return Self::String(text);
        }
        if let Some(inner) = text.strip_suffix(')') {
            if let Some(bytes) = inner.strip_prefix("@ByteArray(") {
                return Self::ByteArray(latin1_bytes(bytes));
            }
            if let Some(string) = inner.strip_prefix("@String(") {
                return Self::String(string.to_owned());
            }
            if let Some(payload) = inner.strip_prefix("@Variant(") {
                return match decode_int_list(&latin1_bytes(payload)) {
                    Some(list) => Self::IntList(list),
                    None => Self::Opaque(text),
                };
            }
            if inner.starts_with("@DateTime(") {
                return Self::Opaque(text);
            }
            if let Some(&[x, y, width, height]) = int_args(inner, "@Rect(").as_deref() {
                return Self::Rect {
                    x,
                    y,
                    width,
                    height,
                };
            }
            if let Some(&[width, height]) = int_args(inner, "@Size(").as_deref() {
                return Self::Size { width, height };
            }
            if let Some(&[x, y]) = int_args(inner, "@Point(").as_deref() {
                return Self::Point { x, y };
            }
            if text == "@Invalid()" {
                return Self::Invalid;
            }
        }
        match text.strip_prefix("@@") {
            Some(rest) => Self::String(format!("@{rest}")),
            None => Self::String(text),
        }
    }

    /// `QSettingsPrivate::variantToString`.
    fn to_qt_string(&self) -> String {
        match self {
            Self::Invalid => "@Invalid()".to_owned(),
            Self::String(text) if text.contains('\0') => format!("@String({text})"),
            Self::String(text) if text.starts_with('@') => format!("@{text}"),
            Self::String(text) => text.clone(),
            Self::ByteArray(bytes) => format!("@ByteArray({})", latin1_string(bytes)),
            Self::Rect {
                x,
                y,
                width,
                height,
            } => format!("@Rect({x} {y} {width} {height})"),
            Self::Size { width, height } => format!("@Size({width} {height})"),
            Self::Point { x, y } => format!("@Point({x} {y})"),
            Self::IntList(list) => format!("@Variant({})", latin1_string(&encode_int_list(list))),
            Self::Opaque(text) => text.clone(),
            // Qt cannot nest lists in an INI value either; flatten to text.
            Self::List(items) => items
                .iter()
                .map(Self::to_text)
                .collect::<Vec<_>>()
                .join(", "),
        }
    }
}

/// `QString::toLatin1()`: characters outside Latin-1 become `?`.
fn latin1_bytes(text: &str) -> Vec<u8> {
    text.chars()
        .map(|c| u8::try_from(u32::from(c)).unwrap_or(b'?'))
        .collect()
}

fn latin1_string(bytes: &[u8]) -> String {
    bytes.iter().map(|&b| char::from(b)).collect()
}

/// Splits `@Name(a b c` (closing parenthesis already removed) into integers,
/// as Qt's `splitArgs` does; unparsable numbers read as 0 like `toInt()`.
fn int_args(inner: &str, prefix: &str) -> Option<Vec<i32>> {
    let args = inner.strip_prefix(prefix)?;
    Some(
        args.split(' ')
            .map(|arg| arg.parse().unwrap_or(0))
            .collect(),
    )
}

fn take<'a>(bytes: &mut &'a [u8], count: usize) -> Option<&'a [u8]> {
    let (head, tail) = bytes.split_at_checked(count)?;
    *bytes = tail;
    Some(head)
}

fn take_u32(bytes: &mut &[u8]) -> Option<u32> {
    take(bytes, 4)?.try_into().ok().map(u32::from_be_bytes)
}

/// Reads a `QVariant` stream holding a `QList<int>`: type id, type name,
/// element count, then big-endian 32-bit elements.
fn decode_int_list(mut bytes: &[u8]) -> Option<Vec<i32>> {
    if take_u32(&mut bytes)? != USER_TYPE {
        return None;
    }
    let name_len = usize::try_from(take_u32(&mut bytes)?).ok()?;
    if take(&mut bytes, name_len)? != INT_LIST_TYPE_NAME {
        return None;
    }
    let count = usize::try_from(take_u32(&mut bytes)?).ok()?;
    let elements = take(&mut bytes, count.checked_mul(4)?)?;
    let (chunks, _) = elements.as_chunks::<4>();
    Some(chunks.iter().map(|&chunk| i32::from_be_bytes(chunk)).collect())
}

fn encode_int_list(list: &[i32]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(12 + INT_LIST_TYPE_NAME.len() + list.len() * 4);
    bytes.extend_from_slice(&USER_TYPE.to_be_bytes());
    // Both lengths are far below `u32::MAX`: the name is a constant and the
    // list holds toolbar buttons.
    bytes.extend_from_slice(&(INT_LIST_TYPE_NAME.len() as u32).to_be_bytes());
    bytes.extend_from_slice(INT_LIST_TYPE_NAME);
    bytes.extend_from_slice(&(list.len() as u32).to_be_bytes());
    for value in list {
        bytes.extend_from_slice(&value.to_be_bytes());
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `buttons` value from `phramer.example.ini`, written by Qt itself.
    const QT_BUTTONS: &str = r"@Variant(\0\0\0\x7f\0\0\0\vQList<int>\0\0\0\0\x14\0\0\0\0\0\0\0\x1\0\0\0\x2\0\0\0\x3\0\0\0\x4\0\0\0\x5\0\0\0\x6\0\0\0\x12\0\0\0\xf\0\0\0\x13\0\0\0\a\0\0\0\b\0\0\0\t\0\0\0\x10\0\0\0\n\0\0\0\v\0\0\0\f\0\0\0\r\0\0\0\xe\0\0\0\x11)";

    fn decode(raw: &str) -> Variant {
        Variant::decode(raw.as_bytes())
    }

    #[test]
    fn int_list_matches_qt_byte_for_byte() {
        let expected = vec![
            0, 1, 2, 3, 4, 5, 6, 18, 15, 19, 7, 8, 9, 16, 10, 11, 12, 13, 14, 17,
        ];
        let variant = decode(QT_BUTTONS);
        assert_eq!(variant, Variant::IntList(expected));
        assert_eq!(variant.encode(), QT_BUTTONS);
    }

    #[test]
    fn int_list_with_separator_bytes_is_quoted_and_reads_back() {
        // 44, 59 and 61 are `,` `;` `=`, which force Qt to quote the value.
        let variant = Variant::IntList(vec![44, 59, 61, -1, 0x4142_4344]);
        let encoded = variant.encode();
        assert!(
            encoded.starts_with('"') && encoded.ends_with('"'),
            "{encoded}"
        );
        assert_eq!(decode(&encoded), variant);
    }

    #[test]
    fn truncated_or_foreign_variants_stay_opaque() {
        let truncated = r"@Variant(\0\0\0\x7f\0\0\0\vQList<int>\0\0\0\0\x14\0\0\0\x1)";
        assert!(matches!(decode(truncated), Variant::Opaque(_)));
        let colour = r"@Variant(\0\0\0\x43\x1\xff\xff\0\0\0\0\0\0\0\0)";
        let variant = decode(colour);
        assert!(matches!(variant, Variant::Opaque(_)));
        assert_eq!(variant.encode(), colour);
        assert_eq!(variant.to_text(), "");
    }

    #[test]
    fn geometry_forms_round_trip() {
        let cases = [
            (
                "@Rect(10 -20 300 400)",
                Variant::Rect {
                    x: 10,
                    y: -20,
                    width: 300,
                    height: 400,
                },
            ),
            (
                "@Size(640 480)",
                Variant::Size {
                    width: 640,
                    height: 480,
                },
            ),
            ("@Point(-5 7)", Variant::Point { x: -5, y: 7 }),
            ("@Invalid()", Variant::Invalid),
        ];
        for (text, expected) in cases {
            assert_eq!(decode(text), expected, "{text}");
            assert_eq!(expected.encode(), text);
        }
        // Wrong arity is not a rectangle; Qt keeps the plain string.
        assert_eq!(
            decode("@Rect(1 2 3)"),
            Variant::String("@Rect(1 2 3)".into())
        );
    }

    #[test]
    fn byte_arrays_and_at_strings_round_trip() {
        let bytes = decode(r"@ByteArray(ab\0\xff\x1)");
        assert_eq!(bytes, Variant::ByteArray(vec![b'a', b'b', 0, 0xff, 1]));
        assert_eq!(bytes.encode(), r"@ByteArray(ab\0\xff\x1)");

        assert_eq!(decode("@@home"), Variant::String("@home".into()));
        assert_eq!(Variant::String("@home".into()).encode(), "@@home");

        assert_eq!(decode(r"@String(a\0z)"), Variant::String("a\0z".into()));
        assert_eq!(Variant::String("a\0z".into()).encode(), r"@String(a\0z)");
    }

    #[test]
    fn lists_decode_each_element() {
        let list = decode("picker, #ff0000, @Invalid()");
        assert_eq!(
            list,
            Variant::List(vec![
                Variant::String("picker".into()),
                Variant::String("#ff0000".into()),
                Variant::Invalid
            ])
        );
        assert_eq!(
            list.to_string_list(),
            Some(vec!["picker".into(), "#ff0000".into(), String::new()])
        );
        assert_eq!(list.to_text(), "");
        assert_eq!(Variant::List(Vec::new()).encode(), "@Invalid()");
    }
}
