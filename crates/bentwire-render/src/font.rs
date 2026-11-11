//! Fonts: the bundled Tahoma (A-FONT-1), fontconfig lookup for a character's own
//! balloon font (B-BAL-2), GDI-like metrics and glyph rasterisation.
//!
//! Layout is advance-only (no kerning, no shaping), like GDI's DrawText for Latin
//! text. Every advance is rounded to whole logical pixels, so the width that the
//! metrics report is exactly the width that is drawn.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use ab_glyph_rasterizer::{Rasterizer, point};
use ttf_parser::{Face, GlyphId, OutlineBuilder, Tag};

use crate::canvas::{Canvas, Rgb};

pub static TAHOMA: &[u8] = include_bytes!("../../../assets/fonts/tahoma.ttf");
pub static TAHOMA_BOLD: &[u8] = include_bytes!("../../../assets/fonts/tahomabd.ttf");

/// GDI font height semantics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FontSize {
    /// Em height in px (negative LOGFONT lfHeight, B-BAL-2).
    Em(u32),
    /// Cell height in px, ascent + descent (positive lfHeight, R-OB-4).
    Cell(u32),
}

impl FontSize {
    /// GDI rule for a LOGFONT height: negative is em, positive is cell.
    pub fn from_lf_height(h: i32) -> FontSize {
        if h < 0 {
            FontSize::Em(h.unsigned_abs())
        } else {
            FontSize::Cell(h as u32)
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct FontSpec {
    pub family: String,
    pub size: FontSize,
    pub bold: bool,
    pub italic: bool,
}

impl FontSpec {
    pub fn tahoma(size: FontSize, bold: bool) -> FontSpec {
        FontSpec {
            family: "Tahoma".into(),
            size,
            bold,
            italic: false,
        }
    }
}

/// A loaded font file.
pub struct Font {
    data: Cow<'static, [u8]>,
    index: u32,
    upem: f32,
    win_ascent: f32,
    win_descent: f32,
    avg_width: Option<f32>,
    is_italic: bool,
    underline: (f32, f32),
}

impl Font {
    pub fn from_data(data: Cow<'static, [u8]>, index: u32) -> Option<Font> {
        let face = Face::parse(&data, index).ok()?;
        let upem = face.units_per_em() as f32;
        let (win_ascent, win_descent) = match face.tables().os2 {
            Some(os2) => (
                os2.windows_ascender() as f32,
                (os2.windows_descender() as f32).abs(),
            ),
            None => (face.ascender() as f32, -(face.descender() as f32)),
        };
        // OS/2 xAvgCharWidth is the i16 at offset 2.
        let avg_width = face
            .raw_face()
            .table(Tag::from_bytes(b"OS/2"))
            .filter(|t| t.len() >= 4)
            .map(|t| i16::from_be_bytes([t[2], t[3]]) as f32);
        let is_italic = face.is_italic();
        let underline = face
            .underline_metrics()
            .map(|m| (m.position as f32, m.thickness as f32))
            .unwrap_or((-upem / 10.0, upem / 14.0));
        Some(Font {
            data,
            index,
            upem,
            win_ascent,
            win_descent,
            avg_width,
            is_italic,
            underline,
        })
    }

    fn face(&self) -> Face<'_> {
        Face::parse(&self.data, self.index).expect("parsed at load")
    }

    fn em_px(&self, size: FontSize) -> f32 {
        match size {
            FontSize::Em(h) => h as f32,
            FontSize::Cell(h) => h as f32 * self.upem / (self.win_ascent + self.win_descent),
        }
    }
}

/// Integer metrics of a font at a size, in logical px.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Metrics {
    pub ascent: i32,
    pub descent: i32,
    /// TM.height = ascent + descent, rounded (B-BAL-2).
    pub height: i32,
    /// TM.aveCharWidth (B-BAL-2).
    pub ave_char_width: i32,
}

/// Font set and glyph cache. Implements the core's text metrics.
pub struct Fonts {
    system: bool,
    regular: Arc<Font>,
    bold: Arc<Font>,
    resolved: Mutex<HashMap<(String, bool, bool), Arc<Font>>>,
    glyphs: Mutex<HashMap<GlyphKey, Arc<GlyphBitmap>>>,
}

#[derive(Clone, PartialEq, Eq, Hash)]
struct GlyphKey {
    font: usize,
    glyph: u16,
    em_bits: u32,
    shear: bool,
}

struct GlyphBitmap {
    left: i32,
    top: i32,
    width: u32,
    height: u32,
    cov: Vec<u8>,
}

impl Fonts {
    /// Only the bundled Tahoma, for every family (golden tests, A-FONT-1).
    pub fn bundled() -> Fonts {
        Fonts {
            system: false,
            regular: Arc::new(Font::from_data(Cow::Borrowed(TAHOMA), 0).expect("tahoma")),
            bold: Arc::new(Font::from_data(Cow::Borrowed(TAHOMA_BOLD), 0).expect("tahomabd")),
            resolved: Mutex::new(HashMap::new()),
            glyphs: Mutex::new(HashMap::new()),
        }
    }

