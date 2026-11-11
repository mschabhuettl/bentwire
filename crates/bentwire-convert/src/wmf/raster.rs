//! Aliased scan conversion on an indexed canvas (bentwire decision W-RAST-1, spec 3.14):
//! a pixel belongs to a shape iff its centre (x + 0.5, y + 0.5) lies inside it.

/// A point in device pixels.
pub type Pt = (f64, f64);

/// Coverage of one primitive. Each covered pixel is set once, so raster operations that
/// read the destination (R2_NOTXORPEN) apply once per pixel.
pub struct Mask {
    pub w: i32,
    pub h: i32,
    pub bits: Vec<bool>,
    /// Bounding box of the set bits (x0, y0, x1, y1 inclusive); empty when x0 > x1.
    pub dirty: (i32, i32, i32, i32),
}

impl Mask {
    pub fn new(w: i32, h: i32) -> Mask {
        Mask {
            w,
            h,
            bits: vec![false; (w.max(0) as usize) * (h.max(0) as usize)],
            dirty: (i32::MAX, i32::MAX, -1, -1),
        }
    }

    /// Visit and clear every set bit, in row-major order.
    pub fn drain(&mut self, mut f: impl FnMut(usize)) {
        let (x0, y0, x1, y1) = self.dirty;
        for y in y0.max(0)..=y1 {
            for x in x0.max(0)..=x1 {
                let i = (y * self.w + x) as usize;
                if self.bits[i] {
                    self.bits[i] = false;
                    f(i);
                }
            }
        }
        self.dirty = (i32::MAX, i32::MAX, -1, -1);
    }
}

/// Pixel coverage target: `plot(x, y)` for every covered pixel inside the canvas.
pub trait Plot {
    fn size(&self) -> (i32, i32);
    fn plot(&mut self, x: i32, y: i32);
}

impl Plot for Mask {
    fn size(&self) -> (i32, i32) {
        (self.w, self.h)
    }

    fn plot(&mut self, x: i32, y: i32) {
        if x >= 0 && y >= 0 && x < self.w && y < self.h {
            self.bits[(y * self.w + x) as usize] = true;
            let d = &mut self.dirty;
            *d = (d.0.min(x), d.1.min(y), d.2.max(x), d.3.max(y));
        }
    }
}

fn row_range<P: Plot>(p: &P, y0: f64, y1: f64) -> (i32, i32) {
    let (_, h) = p.size();
    let a = (y0 - 0.5).ceil().max(0.0) as i32;
    let b = ((y1 - 0.5).floor() as i64).min(i64::from(h) - 1) as i32;
    (a, b)
}

/// Fill polygons (several rings) with even-odd (`winding = false`) or nonzero rule.
pub fn fill_polygons<P: Plot>(p: &mut P, rings: &[Vec<Pt>], winding: bool) {
    let mut edges: Vec<(Pt, Pt)> = Vec::new();
    for r in rings {
        if r.len() < 2 {
            continue;
        }
        for k in 0..r.len() {
            let a = r[k];
            let b = r[(k + 1) % r.len()];
            if a.1 != b.1 {
                edges.push((a, b));
            }
        }
    }
    if edges.is_empty() {
        return;
    }
    let ymin = edges
        .iter()
        .map(|e| e.0.1.min(e.1.1))
        .fold(f64::INFINITY, f64::min);
    let ymax = edges
        .iter()
        .map(|e| e.0.1.max(e.1.1))
        .fold(f64::NEG_INFINITY, f64::max);
    let (w, _) = p.size();
    let (r0, r1) = row_range(p, ymin, ymax);
    let mut xs: Vec<(f64, i32)> = Vec::new();
    for y in r0..=r1 {
        let yc = f64::from(y) + 0.5;
        xs.clear();
        for &(a, b) in &edges {
            let (lo, hi, dir) = if a.1 < b.1 { (a, b, 1) } else { (b, a, -1) };
            if yc >= lo.1 && yc < hi.1 {
                let x = lo.0 + (yc - lo.1) * (hi.0 - lo.0) / (hi.1 - lo.1);
                xs.push((x, dir));
            }
        }
        xs.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut wind = 0;
        for k in 0..xs.len().saturating_sub(1) {
            wind += xs[k].1;
            let inside = if winding { wind != 0 } else { (k + 1) % 2 == 1 };
            if inside {
                span(p, y, xs[k].0, xs[k + 1].0, w);
            }
        }
    }
}

