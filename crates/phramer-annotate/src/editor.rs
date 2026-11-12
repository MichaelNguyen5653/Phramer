//! Editing commands and their undo history.
//!
//! As in the old editor, each edit records the whole object list as it was
//! before, and undo puts that list back. A list is small next to the image,
//! and a snapshot cannot drift out of step with the edit that made it.

use std::collections::VecDeque;

use crate::color::Color;
use crate::document::Document;
use crate::error::EditError;
use crate::tool::{Annotation, ObjectId, Shape};

/// How many edits can be undone unless configured otherwise (the old
/// `undoLimit` setting).
pub const DEFAULT_UNDO_LIMIT: usize = 100;

/// One edit to a document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    /// Places a new object on top. A counter takes the next number in the
    /// sequence, whatever count its shape carries.
    Add {
        color: Color,
        size: u32,
        shape: Shape,
    },
    /// Shifts an object by whole pixels.
    Move {
        id: ObjectId,
        dx: i32,
        dy: i32,
    },
    /// Replaces an object's geometry and tool settings: a resize, a new end
    /// point, edited text. The tool must stay the same; a counter keeps its
    /// number.
    Reshape {
        id: ObjectId,
        shape: Shape,
    },
    SetColor {
        id: ObjectId,
        color: Color,
    },
    SetSize {
        id: ObjectId,
        size: u32,
    },
    /// Removes an object. Counters numbered after a deleted counter move
    /// down by one.
    Delete {
        id: ObjectId,
    },
}

/// A document together with its undo and redo history.
#[derive(Debug, Clone)]
pub struct Editor {
    document: Document,
    undo: VecDeque<Vec<Annotation>>,
    redo: Vec<Vec<Annotation>>,
    limit: usize,
}

impl Editor {
    pub fn new(document: Document) -> Self {
        Self::with_undo_limit(document, DEFAULT_UNDO_LIMIT)
    }

