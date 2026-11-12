//! The overlay's icons: Lucide (ISC; `assets/icons/LICENSE`), vendored as
//! the original SVG files and rasterised with `tiny-skia`.
//!
//! [`ICONS`] is the one table of what is used, by Lucide name, so the
//! Tauri UI can ask for the same icons by the same names. Only the element
//! subset Lucide draws with is read: `path`, `line`, `polyline`, `polygon`,
//! `circle`, `ellipse` and `rect`, all stroked with the icon's colour at
//! Lucide's stroke width of 2 on a 24-unit grid, round caps and joins.

use std::collections::HashMap;
use std::sync::OnceLock;

use tiny_skia::{LineCap, LineJoin, Paint, Path, PathBuilder, PixmapMut, Stroke, Transform};

/// Lucide's grid: every icon is drawn on 24 x 24 units.
const GRID: f32 = 24.0;
/// Lucide's `stroke-width`, in grid units.
const STROKE_WIDTH: f32 = 2.0;

/// Every icon the overlay draws: its Lucide name and the SVG file.
pub const ICONS: &[(&str, &str)] = &[
    (
        "arrow-up-right",
        include_str!("../assets/icons/arrow-up-right.svg"),
    ),
    ("check", include_str!("../assets/icons/check.svg")),
    ("circle", include_str!("../assets/icons/circle.svg")),
    ("contrast", include_str!("../assets/icons/contrast.svg")),
    ("copy", include_str!("../assets/icons/copy.svg")),
    (
        "external-link",
        include_str!("../assets/icons/external-link.svg"),
    ),
    ("file-image", include_str!("../assets/icons/file-image.svg")),
    ("grid-3x3", include_str!("../assets/icons/grid-3x3.svg")),
    ("hand", include_str!("../assets/icons/hand.svg")),
    ("hash", include_str!("../assets/icons/hash.svg")),
    (
        "highlighter",
        include_str!("../assets/icons/highlighter.svg"),
    ),
    ("minus", include_str!("../assets/icons/minus.svg")),
    ("move", include_str!("../assets/icons/move.svg")),
    ("pencil", include_str!("../assets/icons/pencil.svg")),
    ("pin", include_str!("../assets/icons/pin.svg")),
    ("plus", include_str!("../assets/icons/plus.svg")),
    (
        "rectangle-horizontal",
        include_str!("../assets/icons/rectangle-horizontal.svg"),
    ),
    ("redo-2", include_str!("../assets/icons/redo-2.svg")),
    ("save", include_str!("../assets/icons/save.svg")),
    ("scan-text", include_str!("../assets/icons/scan-text.svg")),
    ("shapes", include_str!("../assets/icons/shapes.svg")),
    ("slash", include_str!("../assets/icons/slash.svg")),
    ("square", include_str!("../assets/icons/square.svg")),
    ("square-pen", include_str!("../assets/icons/square-pen.svg")),
    ("type", include_str!("../assets/icons/type.svg")),
    ("undo-2", include_str!("../assets/icons/undo-2.svg")),
    ("x", include_str!("../assets/icons/x.svg")),
];

/// The parsed outlines of every icon in [`ICONS`], in grid units.
fn outlines() -> &'static HashMap<&'static str, Vec<Path>> {
    static OUTLINES: OnceLock<HashMap<&'static str, Vec<Path>>> = OnceLock::new();
    OUTLINES.get_or_init(|| {
        ICONS
            .iter()
            .map(|(name, svg)| (*name, parse_svg(svg)))
            .collect()
    })
}

/// Whether `name` is in [`ICONS`] and has something to draw.
pub fn has_icon(name: &str) -> bool {
    outlines().get(name).is_some_and(|paths| !paths.is_empty())
}

/// Draws icon `name` into the square at (`x`, `y`) of side `size` canvas
/// pixels. The square's corner should sit on whole pixels; the outline is
/// scaled from the vector grid, so it is sharp at any monitor scale.
pub(crate) fn draw_icon(
    canvas: &mut PixmapMut<'_>,
    name: &str,
    x: f32,
    y: f32,
    size: f32,
    paint: &Paint<'_>,
) {
    let Some(paths) = outlines().get(name) else {
        return;
    };
    let scale = size / GRID;
    let transform = Transform::from_row(scale, 0.0, 0.0, scale, x, y);
    let stroke = Stroke {
        width: STROKE_WIDTH,
        line_cap: LineCap::Round,
        line_join: LineJoin::Round,
        ..Stroke::default()
    };
    for path in paths {
        canvas.stroke_path(path, paint, &stroke, transform, None);
    }
}

