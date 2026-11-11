//! Writer for synthetic ACT files (act-format 4): bitmap and WMF actors, scripts with
//! frame, branch and sound commands, pose transitions, strings. Owner: act teammate.

use crate::common::{Out, lz_compress, wav_pcm};

/// One 8-bit image, pixels top-down, `width * height` palette indices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Vec<u8>,
}

impl Image {
    /// Deterministic test pattern with runs (for RLE) and a transparent border of `key`.
    pub fn pattern(width: u32, height: u32, seed: u32, key: u8) -> Image {
        let mut pixels = Vec::with_capacity((width * height) as usize);
        for y in 0..height {
            for x in 0..width {
                let v = if x == 0 || y == 0 || x + 1 == width || y + 1 == height {
                    key
                } else if (x / 3 + y / 2 + seed).is_multiple_of(5) {
                    (x * 7 + y * 13 + seed) as u8
                } else {
                    (40 + (x / 4 + seed) % 8) as u8
                };
                pixels.push(v);
            }
        }
        Image {
            width,
            height,
            pixels,
        }
    }
}

/// How a bitmap image record is coded inside its blob.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Coding {
    Rle,
    Raw,
}

/// Payload source. A `Bitmaps` entry is one DCIK (one image) or MNAK (several) blob and
/// occupies one payload id per image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PayloadSpec {
    /// Children: (id, left, top, right, bottom) in logical units.
    Composite(Vec<(u16, i16, i16, i16, i16)>),
    /// A complete placeable WMF (see [`WmfBuilder`]).
    Wmf(Vec<u8>),
    Bitmaps(Vec<Image>, Coding),
}

/// Script commands (kind, a, b).
pub type Script = Vec<(u16, u16, u16)>;

/// A synthetic actor. Field meanings follow act-format 4.2 to 4.12.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Actor {
    pub major: u16,
    pub minor: u16,
    pub name: Vec<u8>,
    pub kind: u16,
    pub rest_pose: u16,
    pub idle_anim: u16,
    pub logical: (i16, i16),
    pub default_px: (i16, i16),
    pub unk: [u16; 4],
    pub rect: [i16; 4],
    pub nohit_image: u16,
    pub stretch: (u16, u16),
    pub color_table: Vec<[u8; 3]>,
    /// (from_pose, to_pose, anim_id).
    pub transitions: Vec<(u16, u16, u16)>,
    pub payloads: Vec<PayloadSpec>,
    /// Complete RIFF WAVE files.
    pub sounds: Vec<Vec<u8>>,
    /// (group id, variants); each variant is a list of (kind, a, b) commands.
    pub animations: Vec<(u16, Vec<Script>)>,
    /// (group id, raw variants).
    pub strings: Vec<(u16, Vec<Vec<u8>>)>,
}