    /// Bundled Tahoma for "Tahoma", fontconfig for every other family (B-BAL-2).
    pub fn with_system() -> Fonts {
        Fonts {
            system: true,
            ..Fonts::bundled()
        }
    }

    fn font(&self, spec: &FontSpec) -> Arc<Font> {
        let bundled = if spec.bold { &self.bold } else { &self.regular };
        if !self.system || spec.family.eq_ignore_ascii_case("tahoma") {
            return bundled.clone();
        }
        let key = (spec.family.to_lowercase(), spec.bold, spec.italic);
        let mut cache = self.resolved.lock().unwrap();
        cache
            .entry(key)
            .or_insert_with(|| {
                resolve_fontconfig(&spec.family, spec.bold, spec.italic)
                    .unwrap_or_else(|| bundled.clone())
            })
            .clone()
    }

    pub fn metrics(&self, spec: &FontSpec) -> Metrics {
        let f = self.font(spec);
        let s = f.em_px(spec.size) / f.upem;
        let ascent = (f.win_ascent * s).round() as i32;
        let descent = (f.win_descent * s).round() as i32;
        let height = ((f.win_ascent + f.win_descent) * s).round() as i32;
        let ave_char_width = match f.avg_width {
            Some(w) => (w * s).round() as i32,
            None => self.char_advance(&f, spec.size, 'x'),
        };
        Metrics {
            ascent,
            descent,
            height,
            ave_char_width,
        }
    }

    fn char_advance(&self, f: &Font, size: FontSize, c: char) -> i32 {
        let face = f.face();
        let g = face.glyph_index(c).unwrap_or(GlyphId(0));
        let adv = face.glyph_hor_advance(g).unwrap_or(0) as f32;
        (adv * f.em_px(size) / f.upem).round() as i32
    }

    /// Single-line width in logical px; `&` is not interpreted. Newlines count as
    /// zero width.
    pub fn text_width(&self, spec: &FontSpec, text: &str) -> i32 {
        let f = self.font(spec);
        text.chars()
            .filter(|c| *c != '\n' && *c != '\r')
            .map(|c| self.char_advance(&f, spec.size, c))
            .sum()
    }

