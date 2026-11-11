//! Premultiplied RGBA canvas, regions (GDI-style span lists) and simple fills.

/// An sRGB colour with straight alpha.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

impl Rgb {
    pub const BLACK: Rgb = Rgb(0, 0, 0);
    pub const WHITE: Rgb = Rgb(255, 255, 255);

    /// Parses `#RRGGBB`.
    pub fn parse(s: &str) -> Option<Rgb> {
        let h = s.strip_prefix('#')?;
        if h.len() != 6 {
            return None;
        }
        let v = u32::from_str_radix(h, 16).ok()?;
        Some(Rgb((v >> 16) as u8, (v >> 8) as u8, v as u8))
    }
}

/// A straight (non-premultiplied) RGBA image, row-major, 4 bytes per pixel.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RgbaImage {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl RgbaImage {
    pub fn new(width: u32, height: u32) -> Self {
        RgbaImage {
            width,
            height,
            data: vec![0; width as usize * height as usize * 4],
        }
    }

    pub fn alpha(&self, x: u32, y: u32) -> u8 {
        self.data[(y as usize * self.width as usize + x as usize) * 4 + 3]
    }
}

/// Premultiplied RGBA canvas. Drawing happens in buffer pixels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Premultiplied R, G, B, A per pixel.
    pub px: Vec<[u8; 4]>,
}

fn mul255(a: u32, b: u32) -> u32 {
    (a * b + 127) / 255
}

impl Canvas {
    pub fn new(width: u32, height: u32) -> Self {
        Canvas {
            width,
            height,
            px: vec![[0; 4]; width as usize * height as usize],
        }
    }

    fn idx(&self, x: i32, y: i32) -> Option<usize> {
        if x < 0 || y < 0 || x >= self.width as i32 || y >= self.height as i32 {
            None
        } else {
            Some(y as usize * self.width as usize + x as usize)
        }
    }

    /// Blends `c` with coverage `cov` (0..=255) source-over at (x, y).
    pub fn blend(&mut self, x: i32, y: i32, c: Rgb, cov: u8) {
        let Some(i) = self.idx(x, y) else { return };
        if cov == 0 {
            return;
        }
        let a = cov as u32;
        let d = &mut self.px[i];
        let src = [
            mul255(c.0 as u32, a),
            mul255(c.1 as u32, a),
            mul255(c.2 as u32, a),
            a,
        ];
        for k in 0..4 {
            d[k] = (src[k] + mul255(d[k] as u32, 255 - a)) as u8;
        }
    }

    /// Blends a premultiplied pixel source-over.
    pub fn blend_premul(&mut self, x: i32, y: i32, s: [u8; 4]) {
        let Some(i) = self.idx(x, y) else { return };
        let ia = 255 - s[3] as u32;
        let d = &mut self.px[i];
        for k in 0..4 {
            d[k] = (s[k] as u32 + mul255(d[k] as u32, ia)).min(255) as u8;
        }
    }

    pub fn set(&mut self, x: i32, y: i32, c: Rgb) {
        if let Some(i) = self.idx(x, y) {
            self.px[i] = [c.0, c.1, c.2, 255];
        }
    }

