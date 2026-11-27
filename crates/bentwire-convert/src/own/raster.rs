//! Rasteriser for the own characters' shapes, in two styles: coverage anti-aliasing into RGBA
//! (modern) or hard edges into palette indices, a pixel being set iff its centre is inside
//! (retro). Deterministic: IEEE arithmetic and `sqrt` only.

/// A point in canvas units (x right, y down).
pub type P = (f64, f64);

/// Colour roles of all own characters. A character's retro palette is its own list of roles,
/// in order, after the transparent index 0.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Ink,
    CopperDark,
    Copper,
    CopperLight,
    Face,
    White,
    MouthDark,
    Tongue,
    Yellow,
    Blue,
    Grey,
}

pub fn rgb(r: Role) -> [u8; 3] {
    match r {
        Role::Ink => [0x24, 0x16, 0x10],
        Role::CopperDark => [0x8c, 0x44, 0x1c],
        Role::Copper => [0xd0, 0x74, 0x30],
        Role::CopperLight => [0xf6, 0xbc, 0x7c],
        Role::Face => [0xff, 0xf4, 0xdc],
        Role::White => [0xff, 0xff, 0xff],
        Role::MouthDark => [0x7c, 0x1c, 0x20],
        Role::Tongue => [0xe8, 0x74, 0x74],
        Role::Yellow => [0xf8, 0xcc, 0x30],
        Role::Blue => [0x9c, 0xd0, 0xf4],
        Role::Grey => [0x9c, 0x9c, 0xa8],
    }
}

pub enum Shape {
    /// Polyline of the given width, round caps and joins.
    Stroke(Vec<P>, f64, Role),
    Disc(P, f64, Role),
    /// Axis-aligned ellipse: centre, radii.
    Ellipse(P, f64, f64, Role),
    /// Filled polygon (even-odd).
    Polygon(Vec<P>, Role),
    /// Translucent ellipse (centre, radii, alpha); the hard style leaves it out.
    Shadow(P, f64, f64, f64),
}

impl Shape {
    /// The shape under p -> o + (p - c) * z, its sizes times z.
    pub fn scaled(&self, c: P, z: f64, o: P) -> Shape {
        let m = |p: &P| (o.0 + (p.0 - c.0) * z, o.1 + (p.1 - c.1) * z);
        match self {
            Shape::Stroke(v, w, r) => Shape::Stroke(v.iter().map(m).collect(), w * z, *r),
            Shape::Disc(p, r, role) => Shape::Disc(m(p), r * z, *role),
            Shape::Ellipse(p, rx, ry, r) => Shape::Ellipse(m(p), rx * z, ry * z, *r),
            Shape::Polygon(v, r) => Shape::Polygon(v.iter().map(m).collect(), *r),
            Shape::Shadow(p, rx, ry, a) => Shape::Shadow(m(p), rx * z, ry * z, *a),
        }
    }
}

/// A canvas of `w` x `h` output pixels showing canvas units from `origin` at `k` pixels
/// per unit; a hard one holds indices into the palette of `roles`.
pub struct Canvas {
    pub w: usize,
    pub h: usize,
    k: f64,
    origin: P,
    hard: bool,
    roles: &'static [Role],
    rgba: Vec<[f64; 4]>,
    idx: Vec<u8>,
}

impl Canvas {
    pub fn new(w: u32, h: u32, k: f64, origin: P, hard: bool, roles: &'static [Role]) -> Self {
        let n = (w * h) as usize;
        Canvas {
            w: w as usize,
            h: h as usize,
            k,
            origin,
            hard,
            roles,
            rgba: if hard { vec![] } else { vec![[0.0; 4]; n] },
            idx: if hard { vec![0; n] } else { vec![] },
        }
    }

    /// Pixel rectangle covering canvas box (x0, y0, x1, y1), clipped.
    fn rect(&self, b: (f64, f64, f64, f64)) -> (usize, usize, usize, usize) {
        let (ox, oy, k) = (self.origin.0, self.origin.1, self.k);
        let c = |v: f64, max: usize| v.max(0.0).min(max as f64) as usize;
        (
            c(((b.0 - ox) * k).floor(), self.w),
            c(((b.1 - oy) * k).floor(), self.h),
            c(((b.2 - ox) * k).ceil(), self.w),
            c(((b.3 - oy) * k).ceil(), self.h),
        )
    }

    fn centre(&self, x: usize, y: usize) -> P {
        (
            self.origin.0 + (x as f64 + 0.5) / self.k,
            self.origin.1 + (y as f64 + 0.5) / self.k,
        )
    }

    /// Paint pixel (x, y) at signed distance `d` (canvas units, negative inside).
    fn paint(&mut self, x: usize, y: usize, d: f64, role: Role, alpha: f64) {
        let i = y * self.w + x;
        if self.hard {
            if d <= 0.0 {
                self.idx[i] = self.roles.iter().position(|&x| x == role).unwrap() as u8 + 1;
            }
            return;
        }
        let a = (0.5 - d * self.k).clamp(0.0, 1.0) * alpha;
        if a <= 0.0 {
            return;
        }
        let c = rgb(role).map(|v| v as f64 / 255.0);
        let p = &mut self.rgba[i];
        let oa = a + p[3] * (1.0 - a);
        for j in 0..3 {
            p[j] = (c[j] * a + p[j] * p[3] * (1.0 - a)) / oa;
        }
        p[3] = oa;
    }