/// The drawable elements of one SVG file, in its own units.
fn parse_svg(svg: &str) -> Vec<Path> {
    let mut paths = Vec::new();
    let mut rest = svg;
    while let Some(open) = rest.find('<') {
        rest = &rest[open + 1..];
        let Some(close) = rest.find('>') else { break };
        let tag = &rest[..close];
        rest = &rest[close + 1..];
        let name = tag.split_whitespace().next().unwrap_or_default();
        let number = |attribute: &str| attr(tag, attribute).and_then(|v| v.trim().parse().ok());
        let path = match name {
            "path" => attr(tag, "d").and_then(path_data),
            "line" => match (number("x1"), number("y1"), number("x2"), number("y2")) {
                (Some(x1), Some(y1), Some(x2), Some(y2)) => {
                    let mut builder = PathBuilder::new();
                    builder.move_to(x1, y1);
                    builder.line_to(x2, y2);
                    builder.finish()
                }
                _ => None,
            },
            "polyline" | "polygon" => {
                attr(tag, "points").and_then(|points| poly(points, name == "polygon"))
            }
            "circle" => match (number("cx"), number("cy"), number("r")) {
                (Some(cx), Some(cy), Some(r)) => PathBuilder::from_circle(cx, cy, r),
                _ => None,
            },
            "ellipse" => match (number("cx"), number("cy"), number("rx"), number("ry")) {
                (Some(cx), Some(cy), Some(rx), Some(ry)) => {
                    tiny_skia::Rect::from_xywh(cx - rx, cy - ry, 2.0 * rx, 2.0 * ry)
                        .and_then(PathBuilder::from_oval)
                }
                _ => None,
            },
            "rect" => match (number("width"), number("height")) {
                (Some(width), Some(height)) => {
                    let x = number("x").unwrap_or(0.0);
                    let y = number("y").unwrap_or(0.0);
                    // SVG: a missing radius takes the other one's value
                    let rx = number("rx").or(number("ry")).unwrap_or(0.0);
                    let ry = number("ry").or(number("rx")).unwrap_or(0.0);
                    rounded_rect(x, y, width, height, rx, ry)
                }
                _ => None,
            },
            _ => None,
        };
        paths.extend(path);
    }
    paths
}

/// The value of attribute `name` in a tag's text.
fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pattern = format!("{name}=\"");
    let mut from = 0;
    while let Some(found) = tag[from..].find(&pattern) {
        let at = from + found;
        // `x="` must not match the end of `rx="`
        let whole_word = tag[..at]
            .chars()
            .next_back()
            .is_none_or(char::is_whitespace);
        let value_start = at + pattern.len();
        if whole_word {
            let length = tag[value_start..].find('"')?;
            return Some(&tag[value_start..value_start + length]);
        }
        from = value_start;
    }
    None
}

fn poly(points: &str, closed: bool) -> Option<Path> {
    let numbers: Vec<f32> = points
        .split(|c: char| c.is_whitespace() || c == ',')
        .filter(|part| !part.is_empty())
        .filter_map(|part| part.parse().ok())
        .collect();
    let (first, rest) = numbers.as_chunks::<2>().0.split_first()?;
    let mut builder = PathBuilder::new();
    builder.move_to(first[0], first[1]);
    for point in rest {
        builder.line_to(point[0], point[1]);
    }
    if closed {
        builder.close();
    }
    builder.finish()
}

