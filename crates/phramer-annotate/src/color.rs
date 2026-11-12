//! Annotation colours.

use serde::{Deserialize, Serialize};

/// A straight (non-premultiplied) RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    #[serde(default = "opaque")]
    pub a: u8,
}

fn opaque() -> u8 {
    u8::MAX
}

impl Color {
    pub const BLACK: Color = Color::rgb(0, 0, 0);
    pub const WHITE: Color = Color::rgb(255, 255, 255);

    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self {
            r,
            g,
            b,
            a: u8::MAX,
        }
    }

    pub const fn rgba(r: u8, g: u8, b: u8, a: u8) -> Self {
        Self { r, g, b, a }
    }

    /// Whether something drawn on this colour should be white to stay
    /// readable.
    ///
    /// Luma `0.30 r + 0.59 g + 0.11 b` at or below one half counts as dark.
    /// Kept in integers so the answer cannot differ between targets.
    pub fn is_dark(self) -> bool {
        let luma = 30 * u32::from(self.r) + 59 * u32::from(self.g) + 11 * u32::from(self.b);
        // Half of the maximum, 100 * 255.
        luma <= 12_750
    }
}

#[cfg(test)]
mod tests {
    use super::Color;

    #[test]
    fn luma_decides_darkness() {
        assert!(Color::BLACK.is_dark());
        assert!(!Color::WHITE.is_dark());
        assert!(Color::rgb(255, 0, 0).is_dark()); // 0.30
        assert!(!Color::rgb(0, 255, 0).is_dark()); // 0.59
        assert!(Color::rgb(0, 0, 255).is_dark()); // 0.11
        assert!(!Color::rgb(255, 255, 0).is_dark()); // 0.89
    }

    #[test]
    fn alpha_defaults_to_opaque_when_absent() {
        let color: Color = serde_json::from_str(r#"{"r":1,"g":2,"b":3}"#).unwrap();
        assert_eq!(color, Color::rgb(1, 2, 3));
    }
}
