//! Shared conversion back end (character-format 10 "Common"): picture pool with deduplication,
//! the D4 sheet layout, sound files, and the final package.

use std::collections::HashMap;

use bentwire_character::write::{encode_flac, encode_png_indexed, encode_png_rgba};
use bentwire_character::*;

use crate::Error;

/// Sheet id used by the converter (one sheet per character, D4).
pub const SHEET: &str = "a";
/// Widest sheet the D4 layout produces.
pub const MAX_SHEET_WIDTH: u32 = 4096;

/// A decoded source picture, top-down: one palette index per pixel, or RGBA.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Pic {
    pub w: u32,
    pub h: u32,
    pub rgba: bool,
    pub px: Vec<u8>,
}

/// Picture pool; identical pictures are stored once.
#[derive(Default)]
pub struct Pool {
    pub pics: Vec<Pic>,
    index: HashMap<Pic, u32>,
}

impl Pool {
    pub fn add(&mut self, p: Pic) -> u32 {
        if let Some(&i) = self.index.get(&p) {
            return i;
        }
        let i = self.pics.len() as u32;
        self.index.insert(p.clone(), i);
        self.pics.push(p);
        i
    }
}

/// A layer referring to pool picture `i` (sheet position assigned later).
pub fn pool_layer(i: u32, x: i32, y: i32, size: Option<[u32; 2]>) -> Layer {
    Layer {
        image: ImageRef {
            sheet: SHEET.into(),
            index: i,
        },
        x,
        y,
        size,
    }
}

/// Everything a source converter produces; `manifest.sheets` and `manifest.sounds` are filled
/// in by [`Model::finish`]. Layers refer to pool indices.
pub struct Model {
    pub manifest: Manifest,
    pub pool: Pool,
    /// PLTE for indexed pictures (source order) and the transparent index.
    pub palette: Option<Vec<[u8; 3]>>,
    pub transparent: Option<u8>,
    /// Sound id to PCM, in id order.
    pub sounds: Vec<(String, Pcm)>,
    pub icon: Option<Rgba>,
    /// Pool pictures to drop when no layer uses them (ACT: unused payloads are dropped).
    pub drop_unused: bool,
    /// Optional 2x pictures by pool index (vector actors, D2). When non-empty, every picture
    /// of the sheet needs one; they are written as `hidpi = { 2 = ... }` with the same grid.
    pub hidpi2: HashMap<u32, Pic>,
}

fn for_each_layer(m: &mut Manifest, f: &mut dyn FnMut(&mut Layer)) {
    for a in &mut m.animations {
        for fr in &mut a.frames {
            fr.layers.iter_mut().for_each(&mut *f);
            for mo in &mut fr.mouths {
                if let Some(b) = &mut mo.base {
                    b.iter_mut().for_each(&mut *f);
                }
                f(&mut mo.layer);
            }
        }
    }
    for p in m.poses.values_mut() {
        p.iter_mut().for_each(&mut *f);
    }
}

/// Picture order of D4: first use, with the animations of state Showing (or Appear) first, then
/// poses, then (unless dropped) unused pictures.
fn use_order(m: &Manifest, n: usize, drop_unused: bool) -> Vec<u32> {
    let first: Vec<String> = m
        .states
        .iter()
        .filter(|(s, _)| s.eq_ignore_ascii_case("showing") || s.eq_ignore_ascii_case("appear"))
        .flat_map(|(_, l)| l.iter().cloned())
        .collect();
    let mut anims: Vec<&Animation> = m.animations.iter().collect();
    anims.sort_by_key(|a| !first.contains(&a.name)); // stable
    let mut seen = vec![false; n];
    let mut out = Vec::new();
    let mut add = |ls: &[Layer]| {
        for l in ls {
            let i = l.image.index as usize;
            if !seen[i] {
                seen[i] = true;
                out.push(i as u32);
            }
        }
    };
    for a in anims {
        for f in &a.frames {
            add(&f.layers);
            if let Some(p) = f.pose
                && let Some(ls) = m.poses.get(&p)
            {
                add(ls);
            }
            for mo in &f.mouths {
                add(mo.base.as_deref().unwrap_or(&[]));
                add(std::slice::from_ref(&mo.layer));
            }
        }
    }
    for ls in m.poses.values() {
        add(ls);
    }
    if !drop_unused {
        out.extend((0..n as u32).filter(|&i| !seen[i as usize]));
    }
    out
}