    pub fn fill_rect(&mut self, x: i32, y: i32, w: i32, h: i32, c: Rgb) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, c);
            }
        }
    }

    pub fn fill_region(&mut self, r: &Region, c: Rgb) {
        for s in &r.spans {
            for x in s.x0..s.x1 {
                self.set(x, s.y, c);
            }
        }
    }

    /// Draws a straight RGBA image at (x, y), source-over.
    pub fn draw_image(&mut self, img: &RgbaImage, x: i32, y: i32) {
        self.draw_image_part(
            img, 0, 0, img.width, img.height, x, y, img.width, img.height,
        );
    }

    /// Draws the source rectangle (sx, sy, sw, sh) into (dx, dy, dw, dh) with
    /// nearest-neighbour sampling, source-over.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_image_part(
        &mut self,
        img: &RgbaImage,
        sx: u32,
        sy: u32,
        sw: u32,
        sh: u32,
        dx: i32,
        dy: i32,
        dw: u32,
        dh: u32,
    ) {
        if sw == 0 || sh == 0 {
            return;
        }
        for y in 0..dh {
            let syy = sy + ((2 * y as u64 + 1) * sh as u64 / (2 * dh as u64)) as u32;
            for x in 0..dw {
                let sxx = sx + ((2 * x as u64 + 1) * sw as u64 / (2 * dw as u64)) as u32;
                let o = (syy as usize * img.width as usize + sxx as usize) * 4;
                let p = &img.data[o..o + 4];
                let a = p[3] as u32;
                let s = [
                    mul255(p[0] as u32, a) as u8,
                    mul255(p[1] as u32, a) as u8,
                    mul255(p[2] as u32, a) as u8,
                    p[3],
                ];
                self.blend_premul(dx + x as i32, dy + y as i32, s);
            }
        }
    }

    /// Antialiased fill of the set described by `inside(x, y)` (pixel-space
    /// coordinates, continuous) over the box (x0, y0, w, h), 4x4 supersampled.
    pub fn fill_aa(
        &mut self,
        x0: i32,
        y0: i32,
        w: i32,
        h: i32,
        mut colour: impl FnMut(f32, f32) -> Option<Rgb>,
    ) {
        for y in y0..y0 + h {
            for x in x0..x0 + w {
                let (mut n, mut r, mut g, mut b) = (0u32, 0u32, 0u32, 0u32);
                for sy in 0..4 {
                    for sx in 0..4 {
                        let fx = x as f32 + (sx as f32 + 0.5) / 4.0;
                        let fy = y as f32 + (sy as f32 + 0.5) / 4.0;
                        if let Some(c) = colour(fx, fy) {
                            n += 1;
                            r += c.0 as u32;
                            g += c.1 as u32;
                            b += c.2 as u32;
                        }
                    }
                }
                if let (Some(r), Some(g), Some(b)) =
                    (r.checked_div(n), g.checked_div(n), b.checked_div(n))
                {
                    let c = Rgb(r as u8, g as u8, b as u8);
                    self.blend(x, y, c, (n * 255 / 16) as u8);
                }
            }
        }
    }

    /// Converts to a straight RGBA image (for PNG output).
    pub fn to_rgba(&self) -> RgbaImage {
        let mut img = RgbaImage::new(self.width, self.height);
        for (i, p) in self.px.iter().enumerate() {
            let a = p[3] as u32;
            let o = i * 4;
            for (d, &c) in img.data[o..o + 3].iter_mut().zip(&p[..3]) {
                *d = (c as u32 * 255 + a / 2)
                    .checked_div(a)
                    .unwrap_or(0)
                    .min(255) as u8;
            }
            img.data[o + 3] = p[3];
        }
        img
    }

    /// wl_shm ARGB8888 bytes: premultiplied, little-endian B, G, R, A.
    pub fn to_argb8888(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
        out
    }
}

/// A horizontal run of pixels [x0, x1) on row y.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Span {
    pub y: i32,
    pub x0: i32,
    pub x1: i32,
}

/// A rectangle (x, y, w, h).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Rect {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl Rect {
    pub const fn new(x: i32, y: i32, w: i32, h: i32) -> Self {
        Rect { x, y, w, h }
    }
    pub fn right(&self) -> i32 {
        self.x + self.w
    }
    pub fn bottom(&self) -> i32 {
        self.y + self.h
    }
    pub fn contains(&self, x: i32, y: i32) -> bool {
        x >= self.x && y >= self.y && x < self.right() && y < self.bottom()
    }
}

/// A pixel set as sorted, non-overlapping spans per row (like a GDI region).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Region {
    pub spans: Vec<Span>,
}

impl Region {
    fn from_rows(rows: impl IntoIterator<Item = (i32, Vec<(i32, i32)>)>) -> Region {
        let mut spans = Vec::new();
        for (y, mut runs) in rows {
            runs.retain(|r| r.1 > r.0);
            runs.sort();
            let mut merged: Vec<(i32, i32)> = Vec::new();
            for r in runs {
                match merged.last_mut() {
                    Some(m) if r.0 <= m.1 => m.1 = m.1.max(r.1),
                    _ => merged.push(r),
                }
            }
            spans.extend(merged.into_iter().map(|(x0, x1)| Span { y, x0, x1 }));
        }
        Region { spans }
    }

