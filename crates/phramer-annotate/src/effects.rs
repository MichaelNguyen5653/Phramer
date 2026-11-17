//! The tools that transform image pixels instead of painting over them:
//! invert, the frosted redaction fill, and the insecure blur and pixelation.
//!
//! All of them work on a premultiplied RGBA8 buffer, in integer or `f32`
//! arithmetic that gives the same bytes on every target.

/// A premultiplied RGBA8 buffer and the region of it being transformed.
pub(crate) struct Region<'a> {
    pub(crate) data: &'a mut [u8],
    /// Row length of `data` in pixels.
    pub(crate) stride: usize,
    pub(crate) x: usize,
    pub(crate) y: usize,
    pub(crate) width: usize,
    pub(crate) height: usize,
}

/// Bounds within the buffer that may be read around the region: the image.
/// Pixels outside it are workspace, not image, and are never sampled.
#[derive(Clone, Copy)]
pub(crate) struct Readable {
    pub(crate) left: usize,
    pub(crate) top: usize,
    pub(crate) right: usize,
    pub(crate) bottom: usize,
}

impl Region<'_> {
    fn offset(&self, x: usize, y: usize) -> usize {
        (y * self.stride + x) * 4
    }

    fn get(&self, x: usize, y: usize) -> [u8; 4] {
        let o = self.offset(x, y);
        [
            self.data[o],
            self.data[o + 1],
            self.data[o + 2],
            self.data[o + 3],
        ]
    }

    fn set(&mut self, x: usize, y: usize, pixel: [u8; 4]) {
        let o = self.offset(x, y);
        self.data[o..o + 4].copy_from_slice(&pixel);
    }

    /// The region's own pixels, row by row.
    fn copy_out(&self) -> Vec<[u8; 4]> {
        let mut out = Vec::with_capacity(self.width * self.height);
        for y in self.y..self.y + self.height {
            for x in self.x..self.x + self.width {
                out.push(self.get(x, y));
            }
        }
        out
    }
}

fn premultiply(c: u8, a: u8) -> u8 {
    ((u32::from(c) * u32::from(a) + 127) / 255) as u8
}

fn demultiply(c: u8, a: u8) -> u8 {
    if a == 0 {
        return 0;
    }
    ((u32::from(c) * 255 + u32::from(a) / 2) / u32::from(a)).min(255) as u8
}

fn straight(pixel: [u8; 4]) -> [u8; 4] {
    let a = pixel[3];
    [
        demultiply(pixel[0], a),
        demultiply(pixel[1], a),
        demultiply(pixel[2], a),
        a,
    ]
}

/// Inverts the colour of every pixel, keeping its alpha.
pub(crate) fn invert(region: &mut Region<'_>) {
    for y in region.y..region.y + region.height {
        for x in region.x..region.x + region.width {
            let [r, g, b, a] = straight(region.get(x, y));
            region.set(
                x,
                y,
                [
                    premultiply(255 - r, a),
                    premultiply(255 - g, a),
                    premultiply(255 - b, a),
                    a,
                ],
            );
        }
    }
}

#[derive(Clone, Copy, Default, PartialEq, Debug)]
struct Rgb {
    r: f32,
    g: f32,
    b: f32,
}

impl Rgb {
    fn add(self, o: Rgb) -> Rgb {
        Rgb {
            r: self.r + o.r,
            g: self.g + o.g,
            b: self.b + o.b,
        }
    }

    fn sub(self, o: Rgb) -> Rgb {
        Rgb {
            r: self.r - o.r,
            g: self.g - o.g,
            b: self.b - o.b,
        }
    }

    fn scale(self, s: f32) -> Rgb {
        Rgb {
            r: self.r * s,
            g: self.g * s,
            b: self.b * s,
        }
    }
}

type Line = Vec<Rgb>;

fn to_rgb(pixel: [u8; 4]) -> Rgb {
    let [r, g, b, _] = straight(pixel);
    Rgb {
        r: f32::from(r) / 255.0,
        g: f32::from(g) / 255.0,
        b: f32::from(b) / 255.0,
    }
}

