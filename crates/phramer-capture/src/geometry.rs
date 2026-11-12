//! Points and rectangles, one pair of types per coordinate space.
//!
//! See the crate documentation for what the three spaces mean. The types are
//! structurally identical on purpose: the only way from one space to another
//! is through a [`Monitor`](crate::Monitor).

macro_rules! coordinate_space {
    ($(#[$point_doc:meta])* $point:ident, $(#[$rect_doc:meta])* $rect:ident) => {
        $(#[$point_doc])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $point {
            pub x: i32,
            pub y: i32,
        }

        impl $point {
            pub const fn new(x: i32, y: i32) -> Self {
                Self { x, y }
            }
        }

        $(#[$rect_doc])*
        ///
        /// The right and bottom edges are exclusive. A rectangle with a
        /// width or height of zero or less is empty.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
        pub struct $rect {
            pub x: i32,
            pub y: i32,
            pub width: i32,
            pub height: i32,
        }

        impl $rect {
            pub const fn new(x: i32, y: i32, width: i32, height: i32) -> Self {
                Self { x, y, width, height }
            }

            /// From left, top and the exclusive right and bottom edges.
            pub const fn from_edges(left: i32, top: i32, right: i32, bottom: i32) -> Self {
                Self::new(left, top, right.saturating_sub(left), bottom.saturating_sub(top))
            }

            pub const fn origin(&self) -> $point {
                $point::new(self.x, self.y)
            }

            /// The exclusive right edge.
            pub const fn right(&self) -> i32 {
                self.x.saturating_add(self.width)
            }

            /// The exclusive bottom edge.
            pub const fn bottom(&self) -> i32 {
                self.y.saturating_add(self.height)
            }

            pub const fn is_empty(&self) -> bool {
                self.width <= 0 || self.height <= 0
            }

            pub const fn contains(&self, point: $point) -> bool {
                !self.is_empty()
                    && point.x >= self.x
                    && point.x < self.right()
                    && point.y >= self.y
                    && point.y < self.bottom()
            }

            /// The overlap of both rectangles, or `None` when they do not
            /// overlap.
            pub fn intersection(&self, other: &Self) -> Option<Self> {
                let overlap = Self::from_edges(
                    self.x.max(other.x),
                    self.y.max(other.y),
                    self.right().min(other.right()),
                    self.bottom().min(other.bottom()),
                );
                (!self.is_empty() && !other.is_empty() && !overlap.is_empty()).then_some(overlap)
            }

            /// The smallest rectangle holding both. An empty rectangle
            /// contributes nothing.
            pub fn union(&self, other: &Self) -> Self {
                if self.is_empty() {
                    return *other;
                }
                if other.is_empty() {
                    return *self;
                }
                Self::from_edges(
                    self.x.min(other.x),
                    self.y.min(other.y),
                    self.right().max(other.right()),
                    self.bottom().max(other.bottom()),
                )
            }
        }
    };
}

coordinate_space!(
    /// A physical pixel in virtual-desktop coordinates.
    PhysicalPoint,
    /// A rectangle of physical pixels in virtual-desktop coordinates.
    PhysicalRect
);

coordinate_space!(
    /// A physical pixel relative to one monitor's top-left corner.
    LocalPoint,
    /// A rectangle of physical pixels relative to one monitor's top-left
    /// corner.
    LocalRect
);

coordinate_space!(
    /// A device-independent pixel relative to one monitor's top-left corner.
    LogicalPoint,
    /// A rectangle of device-independent pixels relative to one monitor's
    /// top-left corner.
    LogicalRect
);

#[cfg(test)]
mod tests {
    use super::{PhysicalPoint as P, PhysicalRect as R};

    #[test]
    fn right_and_bottom_edges_are_exclusive() {
        let rect = R::new(-10, -20, 30, 40);
        assert!(rect.contains(P::new(-10, -20)));
        assert!(rect.contains(P::new(19, 19)));
        assert!(!rect.contains(P::new(20, 0)));
        assert!(!rect.contains(P::new(0, 20)));
        assert!(!rect.contains(P::new(-11, 0)));
    }

    #[test]
    fn empty_rectangles_contain_nothing() {
        assert!(!R::new(5, 5, 0, 10).contains(P::new(5, 5)));
        assert!(R::new(0, 0, -3, 10).is_empty());
    }

    #[test]
    fn intersection_of_touching_rectangles_is_nothing() {
        let left = R::new(0, 0, 100, 100);
        assert_eq!(left.intersection(&R::new(100, 0, 100, 100)), None);
        assert_eq!(
            left.intersection(&R::new(50, 50, 100, 100)),
            Some(R::new(50, 50, 50, 50))
        );
        assert_eq!(left.intersection(&R::default()), None);
    }

    #[test]
    fn union_spans_negative_coordinates_and_ignores_empty() {
        let primary = R::new(0, 0, 1920, 1080);
        let left = R::new(-2560, -200, 2560, 1440);
        assert_eq!(primary.union(&left), R::new(-2560, -200, 4480, 1440));
        assert_eq!(primary.union(&R::default()), primary);
        assert_eq!(R::default().union(&left), left);
    }
}
