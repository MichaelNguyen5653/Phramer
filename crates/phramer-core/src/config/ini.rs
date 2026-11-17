//! A lossless model of a `QSettings` INI file.
//!
//! The file is split the way Qt's `readIniLine` splits it; everything that is
//! not an entry (comments, blank lines, unparsable lines) is kept as raw
//! bytes, and an entry keeps its original bytes until it is assigned. Writing
//! an unmodified document therefore reproduces the input exactly.

use super::escape;
use super::variant::Variant;

const BOM: &[u8] = b"\xEF\xBB\xBF";
const GENERAL: &str = "General";

#[derive(Debug, Clone)]
struct Entry {
    /// Full key as `QSettings` exposes it: `name` for the General section,
    /// `Section/name` otherwise.
    key: String,
    /// The entry as it stands in the file, without its line terminator.
    raw: Vec<u8>,
    /// Offset of the `=` inside `raw`.
    equals: usize,
    value: Variant,
}

#[derive(Debug, Clone)]
enum Item {
    /// Whitespace, comments and lines Qt skips.
    Other(Vec<u8>),
    /// A `[Section]` header; `prefix` is prepended to the keys below it.
    Section {
        raw: Vec<u8>,
        prefix: String,
    },
    Entry(Entry),
}

/// An INI file held in memory.
#[derive(Debug, Clone)]
pub(super) struct IniDocument {
    items: Vec<Item>,
    eol: &'static str,
}

impl Default for IniDocument {
    fn default() -> Self {
        // `QSettings` writes CRLF on Windows, the only platform shipped.
        Self {
            items: Vec::new(),
            eol: "\r\n",
        }
    }
}

fn is_space(byte: u8) -> bool {
    matches!(byte, b' ' | b'\t' | b'\n' | b'\r' | 0x0b | 0x0c)
}

struct Line {
    start: usize,
    end: usize,
    equals: Option<usize>,
}

/// `QConfFileSettingsPrivate::readIniLine`: finds the next logical line at or
/// after `pos`. Quotes and backslashes can make it span physical lines, and a
/// `;` outside quotes ends it.
fn read_line(data: &[u8], pos: usize) -> Option<Line> {
    let len = data.len();
    let mut start = pos;
    while start < len && is_space(data[start]) {
        start += 1;
    }
    let mut in_quotes = false;
    let mut equals = None;
    let mut i = start;
    while i < len {
        let ch = data[i];
        i += 1;
        match ch {
            b'=' if !in_quotes && equals.is_none() => equals = Some(i - 1),
            b'\n' | b'\r' => {
                if i == start + 1 {
                    start += 1;
                } else if !in_quotes {
                    i -= 1;
                    break;
                }
            }
            b'\\' if i < len => {
                let escaped = data[i];
                i += 1;
                if i < len && matches!((escaped, data[i]), (b'\n', b'\r') | (b'\r', b'\n')) {
                    i += 1;
                }
            }
            b'"' => in_quotes = !in_quotes,
            b';' => {
                if i == start + 1 {
                    // A comment: skip it and the whitespace after it.
                    while i < len && data[i] != b'\n' && data[i] != b'\r' {
                        i += 1;
                    }
                    while i < len && is_space(data[i]) {
                        i += 1;
                    }
                    start = i;
                } else if !in_quotes {
                    i -= 1;
                    break;
                }
            }
            _ => {}
        }
    }
    (i > start).then_some(Line {
        start,
        end: i,
        equals,
    })
}

/// Key prefix for a section name, as `readIniFile` derives it.
fn section_prefix(name: &[u8]) -> String {
    let name = name.trim_ascii();
    if name.eq_ignore_ascii_case(b"general") {
        String::new()
    } else if name.eq_ignore_ascii_case(b"%general") {
        // The escaped spelling of a real group called "general".
        format!("{}/", String::from_utf8_lossy(&name[1..]))
    } else {
        format!("{}/", escape::unescape_key(name))
    }
}