    /// Draws one line. (x, y) is the top-left of the line cell in logical px; the
    /// canvas is in buffer px, `k` = buffer scale.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_text(
        &self,
        canvas: &mut Canvas,
        k: u32,
        x: i32,
        y: i32,
        spec: &FontSpec,
        text: &str,
        colour: Rgb,
        underline: bool,
    ) -> i32 {
        let f = self.font(spec);
        let m = self.metrics(spec);
        let em = f.em_px(spec.size);
        let shear = spec.italic && !f.is_italic;
        let face = f.face();
        let ki = k as i32;
        let baseline = (y + m.ascent) * ki;
        let mut pen = x;
        for c in text.chars().filter(|c| *c != '\n' && *c != '\r') {
            let g = face.glyph_index(c).unwrap_or(GlyphId(0));
            if let Some(bm) = self.glyph(&f, &face, g, em * k as f32, shear) {
                let ox = pen * ki + bm.left;
                let oy = baseline - bm.top;
                for gy in 0..bm.height {
                    for gx in 0..bm.width {
                        let cov = bm.cov[(gy * bm.width + gx) as usize];
                        canvas.blend(ox + gx as i32, oy + gy as i32, colour, cov);
                    }
                }
            }
            pen += self.char_advance(&f, spec.size, c);
        }
        if underline {
            self.draw_underline(canvas, k, x, pen, y, spec, colour);
        }
        pen - x
    }

    /// Underline from x0 to x1 (logical px) for a line whose cell top is y (R-OB-16).
    #[allow(clippy::too_many_arguments)]
    pub fn draw_underline(
        &self,
        canvas: &mut Canvas,
        k: u32,
        x0: i32,
        x1: i32,
        y: i32,
        spec: &FontSpec,
        colour: Rgb,
    ) {
        let f = self.font(spec);
        let m = self.metrics(spec);
        let s = f.em_px(spec.size) / f.upem * k as f32;
        let ki = k as i32;
        let thick = (f.underline.1 * s).round().max(1.0) as i32;
        let top = (y + m.ascent) * ki + (-f.underline.0 * s).round().max(1.0) as i32 - thick / 2;
        canvas.fill_rect(x0 * ki, top, (x1 - x0) * ki, thick, colour);
    }

    fn glyph(
        &self,
        f: &Arc<Font>,
        face: &Face<'_>,
        g: GlyphId,
        em: f32,
        shear: bool,
    ) -> Option<Arc<GlyphBitmap>> {
        let key = GlyphKey {
            font: Arc::as_ptr(f) as usize,
            glyph: g.0,
            em_bits: em.to_bits(),
            shear,
        };
        if let Some(b) = self.glyphs.lock().unwrap().get(&key) {
            return Some(b.clone());
        }
        let s = em / f.upem;
        let sh = if shear { 0.2 } else { 0.0 };
        let mut pts = Collect {
            s,
            sh,
            cmds: Vec::new(),
            last: (0.0, 0.0),
            start: (0.0, 0.0),
        };
        face.outline_glyph(g, &mut pts)?;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for c in &pts.cmds {
            for p in c.points() {
                x0 = x0.min(p.0);
                x1 = x1.max(p.0);
                y0 = y0.min(p.1);
                y1 = y1.max(p.1);
            }
        }
        if x0 > x1 {
            return None;
        }
        let (left, top) = (x0.floor() as i32, y1.ceil() as i32);
        let width = (x1.ceil() as i32 - left).max(1) as u32;
        let height = (top - y0.floor() as i32).max(1) as u32;
        let mut r = Rasterizer::new(width as usize, height as usize);
        let tp = |p: (f32, f32)| point(p.0 - left as f32, top as f32 - p.1);
        for c in &pts.cmds {
            match *c {
                Seg::Line(a, b) => r.draw_line(tp(a), tp(b)),
                Seg::Quad(a, b, c) => r.draw_quad(tp(a), tp(b), tp(c)),
                Seg::Cubic(a, b, c, d) => r.draw_cubic(tp(a), tp(b), tp(c), tp(d)),
            }
        }
        let mut cov = vec![0u8; (width * height) as usize];
        r.for_each_pixel(|i, a| cov[i] = (a.clamp(0.0, 1.0) * 255.0).round() as u8);
        let bm = Arc::new(GlyphBitmap {
            left,
            top,
            width,
            height,
            cov,
        });
        self.glyphs.lock().unwrap().insert(key, bm.clone());
        Some(bm)
    }
}

