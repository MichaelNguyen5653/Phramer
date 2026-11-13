//! `phramer config --check`: 15.0.0's `ConfigHandler::checkForErrors`, with
//! its messages and its order.
//!
//! Three checks run in turn and the first that fails ends the check, so a
//! file with unknown keys reports only those: unrecognized names, shortcut
//! conflicts, then values a key's value handler rejects. Keys are visited in
//! `QSettings::allKeys()` order, which is sorted.

use std::collections::BTreeSet;
use std::path::Path;

use super::buttons::ButtonType;
use super::color::Color;
use super::keyseq;
use super::settings::{GENERAL_RULES, SHORTCUTS, Settings, checked_extension};
use super::variant::Variant;

const SHORTCUTS_PREFIX: &str = "Shortcuts/";

/// How a General key's value handler checks a value (`ValueHandler::check`)
/// and what it says it expected (`ValueHandler::expected`).
#[derive(Debug, Clone, Copy)]
pub(super) enum Rule {
    Bool,
    /// `String` and `FilenamePattern`: anything goes.
    Any,
    Color,
    /// `BoundedInt(min, max)`.
    Bounded(i32, i32),
    /// `LowerBoundedInt(min)`.
    AtLeast(i32),
    ExistingDir,
    SaveFileExtension,
    ButtonList,
    /// `UserColors(min, max)`.
    UserColors(i32, i32),
}

impl Rule {
    fn accepts(self, value: &Variant) -> bool {
        let text = value.to_text();
        match self {
            Self::Bool => text == "true" || text == "false",
            Self::Any => true,
            Self::Color => Color::from_setting(&text).is_some(),
            Self::Bounded(min, max) => to_int(&text).is_some_and(|n| (min..=max).contains(&n)),
            Self::AtLeast(min) => to_int(&text).is_some_and(|n| n >= min),
            Self::ExistingDir => !text.is_empty() && Path::new(&text).is_dir(),
            Self::SaveFileExtension => checked_extension(&text).is_some(),
            // Anything that is not a list of numbers reads as an empty list
            Self::ButtonList => match value {
                Variant::IntList(ids) => ids.iter().all(|&id| ButtonType::from_id(id).is_some()),
                _ => true,
            },
            Self::UserColors(min, max) => value.to_string_list().is_some_and(|names| {
                names
                    .iter()
                    .all(|name| name == "picker" || Color::from_qt_name(name).is_some())
                    && i32::try_from(names.len()).is_ok_and(|n| (min..=max).contains(&n))
            }),
        }
    }

    /// 15.0.0's text, quirks included: `QString::arg(min, max)` takes `max`
    /// as a field width, so `min` is right-aligned in `max` columns and `%2`
    /// is never filled in.
    fn expected(self) -> String {
        match self {
            Self::Bool => "true or false".to_owned(),
            Self::Any => "string".to_owned(),
            Self::Color => "color name or hex value".to_owned(),
            Self::Bounded(min, max) => format!("number between {} and %2", padded(min, max)),
            Self::AtLeast(min) => format!("number >= {min}"),
            Self::ExistingDir => "existing directory".to_owned(),
            Self::SaveFileExtension => "supported image extension".to_owned(),
            Self::ButtonList => "please don't edit by hand".to_owned(),
            Self::UserColors(min, max) => format!(
                "list of colors(min {} and max %2) separated by comma",
                padded(min - 1, max - 1)
            ),
        }
    }
}

/// `QString::number(value)` right-aligned in `width` columns.
fn padded(value: i32, width: i32) -> String {
    let width = usize::try_from(width).unwrap_or(0);
    format!("{value:>width$}")
}

/// `QString::toInt`.
fn to_int(text: &str) -> Option<i32> {
    text.trim().parse().ok()
}

/// `QFileInfo::baseName`: the last path part up to its first dot.
fn base_name(key: &str) -> &str {
    let name = key.rsplit(['/', '\\']).next().unwrap_or(key);
    name.split('.').next().unwrap_or(name)
}

fn general_rule(key: &str) -> Option<Rule> {
    GENERAL_RULES
        .iter()
        .find(|(name, _)| *name == key)
        .map(|&(_, rule)| rule)
}

fn is_shortcut_name(name: &str) -> bool {
    SHORTCUTS.iter().any(|(known, _)| *known == name)
}

impl Settings {
    /// What `phramer config --check` reports, one message per problem in
    /// 15.0.0's words; empty when the file has no errors.
    pub fn errors(&self) -> Vec<String> {
        let keys = self.doc.keys();
        let mut errors = unrecognized(&keys);
        if errors.is_empty() {
            errors = self.shortcut_conflicts(&keys);
        }
        if errors.is_empty() {
            errors = self.bad_values(&keys);
        }
        errors
    }