    /// `limit` edits can be undone; 0 means no limit, as it did before.
    pub fn with_undo_limit(document: Document, limit: usize) -> Self {
        Self {
            document,
            undo: VecDeque::new(),
            redo: Vec::new(),
            limit,
        }
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn into_document(self) -> Document {
        self.document
    }

    pub fn undo_limit(&self) -> usize {
        self.limit
    }

    /// Changes the limit, dropping the oldest steps that no longer fit.
    pub fn set_undo_limit(&mut self, limit: usize) {
        self.limit = limit;
        self.trim();
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Chooses the number the next counter bubble will show. Not an edit:
    /// it is not recorded and undo recomputes it.
    pub fn set_next_counter(&mut self, next: u32) {
        self.document.set_next_counter(next);
    }

    /// Applies an edit and records it for undo. Returns the new object's id
    /// for [`Command::Add`]. A command that changes nothing is not recorded.
    /// Any edit clears the redo history.
    pub fn apply(&mut self, command: Command) -> Result<Option<ObjectId>, EditError> {
        let before = self.document.objects().to_vec();
        let added = self.execute(command)?;
        if self.document.objects() != before.as_slice() {
            self.undo.push_back(before);
            self.redo.clear();
            self.trim();
        }
        Ok(added)
    }

    /// Steps back one edit. Returns false when there is nothing to undo.
    pub fn undo(&mut self) -> bool {
        let Some(previous) = self.undo.pop_back() else {
            return false;
        };
        let current = self.document.restore(previous);
        self.redo.push(current);
        true
    }

    /// Steps forward one undone edit. Returns false when there is nothing to
    /// redo.
    pub fn redo(&mut self) -> bool {
        let Some(next) = self.redo.pop() else {
            return false;
        };
        let current = self.document.restore(next);
        self.undo.push_back(current);
        self.trim();
        true
    }

    fn trim(&mut self) {
        if self.limit == 0 {
            return;
        }
        while self.undo.len() > self.limit {
            self.undo.pop_front();
        }
    }

    fn execute(&mut self, command: Command) -> Result<Option<ObjectId>, EditError> {
        match command {
            Command::Add { color, size, shape } => {
                if !shape.is_drawable() {
                    return Err(EditError::NotDrawable(shape.kind()));
                }
                Ok(Some(self.document.insert(color, size, shape)))
            }
            Command::Move { id, dx, dy } => {
                self.object(id)?.shape.translate(dx, dy);
                Ok(None)
            }
            Command::Reshape { id, mut shape } => {
                let object = self.object(id)?;
                let existing = object.shape.kind();
                if existing != shape.kind() {
                    return Err(EditError::KindMismatch {
                        existing,
                        replacement: shape.kind(),
                    });
                }
                if !shape.is_drawable() {
                    return Err(EditError::NotDrawable(existing));
                }
                if let (
                    Shape::CircleCounter { count: kept, .. },
                    Shape::CircleCounter { count, .. },
                ) = (&object.shape, &mut shape)
                {
                    *count = *kept;
                }
                object.shape = shape;
                Ok(None)
            }
            Command::SetColor { id, color } => {
                self.object(id)?.color = color;
                Ok(None)
            }
            Command::SetSize { id, size } => {
                self.object(id)?.size = size;
                Ok(None)
            }
            Command::Delete { id } => {
                self.document
                    .remove(id)
                    .ok_or(EditError::UnknownObject(id))?;
                Ok(None)
            }
        }
    }

    fn object(&mut self, id: ObjectId) -> Result<&mut Annotation, EditError> {
        self.document
            .get_mut(id)
            .ok_or(EditError::UnknownObject(id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::{Point, Size};
    use crate::tool::ToolKind;

    fn p(x: i32, y: i32) -> Point {
        Point::new(x, y)
    }

    fn editor(limit: usize) -> Editor {
        Editor::with_undo_limit(Document::new(Size::new(200, 100)), limit)
    }

    fn add_line(editor: &mut Editor, y: i32) -> ObjectId {
        editor
            .apply(Command::Add {
                color: Color::BLACK,
                size: 2,
                shape: Shape::Line {
                    start: p(0, y),
                    end: p(50, y),
                },
            })
            .unwrap()
            .unwrap()
    }

    fn counter(x: i32) -> Command {
        Command::Add {
            color: Color::rgb(200, 0, 0),
            size: 3,
            shape: Shape::CircleCounter {
                center: p(x, 20),
                pointer: p(x, 20),
                count: 0,
            },
        }
    }

    fn count(editor: &Editor, id: ObjectId) -> u32 {
        match editor.document().get(id).map(|o| &o.shape) {
            Some(Shape::CircleCounter { count, .. }) => *count,
            other => panic!("not a counter: {other:?}"),
        }
    }

    #[test]
    fn default_limit_is_one_hundred() {
        assert_eq!(
            Editor::new(Document::new(Size::new(1, 1))).undo_limit(),
            100
        );
    }

    #[test]
    fn undo_and_redo_walk_the_history() {
        let mut e = editor(10);
        let a = add_line(&mut e, 10);
        e.apply(Command::Move {
            id: a,
            dx: 5,
            dy: 7,
        })
        .unwrap();
        e.apply(Command::SetColor {
            id: a,
            color: Color::rgb(1, 2, 3),
        })
        .unwrap();
        e.apply(Command::SetSize { id: a, size: 9 }).unwrap();
        let shape = |e: &Editor| {
            e.document()
                .get(a)
                .map(|o| (o.shape.clone(), o.color, o.size))
        };

        let last = shape(&e);
        assert_eq!(
            last,
            Some((
                Shape::Line {
                    start: p(5, 17),
                    end: p(55, 17)
                },
                Color::rgb(1, 2, 3),
                9
            ))
        );
        assert!(e.undo());
        assert_eq!(shape(&e).map(|s| s.2), Some(2));
        assert!(e.undo());
        assert_eq!(shape(&e).map(|s| s.1), Some(Color::BLACK));
        assert!(e.undo());
        assert_eq!(
            shape(&e).map(|s| s.0),
            Some(Shape::Line {
                start: p(0, 10),
                end: p(50, 10)
            })
        );
        assert!(e.undo());
        assert!(e.document().objects().is_empty());
        assert!(!e.undo());

        for _ in 0..4 {
            assert!(e.redo());
        }
        assert!(!e.redo());
        assert_eq!(shape(&e), last);
    }

    #[test]
    fn a_new_edit_after_undo_discards_the_redo_history() {
        let mut e = editor(10);
        add_line(&mut e, 10);
        add_line(&mut e, 20);
        assert!(e.undo());
        assert!(e.can_redo());
        add_line(&mut e, 30);
        assert!(!e.can_redo());
        assert!(!e.redo());
        let ys: Vec<_> = e
            .document()
            .objects()
            .iter()
            .filter_map(|o| {
                if let Shape::Line { start, .. } = o.shape {
                    Some(start.y)
                } else {
                    None
                }
            })
            .collect();
        assert_eq!(ys, [10, 30]);
    }

    #[test]
    fn the_limit_drops_the_oldest_steps() {
        let mut e = editor(3);
        for y in 0..5 {
            add_line(&mut e, y);
        }
        let mut undone = 0;
        while e.undo() {
            undone += 1;
        }
        assert_eq!(undone, 3);
        // The two oldest edits can no longer be undone.
        assert_eq!(e.document().objects().len(), 2);
        // Redo is not limited by what undo already holds.
        while e.redo() {}
        assert_eq!(e.document().objects().len(), 5);
    }

    #[test]
    fn lowering_the_limit_trims_and_zero_means_unlimited() {
        let mut e = editor(0);
        for y in 0..150 {
            add_line(&mut e, y);
        }
        e.set_undo_limit(2);
        assert!(e.undo());
        assert!(e.undo());
        assert!(!e.undo());
        assert_eq!(e.document().objects().len(), 148);
    }

    #[test]
    fn a_command_that_changes_nothing_is_not_recorded() {
        let mut e = editor(10);
        let a = add_line(&mut e, 10);
        e.apply(Command::Move {
            id: a,
            dx: 0,
            dy: 0,
        })
        .unwrap();
        e.apply(Command::SetColor {
            id: a,
            color: Color::BLACK,
        })
        .unwrap();
        assert!(e.undo());
        assert!(!e.can_undo());
    }

    #[test]
    fn invalid_commands_leave_document_and_history_untouched() {
        let mut e = editor(10);
        let a = add_line(&mut e, 10);
        let missing = ObjectId(999);
        assert_eq!(
            e.apply(Command::Delete { id: missing }),
            Err(EditError::UnknownObject(missing))
        );
        assert_eq!(
            e.apply(Command::Add {
                color: Color::BLACK,
                size: 1,
                shape: Shape::Line {
                    start: p(1, 1),
                    end: p(1, 1)
                },
            }),
            Err(EditError::NotDrawable(ToolKind::Line))
        );
        assert_eq!(
            e.apply(Command::Reshape {
                id: a,
                shape: Shape::Invert {
                    start: p(0, 0),
                    end: p(5, 5)
                }
            }),
            Err(EditError::KindMismatch {
                existing: ToolKind::Line,
                replacement: ToolKind::Invert
            })
        );
        assert_eq!(e.document().objects().len(), 1);
        assert!(e.undo());
        assert!(!e.can_undo());
    }

    #[test]
    fn reshape_resizes_and_keeps_a_counters_number() {
        let mut e = editor(10);
        let line = add_line(&mut e, 10);
        e.apply(Command::Reshape {
            id: line,
            shape: Shape::Line {
                start: p(0, 10),
                end: p(90, 40),
            },
        })
        .unwrap();
        assert_eq!(
            e.document().get(line).map(|o| &o.shape),
            Some(&Shape::Line {
                start: p(0, 10),
                end: p(90, 40)
            })
        );
        let first = e.apply(counter(20)).unwrap().unwrap();
        e.apply(Command::Reshape {
            id: first,
            shape: Shape::CircleCounter {
                center: p(30, 30),
                pointer: p(80, 60),
                count: 50,
            },
        })
        .unwrap();
        assert_eq!(count(&e, first), 1);
    }

    #[test]
    fn counters_renumber_on_delete_and_undo_restores_the_sequence() {
        let mut e = editor(10);
        let ids: Vec<_> = (0..4)
            .map(|i| e.apply(counter(i * 40)).unwrap().unwrap())
            .collect();
        assert_eq!(
            ids.iter().map(|id| count(&e, *id)).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        e.apply(Command::Delete { id: ids[1] }).unwrap();
        assert_eq!(count(&e, ids[0]), 1);
        assert_eq!(count(&e, ids[2]), 2);
        assert_eq!(count(&e, ids[3]), 3);
        assert_eq!(e.document().next_counter(), 4);
        assert!(e.undo());
        assert_eq!(
            ids.iter().map(|id| count(&e, *id)).collect::<Vec<_>>(),
            [1, 2, 3, 4]
        );
        assert_eq!(e.document().next_counter(), 5);
        // Undoing the additions winds the sequence back too.
        assert!(e.undo());
        assert!(e.undo());
        assert_eq!(e.document().next_counter(), 3);
        let next = e.apply(counter(200)).unwrap().unwrap();
        assert_eq!(count(&e, next), 3);
    }
}