enum Seg {
    Line((f32, f32), (f32, f32)),
    Quad((f32, f32), (f32, f32), (f32, f32)),
    Cubic((f32, f32), (f32, f32), (f32, f32), (f32, f32)),
}

impl Seg {
    fn points(&self) -> Vec<(f32, f32)> {
        match *self {
            Seg::Line(a, b) => vec![a, b],
            Seg::Quad(a, b, c) => vec![a, b, c],
            Seg::Cubic(a, b, c, d) => vec![a, b, c, d],
        }
    }
}

/// Collects an outline in px (y up), optionally sheared for synthetic italics.
struct Collect {
    s: f32,
    sh: f32,
    cmds: Vec<Seg>,
    last: (f32, f32),
    start: (f32, f32),
}

impl Collect {
    fn p(&self, x: f32, y: f32) -> (f32, f32) {
        ((x + y * self.sh) * self.s, y * self.s)
    }
}

impl OutlineBuilder for Collect {
    fn move_to(&mut self, x: f32, y: f32) {
        self.last = self.p(x, y);
        self.start = self.last;
    }
    fn line_to(&mut self, x: f32, y: f32) {
        let p = self.p(x, y);
        self.cmds.push(Seg::Line(self.last, p));
        self.last = p;
    }
    fn quad_to(&mut self, x1: f32, y1: f32, x: f32, y: f32) {
        let (c, p) = (self.p(x1, y1), self.p(x, y));
        self.cmds.push(Seg::Quad(self.last, c, p));
        self.last = p;
    }
    fn curve_to(&mut self, x1: f32, y1: f32, x2: f32, y2: f32, x: f32, y: f32) {
        let (c1, c2, p) = (self.p(x1, y1), self.p(x2, y2), self.p(x, y));
        self.cmds.push(Seg::Cubic(self.last, c1, c2, p));
        self.last = p;
    }
    fn close(&mut self) {
        if self.last != self.start {
            self.cmds.push(Seg::Line(self.last, self.start));
        }
        self.last = self.start;
    }
}

/// Resolves a family through fontconfig (`fc-match`); a missing family falls back to
/// `sans-serif` (B-BAL-2). Never used by tests (A-FONT-1).
fn resolve_fontconfig(family: &str, bold: bool, italic: bool) -> Option<Arc<Font>> {
    let style = format!(
        "{}{}",
        if bold { ":bold" } else { "" },
        if italic { ":italic" } else { "" }
    );
    let query = |pattern: &str| -> Option<(String, String, u32)> {
        let out = std::process::Command::new("fc-match")
            .args(["-f", "%{family}\n%{file}\n%{index}", pattern])
            .output()
            .ok()?;
        let text = String::from_utf8(out.stdout).ok()?;
        let mut l = text.lines();
        Some((
            l.next()?.to_string(),
            l.next()?.to_string(),
            l.next()?.parse().unwrap_or(0),
        ))
    };
    let escaped = family.replace('-', "\\-").replace(':', "\\:");
    let (families, mut file, mut index) = query(&format!("{escaped}{style}"))?;
    if !families
        .split(',')
        .any(|f| f.trim().eq_ignore_ascii_case(family))
    {
        (_, file, index) = query(&format!("sans-serif{style}"))?;
    }
    let data = std::fs::read(file).ok()?;
    Font::from_data(Cow::Owned(data), index).map(Arc::new)
}

/// Splits a caption with a `&` mnemonic: returns the display text and the char index
/// of the mnemonic letter. `&&` is a literal `&`.
pub fn mnemonic(caption: &str) -> (String, Option<usize>) {
    let mut out = String::new();
    let mut idx = None;
    let mut it = caption.chars().peekable();
    while let Some(c) = it.next() {
        if c == '&' {
            match it.next() {
                Some('&') => out.push('&'),
                Some(n) => {
                    if idx.is_none() {
                        idx = Some(out.chars().count());
                    }
                    out.push(n);
                }
                None => {}
            }
        } else {
            out.push(c);
        }
    }
    (out, idx)
}