/// UTF-16LE bytes of `s` (v2 string groups).
pub fn utf16(s: &str) -> Vec<u8> {
    s.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

fn rle_encode(px: &[u8]) -> Vec<u8> {
    let mut o = Vec::new();
    let mut i = 0;
    while i < px.len() {
        let mut run = 1;
        while i + run < px.len() && px[i + run] == px[i] && run < 127 {
            run += 1;
        }
        if run >= 3 {
            o.push(run as u8);
            o.push(px[i]);
            i += run;
            continue;
        }
        let start = i;
        while i < px.len() && i - start < 127 {
            if i + 2 < px.len() && px[i] == px[i + 1] && px[i] == px[i + 2] {
                break;
            }
            i += 1;
        }
        o.push(0x80 | (i - start) as u8);
        o.extend_from_slice(&px[start..i]);
    }
    o.push(0);
    o
}

fn image_record(im: &Image, coding: Coding) -> Vec<u8> {
    let w = im.width as usize;
    let stride = (w + 3) & !3;
    let mut dib = Vec::with_capacity(stride * im.height as usize);
    for y in (0..im.height as usize).rev() {
        dib.extend_from_slice(&im.pixels[y * w..y * w + w]);
        dib.resize(dib.len() + stride - w, 0);
    }
    let mut o = Out::default();
    o.u32(im.width);
    o.u32(im.height);
    match coding {
        Coding::Rle => {
            o.u32(1);
            o.bytes(&rle_encode(&dib));
        }
        Coding::Raw => {
            o.u32(2);
            o.bytes(&dib);
        }
    }
    o.0
}

fn bitmap_blob(images: &[Image], coding: Coding) -> Vec<u8> {
    let recs: Vec<Vec<u8>> = images.iter().map(|im| image_record(im, coding)).collect();
    let plain: Vec<u8> = recs.concat();
    let mut o = Out::default();
    if images.len() == 1 {
        o.bytes(b"DCIK");
        o.u32(plain.len() as u32);
    } else {
        o.bytes(b"MNAK");
        o.u32(plain.len() as u32);
        o.u32(images.len() as u32);
        let mut start = 0;
        for r in &recs[..recs.len() - 1] {
            start += r.len();
            o.u32(start as u32);
        }
    }
    o.bytes(&lz_compress(&plain, true));
    o.0
}

/// Group index: entries are written in reverse group order, so `first` is not in group
/// order (as in many real files, act-format 4.10).
fn groups(entries: &[(u16, Vec<Vec<u8>>)]) -> (Vec<u8>, Vec<u8>) {
    let mut data = Vec::new();
    let mut ents: Vec<(u32, u16)> = Vec::new();
    let mut firsts = vec![0usize; entries.len()];
    for (g, (_, vs)) in entries.iter().enumerate().rev() {
        firsts[g] = ents.len();
        for v in vs {
            ents.push((data.len() as u32, v.len() as u16));
            data.extend_from_slice(v);
        }
    }
    let mut idx = Out::default();
    idx.u32(entries.len() as u32);
    for (g, (id, vs)) in entries.iter().enumerate() {
        idx.u16(*id);
        idx.u16(vs.len() as u16);
        idx.u16(firsts[g] as u16);
    }
    for (off, sz) in ents {
        idx.u32(off);
        idx.u16(sz);
    }
    (data, idx.0)
}

fn script(cmds: &[(u16, u16, u16)]) -> Vec<u8> {
    let mut o = Out::default();
    o.u16(0x0100);
    o.u16(cmds.len() as u16);
    for &(k, a, b) in cmds {
        o.u16(k);
        o.u16(a);
        o.u16(b);
    }
    o.0
}

impl Actor {
    /// Serialize to `.ACT` bytes.
    pub fn to_bytes(&self) -> Vec<u8> {
        // optional block
        let mut opt = Out::default();
        if self.major == 2 && self.minor != 0 {
            opt.u32(self.color_table.len() as u32);
            for c in &self.color_table {
                opt.bytes(&[c[0], c[1], c[2], 0]);
            }
            for _ in &self.color_table {
                opt.u16(0xFFFF);
            }
        }
        if self.major == 2 && self.minor >= 2 {
            opt.u32(self.transitions.len() as u32);
            opt.u16(self.rest_pose);
            for &(f, t, a) in &self.transitions {
                opt.u16(f);
                opt.u16(t);
                opt.u16(a);
            }
        }
        // payloads
        let mut pdata = Vec::new();
        let mut pidx = Out::default();
        for p in &self.payloads {
            let off = pdata.len() as u32;
            match p {
                PayloadSpec::Composite(cs) => {
                    let mut o = Out::default();
                    o.u16(0x14);
                    o.u16(cs.len() as u16);
                    for &(id, l, t, r, b) in cs {
                        o.u16(id);
                        o.i16(l);
                        o.i16(t);
                        o.i16(r);
                        o.i16(b);
                    }
                    pdata.extend_from_slice(&o.0);
                    pidx.u32(off);
                }
                PayloadSpec::Wmf(b) => {
                    pdata.extend_from_slice(b);
                    pidx.u32(off);
                }
                PayloadSpec::Bitmaps(ims, coding) => {
                    pdata.extend_from_slice(&bitmap_blob(ims, *coding));
                    for k in 0..ims.len() as u32 {
                        pidx.u32(off | (k << 30));
                    }
                }
            }
        }
        pidx.u32(pdata.len() as u32);
        // sounds
        let mut sdata = Vec::new();
        let mut sidx = Out::default();
        for s in &self.sounds {
            sidx.u32(sdata.len() as u32);
            sdata.extend_from_slice(s);
        }
        sidx.u32(sdata.len() as u32);
        let anims: Vec<(u16, Vec<Vec<u8>>)> = self
            .animations
            .iter()
            .map(|(id, vs)| (*id, vs.iter().map(|c| script(c)).collect()))
            .collect();
        let (adata, aidx) = groups(&anims);
        let (tdata, tidx) = groups(&self.strings);

        let blocks = [
            &opt.0[..],
            &pdata,
            &pidx.0,
            &sdata,
            &sidx.0,
            &adata,
            &aidx,
            &tdata,
            &tidx,
        ];
        let mut offs = Vec::new();
        let mut at = 0x46 + opt.len();
        for b in &blocks[1..] {
            offs.push(at as u32);
            at += b.len();
        }
        let mut r = Out::default();
        r.u16(0x46);
        r.u16(self.kind);
        r.u16(self.rest_pose);
        r.u16(self.idle_anim);
        r.i16(self.logical.0);
        r.i16(self.logical.1);
        r.i16(self.default_px.0);
        r.i16(self.default_px.1);
        for u in self.unk {
            r.u16(u);
        }
        for v in self.rect {
            r.i16(v);
        }
        r.u16(self.nohit_image);
        r.u16(self.stretch.0);
        r.u16(self.stretch.1);
        for o in &offs {
            r.u32(*o);
        }
        for b in blocks {
            r.bytes(b);
        }

        let mut f = Out::default();
        f.bytes(b"LP");
        f.u16(self.major);
        f.u16(self.minor);
        f.u16(1);
        f.u16(self.name.len() as u16 + 1);
        let start = 10 + 8 + self.name.len() + 1;
        f.u32(start as u32);
        f.u32((start + r.len()) as u32);
        f.bytes(&self.name);
        f.u8(0);
        f.bytes(&r.0);
        f.0
    }
}

/// A bitmap actor (v2.2, kind 7) in the style of the Office bitmap actors: a two-image
/// MNAK pack, a single DCIK image, a raw-coded image, a composite pose, two animation
/// groups with variants using every command kind, transitions, sounds, strings.
pub fn bitmap_actor() -> Actor {
    let key = 10;
    // payloads: 0,1 MNAK; 2 DCIK; 3 raw DCIK; 4 composite; 5 composite (transition target)
    let payloads = vec![
        PayloadSpec::Bitmaps(
            vec![Image::pattern(12, 9, 1, key), Image::pattern(13, 9, 2, key)],
            Coding::Rle,
        ),
        PayloadSpec::Bitmaps(vec![Image::pattern(10, 7, 3, key)], Coding::Rle),
        PayloadSpec::Bitmaps(vec![Image::pattern(6, 5, 4, key)], Coding::Raw),
        PayloadSpec::Composite(vec![(0, 0, 0, 180, 135), (2, 30, 15, 105, 75)]),
        PayloadSpec::Composite(vec![(1, -15, -15, 195, 150)]),
    ];
    Actor {
        major: 2,
        minor: 2,
        name: b"Fixture Bitmap".to_vec(),
        kind: 7,
        rest_pose: 4,
        idle_anim: 1,
        logical: (180, 135),
        default_px: (12, 9),
        unk: [2083, 2083, 3, 1],
        rect: [10, 20, 170, 120],
        nohit_image: 0xFFFF,
        stretch: (0xFFFF, 0xFFFF),
        color_table: Vec::new(),
        transitions: vec![(4, 5, 10000), (5, 4, 10001)],
        payloads,
        sounds: vec![wav_pcm(11025, 1, 8, 100, 1), wav_pcm(22050, 1, 16, 51, 2)],
        animations: vec![
            (
                1,
                vec![
                    vec![
                        (0, 4, 0),
                        (0, 0, 100),
                        (1, 1, 16384),
                        (1, 4, 0),
                        (0, 3, 50),
                        (0, 4, 0),
                    ],
                    vec![(0, 4, 0), (2, 0, 0x0080), (0, 2, 120), (4, 1, 2), (0, 4, 0)],
                ],
            ),
            (
                2,
                vec![vec![
                    (0, 4, 0),
                    (2, 1, 0x0080),
                    (0, 0xFFFF, 30),
                    (3, 0, 1),
                    (2, 0xFFFF, 0x0100),
                    (0, 3, 40),
                    (1, 0, 32768),
                ]],
            ),
            (10000, vec![vec![(0, 4, 0), (0, 5, 60), (0, 5, 0)]]),
            (10001, vec![vec![(0, 5, 0), (0, 4, 60), (0, 4, 0)]]),
        ],
        strings: vec![
            (1, vec![utf16("Fixture Bitmap")]),
            (8, vec![utf16("A synthetic actor.")]),
            (40, vec![utf16("Hello"), utf16("Hi there")]),
        ],
    }
}

/// A vector actor (v2.2, kind 7): WMF parts 0..n-1, composite poses from n on.
pub fn wmf_actor() -> Actor {
    let red = [0xC0, 0x20, 0x20];
    let blue = [0x20, 0x40, 0xC0];
    let body = WmfBuilder::new([0, 0, 1000, 1000])
        .window([0, 0], [1000, 1000])
        .pen(0, 20, blue)
        .brush(0, red)
        .select(0)
        .select(1)
        .poly_fill_mode(1)
        .polygon(&[(100, 100), (900, 150), (500, 900)])
        .ellipse([300, 300, 700, 700])
        .build();
    let eye = WmfBuilder::new([0, 0, 100, 100])
        .window([0, 0], [100, 100])
        .pen(5, 0, [0, 0, 0])
        .brush(0, blue)
        .select(0)
        .select(1)
        .rect_round([0, 0, 100, 100], [30, 30])
        .build();
    let payloads = vec![
        PayloadSpec::Wmf(body),
        PayloadSpec::Wmf(eye),
        PayloadSpec::Composite(vec![(0, 0, 0, 2880, 2160), (1, 1200, 600, 1500, 900)]),
        PayloadSpec::Composite(vec![(0, -180, -200, 3000, 2400)]),
    ];
    Actor {
        name: b"Fixture Vector".to_vec(),
        rest_pose: 2,
        logical: (2880, 2160),
        default_px: (192, 144),
        rect: [442, 452, 2417, 1825],
        color_table: vec![red, blue, [0, 0, 0]],
        transitions: Vec::new(),
        payloads,
        sounds: vec![wav_pcm(11025, 1, 8, 33, 5)],
        animations: vec![
            (
                1,
                vec![vec![
                    (0, 2, 0),
                    (0, 3, 200),
                    (1, 0, 0),
                    (1, 1, 32768),
                    (0, 2, 0),
                ]],
            ),
            (
                32,
                vec![vec![(0, 0xFFFF, 0), (2, 0, 0x0080), (0, 2, 100), (0, 2, 0)]],
            ),
        ],
        ..bitmap_actor()
    }
}

/// A Microsoft Bob style actor (v1.0, kind 1): no optional block, ANSI strings.
pub fn bob_actor() -> Actor {
    let mut a = wmf_actor();
    a.major = 1;
    a.minor = 0;
    a.kind = 1;
    a.nohit_image = 0;
    a.color_table.clear();
    a.name = b"Fixture Bob".to_vec();
    a.strings = vec![
        (1, vec![b"Fixture Bob".to_vec()]),
        (5, vec![b"Caf\xe9".to_vec()]),
    ];
    for (_, vs) in &mut a.animations {
        for v in vs {
            for c in v {
                if c.0 == 2 {
                    c.2 = 0x0100;
                }
            }
        }
    }
    a
}

/// Builder for placeable WMF payloads (act-format 4.6). Coordinates are logical units.
#[derive(Debug, Clone)]
pub struct WmfBuilder {
    bbox: [i16; 4],
    recs: Vec<u8>,
    max_rec: u32,
    objects: u16,
}

fn colorref(c: [u8; 3]) -> u32 {
    u32::from(c[0]) | (u32::from(c[1]) << 8) | (u32::from(c[2]) << 16)
}

impl WmfBuilder {
    /// Start a metafile with placeable bbox (left, top, right, bottom) and SetMapMode(8).
    pub fn new(bbox: [i16; 4]) -> Self {
        WmfBuilder {
            bbox,
            recs: Vec::new(),
            max_rec: 0,
            objects: 0,
        }
        .record(0x0103, &[8])
    }

    /// Append a raw record with parameter words.
    pub fn record(mut self, function: u16, params: &[u16]) -> Self {
        let size = 3 + params.len() as u32;
        self.recs.extend_from_slice(&size.to_le_bytes());
        self.recs.extend_from_slice(&function.to_le_bytes());
        for p in params {
            self.recs.extend_from_slice(&p.to_le_bytes());
        }
        self.max_rec = self.max_rec.max(size);
        self
    }

    /// SetWindowOrg + SetWindowExt.
    pub fn window(self, org: [i16; 2], ext: [i16; 2]) -> Self {
        self.record(0x020B, &[org[1] as u16, org[0] as u16])
            .record(0x020C, &[ext[1] as u16, ext[0] as u16])
    }

    /// CreatePenIndirect(style, width, colour); takes the lowest free object slot.
    pub fn pen(mut self, style: u16, width: i16, c: [u8; 3]) -> Self {
        self.objects += 1;
        let cr = colorref(c);
        self.record(
            0x02FA,
            &[style, width as u16, 0, cr as u16, (cr >> 16) as u16],
        )
    }

    /// CreateBrushIndirect(style, colour, hatch 0).
    pub fn brush(mut self, style: u16, c: [u8; 3]) -> Self {
        self.objects += 1;
        let cr = colorref(c);
        self.record(0x02FC, &[style, cr as u16, (cr >> 16) as u16, 0])
    }

    /// CreateFontIndirect with height, weight, face name.
    pub fn font(mut self, height: i16, weight: u16, face: &str) -> Self {
        self.objects += 1;
        let mut name = face.as_bytes().to_vec();
        name.resize(32, 0);
        let mut words = vec![height as u16, 0, 0, 0, weight, 0, 0, 0, 0];
        words.extend(name.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])));
        self.record(0x02FB, &words)
    }

    pub fn select(self, slot: u16) -> Self {
        self.record(0x012D, &[slot])
    }

    pub fn delete(self, slot: u16) -> Self {
        self.record(0x01F0, &[slot])
    }

    pub fn poly_fill_mode(self, mode: u16) -> Self {
        self.record(0x0106, &[mode])
    }

    pub fn rop2(self, rop: u16) -> Self {
        self.record(0x0104, &[rop])
    }

    pub fn bk_mode(self, mode: u16) -> Self {
        self.record(0x0102, &[mode])
    }

    pub fn text_color(self, c: [u8; 3]) -> Self {
        let cr = colorref(c);
        self.record(0x0209, &[cr as u16, (cr >> 16) as u16])
    }

    pub fn bk_color(self, c: [u8; 3]) -> Self {
        let cr = colorref(c);
        self.record(0x0201, &[cr as u16, (cr >> 16) as u16])
    }

    pub fn text_align(self, align: u16) -> Self {
        self.record(0x012E, &[align])
    }

    pub fn save_dc(self) -> Self {
        self.record(0x001E, &[])
    }

    pub fn restore_dc(self, n: i16) -> Self {
        self.record(0x0127, &[n as u16])
    }

    /// MFCOMMENT escape (function 15).
    pub fn comment(self, data: &[u8]) -> Self {
        let mut w = vec![15u16, data.len() as u16];
        let mut d = data.to_vec();
        if d.len() % 2 == 1 {
            d.push(0);
        }
        w.extend(d.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])));
        self.record(0x0626, &w)
    }

    fn points(pts: &[(i16, i16)]) -> Vec<u16> {
        pts.iter()
            .flat_map(|&(x, y)| [x as u16, y as u16])
            .collect()
    }

    pub fn polygon(self, pts: &[(i16, i16)]) -> Self {
        let mut w = vec![pts.len() as u16];
        w.extend(Self::points(pts));
        self.record(0x0324, &w)
    }

    pub fn polyline(self, pts: &[(i16, i16)]) -> Self {
        let mut w = vec![pts.len() as u16];
        w.extend(Self::points(pts));
        self.record(0x0325, &w)
    }

    pub fn poly_polygon(self, polys: &[&[(i16, i16)]]) -> Self {
        let mut w = vec![polys.len() as u16];
        w.extend(polys.iter().map(|p| p.len() as u16));
        for p in polys {
            w.extend(Self::points(p));
        }
        self.record(0x0538, &w)
    }

    /// Ellipse(left, top, right, bottom); stored bottom, right, top, left.
    pub fn ellipse(self, r: [i16; 4]) -> Self {
        self.record(
            0x0418,
            &[r[3] as u16, r[2] as u16, r[1] as u16, r[0] as u16],
        )
    }

    /// RoundRect(rect, corner (width, height)).
    pub fn rect_round(self, r: [i16; 4], corner: [i16; 2]) -> Self {
        self.record(
            0x061C,
            &[
                corner[1] as u16,
                corner[0] as u16,
                r[3] as u16,
                r[2] as u16,
                r[1] as u16,
                r[0] as u16,
            ],
        )
    }

    /// TextOut(x, y, text).
    pub fn text_out(self, x: i16, y: i16, text: &[u8]) -> Self {
        let mut w = vec![text.len() as u16];
        let mut t = text.to_vec();
        if t.len() % 2 == 1 {
            t.push(0);
        }
        w.extend(t.chunks(2).map(|c| u16::from_le_bytes([c[0], c[1]])));
        w.push(y as u16);
        w.push(x as u16);
        self.record(0x0521, &w)
    }

    /// Finish with EOF; returns the placeable header plus metafile.
    pub fn build(self) -> Vec<u8> {
        let b = self.record(0x0000, &[]);
        let mut o = Out::default();
        let mut ph = Out::default();
        ph.u32(0x9AC6_CDD7);
        ph.u16(0);
        for v in b.bbox {
            ph.i16(v);
        }
        ph.u16(1440);
        ph.u32(0);
        let chk =
            ph.0.chunks(2)
                .fold(0u16, |a, c| a ^ u16::from_le_bytes([c[0], c[1]]));
        ph.u16(chk);
        o.bytes(&ph.0);
        o.u16(1);
        o.u16(9);
        o.u16(0x0300);
        o.u32(9 + b.recs.len() as u32 / 2);
        o.u16(b.objects);
        o.u32(b.max_rec);
        o.u16(0);
        o.bytes(&b.recs);
        o.0
    }
}