fn rounded_rect(x: f32, y: f32, w: f32, h: f32, rx: f32, ry: f32) -> Option<Path> {
    let rx = rx.min(w / 2.0).max(0.0);
    let ry = ry.min(h / 2.0).max(0.0);
    if rx == 0.0 || ry == 0.0 {
        return tiny_skia::Rect::from_xywh(x, y, w, h).map(PathBuilder::from_rect);
    }
    // A quarter ellipse as one cubic
    let (kx, ky) = (rx * 0.552_284_8, ry * 0.552_284_8);
    let mut p = PathBuilder::new();
    p.move_to(x + rx, y);
    p.line_to(x + w - rx, y);
    p.cubic_to(x + w - rx + kx, y, x + w, y + ry - ky, x + w, y + ry);
    p.line_to(x + w, y + h - ry);
    p.cubic_to(
        x + w,
        y + h - ry + ky,
        x + w - rx + kx,
        y + h,
        x + w - rx,
        y + h,
    );
    p.line_to(x + rx, y + h);
    p.cubic_to(x + rx - kx, y + h, x, y + h - ry + ky, x, y + h - ry);
    p.line_to(x, y + ry);
    p.cubic_to(x, y + ry - ky, x + rx - kx, y, x + rx, y);
    p.close();
    p.finish()
}

/// Reads SVG path data (`d`) number by number.
struct Tokens<'a> {
    text: &'a [u8],
    at: usize,
}

impl Tokens<'_> {
    fn skip_separators(&mut self) {
        while self
            .text
            .get(self.at)
            .is_some_and(|c| c.is_ascii_whitespace() || *c == b',')
        {
            self.at += 1;
        }
    }

    /// The next command letter, if one comes before the next number.
    fn command(&mut self) -> Option<u8> {
        self.skip_separators();
        let c = *self.text.get(self.at)?;
        if c.is_ascii_alphabetic() && c != b'e' && c != b'E' {
            self.at += 1;
            Some(c)
        } else {
            None
        }
    }

    fn at_number(&mut self) -> bool {
        self.skip_separators();
        self.text
            .get(self.at)
            .is_some_and(|c| c.is_ascii_digit() || matches!(c, b'-' | b'+' | b'.'))
    }

    fn number(&mut self) -> Option<f32> {
        self.skip_separators();
        let start = self.at;
        let mut end = start;
        if matches!(self.text.get(end), Some(b'-' | b'+')) {
            end += 1;
        }
        let mut dot = false;
        while let Some(&c) = self.text.get(end) {
            if c.is_ascii_digit() {
                end += 1;
            } else if c == b'.' && !dot {
                dot = true;
                end += 1;
            } else {
                break;
            }
        }
        if matches!(self.text.get(end), Some(b'e' | b'E')) {
            let mut exponent = end + 1;
            if matches!(self.text.get(exponent), Some(b'-' | b'+')) {
                exponent += 1;
            }
            if self.text.get(exponent).is_some_and(u8::is_ascii_digit) {
                end = exponent;
                while self.text.get(end).is_some_and(u8::is_ascii_digit) {
                    end += 1;
                }
            }
        }
        self.at = end;
        std::str::from_utf8(&self.text[start..end])
            .ok()?
            .parse()
            .ok()
    }

    /// An arc flag: a single 0 or 1, which may run into the next number.
    fn flag(&mut self) -> Option<bool> {
        self.skip_separators();
        let c = *self.text.get(self.at)?;
        self.at += 1;
        match c {
            b'0' => Some(false),
            b'1' => Some(true),
            _ => None,
        }
    }
}