/// Three box passes: a Gaussian in linear time. A side can be thousands of
/// pixels long and this runs while the box is being dragged.
fn smooth(line: &mut Line, radius: usize) {
    let n = line.len();
    if n < 2 || radius < 1 {
        return;
    }
    let mut prefix = vec![Rgb::default(); n + 1];
    for _ in 0..3 {
        for i in 0..n {
            prefix[i + 1] = prefix[i].add(line[i]);
        }
        for (i, value) in line.iter_mut().enumerate() {
            let lo = i.saturating_sub(radius);
            let hi = (i + radius).min(n - 1);
            *value = prefix[hi + 1]
                .sub(prefix[lo])
                .scale(1.0 / (hi - lo + 1) as f32);
        }
    }
}

fn sample(line: &Line, t: f32) -> Rgb {
    if line.len() == 1 {
        return line[0];
    }
    let position = t * (line.len() - 1) as f32;
    let index = (position as usize).min(line.len() - 2);
    let fraction = position - index as f32;
    line[index]
        .scale(1.0 - fraction)
        .add(line[index + 1].scale(fraction))
}

fn average(candidates: [Option<Rgb>; 2], fallback: Rgb) -> Rgb {
    let present: Vec<Rgb> = candidates.into_iter().flatten().collect();
    if present.is_empty() {
        return fallback;
    }
    present
        .iter()
        .fold(Rgb::default(), |sum, c| sum.add(*c))
        .scale(1.0 / present.len() as f32)
}

fn gradient(from: Rgb, to: Rgb, length: usize) -> Line {
    let n = length.max(1);
    (0..n)
        .map(|i| {
            let t = if n > 1 {
                i as f32 / (n - 1) as f32
            } else {
                0.0
            };
            from.scale(1.0 - t).add(to.scale(t))
        })
        .collect()
}

fn to_byte(value: f32) -> u8 {
    // Truncating like the C++ `int(v * 255 + 0.5)`, then clamped.
    ((value * 255.0 + 0.5) as i32).clamp(0, 255) as u8
}

const GRAIN: usize = 64;

/// A fixed tile of grain in -3..=3. A tile rather than per-pixel randomness
/// keeps the grain identical from frame to frame instead of shimmering.
fn grain_tile() -> Vec<i32> {
    // xorshift32 from a fixed seed: the same tile everywhere, forever.
    let mut state: u32 = 0x5eed;
    (0..GRAIN * GRAIN)
        .map(|_| {
            loop {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                // Rejection keeps the seven values equally likely.
                let bucket = state >> 29;
                if bucket < 7 {
                    return bucket as i32 - 3;
                }
            }
        })
        .collect()
}

