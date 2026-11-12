//! Colours as the settings file spells them: the names and hex forms
//! `QColor` accepts, plus the application's own `#RRGGBBAA`.

/// An 8-bit RGBA colour.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Color {
    pub r: u8,
    pub g: u8,
    pub b: u8,
    pub a: u8,
}

impl Color {
    /// An opaque colour.
    pub const fn rgb(r: u8, g: u8, b: u8) -> Self {
        Self { r, g, b, a: 255 }
    }

    /// `QColor::fromString`: `#RGB`, `#RRGGBB`, `#AARRGGBB`, `#RRRGGGBBB`,
    /// `#RRRRGGGGBBBB` or an SVG colour name (case and blanks ignored).
    pub(super) fn from_qt_name(name: &str) -> Option<Self> {
        match name.strip_prefix('#') {
            Some(hex) => Self::from_qt_hex(hex),
            None if name.is_empty() || name.len() > 255 => None,
            None => {
                let key: String = name
                    .chars()
                    .filter(|c| !matches!(c, ' ' | '\t'))
                    .map(|c| c.to_ascii_lowercase())
                    .collect();
                let index = NAMED
                    .binary_search_by(|(candidate, _)| candidate.cmp(&key.as_str()))
                    .ok()?;
                let rgb = NAMED[index].1;
                // "transparent" is the one name with no alpha.
                let a = if key == "transparent" { 0 } else { 255 };
                Some(Self {
                    r: (rgb >> 16) as u8,
                    g: (rgb >> 8) as u8,
                    b: rgb as u8,
                    a,
                })
            }
        }
    }

    fn from_qt_hex(hex: &str) -> Option<Self> {
        if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        // Channel `index`, `width` digits wide, reduced to its top 8 bits.
        let channel = |index: usize, width: usize| -> Option<u8> {
            let value =
                u32::from_str_radix(hex.get(index * width..(index + 1) * width)?, 16).ok()?;
            let byte = match width {
                1 => value * 0x11,
                2 => value,
                _ => value >> (4 * (width - 2)),
            };
            u8::try_from(byte).ok()
        };
        match hex.len() {
            8 => Some(Self {
                a: channel(0, 2)?,
                r: channel(1, 2)?,
                g: channel(2, 2)?,
                b: channel(3, 2)?,
            }),
            len @ (3 | 6 | 9 | 12) => {
                let width = len / 3;
                Some(Self::rgb(
                    channel(0, width)?,
                    channel(1, width)?,
                    channel(2, width)?,
                ))
            }
            _ => None,
        }
    }

    /// Reads a colour setting (the C++ `Color` value handler): any name
    /// `QColor` knows except the `#RGB`, `#RRRGGGBBB` and `#RRRRGGGGBBBB`
    /// forms, with eight hex digits meaning `#RRGGBBAA` rather than Qt's
    /// `#AARRGGBB`.
    pub(super) fn from_setting(text: &str) -> Option<Self> {
        let qt = Self::from_qt_name(text)?;
        let Some(hex) = text.strip_prefix('#') else {
            return Some(qt);
        };
        match hex.len() {
            3 | 9 | 12 => None,
            // Qt read the digits as AARRGGBB; they are stored as RRGGBBAA.
            8 => Some(Self {
                r: qt.a,
                g: qt.r,
                b: qt.g,
                a: qt.b,
            }),
            _ => Some(qt),
        }
    }

    /// `#rrggbb`, the form `QColor::name()` gives.
    pub(super) fn hex_rgb(self) -> String {
        format!("#{:02x}{:02x}{:02x}", self.r, self.g, self.b)
    }

    /// The form [`Color::from_setting`] reads back: `#rrggbb`, or
    /// `#rrggbbaa` when the colour is translucent.
    pub(super) fn to_setting(self) -> String {
        if self.a == 255 {
            self.hex_rgb()
        } else {
            format!("{}{:02x}", self.hex_rgb(), self.a)
        }
    }
}