impl Model {
    /// Lay out the sheet, encode PNGs and FLACs, and return the package.
    pub fn finish(mut self, header: Vec<String>) -> Result<PackageData, Error> {
        // Indices beyond a short palette are drawn transparent by the source runtimes'
        // colour-key compositing; keep them so in the indexed PNG (A-CONV-6).
        if let (Some(pal), Some(t)) = (&self.palette, self.transparent) {
            let n = pal.len();
            if t as usize >= n {
                return Err(Error::Source(format!(
                    "transparent index {t} beyond the {n} palette entries"
                )));
            }
            if n < 256 {
                for p in self.pool.pics.iter_mut().filter(|p| !p.rgba) {
                    for v in p.px.iter_mut().filter(|v| **v as usize >= n) {
                        *v = t;
                    }
                }
            }
        }
        let order = use_order(&self.manifest, self.pool.pics.len(), self.drop_unused);
        let mut pos = vec![u32::MAX; self.pool.pics.len()];
        for (k, &i) in order.iter().enumerate() {
            pos[i as usize] = k as u32;
        }
        for_each_layer(&mut self.manifest, &mut |l| {
            l.image.index = pos[l.image.index as usize];
        });
        let mut files = Vec::new();
        if !order.is_empty() {
            let pics: Vec<&Pic> = order.iter().map(|&i| &self.pool.pics[i as usize]).collect();
            let rgba = pics[0].rgba;
            if pics.iter().any(|p| p.rgba != rgba) {
                return Err(Error::Source("mixed indexed and RGBA pictures".into()));
            }
            let cw = pics.iter().map(|p| p.w).max().unwrap();
            let ch = pics.iter().map(|p| p.h).max().unwrap();
            let cols = (MAX_SHEET_WIDTH / cw).max(1).min(pics.len() as u32);
            let rows = (pics.len() as u32).div_ceil(cols);
            let (sw, sh) = (cw * cols, ch * rows);
            let bpp = if rgba { 4 } else { 1 };
            let fill: Vec<u8> = if rgba {
                vec![0; 4]
            } else {
                vec![self.transparent.unwrap_or(0)]
            };
            let encode = |pics: &[&Pic], k: u32| -> Result<Vec<u8>, Error> {
                let (cw, ch, sw, sh) = (cw * k, ch * k, sw * k, sh * k);
                let mut buf = fill.repeat(sw as usize * sh as usize);
                for (n, p) in pics.iter().enumerate() {
                    let (ox, oy) = ((n as u32 % cols) * cw, (n as u32 / cols) * ch);
                    for y in 0..p.h {
                        let d = ((oy + y) as usize * sw as usize + ox as usize) * bpp as usize;
                        let s = (y * p.w * bpp) as usize;
                        let len = (p.w * bpp) as usize;
                        buf[d..d + len].copy_from_slice(&p.px[s..s + len]);
                    }
                }
                if rgba {
                    return Ok(encode_png_rgba(sw, sh, &buf)?);
                }
                let plte = self
                    .palette
                    .as_ref()
                    .ok_or_else(|| Error::Source("indexed pictures without a palette".into()))?;
                let trns: Option<Vec<u8>> = self.transparent.map(|t| {
                    let mut v = vec![255u8; t as usize];
                    v.push(0);
                    v
                });
                Ok(encode_png_indexed(sw, sh, &buf, plte, trns.as_deref())?)
            };
            let png = encode(&pics, 1)?;
            let mut hidpi = std::collections::BTreeMap::new();
            if !self.hidpi2.is_empty() {
                let two: Vec<&Pic> = order
                    .iter()
                    .map(|i| {
                        self.hidpi2
                            .get(i)
                            .filter(|q| q.rgba == rgba)
                            .ok_or_else(|| Error::Source("2x picture missing".into()))
                    })
                    .collect::<Result<_, _>>()?;
                for (a, b) in pics.iter().zip(&two) {
                    if (b.w, b.h) != (a.w * 2, a.h * 2) {
                        return Err(Error::Source("2x picture is not twice the 1x size".into()));
                    }
                }
                let f2 = format!("sheets/{SHEET}@2x.png");
                files.push((f2.clone(), encode(&two, 2)?));
                hidpi.insert(2, f2);
            }
            let sizes = pics
                .iter()
                .any(|p| (p.w, p.h) != (cw, ch))
                .then(|| pics.iter().map(|p| [p.w, p.h]).collect());
            let file = format!("sheets/{SHEET}.png");
            self.manifest.sheets = vec![Sheet {
                id: SHEET.into(),
                file: file.clone(),
                cell: [cw, ch],
                count: pics.len() as u32,
                sizes,
                scale: None,
                hidpi,
            }];
            files.push((file, png));
        }
        self.manifest.sounds.clear();
        for (id, pcm) in &self.sounds {
            let f = format!("sounds/{id}.flac");
            files.push((f.clone(), encode_flac(pcm)?));
            self.manifest.sounds.push((id.clone(), f));
        }
        if let Some(icon) = &self.icon {
            files.push((
                "icon.png".into(),
                encode_png_rgba(icon.width, icon.height, &icon.data)?,
            ));
            self.manifest.icon = Some(IconRef {
                file: "icon.png".into(),
            });
        }
        let errs = validate(&self.manifest);
        if !errs.is_empty() {
            return Err(Error::Invalid(errs));
        }
        Ok(PackageData {
            manifest: self.manifest,
            header,
            files,
        })
    }
}