    fn rows(&self) -> std::collections::BTreeMap<i32, Vec<(i32, i32)>> {
        let mut m: std::collections::BTreeMap<i32, Vec<(i32, i32)>> = Default::default();
        for s in &self.spans {
            m.entry(s.y).or_default().push((s.x0, s.x1));
        }
        m
    }

    pub fn rect(r: Rect) -> Region {
        Region::from_rows((r.y..r.bottom()).map(|y| (y, vec![(r.x, r.right())])))
    }

    /// Rounded rectangle [l, r) x [t, b) with a corner ellipse of ew x eh
    /// (CreateRoundRectRgn-like: per row, the ellipse is sampled at the pixel centre).
    pub fn round_rect(r: Rect, ew: i32, eh: i32) -> Region {
        let rx = (ew as f64 / 2.0).min(r.w as f64 / 2.0);
        let ry = (eh as f64 / 2.0).min(r.h as f64 / 2.0);
        Region::from_rows((r.y..r.bottom()).map(|y| {
            let cy = y as f64 + 0.5;
            let dy = if cy < r.y as f64 + ry {
                r.y as f64 + ry - cy
            } else if cy > r.bottom() as f64 - ry {
                cy - (r.bottom() as f64 - ry)
            } else {
                0.0
            };
            let inset = if dy > 0.0 {
                let t = (1.0 - (dy / ry).powi(2)).max(0.0);
                (rx - rx * t.sqrt()).round() as i32
            } else {
                0
            };
            (y, vec![(r.x + inset, r.right() - inset)])
        }))
    }

    /// Filled polygon (even-odd), GDI-like: a vertex (x, y) names pixel (x, y); row y
    /// covers pixels with xl <= x < xr where the scanline through the pixel centres
    /// meets the edges, so right and bottom edges are excluded.
    pub fn polygon(pts: &[(f64, f64)]) -> Region {
        let ymin = pts.iter().map(|p| p.1).fold(f64::MAX, f64::min).floor() as i32;
        let ymax = pts.iter().map(|p| p.1).fold(f64::MIN, f64::max).ceil() as i32 + 1;
        Region::from_rows((ymin..ymax).map(|y| {
            let cy = y as f64;
            let mut xs: Vec<f64> = Vec::new();
            for i in 0..pts.len() {
                let (a, b) = (pts[i], pts[(i + 1) % pts.len()]);
                if (a.1 <= cy && b.1 > cy) || (b.1 <= cy && a.1 > cy) {
                    xs.push(a.0 + (cy - a.1) * (b.0 - a.0) / (b.1 - a.1));
                }
            }
            xs.sort_by(|a, b| a.total_cmp(b));
            let runs = xs
                .chunks(2)
                .filter(|c| c.len() == 2)
                .map(|c| (c[0].ceil() as i32, c[1].ceil() as i32))
                .collect();
            (y, runs)
        }))
    }

    pub fn union(&self, other: &Region) -> Region {
        let mut rows = self.rows();
        for (y, r) in other.rows() {
            rows.entry(y).or_default().extend(r);
        }
        Region::from_rows(rows)
    }

    pub fn contains(&self, x: i32, y: i32) -> bool {
        self.spans.iter().any(|s| s.y == y && x >= s.x0 && x < s.x1)
    }

    /// The 1 px inner frame (FrameRgn with a 1 x 1 brush): pixels of the region that
    /// have a 4-neighbour outside it.
    pub fn frame(&self) -> Region {
        let rows = self.rows();
        let inside = |y: i32, x: i32| {
            rows.get(&y)
                .is_some_and(|r| r.iter().any(|&(a, b)| x >= a && x < b))
        };
        Region::from_rows(rows.iter().map(|(&y, runs)| {
            let mut out = Vec::new();
            for &(a, b) in runs {
                for x in a..b {
                    if !inside(y, x - 1)
                        || !inside(y, x + 1)
                        || !inside(y - 1, x)
                        || !inside(y + 1, x)
                    {
                        out.push((x, x + 1));
                    }
                }
            }
            (y, out)
        }))
    }