/// SVG path data as a path; arcs become cubic curves. `None` for data that
/// does not parse.
fn path_data(d: &str) -> Option<Path> {
    let mut tokens = Tokens {
        text: d.as_bytes(),
        at: 0,
    };
    let mut builder = PathBuilder::new();
    let (mut x, mut y) = (0.0f32, 0.0f32);
    let (mut start_x, mut start_y) = (0.0f32, 0.0f32);
    // The last control point, for the smooth curve commands
    let mut last_cubic: Option<(f32, f32)> = None;
    let mut last_quad: Option<(f32, f32)> = None;
    let mut command = tokens.command()?;
    loop {
        let relative = command.is_ascii_lowercase();
        let (ox, oy) = if relative { (x, y) } else { (0.0, 0.0) };
        let mut cubic = None;
        let mut quad = None;
        match command.to_ascii_uppercase() {
            b'M' => {
                let (nx, ny) = (tokens.number()? + ox, tokens.number()? + oy);
                builder.move_to(nx, ny);
                (x, y, start_x, start_y) = (nx, ny, nx, ny);
                // Further pairs are line-tos
                command = if relative { b'l' } else { b'L' };
                if !tokens.at_number() {
                    match tokens.command() {
                        Some(next) => command = next,
                        None => break,
                    }
                }
                last_cubic = None;
                last_quad = None;
                continue;
            }
            b'L' => {
                (x, y) = (tokens.number()? + ox, tokens.number()? + oy);
                builder.line_to(x, y);
            }
            b'H' => {
                x = tokens.number()? + ox;
                builder.line_to(x, y);
            }
            b'V' => {
                y = tokens.number()? + oy;
                builder.line_to(x, y);
            }
            b'C' => {
                let (x1, y1) = (tokens.number()? + ox, tokens.number()? + oy);
                let (x2, y2) = (tokens.number()? + ox, tokens.number()? + oy);
                (x, y) = (tokens.number()? + ox, tokens.number()? + oy);
                builder.cubic_to(x1, y1, x2, y2, x, y);
                cubic = Some((x2, y2));
            }
            b'S' => {
                let (x1, y1) = last_cubic.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                let (x2, y2) = (tokens.number()? + ox, tokens.number()? + oy);
                (x, y) = (tokens.number()? + ox, tokens.number()? + oy);
                builder.cubic_to(x1, y1, x2, y2, x, y);
                cubic = Some((x2, y2));
            }
            b'Q' => {
                let (x1, y1) = (tokens.number()? + ox, tokens.number()? + oy);
                (x, y) = (tokens.number()? + ox, tokens.number()? + oy);
                builder.quad_to(x1, y1, x, y);
                quad = Some((x1, y1));
            }
            b'T' => {
                let (x1, y1) = last_quad.map_or((x, y), |(cx, cy)| (2.0 * x - cx, 2.0 * y - cy));
                (x, y) = (tokens.number()? + ox, tokens.number()? + oy);
                builder.quad_to(x1, y1, x, y);
                quad = Some((x1, y1));
            }
            b'A' => {
                let rx = tokens.number()?;
                let ry = tokens.number()?;
                let rotation = tokens.number()?;
                let large = tokens.flag()?;
                let sweep = tokens.flag()?;
                let (nx, ny) = (tokens.number()? + ox, tokens.number()? + oy);
                arc(
                    &mut builder,
                    (x, y),
                    (rx, ry),
                    rotation,
                    large,
                    sweep,
                    (nx, ny),
                );
                (x, y) = (nx, ny);
            }
            b'Z' => {
                builder.close();
                (x, y) = (start_x, start_y);
            }
            _ => return None,
        }
        last_cubic = cubic;
        last_quad = quad;
        // A number repeats the command; Z takes none
        if command.eq_ignore_ascii_case(&b'Z') || !tokens.at_number() {
            match tokens.command() {
                Some(next) => command = next,
                None => break,
            }
        }
    }
    builder.finish()
}