fn span<P: Plot>(p: &mut P, y: i32, xa: f64, xb: f64, w: i32) {
    let a = (xa - 0.5).ceil().max(0.0);
    let b = (xb - 0.5).ceil().min(f64::from(w));
    let (a, b) = (a as i32, b as i32);
    for x in a..b {
        p.plot(x, y);
    }
}

/// Fill everything for which `inside(cx, cy)` holds, within a bounding box.
pub fn fill_test<P: Plot>(
    p: &mut P,
    bbox: (f64, f64, f64, f64),
    inside: impl Fn(f64, f64) -> bool,
) {
    let (w, _) = p.size();
    let (r0, r1) = row_range(p, bbox.1, bbox.3);
    let c0 = (bbox.0 - 0.5).ceil().max(0.0) as i32;
    let c1 = ((bbox.2 - 0.5).floor() as i64).min(i64::from(w) - 1) as i32;
    for y in r0..=r1 {
        for x in c0..=c1 {
            if inside(f64::from(x) + 0.5, f64::from(y) + 0.5) {
                p.plot(x, y);
            }
        }
    }
}

/// Inside test for an ellipse inscribed in (l, t, r, b).
pub fn in_ellipse(b: (f64, f64, f64, f64), x: f64, y: f64) -> bool {
    let rx = (b.2 - b.0) / 2.0;
    let ry = (b.3 - b.1) / 2.0;
    if rx <= 0.0 || ry <= 0.0 {
        return false;
    }
    let dx = (x - (b.0 + rx)) / rx;
    let dy = (y - (b.1 + ry)) / ry;
    dx * dx + dy * dy <= 1.0
}

/// Inside test for a rounded rectangle with corner ellipse size (ew, eh).
pub fn in_round_rect(b: (f64, f64, f64, f64), ew: f64, eh: f64, x: f64, y: f64) -> bool {
    if x < b.0 || x > b.2 || y < b.1 || y > b.3 {
        return false;
    }
    let rx = (ew / 2.0).min((b.2 - b.0) / 2.0).max(0.0);
    let ry = (eh / 2.0).min((b.3 - b.1) / 2.0).max(0.0);
    let cx = if x < b.0 + rx {
        b.0 + rx
    } else if x > b.2 - rx {
        b.2 - rx
    } else {
        return true;
    };
    let cy = if y < b.1 + ry {
        b.1 + ry
    } else if y > b.3 - ry {
        b.3 - ry
    } else {
        return true;
    };
    if rx == 0.0 || ry == 0.0 {
        return true;
    }
    let dx = (x - cx) / rx;
    let dy = (y - cy) / ry;
    dx * dx + dy * dy <= 1.0
}

