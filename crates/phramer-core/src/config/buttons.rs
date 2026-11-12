//! The capture toolbar buttons stored in the `buttons` setting.

/// A toolbar button. The discriminants are the C++ `CaptureTool::Type`
/// values persisted in user settings and must never change.
///
/// Only the types the Windows build offers as buttons are listed; any other
/// number in a stored list makes the whole list invalid, as in the C++.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(i32)]
pub enum ButtonType {
    Pencil = 0,
    /// The straight-line tool (`TYPE_DRAWER`).
    Line = 1,
    Arrow = 2,
    Selection = 3,
    Rectangle = 4,
    Circle = 5,
    Marker = 6,
    MoveSelection = 8,
    Undo = 9,
    Copy = 10,
    Save = 11,
    Exit = 12,
    OpenApp = 14,
    Pixelate = 15,
    Redo = 16,
    Pin = 17,
    Text = 18,
    CircleCount = 19,
    SizeIncrease = 20,
    SizeDecrease = 21,
    Invert = 22,
    Accept = 23,
    Ocr = 25,
    OpenInEditor = 26,
    Shape = 27,
    MoveObject = 28,
    CopyFile = 29,
}

impl ButtonType {
    /// Every button, in the order of `CaptureToolButton::iterableButtonTypes`.
    pub(super) const ALL: [Self; 27] = [
        Self::Pencil,
        Self::Line,
        Self::Arrow,
        Self::Shape,
        Self::Selection,
        Self::Rectangle,
        Self::Circle,
        Self::Marker,
        Self::Text,
        Self::CircleCount,
        Self::Pixelate,
        Self::Invert,
        Self::MoveObject,
        Self::MoveSelection,
        Self::Undo,
        Self::Redo,
        Self::Copy,
        Self::CopyFile,
        Self::Save,
        Self::Exit,
        Self::OpenApp,
        Self::Ocr,
        Self::Pin,
        Self::SizeIncrease,
        Self::SizeDecrease,
        Self::Accept,
        Self::OpenInEditor,
    ];

    /// The number stored in the settings file.
    pub const fn id(self) -> i32 {
        self as i32
    }

    pub(super) fn from_id(id: i32) -> Option<Self> {
        Self::ALL.into_iter().find(|button| button.id() == id)
    }

    /// Position on the toolbar (`CaptureToolButton::getPriorityByButton`).
    fn priority(self) -> u8 {
        match self {
            Self::Pencil => 0,
            Self::Line => 1,
            Self::Arrow => 2,
            // Shape shares a slot with the selection tool it replaces.
            Self::Shape | Self::Selection => 3,
            Self::Rectangle => 4,
            Self::Circle => 5,
            Self::Marker => 6,
            Self::Text => 7,
            Self::Pixelate => 8,
            Self::Invert => 9,
            Self::CircleCount => 10,
            Self::MoveObject => 11,
            Self::MoveSelection => 12,
            Self::Undo => 13,
            Self::Redo => 14,
            Self::Copy => 15,
            Self::CopyFile => 16,
            Self::Save => 17,
            Self::Accept => 19,
            Self::OpenApp => 20,
            Self::Exit => 21,
            Self::Pin => 22,
            Self::SizeIncrease => 23,
            Self::SizeDecrease => 24,
            Self::Ocr => 25,
            Self::OpenInEditor => 26,
        }
    }
}

/// Orders buttons as the toolbar shows them.
pub(super) fn sort(buttons: &mut [ButtonType]) {
    buttons.sort_by_key(|button| button.priority());
}

/// `ButtonList::fallback`: every button except the size steppers and the
/// three tools the Shape picker consolidates.
pub(super) fn default_buttons() -> Vec<ButtonType> {
    use ButtonType::{Circle, Rectangle, Selection, SizeDecrease, SizeIncrease};
    let mut buttons: Vec<ButtonType> = ButtonType::ALL
        .into_iter()
        .filter(|button| {
            !matches!(
                button,
                SizeDecrease | SizeIncrease | Rectangle | Circle | Selection
            )
        })
        .collect();
    sort(&mut buttons);
    buttons
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ids_round_trip_and_unknown_ids_are_rejected() {
        for button in ButtonType::ALL {
            assert_eq!(ButtonType::from_id(button.id()), Some(button));
        }
        // 7 was never assigned, 13 is the Imgur uploader (not built), 24 is
        // Cancel (not a toolbar button).
        for id in [-1, 7, 13, 24, 30, 99] {
            assert_eq!(ButtonType::from_id(id), None, "{id}");
        }
    }

    #[test]
    fn default_toolbar_matches_the_cpp_fallback() {
        let ids: Vec<i32> = default_buttons().into_iter().map(ButtonType::id).collect();
        assert_eq!(
            ids,
            [
                0, 1, 2, 27, 6, 18, 15, 22, 19, 28, 8, 9, 16, 10, 29, 11, 23, 14, 12, 17, 25, 26
            ]
        );
    }
}