/// An elliptical arc as cubic curves (SVG 1.1, appendix F.6.5 and F.6.6),
/// one per quarter turn or less.
fn arc(
    builder: &mut PathBuilder,
    from: (f32, f32),
    radii: (f32, f32),
    rotation_degrees: f32,
    large: bool,
    sweep: bool,
    to: (f32, f32),
) {
    let (x1, y1) = (f64::from(from.0), f64::from(from.1));
    let (x2, y2) = (f64::from(to.0), f64::from(to.1));
    let (mut rx, mut ry) = (f64::from(radii.0).abs(), f64::from(radii.1).abs());
    if (x1 - x2).abs() < 1e-9 && (y1 - y2).abs() < 1e-9 {
        return;
    }
    if rx < 1e-9 || ry < 1e-9 {
        builder.line_to(to.0, to.1);
        return;
    }
    let phi = f64::from(rotation_degrees).to_radians();
    let (sin_phi, cos_phi) = phi.sin_cos();
    let (dx, dy) = ((x1 - x2) / 2.0, (y1 - y2) / 2.0);
    let x1p = cos_phi * dx + sin_phi * dy;
    let y1p = -sin_phi * dx + cos_phi * dy;
    // Radii too small for the end points grow until they fit
    let lambda = (x1p * x1p) / (rx * rx) + (y1p * y1p) / (ry * ry);
    if lambda > 1.0 {
        let grow = lambda.sqrt();
        rx *= grow;
        ry *= grow;
    }
    let numerator = rx * rx * ry * ry - rx * rx * y1p * y1p - ry * ry * x1p * x1p;
    let denominator = rx * rx * y1p * y1p + ry * ry * x1p * x1p;
    let mut factor = (numerator / denominator).max(0.0).sqrt();
    if large == sweep {
        factor = -factor;
    }
    let cxp = factor * rx * y1p / ry;
    let cyp = -factor * ry * x1p / rx;
    let cx = cos_phi * cxp - sin_phi * cyp + (x1 + x2) / 2.0;
    let cy = sin_phi * cxp + cos_phi * cyp + (y1 + y2) / 2.0;
    let angle = |ux: f64, uy: f64, vx: f64, vy: f64| {
        let sign = if ux * vy - uy * vx < 0.0 { -1.0 } else { 1.0 };
        let dot = (ux * vx + uy * vy) / ((ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt());
        sign * dot.clamp(-1.0, 1.0).acos()
    };
    let (ux, uy) = ((x1p - cxp) / rx, (y1p - cyp) / ry);
    let (vx, vy) = ((-x1p - cxp) / rx, (-y1p - cyp) / ry);
    let theta1 = angle(1.0, 0.0, ux, uy);
    let mut delta = angle(ux, uy, vx, vy);
    if !sweep && delta > 0.0 {
        delta -= std::f64::consts::TAU;
    } else if sweep && delta < 0.0 {
        delta += std::f64::consts::TAU;
    }
    let segments = (delta.abs() / std::f64::consts::FRAC_PI_2).ceil().max(1.0);
    let step = delta / segments;
    let k = 4.0 / 3.0 * (step / 4.0).tan();
    let point = |t: f64| {
        let (sin_t, cos_t) = t.sin_cos();
        (
            cx + rx * cos_t * cos_phi - ry * sin_t * sin_phi,
            cy + rx * cos_t * sin_phi + ry * sin_t * cos_phi,
        )
    };
    let derivative = |t: f64| {
        let (sin_t, cos_t) = t.sin_cos();
        (
            -rx * sin_t * cos_phi - ry * cos_t * sin_phi,
            -rx * sin_t * sin_phi + ry * cos_t * cos_phi,
        )
    };
    let mut t = theta1;
    for index in 0..segments as usize {
        let next = theta1 + step * (index + 1) as f64;
        let (p0, d0) = (point(t), derivative(t));
        let (p3, d3) = (point(next), derivative(next));
        builder.cubic_to(
            (p0.0 + k * d0.0) as f32,
            (p0.1 + k * d0.1) as f32,
            (p3.0 - k * d3.0) as f32,
            (p3.1 - k * d3.1) as f32,
            p3.0 as f32,
            p3.1 as f32,
        );
        t = next;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_icon_parses_into_outlines() {
        for (name, _) in ICONS {
            assert!(has_icon(name), "{name}");
        }
    }

    #[test]
    fn attributes_match_whole_names_only() {
        let tag = r#"rect width="14" x="8" rx="2" ry="3""#;
        assert_eq!(attr(tag, "x"), Some("8"));
        assert_eq!(attr(tag, "rx"), Some("2"));
        assert_eq!(attr(tag, "y"), None);
    }

    #[test]
    fn compact_path_data_reads_like_the_spec() {
        // Implicit line-tos after a move, signs as separators, relative arcs
        let path = path_data("M2 2 4 4l2-2h1v1a1 1 0 0 0 2 0z").expect("parses");
        let bounds = path.bounds();
        assert!((bounds.left() - 2.0).abs() < 1e-4, "{bounds:?}");
        assert!((bounds.right() - 9.0).abs() < 1e-4, "{bounds:?}");
        // A half circle of radius 1 bulges one unit below y = 3
        assert!((bounds.bottom() - 4.0).abs() < 1e-3, "{bounds:?}");
        assert!(path_data("M1 1 Q").is_none());
    }

    #[test]
    fn arcs_follow_the_sweep_flag() {
        // From (0,0) to (2,0) on a unit circle. With y growing downwards
        // the positive-angle sweep (1) is clockwise on screen: over the top
        let over = path_data("M0 0A1 1 0 0 1 2 0").expect("parses").bounds();
        let under = path_data("M0 0A1 1 0 0 0 2 0").expect("parses").bounds();
        assert!((over.top() + 1.0).abs() < 1e-3, "{over:?}");
        assert!((under.bottom() - 1.0).abs() < 1e-3, "{under:?}");
    }
}
