//! TextOut with the bundled Wine Tahoma as the font for every face name (bentwire
//! decision W-TEXT-1, spec 3.14): glyph outlines filled aliased with the nonzero rule.

use std::sync::OnceLock;

use super::raster::{Mask, Pt, fill_polygons};

static REGULAR: &[u8] = include_bytes!("../../../../assets/fonts/tahoma.ttf");
static BOLD: &[u8] = include_bytes!("../../../../assets/fonts/tahomabd.ttf");

fn face(bold: bool) -> Option<&'static ttf_parser::Face<'static>> {
    static R: OnceLock<Option<ttf_parser::Face<'static>>> = OnceLock::new();
    static B: OnceLock<Option<ttf_parser::Face<'static>>> = OnceLock::new();
    let cell = if bold { &B } else { &R };
    cell.get_or_init(|| ttf_parser::Face::parse(if bold { BOLD } else { REGULAR }, 0).ok())
        .as_ref()
}

/// cp1252 byte to char (undefined bytes map to U+FFFD, which has no glyph).
fn cp1252(b: u8) -> char {
    const HI: [u16; 32] = [
        0x20AC, 0xFFFD, 0x201A, 0x0192, 0x201E, 0x2026, 0x2020, 0x2021, 0x02C6, 0x2030, 0x0160,
        0x2039, 0x0152, 0xFFFD, 0x017D, 0xFFFD, 0xFFFD, 0x2018, 0x2019, 0x201C, 0x201D, 0x2022,
        0x2013, 0x2014, 0x02DC, 0x2122, 0x0161, 0x203A, 0x0153, 0xFFFD, 0x017E, 0x0178,
    ];
    if (0x80..0xA0).contains(&b) {
        char::from_u32(u32::from(HI[usize::from(b - 0x80)])).unwrap_or('\u{FFFD}')
    } else {
        char::from(b)
    }
}

struct Outline {
    rings: Vec<Vec<Pt>>,
    cur: Vec<Pt>,
    ox: f64,
    oy: f64,
    s: f64,
    last: Pt,
}

impl Outline {
    fn map(&self, x: f32, y: f32) -> Pt {
        (
            self.ox + f64::from(x) * self.s,
            self.oy - f64::from(y) * self.s,
        )
    }
}

impl ttf_parser::OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        if !self.cur.is_empty() {
            self.rings.push(std::mem::take(&mut self.cur));
        }
        self.last = self.map(x, y);
        self.cur.push(self.last);
    }

    fn line_to(&mut self, x: f32, y: f32) {
        self.last = self.map(x, y);
        self.cur.push(self.last);
    }

    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (p0, p1, p2) = (self.last, self.map(x1, y1), self.map(x, y));
        for k in 1..=8 {
            let t = f64::from(k) / 8.0;
            let u = 1.0 - t;
            self.cur.push((
                u * u * p0.0 + 2.0 * u * t * p1.0 + t * t * p2.0,
                u * u * p0.1 + 2.0 * u * t * p1.1 + t * t * p2.1,
            ));
        }
        self.last = p2;
    }

    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (p0, p1, p2, p3) = (
            self.last,
            self.map(x1, y1),
            self.map(x2, y2),
            self.map(x, y),
        );
        for k in 1..=12 {
            let t = f64::from(k) / 12.0;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            self.cur.push((
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            ));
        }
        self.last = p3;
    }

    fn close(&mut self) {
        if !self.cur.is_empty() {
            self.rings.push(std::mem::take(&mut self.cur));
        }
    }
}

/// Text placement inputs, all in device pixels.
pub struct TextRun<'a> {
    pub bytes: &'a [u8],
    pub x: f64,
    pub y: f64,
    /// LOGFONT height already scaled to device pixels (sign kept: negative = em height,
    /// positive = cell height, 0 = 12 px em).
    pub height: f64,
    pub bold: bool,
    /// SetTextAlign flags (TA_RIGHT 2, TA_CENTER 6, TA_BOTTOM 8, TA_BASELINE 24).
    pub align: u16,
}

/// Plot the glyphs of `run` into `glyphs`; returns the text cell (l, t, r, b) for an
/// opaque background.
pub fn draw(glyphs: &mut Mask, run: &TextRun) -> Option<(f64, f64, f64, f64)> {
    let f = face(run.bold)?;
    let upem = f64::from(f.units_per_em());
    let asc = f64::from(f.ascender());
    let desc = f64::from(f.descender()); // negative
    let s = if run.height < 0.0 {
        -run.height / upem
    } else if run.height > 0.0 {
        run.height / (asc - desc)
    } else {
        12.0 / upem
    };
    let ids: Vec<_> = run
        .bytes
        .iter()
        .map(|&b| f.glyph_index(cp1252(b)).unwrap_or(ttf_parser::GlyphId(0)))
        .collect();
    let width: f64 = ids
        .iter()
        .map(|&g| f64::from(f.glyph_hor_advance(g).unwrap_or(0)) * s)
        .sum();
    let x = match run.align & 6 {
        2 => run.x - width,
        6 => run.x - width / 2.0,
        _ => run.x,
    };
    let base = match run.align & 24 {
        24 => run.y,
        8 => run.y + desc * s,
        _ => run.y + asc * s,
    };
    let mut pen = x;
    for (&g, &b) in ids.iter().zip(run.bytes) {
        if g.0 != 0 && b >= 0x20 {
            let mut o = Outline {
                rings: Vec::new(),
                cur: Vec::new(),
                ox: pen,
                oy: base,
                s,
                last: (0.0, 0.0),
            };
            if f.outline_glyph(g, &mut o).is_some() {
                ttf_parser::OutlineBuilder::close(&mut o);
                fill_polygons(glyphs, &o.rings, true);
            }
        }
        pen += f64::from(f.glyph_hor_advance(g).unwrap_or(0)) * s;
    }
    Some((x, base - asc * s, x + width, base - desc * s))
}