/// Qt's `rgbTbl`: the SVG 1.0 colour names, sorted for binary search.
const NAMED: &[(&str, u32)] = &[
    ("aliceblue", 0xf0f8ff),
    ("antiquewhite", 0xfaebd7),
    ("aqua", 0x00ffff),
    ("aquamarine", 0x7fffd4),
    ("azure", 0xf0ffff),
    ("beige", 0xf5f5dc),
    ("bisque", 0xffe4c4),
    ("black", 0x000000),
    ("blanchedalmond", 0xffebcd),
    ("blue", 0x0000ff),
    ("blueviolet", 0x8a2be2),
    ("brown", 0xa52a2a),
    ("burlywood", 0xdeb887),
    ("cadetblue", 0x5f9ea0),
    ("chartreuse", 0x7fff00),
    ("chocolate", 0xd2691e),
    ("coral", 0xff7f50),
    ("cornflowerblue", 0x6495ed),
    ("cornsilk", 0xfff8dc),
    ("crimson", 0xdc143c),
    ("cyan", 0x00ffff),
    ("darkblue", 0x00008b),
    ("darkcyan", 0x008b8b),
    ("darkgoldenrod", 0xb8860b),
    ("darkgray", 0xa9a9a9),
    ("darkgreen", 0x006400),
    ("darkgrey", 0xa9a9a9),
    ("darkkhaki", 0xbdb76b),
    ("darkmagenta", 0x8b008b),
    ("darkolivegreen", 0x556b2f),
    ("darkorange", 0xff8c00),
    ("darkorchid", 0x9932cc),
    ("darkred", 0x8b0000),
    ("darksalmon", 0xe9967a),
    ("darkseagreen", 0x8fbc8f),
    ("darkslateblue", 0x483d8b),
    ("darkslategray", 0x2f4f4f),
    ("darkslategrey", 0x2f4f4f),
    ("darkturquoise", 0x00ced1),
    ("darkviolet", 0x9400d3),
    ("deeppink", 0xff1493),
    ("deepskyblue", 0x00bfff),
    ("dimgray", 0x696969),
    ("dimgrey", 0x696969),
    ("dodgerblue", 0x1e90ff),
    ("firebrick", 0xb22222),
    ("floralwhite", 0xfffaf0),
    ("forestgreen", 0x228b22),
    ("fuchsia", 0xff00ff),
    ("gainsboro", 0xdcdcdc),
    ("ghostwhite", 0xf8f8ff),
    ("gold", 0xffd700),
    ("goldenrod", 0xdaa520),
    ("gray", 0x808080),
    ("green", 0x008000),
    ("greenyellow", 0xadff2f),
    ("grey", 0x808080),
    ("honeydew", 0xf0fff0),
    ("hotpink", 0xff69b4),
    ("indianred", 0xcd5c5c),
    ("indigo", 0x4b0082),
    ("ivory", 0xfffff0),
    ("khaki", 0xf0e68c),
    ("lavender", 0xe6e6fa),
    ("lavenderblush", 0xfff0f5),
    ("lawngreen", 0x7cfc00),
    ("lemonchiffon", 0xfffacd),
    ("lightblue", 0xadd8e6),
    ("lightcoral", 0xf08080),
    ("lightcyan", 0xe0ffff),
    ("lightgoldenrodyellow", 0xfafad2),
    ("lightgray", 0xd3d3d3),
    ("lightgreen", 0x90ee90),
    ("lightgrey", 0xd3d3d3),
    ("lightpink", 0xffb6c1),
    ("lightsalmon", 0xffa07a),
    ("lightseagreen", 0x20b2aa),
    ("lightskyblue", 0x87cefa),
    ("lightslategray", 0x778899),
    ("lightslategrey", 0x778899),
    ("lightsteelblue", 0xb0c4de),
    ("lightyellow", 0xffffe0),
    ("lime", 0x00ff00),
    ("limegreen", 0x32cd32),
    ("linen", 0xfaf0e6),
    ("magenta", 0xff00ff),
    ("maroon", 0x800000),
    ("mediumaquamarine", 0x66cdaa),
    ("mediumblue", 0x0000cd),
    ("mediumorchid", 0xba55d3),
    ("mediumpurple", 0x9370db),
    ("mediumseagreen", 0x3cb371),
    ("mediumslateblue", 0x7b68ee),
    ("mediumspringgreen", 0x00fa9a),
    ("mediumturquoise", 0x48d1cc),
    ("mediumvioletred", 0xc71585),
    ("midnightblue", 0x191970),
    ("mintcream", 0xf5fffa),
    ("mistyrose", 0xffe4e1),
    ("moccasin", 0xffe4b5),
    ("navajowhite", 0xffdead),
    ("navy", 0x000080),
    ("oldlace", 0xfdf5e6),
    ("olive", 0x808000),
    ("olivedrab", 0x6b8e23),
    ("orange", 0xffa500),
    ("orangered", 0xff4500),
    ("orchid", 0xda70d6),
    ("palegoldenrod", 0xeee8aa),
    ("palegreen", 0x98fb98),
    ("paleturquoise", 0xafeeee),
    ("palevioletred", 0xdb7093),
    ("papayawhip", 0xffefd5),
    ("peachpuff", 0xffdab9),
    ("peru", 0xcd853f),
    ("pink", 0xffc0cb),
    ("plum", 0xdda0dd),
    ("powderblue", 0xb0e0e6),
    ("purple", 0x800080),
    ("red", 0xff0000),
    ("rosybrown", 0xbc8f8f),
    ("royalblue", 0x4169e1),
    ("saddlebrown", 0x8b4513),
    ("salmon", 0xfa8072),
    ("sandybrown", 0xf4a460),
    ("seagreen", 0x2e8b57),
    ("seashell", 0xfff5ee),
    ("sienna", 0xa0522d),
    ("silver", 0xc0c0c0),
    ("skyblue", 0x87ceeb),
    ("slateblue", 0x6a5acd),
    ("slategray", 0x708090),
    ("slategrey", 0x708090),
    ("snow", 0xfffafa),
    ("springgreen", 0x00ff7f),
    ("steelblue", 0x4682b4),
    ("tan", 0xd2b48c),
    ("teal", 0x008080),
    ("thistle", 0xd8bfd8),
    ("tomato", 0xff6347),
    ("transparent", 0x000000),
    ("turquoise", 0x40e0d0),
    ("violet", 0xee82ee),
    ("wheat", 0xf5deb3),
    ("white", 0xffffff),
    ("whitesmoke", 0xf5f5f5),
    ("yellow", 0xffff00),
    ("yellowgreen", 0x9acd32),
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn name_table_is_sorted_for_binary_search() {
        assert!(NAMED.windows(2).all(|pair| pair[0].0 < pair[1].0));
        assert_eq!(NAMED.len(), 148);
    }

    #[test]
    fn qt_names_and_hex_forms_parse() {
        assert_eq!(Color::from_qt_name("red"), Some(Color::rgb(255, 0, 0)));
        assert_eq!(
            Color::from_qt_name("Dark Slate Gray"),
            Some(Color::rgb(0x2f, 0x4f, 0x4f))
        );
        assert_eq!(
            Color::from_qt_name("transparent"),
            Some(Color {
                r: 0,
                g: 0,
                b: 0,
                a: 0
            })
        );
        assert_eq!(
            Color::from_qt_name("#f80"),
            Some(Color::rgb(0xff, 0x88, 0x00))
        );
        assert_eq!(
            Color::from_qt_name("#3899C2"),
            Some(Color::rgb(0x38, 0x99, 0xc2))
        );
        assert_eq!(
            Color::from_qt_name("#80112233"),
            Some(Color {
                r: 0x11,
                g: 0x22,
                b: 0x33,
                a: 0x80
            })
        );
        assert_eq!(
            Color::from_qt_name("#123456789"),
            Some(Color::rgb(0x12, 0x45, 0x78))
        );
        assert_eq!(
            Color::from_qt_name("#123456789abc"),
            Some(Color::rgb(0x12, 0x56, 0x9a))
        );
        for bad in [
            "",
            "#",
            "#12",
            "#12345",
            "#gggggg",
            "#1234567",
            "notacolour",
            "#ff0000 ",
        ] {
            assert_eq!(Color::from_qt_name(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn settings_use_rrggbbaa_and_reject_odd_hex_lengths() {
        assert_eq!(
            Color::from_setting("#11223380"),
            Some(Color {
                r: 0x11,
                g: 0x22,
                b: 0x33,
                a: 0x80
            })
        );
        assert_eq!(
            Color::from_setting("#112233"),
            Some(Color::rgb(0x11, 0x22, 0x33))
        );
        assert_eq!(Color::from_setting("blue"), Some(Color::rgb(0, 0, 255)));
        for bad in ["#f80", "#123456789", "#123456789abc", "nope", ""] {
            assert_eq!(Color::from_setting(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn written_colours_read_back() {
        for colour in [
            Color::rgb(0x38, 0x99, 0xc2),
            Color {
                r: 1,
                g: 2,
                b: 3,
                a: 4,
            },
        ] {
            assert_eq!(Color::from_setting(&colour.to_setting()), Some(colour));
        }
        assert_eq!(Color::rgb(0x38, 0x99, 0xc2).to_setting(), "#3899c2");
    }
}