impl IniDocument {
    pub(super) fn parse(data: &[u8]) -> Self {
        let eol = if data.contains(&b'\n') && !data.windows(2).any(|pair| pair == b"\r\n") {
            "\n"
        } else {
            "\r\n"
        };
        let mut items = Vec::new();
        let mut prefix = String::new();
        let mut pos = if data.starts_with(BOM) { BOM.len() } else { 0 };
        let mut consumed = 0;

        while let Some(line) = read_line(data, pos) {
            if line.start > consumed {
                items.push(Item::Other(data[consumed..line.start].to_vec()));
            }
            let raw = data[line.start..line.end].to_vec();
            if raw.starts_with(b"[") {
                let name_end = raw.iter().position(|&b| b == b']').unwrap_or(raw.len());
                prefix = section_prefix(&raw[1..name_end]);
                items.push(Item::Section {
                    raw,
                    prefix: prefix.clone(),
                });
            } else if let Some(equals) = line.equals.map(|at| at - line.start) {
                let name = escape::unescape_key(raw[..equals].trim_ascii());
                let value = Variant::decode(&raw[equals + 1..]);
                items.push(Item::Entry(Entry {
                    key: format!("{prefix}{name}"),
                    raw,
                    equals,
                    value,
                }));
            } else {
                items.push(Item::Other(raw));
            }
            pos = line.end;
            consumed = line.end;
        }
        if consumed < data.len() {
            items.push(Item::Other(data[consumed..].to_vec()));
        }
        Self { items, eol }
    }

    pub(super) fn to_bytes(&self) -> Vec<u8> {
        let mut out = Vec::new();
        for item in &self.items {
            match item {
                Item::Other(raw) | Item::Section { raw, .. } => out.extend_from_slice(raw),
                Item::Entry(entry) => out.extend_from_slice(&entry.raw),
            }
        }
        out
    }

    fn entries(&self) -> impl DoubleEndedIterator<Item = &Entry> {
        self.items.iter().filter_map(|item| match item {
            Item::Entry(entry) => Some(entry),
            _ => None,
        })
    }

    /// The stored value; when a key is repeated the last one wins, as in Qt.
    pub(super) fn get(&self, key: &str) -> Option<&Variant> {
        self.entries()
            .rev()
            .find(|entry| entry.key == key)
            .map(|entry| &entry.value)
    }

    /// Every key once, sorted, as `QSettings::allKeys` lists them.
    pub(super) fn keys(&self) -> Vec<&str> {
        let mut keys: Vec<&str> = self.entries().map(|entry| entry.key.as_str()).collect();
        keys.sort_unstable();
        keys.dedup();
        keys
    }

