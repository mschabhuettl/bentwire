//! WMF rasteriser for vector ACT actors (act-format 4.6, decision D2). Owner: act teammate.
//!
//! Plays the 24 record types that occur in the actor metafiles into an indexed
//! [`Canvas`], aliased (no antialiasing). Geometry follows MM_ANISOTROPIC with the
//! viewport set to the destination rectangle and the window from the placeable bbox
//! or the metafile's own SetWindowOrg/SetWindowExt. The rules where GDI behaviour is
//! not documented are bentwire decisions W-RAST-1 to W-TEXT-1 in spec 3.14.

mod raster;
mod text;

use agent_formats::act::{ActFile, Payload, Wmf, WmfRecord};
use raster::{
    Mask, Pt, cosmetic_line, fill_polygons, fill_test, in_ellipse, in_round_rect, wide_segment,
};

/// COLORREF that Office 97 replaces by its transparent colour (act-format 4.6).
pub const MAGIC_TRANSPARENT: u32 = 0x00F5_FAF5;

/// An indexed picture. `palette[transparent]` is never chosen for a colour.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    /// Palette indices, row-major, top-down.
    pub pixels: Vec<u8>,
    pub palette: Vec<[u8; 3]>,
    pub transparent: u8,
}

impl Canvas {
    /// A fully transparent canvas. `palette` must hold at most 256 entries and include the
    /// `transparent` slot.
    pub fn new(width: u32, height: u32, palette: Vec<[u8; 3]>, transparent: u8) -> Canvas {
        Canvas {
            width,
            height,
            pixels: vec![transparent; width as usize * height as usize],
            palette,
            transparent,
        }
    }

    /// Nearest palette entry (squared RGB distance, lowest index on ties), never the
    /// transparent one (W-COL-1).
    fn nearest(&self, c: [u8; 3]) -> u8 {
        let mut best = (u32::MAX, self.transparent);
        for (i, p) in self.palette.iter().enumerate().take(256) {
            if i == usize::from(self.transparent) {
                continue;
            }
            let d: u32 = (0..3)
                .map(|k| (i32::from(p[k]) - i32::from(c[k])).pow(2) as u32)
                .sum();
            if d < best.0 {
                best = (d, i as u8);
            }
        }
        best.1
    }

    fn color_index(&self, cr: u32) -> u8 {
        if cr & 0x00FF_FFFF == MAGIC_TRANSPARENT {
            self.transparent
        } else {
            self.nearest(rgb(cr))
        }
    }
}

fn rgb(cr: u32) -> [u8; 3] {
    [cr as u8, (cr >> 8) as u8, (cr >> 16) as u8]
}

/// Destination rectangle in canvas pixels (right and bottom exclusive).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum WmfError {
    #[error("record {index} (function {function:#06x}): {detail}")]
    Malformed {
        index: usize,
        function: u16,
        detail: &'static str,
    },
    #[error("record {index}: unsupported function {function:#06x}")]
    Unsupported { index: usize, function: u16 },
    #[error("pose {pose}: {detail}")]
    Pose { pose: u16, detail: String },
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Pen {
    style: u16,
    width: i16,
    color: u32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct Brush {
    style: u16,
    color: u32,
}

#[derive(Debug, Clone, PartialEq)]
struct Font {
    height: i16,
    weight: u16,
}

#[derive(Debug, Clone, PartialEq)]
enum Obj {
    Pen(Pen),
    Brush(Brush),
    Font(Font),
    /// Palettes and regions occupy a slot but are not used (none occur in the samples).
    Other,
}

#[derive(Debug, Clone, PartialEq)]
struct Dc {
    pen: Pen,
    brush: Brush,
    font: Font,
    text_color: u32,
    bk_color: u32,
    bk_opaque: bool,
    winding: bool,
    rop2: u16,
    align: u16,
    worg: (f64, f64),
    wext: (f64, f64),
}

const PS_NULL: u16 = 5;
const PS_INSIDEFRAME: u16 = 6;
const BS_NULL: u16 = 1;
const R2_COPYPEN: u16 = 13;

/// Raster operation on RGB channels, ROP2 code 1..=16 (bit `2*P + D` of `code - 1`).
fn rop(code: u16, d: [u8; 3], p: [u8; 3]) -> [u8; 3] {
    let f = (code.wrapping_sub(1) & 15) as u8;
    let mut out = [0u8; 3];
    for k in 0..3 {
        let (dv, pv) = (d[k], p[k]);
        let mut r = 0u8;
        for (bit, m) in [(0, !pv & !dv), (1, !pv & dv), (2, pv & !dv), (3, pv & dv)] {
            if f >> bit & 1 == 1 {
                r |= m;
            }
        }
        out[k] = r;
    }
    out
}

struct Player<'c> {
    c: &'c mut Canvas,
    dest: (f64, f64, f64, f64),
    dc: Dc,
    stack: Vec<Dc>,
    objs: Vec<Option<Obj>>,
    mask: Mask,
}