    /// Translates the region.
    pub fn offset(&self, dx: i32, dy: i32) -> Region {
        Region {
            spans: self
                .spans
                .iter()
                .map(|s| Span {
                    y: s.y + dy,
                    x0: s.x0 + dx,
                    x1: s.x1 + dx,
                })
                .collect(),
        }
    }

    /// Scales by an integer factor (each pixel becomes k x k).
    pub fn scale(&self, k: i32) -> Region {
        let mut spans = Vec::new();
        for s in &self.spans {
            for j in 0..k {
                spans.push(Span {
                    y: s.y * k + j,
                    x0: s.x0 * k,
                    x1: s.x1 * k,
                });
            }
        }
        Region { spans }
    }

    /// Rectangles covering the region exactly: equal runs on consecutive rows merge.
    pub fn rects(&self) -> Vec<Rect> {
        let mut done: Vec<Rect> = Vec::new();
        let mut open: Vec<Rect> = Vec::new();
        let rows = self.rows();
        let mut prev_y = None;
        for (&y, runs) in &rows {
            let mut next = Vec::new();
            for &(a, b) in runs {
                let pos = if prev_y == Some(y - 1) {
                    open.iter().position(|r| r.x == a && r.right() == b)
                } else {
                    None
                };
                match pos {
                    Some(p) => {
                        let mut r = open.remove(p);
                        r.h += 1;
                        next.push(r);
                    }
                    None => next.push(Rect::new(a, y, b - a, 1)),
                }
            }
            done.append(&mut open);
            open = next;
            prev_y = Some(y);
        }
        done.append(&mut open);
        done.sort_by_key(|r| (r.y, r.x));
        done
    }

    /// The set of pixels with alpha > 0 in an image (B-PIX-3).
    pub fn from_alpha(img: &RgbaImage) -> Region {
        Region::from_rows((0..img.height).map(|y| {
            let mut runs = Vec::new();
            let mut x = 0;
            while x < img.width {
                if img.alpha(x, y) > 0 {
                    let s = x;
                    while x < img.width && img.alpha(x, y) > 0 {
                        x += 1;
                    }
                    runs.push((s as i32, x as i32));
                } else {
                    x += 1;
                }
            }
            (y as i32, runs)
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn b_wl_6_argb8888_premultiplied_byte_order() {
        let mut c = Canvas::new(1, 1);
        c.blend(0, 0, Rgb(200, 100, 50), 128);
        // premultiplied: round(200*128/255)=100, 50, 25
        assert_eq!(c.to_argb8888(), vec![25, 50, 100, 128]);
    }

    #[test]
    fn region_rects_merge_rows() {
        let r = Region::rect(Rect::new(1, 2, 3, 4)).union(&Region::rect(Rect::new(10, 2, 1, 1)));
        assert_eq!(
            r.rects(),
            vec![Rect::new(1, 2, 3, 4), Rect::new(10, 2, 1, 1)]
        );
    }

    #[test]
    fn frame_of_rect_is_border() {
        let f = Region::rect(Rect::new(0, 0, 4, 3)).frame();
        let n: i32 = f.spans.iter().map(|s| s.x1 - s.x0).sum();
        assert_eq!(n, 4 + 4 + 2);
    }

    #[test]
    fn polygon_right_triangle() {
        let r = Region::polygon(&[(0.0, 0.0), (4.0, 0.0), (0.0, 4.0)]);
        let w: Vec<i32> = r.spans.iter().map(|s| s.x1 - s.x0).collect();
        assert_eq!(w, vec![4, 3, 2, 1]);
    }

    #[test]
    fn round_rect_corners_cut() {
        let r = Region::round_rect(Rect::new(0, 0, 40, 30), 16, 20);
        assert!(!r.contains(0, 0));
        assert!(r.contains(20, 0));
        assert!(r.contains(0, 15));
    }
}