/// An empty agent or office97 manifest to fill.
pub fn empty_manifest(family: Family, canvas: [u32; 2]) -> Manifest {
    Manifest {
        format: 1,
        family,
        character: CharacterSection {
            canvas,
            ..Default::default()
        },
        balloon: None,
        voice: None,
        icon: None,
        sheets: Vec::new(),
        sounds: Vec::new(),
        states: Vec::new(),
        poses: Default::default(),
        office97: None,
        animations: Vec::new(),
        source: None,
        unknown_keys: Vec::new(),
    }
}

/// Windows LANGID to BCP 47 ([MS-LCID]; the prototype's `common.lang_tag`): the table entry, else
/// for a primary-only id the language of the first entry with that primary language, else a
/// private-use tag.
pub fn lang_tag(langid: u16) -> String {
    use crate::lcid_table::LCIDS;
    if let Some((_, n)) = LCIDS.iter().find(|(k, _)| *k == langid) {
        return (*n).to_owned();
    }
    if langid < 0x400
        && let Some((_, n)) = LCIDS.iter().find(|(k, _)| k & 0x3FF == langid)
    {
        return n.split('-').next().unwrap_or(n).to_owned();
    }
    format!("und-x-lcid-{langid:04x}")
}

/// `#RRGGBB` from a COLORREF 0x00BBGGRR.
pub fn colorref(v: u32) -> String {
    format!(
        "#{:02X}{:02X}{:02X}",
        v & 0xFF,
        (v >> 8) & 0xFF,
        (v >> 16) & 0xFF
    )
}

/// Canonical spelling of the Agent state names (the prototype's `norm_states`); others unchanged.
pub fn canonical_state(name: &str) -> String {
    AGENT_STATES
        .iter()
        .find(|s| s.eq_ignore_ascii_case(name))
        .map_or_else(|| name.to_owned(), |s| (*s).to_owned())
}

/// (width, height, palette, top-down indices).
pub type Dib = (u32, u32, Vec<[u8; 3]>, Vec<u8>);

/// Decode a packed DIB (BITMAPINFOHEADER, colour table of `1 << bpp`, bits) to
/// (width, height, palette, top-down indices). 1, 4 and 8 bpp.
pub fn dib_decode(b: &[u8]) -> Result<Dib, Error> {
    let bad = || Error::Source("bad DIB".into());
    if b.len() < 40 {
        return Err(bad());
    }
    let w = i32::from_le_bytes([b[4], b[5], b[6], b[7]]);
    let h = i32::from_le_bytes([b[8], b[9], b[10], b[11]]);
    let bpp = u16::from_le_bytes([b[14], b[15]]) as usize;
    if w <= 0 || h <= 0 || ![1, 4, 8].contains(&bpp) {
        return Err(bad());
    }
    let (w, h) = (w as usize, h as usize);
    let ncol = 1usize << bpp;
    let pal_end = 40 + 4 * ncol;
    let pal: Vec<[u8; 3]> = b
        .get(40..pal_end)
        .ok_or_else(bad)?
        .chunks(4)
        .map(|c| [c[2], c[1], c[0]])
        .collect();
    let bits = &b[pal_end..];
    let stride = (w * bpp).div_ceil(32) * 4;
    if stride.checked_mul(h).is_none_or(|n| n > bits.len()) {
        return Err(bad());
    }
    let mut px = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = bits
            .get((h - 1 - y) * stride..(h - y) * stride)
            .ok_or_else(bad)?;
        for x in 0..w {
            px.push(match bpp {
                1 => (row[x >> 3] >> (7 - (x & 7))) & 1,
                4 => (row[x >> 1] >> if x % 2 == 0 { 4 } else { 0 }) & 15,
                _ => row[x],
            });
        }
    }
    Ok((w as u32, h as u32, pal, px))
}

/// Tray icon (mask DIB, colour DIB) to RGBA: mask 1 is transparent (character-format 9.1).
pub fn icon_rgba(mask: &[u8], color: &[u8]) -> Result<Rgba, Error> {
    let (mw, mh, _, m) = dib_decode(mask)?;
    let (w, h, pal, c) = dib_decode(color)?;
    if mw < w || mh < h {
        return Err(Error::Source(
            "icon mask smaller than the colour bitmap".into(),
        ));
    }
    let mut out = Rgba::new(w, h);
    for y in 0..h as usize {
        for x in 0..w as usize {
            if m[y * mw as usize + x] == 0 {
                let p = pal[c[y * w as usize + x] as usize];
                let i = (y * w as usize + x) * 4;
                out.data[i..i + 4].copy_from_slice(&[p[0], p[1], p[2], 255]);
            }
        }
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    #[test]
    fn hostile_dib_is_an_error() {
        let mut d = vec![0u8; 64];
        d[0] = 40;
        d[4..8].copy_from_slice(&0x7fff_0000i32.to_le_bytes());
        d[8..12].copy_from_slice(&0x7fff_0000i32.to_le_bytes());
        d[12] = 1;
        d[14] = 1;
        assert!(super::dib_decode(&d).is_err());
    }
}