type R<T> = Result<T, WmfError>;

impl Player<'_> {
    fn tx(&self, x: f64) -> f64 {
        self.dest.0 + (x - self.dc.worg.0) * (self.dest.2 - self.dest.0) / self.dc.wext.0
    }

    fn ty(&self, y: f64) -> f64 {
        self.dest.1 + (y - self.dc.worg.1) * (self.dest.3 - self.dest.1) / self.dc.wext.1
    }

    /// Logical point to device, rounded to whole pixels (W-RAST-1), clamped far outside.
    fn pt(&self, x: i16, y: i16) -> (i64, i64) {
        let c = |v: f64| v.round().clamp(-1e7, 1e7) as i64;
        (c(self.tx(f64::from(x))), c(self.ty(f64::from(y))))
    }

    fn scale_x(&self) -> f64 {
        ((self.dest.2 - self.dest.0) / self.dc.wext.0).abs()
    }

    fn scale_y(&self) -> f64 {
        ((self.dest.3 - self.dest.1) / self.dc.wext.1).abs()
    }

    /// Device pen width; 0 means cosmetic (one pixel).
    fn pen_px(&self) -> f64 {
        let w = (f64::from(self.dc.pen.width.max(0)) * self.scale_x()).round();
        if w <= 1.0 { 0.0 } else { w }
    }

    fn degenerate(&self) -> bool {
        self.dc.wext.0 == 0.0 || self.dc.wext.1 == 0.0
    }

    /// Paint the current mask with colour `cr` through the current ROP2, then clear it.
    fn paint(&mut self, cr: u32) {
        let idx = self.c.color_index(cr);
        let code = self.dc.rop2;
        for (i, m) in self.mask.bits.iter_mut().enumerate() {
            if !*m {
                continue;
            }
            *m = false;
            let px = self.c.pixels[i];
            let v: u8 = match code {
                R2_COPYPEN => idx,
                // ops that ignore the destination
                1 | 4 | 16 => {
                    if cr & 0xFF_FFFF == MAGIC_TRANSPARENT {
                        self.c.transparent
                    } else {
                        self.c.nearest(rop(code, [0; 3], rgb(cr)))
                    }
                }
                11 => px,
                // destination-dependent ops leave transparent pixels transparent (W-ROP-1)
                _ if px == self.c.transparent => px,
                _ => {
                    let d = self
                        .c
                        .palette
                        .get(usize::from(px))
                        .copied()
                        .unwrap_or([0; 3]);
                    self.c.nearest(rop(code, d, rgb(cr)))
                }
            };
            self.c.pixels[i] = v;
        }
    }

    fn stroke_rings(&mut self, rings: &[Vec<(i64, i64)>], closed: bool) {
        if self.dc.pen.style == PS_NULL {
            return;
        }
        let w = self.pen_px();
        let f = |p: (i64, i64)| (p.0 as f64, p.1 as f64);
        for r in rings {
            let n = r.len();
            if n == 0 {
                continue;
            }
            let segs = if closed { n } else { n - 1 };
            for k in 0..segs {
                let (a, b) = (r[k], r[(k + 1) % n]);
                if w == 0.0 {
                    cosmetic_line(&mut self.mask, a, b);
                } else {
                    wide_segment(&mut self.mask, f(a), f(b), w / 2.0);
                }
            }
            if n == 1 && w > 0.0 {
                wide_segment(&mut self.mask, f(r[0]), f(r[0]), w / 2.0);
            }
        }
        self.paint(self.dc.pen.color);
    }

    fn polygons(&mut self, rings: Vec<Vec<(i64, i64)>>) {
        if self.dc.brush.style != BS_NULL {
            let f: Vec<Vec<Pt>> = rings
                .iter()
                .map(|r| r.iter().map(|&(x, y)| (x as f64, y as f64)).collect())
                .collect();
            fill_polygons(&mut self.mask, &f, self.dc.winding);
            self.paint(self.dc.brush.color);
        }
        self.stroke_rings(&rings, true);
    }

    /// Ellipse or rounded rectangle in device box `b`; `corner` = Some((ew, eh)) for
    /// RoundRect. The brush fills the box shape, then the pen paints a ring of its width:
    /// centred on the edge, or inside the box for PS_INSIDEFRAME and cosmetic pens
    /// (W-RAST-2).
    fn boxed(&mut self, b: (f64, f64, f64, f64), corner: Option<(f64, f64)>) {
        let shape = |bx: (f64, f64, f64, f64), x: f64, y: f64| match corner {
            None => in_ellipse(bx, x, y),
            Some((ew, eh)) => {
                let g = 2.0 * (b.0 - bx.0);
                in_round_rect(bx, (ew + g).max(0.0), (eh + g).max(0.0), x, y)
            }
        };
        let grow = |bx: (f64, f64, f64, f64), d: f64| (bx.0 - d, bx.1 - d, bx.2 + d, bx.3 + d);
        if self.dc.brush.style != BS_NULL {
            fill_test(&mut self.mask, b, |x, y| shape(b, x, y));
            self.paint(self.dc.brush.color);
        }
        if self.dc.pen.style != PS_NULL {
            let w = self.pen_px().max(1.0);
            let outer = if w > 1.0 && self.dc.pen.style != PS_INSIDEFRAME {
                grow(b, w / 2.0)
            } else {
                b
            };
            let inner = grow(outer, -w);
            fill_test(&mut self.mask, outer, |x, y| {
                shape(outer, x, y) && !shape(inner, x, y)
            });
            self.paint(self.dc.pen.color);
        }
    }

    fn device_box(&self, l: i16, t: i16, r: i16, b: i16) -> (f64, f64, f64, f64) {
        let (x0, y0) = self.pt(l, t);
        let (x1, y1) = self.pt(r, b);
        (
            x0.min(x1) as f64,
            y0.min(y1) as f64,
            x0.max(x1) as f64,
            y0.max(y1) as f64,
        )
    }

    fn add_obj(&mut self, o: Obj) {
        match self.objs.iter().position(Option::is_none) {
            Some(k) => self.objs[k] = Some(o),
            None => self.objs.push(Some(o)),
        }
    }

    fn record(&mut self, index: usize, r: &WmfRecord) -> R<()> {
        let bad = |detail| WmfError::Malformed {
            index,
            function: r.function,
            detail,
        };
        let w = |i| r.word(i).ok_or(bad("parameters too short"));
        let int = |i| r.int(i).ok_or(bad("parameters too short"));
        let dw = |i| r.dword(i).ok_or(bad("parameters too short"));
        match r.function {
            0x0000 => {}
            // SetMapMode: always MM_ANISOTROPIC (8) in the samples; treated as such.
            0x0103 => {}
            0x020B => self.dc.worg = (f64::from(int(1)?), f64::from(int(0)?)),
            0x020C => self.dc.wext = (f64::from(int(1)?), f64::from(int(0)?)),
            0x0102 => self.dc.bk_opaque = w(0)? == 2,
            0x0201 => self.dc.bk_color = dw(0)?,
            0x0209 => self.dc.text_color = dw(0)?,
            0x0106 => self.dc.winding = w(0)? == 2,
            0x0104 => self.dc.rop2 = w(0)?,
            0x012E => self.dc.align = w(0)?,
            0x0626 => {} // Escape (MFCOMMENT in the samples)
            0x001E => {
                if self.stack.len() >= 1024 {
                    return Err(bad("SaveDC nested too deep"));
                }
                self.stack.push(self.dc.clone());
            }
            0x0127 => {
                let n = int(0)?;
                let keep = if n < 0 {
                    self.stack.len().checked_sub(usize::from(n.unsigned_abs()))
                } else {
                    usize::from(n.unsigned_abs()).checked_sub(1)
                };
                if let Some(k) = keep.filter(|&k| k < self.stack.len()) {
                    self.dc = self.stack[k].clone();
                    self.stack.truncate(k);
                }
            }
            0x02FA => {
                let p = Pen {
                    style: w(0)? & 0xFF,
                    width: int(1)?,
                    color: dw(3)?,
                };
                self.add_obj(Obj::Pen(p));
            }
            0x02FC => {
                let b = Brush {
                    style: w(0)?,
                    color: dw(1)?,
                };
                self.add_obj(Obj::Brush(b));
            }
            0x02FB => {
                let f = Font {
                    height: int(0)?,
                    weight: w(4)?,
                };
                self.add_obj(Obj::Font(f));
            }
            0x00F7 | 0x06FF => self.add_obj(Obj::Other),
            0x012D => match self.objs.get(usize::from(w(0)?)).cloned().flatten() {
                Some(Obj::Pen(p)) => self.dc.pen = p,
                Some(Obj::Brush(b)) => self.dc.brush = b,
                Some(Obj::Font(f)) => self.dc.font = f,
                _ => {}
            },
            0x01F0 => {
                if let Some(s) = self.objs.get_mut(usize::from(w(0)?)) {
                    *s = None;
                }
            }
            0x0324 | 0x0325 => {
                let n = usize::from(w(0)?);
                let mut pts = Vec::with_capacity(n.min(r.params.len() / 4));
                for k in 0..n {
                    pts.push(self.pt(int(1 + 2 * k)?, int(2 + 2 * k)?));
                }
                if self.degenerate() {
                    return Ok(());
                }
                if r.function == 0x0324 {
                    self.polygons(vec![pts]);
                } else {
                    self.stroke_rings(&[pts], false);
                }
            }
            0x0538 => {
                let n = usize::from(w(0)?);
                let mut counts = Vec::with_capacity(n.min(r.params.len() / 2));
                for k in 0..n {
                    counts.push(usize::from(w(1 + k)?));
                }
                let mut at = 1 + n;
                let mut rings = Vec::with_capacity(n.min(r.params.len() / 4));
                for c in counts {
                    let mut ring = Vec::with_capacity(c.min(r.params.len() / 4));
                    for _ in 0..c {
                        ring.push(self.pt(int(at)?, int(at + 1)?));
                        at += 2;
                    }
                    rings.push(ring);
                }
                if !self.degenerate() {
                    self.polygons(rings);
                }
            }
            0x0418 => {
                let (b, rr, t, l) = (int(0)?, int(1)?, int(2)?, int(3)?);
                if !self.degenerate() {
                    let bx = self.device_box(l, t, rr, b);
                    self.boxed(bx, None);
                }
            }
            0x061C => {
                let (eh, ew, b, rr, t, l) = (int(0)?, int(1)?, int(2)?, int(3)?, int(4)?, int(5)?);
                if !self.degenerate() {
                    let bx = self.device_box(l, t, rr, b);
                    let corner = (
                        (f64::from(ew) * self.scale_x()).round(),
                        (f64::from(eh) * self.scale_y()).round(),
                    );
                    self.boxed(bx, Some(corner));
                }
            }
            0x0521 => {
                let n = usize::from(w(0)?);
                let words = n.div_ceil(2);
                let bytes = r.params.get(2..2 + n).ok_or(bad("string too short"))?;
                let (y, x) = (int(1 + words)?, int(2 + words)?);
                if self.degenerate() {
                    return Ok(());
                }
                let (px, py) = self.pt(x, y);
                let run = text::TextRun {
                    bytes,
                    x: px as f64,
                    y: py as f64,
                    height: f64::from(self.dc.font.height) * self.scale_y(),
                    bold: self.dc.font.weight >= 600,
                    align: self.dc.align,
                };
                if let Some(cell) = text::draw(&mut self.mask, &run) {
                    if self.dc.bk_opaque {
                        let blank = Mask::new(self.mask.w, self.mask.h);
                        let glyphs = std::mem::replace(&mut self.mask, blank);
                        let cb = (
                            cell.0.round(),
                            cell.1.round(),
                            cell.2.round(),
                            cell.3.round(),
                        );
                        fill_test(&mut self.mask, cb, |_, _| true);
                        self.paint(self.dc.bk_color);
                        self.mask = glyphs;
                    }
                    self.paint(self.dc.text_color);
                }
            }
            f => return Err(WmfError::Unsupported { index, function: f }),
        }
        Ok(())
    }
}