    /// `checkShortcutConflicts`: two actions explicitly given the same text.
    fn shortcut_conflicts(&self, keys: &[&str]) -> Vec<String> {
        let shortcuts: Vec<(&str, String)> = keys
            .iter()
            .filter_map(|key| {
                let name = key.strip_prefix(SHORTCUTS_PREFIX)?;
                Some((name, self.doc.get(key)?.to_text()))
            })
            .collect();
        let mut reported = BTreeSet::new();
        let mut errors = Vec::new();
        for (index, (first, value)) in shortcuts.iter().enumerate() {
            for (second, other) in &shortcuts[index + 1..] {
                if value.is_empty() || value != other {
                    continue;
                }
                if !reported.contains(first) && !reported.contains(second) {
                    reported.insert(*first);
                    reported.insert(*second);
                    errors.push(format!(
                        "Shortcut conflict: '{first}' and '{second}' have the same shortcut: {value}"
                    ));
                }
            }
        }
        errors
    }

    /// `checkSemantics`: values of known keys their handler rejects.
    fn bad_values(&self, keys: &[&str]) -> Vec<String> {
        keys.iter()
            .filter_map(|&key| {
                let value = self
                    .doc
                    .get(key)
                    .filter(|value| **value != Variant::Invalid)?;
                let failed = match general_rule(key) {
                    Some(rule) => (!rule.accepts(value)).then(|| rule.expected()),
                    None if key.starts_with(SHORTCUTS_PREFIX)
                        && is_shortcut_name(base_name(key)) =>
                    {
                        (!keyseq::is_valid(&value.to_text()))
                            .then(|| "keyboard shortcut".to_owned())
                    }
                    None => None,
                }?;
                Some(format!("Bad value in '{key}'. Expected: {failed}"))
            })
            .collect()
    }
}

/// `checkUnrecognizedSettings`: General keys first, then shortcut names.
/// Keys in any other section are not checked.
fn unrecognized(keys: &[&str]) -> Vec<String> {
    let general = keys
        .iter()
        .filter(|key| !key.contains('/') && general_rule(key).is_none())
        .map(|key| format!("Unrecognized setting: '{key}'"));
    let shortcuts: BTreeSet<&str> = keys
        .iter()
        .filter(|key| key.starts_with(SHORTCUTS_PREFIX))
        .map(|key| base_name(key))
        .filter(|name| !is_shortcut_name(name))
        .collect();
    general
        .chain(
            shortcuts
                .into_iter()
                .map(|name| format!("Unrecognized shortcut name: '{name}'.")),
        )
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn errors(text: &str) -> Vec<String> {
        Settings::parse(text.as_bytes()).errors()
    }

    #[test]
    fn a_valid_or_empty_file_has_no_errors() {
        assert!(errors("").is_empty());
        assert!(
            errors(
                "[General]\nshowHelp=false\nundoLimit= 50 \nuiColor=#ff000080\nuiLanguage=@Invalid()\n\
                 filenamePattern=\n[Shortcuts]\nTYPE_PIN=\nTYPE_SHAPE=\nTYPE_COPY=Ctrl+C\nTYPE_SAVE=ctrl+c\n\
                 [Elsewhere]\nanything=1\n"
            )
            .is_empty()
        );
    }

    #[test]
    fn unknown_names_end_the_check() {
        assert_eq!(
            errors("zeta=1\nalpha=2\nshowHelp=maybe\n[Shortcuts]\nTYPE_NOPE=x\nTYPE_A.b=y\n"),
            [
                "Unrecognized setting: 'alpha'",
                "Unrecognized setting: 'zeta'",
                "Unrecognized shortcut name: 'TYPE_A'.",
                "Unrecognized shortcut name: 'TYPE_NOPE'.",
            ]
        );
    }

    #[test]
    fn each_pair_of_conflicting_shortcuts_is_named_once() {
        assert_eq!(
            errors(
                "undoLimit=1000\n[Shortcuts]\nTYPE_TEXT=K\nTYPE_PENCIL=K\nTYPE_ARROW=K\nTYPE_UNDO=x\nTYPE_REDO=x\n"
            ),
            [
                "Shortcut conflict: 'TYPE_ARROW' and 'TYPE_PENCIL' have the same shortcut: K",
                "Shortcut conflict: 'TYPE_REDO' and 'TYPE_UNDO' have the same shortcut: x",
            ]
        );
    }

    #[test]
    fn bad_values_name_what_was_expected() {
        assert_eq!(
            errors(
                "arrowStyle=2\nshowHelp=yes\ndrawThickness=0\nuserColors=picker, red\n\
                 [Shortcuts]\nTYPE_COPY=Ctrl+Nope\n"
            ),
            [
                "Bad value in 'Shortcuts/TYPE_COPY'. Expected: keyboard shortcut".to_owned(),
                "Bad value in 'arrowStyle'. Expected: number between 0 and %2".to_owned(),
                "Bad value in 'drawThickness'. Expected: number >= 1".to_owned(),
                "Bad value in 'showHelp'. Expected: true or false".to_owned(),
                format!(
                    "Bad value in 'userColors'. Expected: list of colors(min {}2 and max %2) separated by comma",
                    " ".repeat(15)
                ),
            ]
        );
        let undo = &errors("undoLimit=1000\n")[0];
        assert_eq!(
            *undo,
            format!(
                "Bad value in 'undoLimit'. Expected: number between {}0 and %2",
                " ".repeat(998)
            )
        );
    }

    #[test]
    fn every_general_key_has_a_rule_with_a_message() {
        for (key, rule) in GENERAL_RULES {
            assert!(!rule.expected().is_empty(), "{key}");
        }
    }
}