/// Redaction that looks like frosted glass, built only from the pixels
/// bordering the region; the region's own pixels are never read, so nothing
/// inside can be recovered. `strength` is the tool size: larger is softer.
pub(crate) fn frosted(region: &mut Region<'_>, readable: Readable, strength: u32) {
    let (w, h) = (region.width, region.height);
    if w == 0 || h == 0 {
        return;
    }
    let (left, top) = (region.x, region.y);
    let (right, bottom) = (left + w, top + h);
    // A side touching the image edge has nothing outside it to read.
    let read_row = |y: usize| {
        (left..right)
            .map(|x| to_rgb(region.get(x, y)))
            .collect::<Line>()
    };
    let read_col = |x: usize| {
        (top..bottom)
            .map(|y| to_rgb(region.get(x, y)))
            .collect::<Line>()
    };
    let mut top_line = if top > readable.top {
        read_row(top - 1)
    } else {
        Line::new()
    };
    let mut bottom_line = if bottom < readable.bottom {
        read_row(bottom)
    } else {
        Line::new()
    };
    let mut left_line = if left > readable.left {
        read_col(left - 1)
    } else {
        Line::new()
    };
    let mut right_line = if right < readable.right {
        read_col(right)
    } else {
        Line::new()
    };

    // Softer with a larger tool size, but never so little that the detail
    // of what surrounds the box survives into it.
    let spread = (0.03 + strength.min(1000) as f32 * 0.006).clamp(0.03, 0.3);
    smooth(&mut top_line, (w as f32 * spread) as usize);
    smooth(&mut bottom_line, (w as f32 * spread) as usize);
    smooth(&mut left_line, (h as f32 * spread) as usize);
    smooth(&mut right_line, (h as f32 * spread) as usize);

    let all: Vec<Rgb> = [&top_line, &bottom_line, &left_line, &right_line]
        .into_iter()
        .flatten()
        .copied()
        .collect();
    let mean = if all.is_empty() {
        Rgb {
            r: 0.5,
            g: 0.5,
            b: 0.5,
        }
    } else {
        all.iter()
            .fold(Rgb::default(), |s, c| s.add(*c))
            .scale(1.0 / all.len() as f32)
    };
    let top_left = average(
        [top_line.first().copied(), left_line.first().copied()],
        mean,
    );
    let top_right = average(
        [top_line.last().copied(), right_line.first().copied()],
        mean,
    );
    let bottom_left = average(
        [bottom_line.first().copied(), left_line.last().copied()],
        mean,
    );
    let bottom_right = average(
        [bottom_line.last().copied(), right_line.last().copied()],
        mean,
    );
    // A missing side is bridged between its neighbours' ends.
    if top_line.is_empty() {
        top_line = gradient(top_left, top_right, w);
    }
    if bottom_line.is_empty() {
        bottom_line = gradient(bottom_left, bottom_right, w);
    }
    if left_line.is_empty() {
        left_line = gradient(top_left, bottom_left, h);
    }
    if right_line.is_empty() {
        right_line = gradient(top_right, bottom_right, h);
    }

    // The patch is smooth, so a coarse grid scaled up with filtering looks
    // the same as evaluating every pixel at a fraction of the cost.
    let gw = (w / 6).clamp(2, 400);
    let gh = (h / 6).clamp(2, 400);
    let mut grid = vec![[0u8; 3]; gw * gh];
    for gy in 0..gh {
        let v = gy as f32 / (gh - 1) as f32;
        for gx in 0..gw {
            let u = gx as f32 / (gw - 1) as f32;
            // Coons patch: two ruled surfaces minus the bilinear corner
            // blend they both contain.
            let c = sample(&top_line, u)
                .scale(1.0 - v)
                .add(sample(&bottom_line, u).scale(v))
                .add(sample(&left_line, v).scale(1.0 - u))
                .add(sample(&right_line, v).scale(u))
                .sub(
                    top_left
                        .scale((1.0 - u) * (1.0 - v))
                        .add(top_right.scale(u * (1.0 - v)))
                        .add(bottom_left.scale((1.0 - u) * v))
                        .add(bottom_right.scale(u * v)),
                );
            grid[gy * gw + gx] = [to_byte(c.r), to_byte(c.g), to_byte(c.b)];
        }
    }

    let grain = grain_tile();
    for y in 0..h {
        let (y0, y1, fy) = bilinear_source(y, h, gh);
        for x in 0..w {
            let (x0, x1, fx) = bilinear_source(x, w, gw);
            let n = grain[(y % GRAIN) * GRAIN + x % GRAIN];
            let mut out = [0u8; 4];
            for (channel, slot) in out.iter_mut().take(3).enumerate() {
                let at = |gx: usize, gy: usize| f32::from(grid[gy * gw + gx][channel]);
                let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
                let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
                let value = (top * (1.0 - fy) + bottom * fy + 0.5) as i32;
                *slot = (value.clamp(0, 255) + n).clamp(0, 255) as u8;
            }
            out[3] = 255;
            region.set(left + x, top + y, out);
        }
    }
    rim(region);
}

/// Source pixels and weight for filtered scaling of `n` samples onto
/// `size` pixels, sampling at pixel centres.
fn bilinear_source(i: usize, size: usize, n: usize) -> (usize, usize, f32) {
    let position = ((i as f32 + 0.5) * n as f32 / size as f32 - 0.5).max(0.0);
    let i0 = (position as usize).min(n - 1);
    let i1 = (i0 + 1).min(n - 1);
    (i0, i1, position - i0 as f32)
}