    fn fill(&mut self, b: (f64, f64, f64, f64), role: Role, alpha: f64, f: impl Fn(P) -> f64) {
        let m = 1.0 / self.k + 0.5;
        let (x0, y0, x1, y1) = self.rect((b.0 - m, b.1 - m, b.2 + m, b.3 + m));
        for y in y0..y1 {
            for x in x0..x1 {
                let d = f(self.centre(x, y));
                self.paint(x, y, d, role, alpha);
            }
        }
    }

    pub fn draw(&mut self, s: &Shape) {
        match s {
            Shape::Stroke(pts, w, role) => self.stroke(pts, *w, *role),
            &Shape::Disc(c, r, role) => {
                self.fill((c.0 - r, c.1 - r, c.0 + r, c.1 + r), role, 1.0, |p| {
                    len(p.0 - c.0, p.1 - c.1) - r
                })
            }
            &Shape::Ellipse(c, rx, ry, role) => self.ellipse(c, rx, ry, role, 1.0),
            Shape::Polygon(pts, role) => self.polygon(pts, *role),
            &Shape::Shadow(c, rx, ry, a) => {
                if !self.hard {
                    self.ellipse(c, rx, ry, Role::Ink, a)
                }
            }
        }
    }

    fn ellipse(&mut self, c: P, rx: f64, ry: f64, role: Role, alpha: f64) {
        self.fill((c.0 - rx, c.1 - ry, c.0 + rx, c.1 + ry), role, alpha, |p| {
            let (u, v) = ((p.0 - c.0) / rx, (p.1 - c.1) / ry);
            (len(u, v) - 1.0) * rx.min(ry)
        });
    }

    fn stroke(&mut self, pts: &[P], width: f64, role: Role) {
        let r = width / 2.0;
        let bbox = bounds(pts, r);
        let (x0, y0, x1, y1) = self.rect(bbox);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        // Nearest segment distance per pixel, each segment visiting only its own box.
        let bw = x1 - x0;
        let mut best = vec![f64::MAX; bw * (y1 - y0)];
        let one = [pts[0], pts[0]];
        let segs: Vec<&[P]> = if pts.len() == 1 {
            vec![&one[..]]
        } else {
            pts.windows(2).collect()
        };
        let m = 1.0 / self.k + 0.5;
        for s in segs {
            let (a, b) = (s[0], s[1]);
            let (sx0, sy0, sx1, sy1) = self.rect((
                a.0.min(b.0) - r - m,
                a.1.min(b.1) - r - m,
                a.0.max(b.0) + r + m,
                a.1.max(b.1) + r + m,
            ));
            for y in sy0.max(y0)..sy1.min(y1) {
                for x in sx0.max(x0)..sx1.min(x1) {
                    let d = seg_dist(self.centre(x, y), a, b);
                    let e = &mut best[(y - y0) * bw + x - x0];
                    if d < *e {
                        *e = d;
                    }
                }
            }
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let d = best[(y - y0) * bw + x - x0];
                if d < f64::MAX {
                    self.paint(x, y, d - r, role, 1.0);
                }
            }
        }
    }

    fn polygon(&mut self, pts: &[P], role: Role) {
        let n = pts.len();
        self.fill(bounds(pts, 0.0), role, 1.0, |p| {
            let mut inside = false;
            let mut d = f64::MAX;
            for i in 0..n {
                let (a, b) = (pts[i], pts[(i + 1) % n]);
                if (a.1 > p.1) != (b.1 > p.1) && p.0 < a.0 + (p.1 - a.1) * (b.0 - a.0) / (b.1 - a.1)
                {
                    inside = !inside;
                }
                d = d.min(seg_dist(p, a, b));
            }
            if inside { -d } else { d }
        });
    }

    /// Straight-alpha RGBA8 (modern style).
    pub fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.rgba.len() * 4);
        for p in &self.rgba {
            let a = (p[3] * 255.0).round() as u8;
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                for v in &p[..3] {
                    out.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
                }
                out.push(a);
            }
        }
        out
    }

    /// Palette indices, 0 = transparent (retro style).
    pub fn indices(&self) -> Vec<u8> {
        self.idx.clone()
    }
}

/// The retro palette: index 0 (transparent) then `roles`.
pub fn palette(roles: &[Role]) -> Vec<[u8; 3]> {
    std::iter::once([0, 0, 0])
        .chain(roles.iter().map(|&r| rgb(r)))
        .collect()
}

fn len(x: f64, y: f64) -> f64 {
    (x * x + y * y).sqrt()
}

fn bounds(pts: &[P], r: f64) -> (f64, f64, f64, f64) {
    let mut b = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
    for &(x, y) in pts {
        b = (b.0.min(x), b.1.min(y), b.2.max(x), b.3.max(y));
    }
    (b.0 - r, b.1 - r, b.2 + r, b.3 + r)
}

fn seg_dist(p: P, a: P, b: P) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let t = if l2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0)
    };
    len(p.0 - a.0 - t * vx, p.1 - a.1 - t * vy)
}