/// One-pixel line from `a` to `b` (integer device points), last point excluded.
pub fn cosmetic_line<P: Plot>(p: &mut P, a: (i64, i64), b: (i64, i64)) {
    let (w, h) = p.size();
    let dx = (b.0 - a.0).abs();
    let dy = (b.1 - a.1).abs();
    let n = dx.max(dy);
    // Skip lines that lie wholly outside the canvas.
    if a.0.max(b.0) < 0
        || a.1.max(b.1) < 0
        || a.0.min(b.0) >= i64::from(w)
        || a.1.min(b.1) >= i64::from(h)
    {
        return;
    }
    // Restrict the steps to those that can land inside the canvas (exact test below).
    let (mut k0, mut k1) = (0i64, n);
    for (a, d, size) in [
        (a.0, b.0 - a.0, i64::from(w)),
        (a.1, b.1 - a.1, i64::from(h)),
    ] {
        if d != 0 {
            let t0 = (-a) as f64 * n as f64 / d as f64;
            let t1 = (size - a) as f64 * n as f64 / d as f64;
            k0 = k0.max(t0.min(t1).floor() as i64 - 2);
            k1 = k1.min(t0.max(t1).ceil() as i64 + 2);
        }
    }
    for k in k0.max(0)..k1.min(n) {
        // round-half-up along the minor axis
        let x = a.0 + (2 * k * (b.0 - a.0) + n).div_euclid(2 * n);
        let y = a.1 + (2 * k * (b.1 - a.1) + n).div_euclid(2 * n);
        if (0..i64::from(w)).contains(&x) && (0..i64::from(h)).contains(&y) {
            p.plot(x as i32, y as i32);
        }
    }
}

/// Wide line: every pixel whose centre is within `r` of the segment (round caps). Each
/// row only tests the columns near the capsule's extent on that row.
pub fn wide_segment<P: Plot>(p: &mut P, a: Pt, b: Pt, r: f64) {
    let (w, _) = p.size();
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let inside = |x: f64, y: f64| {
        let t = if l2 == 0.0 {
            0.0
        } else {
            (((x - a.0) * vx + (y - a.1) * vy) / l2).clamp(0.0, 1.0)
        };
        let (dx, dy) = (x - a.0 - t * vx, y - a.1 - t * vy);
        dx * dx + dy * dy <= r * r
    };
    let len = l2.sqrt();
    let n = if len > 0.0 {
        (-vy / len * r, vx / len * r)
    } else {
        (0.0, 0.0)
    };
    let (r0, r1) = row_range(p, a.1.min(b.1) - r, a.1.max(b.1) + r);
    for y in r0..=r1 {
        let yc = f64::from(y) + 0.5;
        // the capsule is convex: its row extent is the min/max over the end circles and
        // the two long edges
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for c in [a, b] {
            let dy = yc - c.1;
            if dy.abs() <= r {
                let dx = (r * r - dy * dy).sqrt();
                lo = lo.min(c.0 - dx);
                hi = hi.max(c.0 + dx);
            }
        }
        for s in [1.0, -1.0] {
            let (p0, p1) = (
                (a.0 + s * n.0, a.1 + s * n.1),
                (b.0 + s * n.0, b.1 + s * n.1),
            );
            if p0.1 != p1.1 {
                let t = (yc - p0.1) / (p1.1 - p0.1);
                if (0.0..=1.0).contains(&t) {
                    let x = p0.0 + t * (p1.0 - p0.0);
                    lo = lo.min(x);
                    hi = hi.max(x);
                }
            } else if (yc - p0.1).abs() < 1e-9 {
                lo = lo.min(p0.0.min(p1.0));
                hi = hi.max(p0.0.max(p1.0));
            }
        }
        if lo > hi {
            continue;
        }
        // one pixel of slack on both sides; the exact test decides
        let c0 = (lo - 1.5).ceil().max(0.0) as i64;
        let c1 = ((hi + 0.5).floor() as i64).min(i64::from(w) - 1);
        for x in c0..=c1 {
            if inside(x as f64 + 0.5, yc) {
                p.plot(x as i32, y);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d2_wmf_long_cosmetic_line_is_clipped_not_truncated() {
        let mut m = Mask::new(64, 16);
        cosmetic_line(&mut m, (-10_000_000, 5), (10_000_000, 5));
        assert_eq!(m.bits.iter().filter(|b| **b).count(), 64);
        let mut m = Mask::new(64, 16);
        cosmetic_line(&mut m, (-3_000_000, -3_000_000), (3_000_000, 3_000_000));
        assert_eq!(m.bits.iter().filter(|b| **b).count(), 16);
    }
}