/// Play `wmf` into `canvas` with the viewport mapped to `dest`. Drawing is clipped to the
/// canvas only (GDI does not clip to the viewport).
pub fn draw(canvas: &mut Canvas, wmf: &Wmf, dest: PixelRect) -> Result<(), WmfError> {
    let bb = wmf.bbox;
    let dc = Dc {
        pen: Pen {
            style: 0,
            width: 0,
            color: 0,
        },
        brush: Brush {
            style: 0,
            color: 0xFF_FFFF,
        },
        font: Font {
            height: 0,
            weight: 400,
        },
        text_color: 0,
        bk_color: 0xFF_FFFF,
        bk_opaque: true,
        winding: false,
        rop2: R2_COPYPEN,
        align: 0,
        worg: (f64::from(bb[0]), f64::from(bb[1])),
        wext: (
            f64::from(i32::from(bb[2]) - i32::from(bb[0])),
            f64::from(i32::from(bb[3]) - i32::from(bb[1])),
        ),
    };
    let (w, h) = (canvas.width as i32, canvas.height as i32);
    let mut p = Player {
        dest: (
            f64::from(dest.left),
            f64::from(dest.top),
            f64::from(dest.right),
            f64::from(dest.bottom),
        ),
        c: canvas,
        dc,
        stack: Vec::new(),
        objs: Vec::new(),
        mask: Mask::new(w, h),
    };
    for (i, r) in wmf.records.iter().enumerate() {
        p.record(i, r)?;
    }
    Ok(())
}