/// A faint light rim, which makes the fill read as glass rather than as a
/// smudge: white at alpha 40 on the region's outermost pixels.
fn rim(region: &mut Region<'_>) {
    let (l, t) = (region.x, region.y);
    let (r, b) = (l + region.width - 1, t + region.height - 1);
    let blend = |c: u8, over: u32| ((over * 255 + u32::from(c) * 215 + 127) / 255) as u8;
    let touch = |region: &mut Region<'_>, x: usize, y: usize| {
        let p = region.get(x, y);
        region.set(
            x,
            y,
            [
                blend(p[0], 40),
                blend(p[1], 40),
                blend(p[2], 40),
                blend(p[3], 40),
            ],
        );
    };
    for x in l..=r {
        touch(region, x, t);
        if b != t {
            touch(region, x, b);
        }
    }
    for y in t + 1..b {
        touch(region, l, y);
        if r != l {
            touch(region, r, y);
        }
    }
}

/// The insecure setting at sizes 0 and 1: the region blurred, drawn twice
/// over itself (a softer and a stronger pass) so the blur is heavy. The
/// blurred copy fades out at its edges, so the boundary blends into the
/// surroundings rather than ending in a hard line.
pub(crate) fn blur(region: &mut Region<'_>) {
    let original = region.copy_out();
    // Box radii whose three passes approximate blur radii 10 and 12.
    for radius in [3, 4] {
        let blurred = box_blur_3(&original, region.width, region.height, radius);
        for y in 0..region.height {
            for x in 0..region.width {
                let src = blurred[y * region.width + x];
                let dst = region.get(region.x + x, region.y + y);
                let inv = 255 - u32::from(src[3]);
                let over =
                    |s: u8, d: u8| (u32::from(s) + (u32::from(d) * inv + 127) / 255).min(255) as u8;
                region.set(
                    region.x + x,
                    region.y + y,
                    [
                        over(src[0], dst[0]),
                        over(src[1], dst[1]),
                        over(src[2], dst[2]),
                        over(src[3], dst[3]),
                    ],
                );
            }
        }
    }
}

/// Three box passes over premultiplied pixels with transparent padding.
fn box_blur_3(pixels: &[[u8; 4]], w: usize, h: usize, radius: usize) -> Vec<[u8; 4]> {
    let mut work: Vec<[u32; 4]> = pixels
        .iter()
        .map(|p| [p[0].into(), p[1].into(), p[2].into(), p[3].into()])
        .collect();
    let window = (2 * radius + 1) as u32;
    let pass = |work: &mut Vec<[u32; 4]>, horizontal: bool| {
        let (lines, len) = if horizontal { (h, w) } else { (w, h) };
        let index = |line: usize, i: usize| {
            if horizontal {
                line * w + i
            } else {
                i * w + line
            }
        };
        let mut out = work.clone();
        for line in 0..lines {
            for i in 0..len {
                let lo = i.saturating_sub(radius);
                let hi = (i + radius).min(len - 1);
                let mut sum = [0u32; 4];
                for j in lo..=hi {
                    let p = work[index(line, j)];
                    for c in 0..4 {
                        sum[c] += p[c];
                    }
                }
                // Missing neighbours past the edge count as transparent.
                out[index(line, i)] = sum.map(|s| (s + window / 2) / window);
            }
        }
        *work = out;
    };
    for _ in 0..3 {
        pass(&mut work, true);
        pass(&mut work, false);
    }
    work.into_iter()
        .map(|p| p.map(|c| c.min(255) as u8))
        .collect()
}