    /// Names and values of the keys directly inside `section`.
    pub(super) fn section_entries<'a>(
        &'a self,
        section: &'a str,
    ) -> impl Iterator<Item = (&'a str, &'a Variant)> {
        self.entries().filter_map(move |entry| {
            let name = entry.key.strip_prefix(section)?.strip_prefix('/')?;
            Some((name, &entry.value))
        })
    }

    /// Assigns `key`, rewriting its entry in place or adding one to its
    /// section. No other byte of the document changes.
    pub(super) fn set(&mut self, key: &str, value: Variant) {
        let encoded = value.encode();
        let existing = self.items.iter_mut().rev().find_map(|item| match item {
            Item::Entry(entry) if entry.key == key => Some(entry),
            _ => None,
        });
        if let Some(entry) = existing {
            entry.raw.truncate(entry.equals + 1);
            entry.raw.extend_from_slice(encoded.as_bytes());
            entry.value = value;
            return;
        }

        let (section, name) = key.split_once('/').unwrap_or((GENERAL, key));
        let prefix = if section == GENERAL {
            String::new()
        } else {
            format!("{section}/")
        };
        let raw = format!("{}={encoded}", escape::escape_key(name)).into_bytes();
        let equals = raw.len() - encoded.len() - 1;
        let entry = Item::Entry(Entry {
            key: key.to_owned(),
            raw,
            equals,
            value,
        });

        match self.last_item_of_section(&prefix) {
            Some(index) => self.insert_line_after(index, entry),
            None => self.append_section(section, entry),
        }
    }

    /// Index of the last header or entry belonging to the section `prefix`.
    fn last_item_of_section(&self, prefix: &str) -> Option<usize> {
        let mut current = "";
        let mut last = None;
        for (index, item) in self.items.iter().enumerate() {
            match item {
                Item::Section {
                    prefix: section, ..
                } => {
                    current = section;
                    if current == prefix {
                        last = Some(index);
                    }
                }
                Item::Entry(_) if current == prefix => last = Some(index),
                _ => {}
            }
        }
        last
    }

    /// Puts `entry` on its own line directly below the line of `index`.
    fn insert_line_after(&mut self, index: usize, entry: Item) {
        let eol = self.eol.as_bytes();
        let line_end = match self.items.get(index + 1) {
            Some(Item::Other(raw)) => raw.iter().position(|&b| b == b'\n').map(|at| at + 1),
            _ => None,
        };
        match (self.items.get_mut(index + 1), line_end) {
            // The line continues (trailing comment) up to a terminator:
            // split there and slot the entry in between.
            (Some(Item::Other(raw)), Some(line_end)) => {
                let mut rest = eol.to_vec();
                rest.extend_from_slice(&raw[line_end..]);
                raw.truncate(line_end);
                self.items
                    .splice(index + 2..index + 2, [entry, Item::Other(rest)]);
            }
            // The line is the last of the file and has no terminator.
            (Some(Item::Other(raw)), None) => {
                raw.extend_from_slice(eol);
                self.items
                    .splice(index + 2..index + 2, [entry, Item::Other(eol.to_vec())]);
            }
            _ => {
                let new_items = [Item::Other(eol.to_vec()), entry, Item::Other(eol.to_vec())];
                self.items.splice(index + 1..index + 1, new_items);
            }
        }
    }

    fn append_section(&mut self, section: &str, entry: Item) {
        let eol = self.eol.as_bytes();
        let existing = self.to_bytes();
        let content = existing.strip_prefix(BOM).unwrap_or(&existing);
        if !content.is_empty() {
            let mut lead = Vec::new();
            if !content.ends_with(b"\n") {
                lead.extend_from_slice(eol);
            }
            // Qt separates sections with a blank line.
            lead.extend_from_slice(eol);
            self.items.push(Item::Other(lead));
        }
        let prefix = if section == GENERAL {
            String::new()
        } else {
            format!("{section}/")
        };
        let header = format!("[{}]", escape::escape_key(section)).into_bytes();
        self.items.extend([
            Item::Section {
                raw: header,
                prefix,
            },
            Item::Other(eol.to_vec()),
            entry,
            Item::Other(eol.to_vec()),
        ]);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(doc: &IniDocument) -> String {
        String::from_utf8(doc.to_bytes()).expect("test documents are UTF-8")
    }

    fn string(value: &str) -> Variant {
        Variant::String(value.to_owned())
    }

    const MESSY: &str = "\u{feff}; leading comment\r\n\r\ntopLevel = 1\r\n[General]\r\n  spaced = \" kept \"  ; trailing\r\n\
        multi=\"line one\nline two\"\r\ncontinued=a\\\r\nb\r\nnot an entry\r\n\r\n[Shortcuts]\r\nTYPE_COPY=Ctrl+C\r\n\
        [Odd%20Group]\r\nk%20ey=@Rect(1 2 3 4)\r\ndup=1\r\ndup=2";

    #[test]
    fn unmodified_documents_are_reproduced_exactly() {
        for source in [
            MESSY,
            "",
            "\n\n",
            "a=1",
            "[General]\na=1\n",
            "; only a comment",
            "\u{feff}",
        ] {
            assert_eq!(text(&IniDocument::parse(source.as_bytes())), source);
        }
    }

    #[test]
    fn invalid_utf8_survives_a_round_trip() {
        let source = b"name=caf\xe9\n\xff\xfe junk\n";
        assert_eq!(IniDocument::parse(source).to_bytes(), source);
    }

    #[test]
    fn entries_are_read_like_qsettings() {
        let doc = IniDocument::parse(MESSY.as_bytes());
        assert_eq!(doc.get("topLevel"), Some(&string("1")));
        assert_eq!(doc.get("spaced"), Some(&string(" kept ")));
        assert_eq!(doc.get("multi"), Some(&string("line one\nline two")));
        assert_eq!(doc.get("continued"), Some(&string("ab")));
        assert_eq!(doc.get("Shortcuts/TYPE_COPY"), Some(&string("Ctrl+C")));
        assert_eq!(
            doc.get("Odd Group/k ey"),
            Some(&Variant::Rect {
                x: 1,
                y: 2,
                width: 3,
                height: 4
            })
        );
        assert_eq!(doc.get("Odd Group/dup"), Some(&string("2")));
        assert_eq!(doc.get("not an entry"), None);
        assert_eq!(doc.get("missing"), None);
    }

    #[test]
    fn section_names_ignore_case_only_for_general() {
        let doc = IniDocument::parse(b"[GENERAL]\na=1\n[%General]\nb=2\n[shortcuts]\nc=3\n");
        assert_eq!(doc.get("a"), Some(&string("1")));
        assert_eq!(doc.get("General/b"), Some(&string("2")));
        assert_eq!(doc.get("shortcuts/c"), Some(&string("3")));
        assert_eq!(doc.get("Shortcuts/c"), None);
    }

    #[test]
    fn commented_out_entries_are_not_entries() {
        let doc = IniDocument::parse(b";[General]\n;showHelp=false\n");
        assert_eq!(doc.get("showHelp"), None);
    }

    #[test]
    fn assigning_an_existing_key_touches_only_its_value() {
        let mut doc = IniDocument::parse(MESSY.as_bytes());
        doc.set("spaced", string("new;value"));
        doc.set("Odd Group/dup", string("3"));
        let expected = MESSY
            .replace(
                "spaced = \" kept \"  ; trailing",
                "spaced =\"new;value\"; trailing",
            )
            .replace("dup=2", "dup=3");
        assert_eq!(text(&doc), expected);
        assert_eq!(doc.get("spaced"), Some(&string("new;value")));
    }

    #[test]
    fn new_keys_join_their_section() {
        let mut doc = IniDocument::parse(b"[General]\na=1 ; note\n\n[Shortcuts]\nX=1");
        doc.set("b", string("2"));
        doc.set("Shortcuts/Y", string("Ctrl+Y"));
        doc.set("Extra/z", Variant::List(vec![string("p"), string("q")]));
        assert_eq!(
            text(&doc),
            "[General]\na=1 ; note\nb=2\n\n[Shortcuts]\nX=1\nY=Ctrl+Y\n\n[Extra]\nz=p, q\n"
        );
        assert_eq!(doc.get("b"), Some(&string("2")));
        assert_eq!(doc.get("Shortcuts/Y"), Some(&string("Ctrl+Y")));
    }

    #[test]
    fn keys_before_any_header_belong_to_general() {
        let mut doc = IniDocument::parse(b"a=1\n[Shortcuts]\nX=1\n");
        doc.set("b", string("2"));
        assert_eq!(text(&doc), "a=1\nb=2\n[Shortcuts]\nX=1\n");
        assert_eq!(doc.get("b"), Some(&string("2")));
    }

    #[test]
    fn a_missing_general_section_is_appended() {
        let mut doc = IniDocument::parse(b"[Shortcuts]\nX=1");
        doc.set("b", string("2"));
        assert_eq!(text(&doc), "[Shortcuts]\nX=1\n\n[General]\nb=2\n");
        assert_eq!(doc.get("b"), Some(&string("2")));
    }

    #[test]
    fn an_empty_document_is_written_the_way_qt_does() {
        let mut doc = IniDocument::default();
        doc.set("showHelp", string("false"));
        doc.set("Shortcuts/TYPE_COPY", string("Ctrl+c"));
        assert_eq!(
            text(&doc),
            "[General]\r\nshowHelp=false\r\n\r\n[Shortcuts]\r\nTYPE_COPY=Ctrl+c\r\n"
        );
    }

    #[test]
    fn section_entries_lists_direct_children() {
        let doc = IniDocument::parse(b"a=1\n[Shortcuts]\nX=1\nY=2\n[ShortcutsExtra]\nZ=3\n");
        let names: Vec<&str> = doc
            .section_entries("Shortcuts")
            .map(|(name, _)| name)
            .collect();
        assert_eq!(names, ["X", "Y"]);
    }
}