impl Fonts {
    /// Draws a caption with `&` mnemonic; the mnemonic letter is underlined when
    /// `show_mnemonic`. Returns the drawn width.
    #[allow(clippy::too_many_arguments)]
    pub fn draw_caption(
        &self,
        canvas: &mut Canvas,
        k: u32,
        x: i32,
        y: i32,
        spec: &FontSpec,
        caption: &str,
        colour: Rgb,
        show_mnemonic: bool,
    ) -> i32 {
        let (text, idx) = mnemonic(caption);
        let w = self.draw_text(canvas, k, x, y, spec, &text, colour, false);
        if let (Some(i), true) = (idx, show_mnemonic) {
            let before: String = text.chars().take(i).collect();
            let letter: String = text.chars().skip(i).take(1).collect();
            let x0 = x + self.text_width(spec, &before);
            let x1 = x0 + self.text_width(spec, &letter);
            self.draw_underline(canvas, k, x0, x1, y, spec, colour);
        }
        w
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};

    fn hex(d: &[u8]) -> String {
        d.iter().map(|b| format!("{b:02x}")).collect()
    }

    #[test]
    fn a_font_1_bundled_tahoma_hashes() {
        let readme = include_str!("../../../assets/fonts/README.md");
        let pinned = |name: &str| {
            readme
                .lines()
                .find(|l| l.trim_end().ends_with(name))
                .and_then(|l| l.split_whitespace().next())
                .unwrap()
                .to_string()
        };
        assert_eq!(hex(&Sha256::digest(TAHOMA)), pinned("tahoma.ttf"));
        assert_eq!(hex(&Sha256::digest(TAHOMA_BOLD)), pinned("tahomabd.ttf"));
    }

    #[test]
    fn b_bal_2_metrics_from_font() {
        let f = Fonts::bundled();
        let spec = FontSpec::tahoma(FontSize::Em(13), false);
        let m = f.metrics(&spec);
        assert_eq!(m.height, m.ascent + m.descent);
        assert!(m.ave_char_width > 0);
        // R-OB-4: a positive (cell) height gives exactly that TM.height.
        for h in [13, 16] {
            assert_eq!(
                f.metrics(&FontSpec::tahoma(FontSize::Cell(h), false))
                    .height,
                h as i32
            );
            assert_eq!(
                f.metrics(&FontSpec::tahoma(FontSize::Cell(h), true)).height,
                h as i32
            );
        }
    }

    #[test]
    fn text_width_is_sum_of_advances() {
        let f = Fonts::bundled();
        let spec = FontSpec::tahoma(FontSize::Cell(13), false);
        let w = f.text_width(&spec, "Hello world");
        let sum: i32 = "Hello world"
            .chars()
            .map(|c| f.text_width(&spec, &c.to_string()))
            .sum();
        assert_eq!(w, sum);
        assert!(w > 40 && w < 90, "{w}");
    }

    #[test]
    fn b_menu_2_mnemonic_parsing() {
        assert_eq!(mnemonic("&Hide"), ("Hide".into(), Some(0)));
        assert_eq!(
            mnemonic("Assistenten &wählen..."),
            ("Assistenten wählen...".into(), Some(12))
        );
        assert_eq!(mnemonic("A&&B"), ("A&B".into(), None));
        assert_eq!(mnemonic("OK"), ("OK".into(), None));
    }

    #[test]
    fn draw_text_marks_pixels_inside_cell() {
        let f = Fonts::bundled();
        let spec = FontSpec::tahoma(FontSize::Cell(13), false);
        let mut c = Canvas::new(60, 13);
        f.draw_text(&mut c, 1, 0, 0, &spec, "Hg", Rgb::BLACK, false);
        assert!(c.px.iter().any(|p| p[3] > 200));
    }
}