/// The insecure setting above size 1: averaged down to blocks, then scaled
/// back up without filtering. Larger sizes give bigger blocks.
pub(crate) fn pixelate(region: &mut Region<'_>, size: u32) {
    let (w, h) = (region.width, region.height);
    if w == 0 || h == 0 {
        return;
    }
    let factor = 0.5 / f64::from(size.saturating_add(1).max(1));
    let bw = ((w as f64 * factor) as usize).max(1);
    let bh = ((h as f64 * factor) as usize).max(1);
    let original = region.copy_out();
    let mut blocks = vec![[0u8; 4]; bw * bh];
    for by in 0..bh {
        let (y0, y1) = (by * h / bh, ((by + 1) * h / bh).max(by * h / bh + 1));
        for bx in 0..bw {
            let (x0, x1) = (bx * w / bw, ((bx + 1) * w / bw).max(bx * w / bw + 1));
            let mut sum = [0u64; 4];
            for y in y0..y1 {
                for x in x0..x1 {
                    let p = original[y * w + x];
                    for c in 0..4 {
                        sum[c] += u64::from(p[c]);
                    }
                }
            }
            let count = ((y1 - y0) * (x1 - x0)) as u64;
            blocks[by * bw + bx] = sum.map(|s| ((s + count / 2) / count) as u8);
        }
    }
    for y in 0..h {
        for x in 0..w {
            let block = blocks[(y * bh / h) * bw + x * bw / w];
            region.set(region.x + x, region.y + y, block);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buffer(w: usize, h: usize, f: impl Fn(usize, usize) -> [u8; 4]) -> Vec<u8> {
        let mut out = Vec::with_capacity(w * h * 4);
        for y in 0..h {
            for x in 0..w {
                out.extend_from_slice(&f(x, y));
            }
        }
        out
    }

    #[test]
    fn invert_flips_colour_and_keeps_alpha() {
        let mut data = vec![10, 200, 30, 255, 0, 0, 0, 0];
        let mut region = Region {
            data: &mut data,
            stride: 2,
            x: 0,
            y: 0,
            width: 2,
            height: 1,
        };
        invert(&mut region);
        assert_eq!(data, [245, 55, 225, 255, 0, 0, 0, 0]);
    }

    #[test]
    fn frosted_fill_never_reads_the_covered_pixels() {
        let (w, h) = (40, 30);
        let surround = |x: usize, y: usize| [(x * 6) as u8, (y * 8) as u8, 90, 255];
        let mut a = buffer(w, h, surround);
        // Same surroundings, completely different secret inside the box.
        let mut b = buffer(w, h, |x, y| {
            if (10..30).contains(&x) && (8..22).contains(&y) {
                [255, (x * y) as u8, 0, 255]
            } else {
                surround(x, y)
            }
        });
        let readable = Readable {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        for data in [&mut a, &mut b] {
            let mut region = Region {
                data,
                stride: w,
                x: 10,
                y: 8,
                width: 20,
                height: 14,
            };
            frosted(&mut region, readable, 3);
        }
        assert_eq!(a, b);
    }

    #[test]
    fn frosted_fill_matches_its_surroundings_in_tone() {
        let (w, h) = (60, 60);
        let mut data = buffer(w, h, |_, _| [200, 40, 40, 255]);
        let readable = Readable {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let mut region = Region {
            data: &mut data,
            stride: w,
            x: 10,
            y: 10,
            width: 40,
            height: 40,
        };
        frosted(&mut region, readable, 3);
        // Away from the rim the fill is the surrounding red, give or take grain.
        let o = (30 * w + 30) * 4;
        assert!((197..=203).contains(&data[o]), "{:?}", &data[o..o + 4]);
        assert!((37..=43).contains(&data[o + 1]));
    }

    #[test]
    fn grain_stays_within_three_levels_and_is_stable() {
        let tile = grain_tile();
        assert_eq!(tile.len(), GRAIN * GRAIN);
        assert!(tile.iter().all(|v| (-3..=3).contains(v)));
        assert!((-3..=3).all(|v| tile.contains(&v)));
        assert_eq!(tile, grain_tile());
    }

    #[test]
    fn pixelate_makes_uniform_blocks() {
        let (w, h) = (20, 20);
        let mut data = buffer(w, h, |x, y| [(x * 12) as u8, (y * 12) as u8, 0, 255]);
        let mut region = Region {
            data: &mut data,
            stride: w,
            x: 0,
            y: 0,
            width: w,
            height: h,
        };
        // Size 4: 20 * 0.1 = 2 blocks a side, each 10 pixels.
        pixelate(&mut region, 4);
        let px = |x: usize, y: usize| data[(y * w + x) * 4];
        assert_eq!(px(0, 0), px(9, 9));
        assert_ne!(px(9, 0), px(10, 0));
    }
}