/// Map a logical rectangle to canvas pixels by character-format 5.3 (truncation toward
/// zero), with the canvas size as output size.
pub fn map_rect(l: i16, t: i16, r: i16, b: i16, logical: (i16, i16), out: (u32, u32)) -> PixelRect {
    let m = |v: i16, o: u32, lw: i16| -> i32 {
        let lw = i64::from(lw).max(1);
        (i64::from(v) * i64::from(o) / lw) as i32
    };
    PixelRect {
        left: m(l, out.0, logical.0),
        top: m(t, out.1, logical.1),
        right: m(r, out.0, logical.0),
        bottom: m(b, out.1, logical.1),
    }
}

/// Draw pose `pose` of a vector actor at the canvas size: a composite of WMF children
/// (placed by [`map_rect`]) or a single WMF stretched to the whole canvas. Bitmap children
/// and nested composites are rejected (none occur in vector actors).
pub fn draw_pose(canvas: &mut Canvas, act: &ActFile, pose: u16) -> Result<(), WmfError> {
    let perr = |detail: String| WmfError::Pose { pose, detail };
    let logical = (act.header.logical_w, act.header.logical_h);
    let out = (canvas.width, canvas.height);
    match act.payloads.get(usize::from(pose)) {
        Some(Payload::Wmf(w)) => draw(
            canvas,
            w,
            map_rect(0, 0, logical.0, logical.1, logical, out),
        ),
        Some(Payload::Composite(children)) => {
            for c in children {
                match act.payloads.get(usize::from(c.id)) {
                    Some(Payload::Wmf(w)) => draw(
                        canvas,
                        w,
                        map_rect(c.left, c.top, c.right, c.bottom, logical, out),
                    )?,
                    None if c.id == agent_formats::act::NONE => {}
                    _ => return Err(perr(format!("child {} is not a WMF image", c.id))),
                }
            }
            Ok(())
        }
        _ => Err(perr("not a WMF image or composite".into())),
    }
}

/// Distinct colours a set of metafiles can paint (pens, brushes, text and background
/// colours, without 0xF5FAF5), in first-use order. For v1.0 actors, which have no colour
/// table.
pub fn collect_colors(act: &ActFile) -> Vec<[u8; 3]> {
    let mut out: Vec<[u8; 3]> = Vec::new();
    let mut add = |cr: Option<u32>| {
        if let Some(cr) = cr.filter(|c| c & 0xFF_FFFF != MAGIC_TRANSPARENT) {
            let c = rgb(cr);
            if !out.contains(&c) {
                out.push(c);
            }
        }
    };
    for p in &act.payloads {
        if let Payload::Wmf(w) = p {
            for r in &w.records {
                match r.function {
                    0x02FA if r.word(0).map(|s| s & 0xFF) != Some(PS_NULL) => add(r.dword(3)),
                    0x02FC if r.word(0) != Some(BS_NULL) => add(r.dword(1)),
                    0x0209 | 0x0201 => add(r.dword(0)),
                    _ => {}
                }
            }
        }
    }
    out
}
