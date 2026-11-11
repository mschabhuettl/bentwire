//! PNG sheets (character-format 5.1, 5.2) and the normative compositor (5.3).

use std::collections::HashMap;

use crate::Error;
use crate::model::{Layer, Manifest, Sheet};

/// Largest output picture `compose` produces (2^28 pixels, A-CHAR-4); larger requests give an
/// empty 0 x 0 picture.
pub const MAX_PIXELS: u64 = 1 << 28;

/// An RGBA8 picture, straight alpha, row-major, `width * height * 4` bytes.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rgba {
    pub width: u32,
    pub height: u32,
    pub data: Vec<u8>,
}

impl Rgba {
    /// Fully transparent picture.
    pub fn new(width: u32, height: u32) -> Self {
        Rgba {
            width,
            height,
            data: vec![0; width as usize * height as usize * 4],
        }
    }

    /// Alpha of pixel (x, y).
    pub fn alpha(&self, x: u32, y: u32) -> u8 {
        self.data[(y as usize * self.width as usize + x as usize) * 4 + 3]
    }
}

/// Pixels of a decoded PNG file.
#[derive(Clone, Debug)]
pub enum Pixels {
    /// One byte per pixel plus the colour table (memory format of character-format 5.2).
    Indexed {
        data: Vec<u8>,
        /// `PLTE` entries as stored.
        plte: Vec<[u8; 3]>,
        /// `tRNS` alpha values as stored (may be shorter than `plte`).
        trns: Option<Vec<u8>>,
        /// RGBA per index (index >= PLTE length never occurs, checked at decode).
        table: Box<[[u8; 4]; 256]>,
    },
    Rgba(Vec<u8>),
}

/// (palette entry count, RGB bytes, transparent index).
pub type PaletteInfo = (usize, Vec<u8>, Option<usize>);

/// A decoded PNG file.
#[derive(Clone, Debug)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub pixels: Pixels,
}

impl Image {
    #[inline]
    pub fn pixel(&self, x: u32, y: u32) -> [u8; 4] {
        let i = y as usize * self.width as usize + x as usize;
        match &self.pixels {
            Pixels::Indexed { data, table, .. } => table[data[i] as usize],
            Pixels::Rgba(d) => [d[i * 4], d[i * 4 + 1], d[i * 4 + 2], d[i * 4 + 3]],
        }
    }

    /// The whole file as RGBA (not canonicalized: colours of alpha-0 pixels are kept).
    pub fn to_rgba(&self) -> Rgba {
        let mut out = Rgba::new(self.width, self.height);
        for y in 0..self.height {
            for x in 0..self.width {
                let i = (y as usize * self.width as usize + x as usize) * 4;
                out.data[i..i + 4].copy_from_slice(&self.pixel(x, y));
            }
        }
        out
    }

    /// For indexed files: (palette entry count, RGB bytes, transparent index). The transparent
    /// index is the single index with alpha 0; `None` when there is no tRNS. Errors when tRNS
    /// makes several indices fully transparent.
    pub fn palette_info(&self) -> Option<Result<PaletteInfo, Error>> {
        match &self.pixels {
            Pixels::Rgba(_) => None,
            Pixels::Indexed { plte, trns, .. } => {
                let rgb: Vec<u8> = plte.iter().flatten().copied().collect();
                let t = match trns {
                    None => Ok(None),
                    Some(t) => {
                        let zeros: Vec<usize> = t
                            .iter()
                            .enumerate()
                            .filter(|(_, a)| **a == 0)
                            .map(|(i, _)| i)
                            .collect();
                        match zeros.len() {
                            0 => Ok(None),
                            1 => Ok(Some(zeros[0])),
                            _ => Err(Error::Image("tRNS has several transparent indices".into())),
                        }
                    }
                };
                Some(t.map(|t| (plte.len(), rgb, t)))
            }
        }
    }
}

/// Decode a PNG: indexed (any bit depth, `tRNS` kept) stays indexed; every other colour type
/// becomes RGBA8.
pub fn decode_png(bytes: &[u8]) -> Result<Image, Error> {
    let e = |x: png::DecodingError| Error::Image(x.to_string());
    let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
    dec.set_transformations(png::Transformations::IDENTITY);
    let info = dec.read_header_info().map_err(e)?;
    let indexed = info.color_type == png::ColorType::Indexed;
    if !indexed {
        let mut dec = png::Decoder::new(std::io::Cursor::new(bytes));
        dec.set_transformations(png::Transformations::normalize_to_color8());
        let mut r = dec.read_info().map_err(e)?;
        let mut buf = vec![0; r.output_buffer_size()];
        let fi = r.next_frame(&mut buf).map_err(e)?;
        let (w, h) = (fi.width, fi.height);
        let n = w as usize * h as usize;
        let mut out = Vec::with_capacity(n * 4);
        let (ct, _) = r.output_color_type();
        for y in 0..h as usize {
            let row = &buf[y * fi.line_size..(y + 1) * fi.line_size];
            for x in 0..w as usize {
                let px = match ct {
                    png::ColorType::Rgba => {
                        [row[x * 4], row[x * 4 + 1], row[x * 4 + 2], row[x * 4 + 3]]
                    }
                    png::ColorType::Rgb => [row[x * 3], row[x * 3 + 1], row[x * 3 + 2], 255],
                    png::ColorType::GrayscaleAlpha => {
                        [row[x * 2], row[x * 2], row[x * 2], row[x * 2 + 1]]
                    }
                    png::ColorType::Grayscale => [row[x], row[x], row[x], 255],
                    png::ColorType::Indexed => unreachable!("expanded"),
                };
                out.extend_from_slice(&px);
            }
        }
        return Ok(Image {
            width: w,
            height: h,
            pixels: Pixels::Rgba(out),
        });
    }
    let mut r = dec.read_info().map_err(e)?;
    let (plte, trns, depth) = {
        let i = r.info();
        let plte: Vec<[u8; 3]> = i
            .palette
            .as_ref()
            .ok_or_else(|| Error::Image("indexed PNG without PLTE".into()))?
            .as_chunks::<3>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        (plte, i.trns.as_ref().map(|t| t.to_vec()), i.bit_depth as u8)
    };
    let mut buf = vec![0; r.output_buffer_size()];
    let fi = r.next_frame(&mut buf).map_err(e)?;
    let (w, h) = (fi.width, fi.height);
    let mut data = Vec::with_capacity(w as usize * h as usize);
    for y in 0..h as usize {
        let row = &buf[y * fi.line_size..(y + 1) * fi.line_size];
        if depth == 8 {
            data.extend_from_slice(&row[..w as usize]);
        } else {
            let per = 8 / depth as usize;
            let mask = (1u16 << depth) as u8 - 1;
            for x in 0..w as usize {
                let b = row[x / per];
                let shift = 8 - depth as usize * (x % per + 1);
                data.push((b >> shift) & mask);
            }
        }
    }
    if let Some(&mx) = data.iter().max()
        && mx as usize >= plte.len()
    {
        return Err(Error::Image(format!(
            "pixel index {mx} beyond the {} palette entries",
            plte.len()
        )));
    }
    let mut table = Box::new([[0, 0, 0, 255]; 256]);
    for (i, c) in plte.iter().enumerate() {
        let a = trns.as_ref().and_then(|t| t.get(i).copied()).unwrap_or(255);
        table[i] = [c[0], c[1], c[2], a];
    }
    Ok(Image {
        width: w,
        height: h,
        pixels: Pixels::Indexed {
            data,
            plte,
            trns,
            table,
        },
    })
}

/// The decoded files of all sheets, keyed by (sheet id, density key; 1 = the base file).
#[derive(Default)]
pub struct SheetSet {
    pub(crate) files: HashMap<(String, u32), Result<Image, String>>,
}

impl SheetSet {
    pub fn base(&self, sheet: &str) -> Option<&Image> {
        self.files
            .get(&(sheet.to_owned(), 1))
            .and_then(|r| r.as_ref().ok())
    }

    /// The decoded file of `sheet` at density `key` (1 = base file).
    pub fn file(&self, sheet: &str, key: u32) -> Option<&Image> {
        self.files
            .get(&(sheet.to_owned(), key))
            .and_then(|r| r.as_ref().ok())
    }

    /// True when the base file of `sheet` decoded and holds every cell.
    pub fn ok(&self, sheet: &str) -> bool {
        self.base(sheet).is_some()
    }
}

/// Check that a decoded sheet file is large enough for `count` cells of `cell * key`.
pub(crate) fn check_grid(s: &Sheet, key: u32, img: &Image) -> Result<(), String> {
    let cw = s.cell[0] as u64 * key as u64;
    let ch = s.cell[1] as u64 * key as u64;
    let cols = img.width as u64 / cw;
    if cols == 0 {
        return Err(format!("{}: narrower than one cell", s.file));
    }
    let rows = (s.count as u64).div_ceil(cols);
    if rows * ch > img.height as u64 {
        return Err(format!(
            "{}: {} cells of {cw}x{ch} do not fit in {}x{}",
            s.file, s.count, img.width, img.height
        ));
    }
    Ok(())
}

fn trunc_div(a: i64, b: i64) -> i64 {
    // Rust integer division truncates toward zero, as character-format 5.3 asks.
    a / b
}

/// Composite `layers` back to front at output size `out` (character-format 5.3).
///
/// Sheet file choice (bentwire decision A-CHAR-3): per layer, the file (base = key 1, or a
/// `hidpi` key) with the smallest key whose picture covers the destination rectangle, else the
/// densest file.
pub fn compose(m: &Manifest, sheets: &SheetSet, layers: &[Layer], out: [u32; 2]) -> Rgba {
    let [ow, oh] = out;
    if ow as u64 * oh as u64 > MAX_PIXELS {
        return Rgba::new(0, 0);
    }
    let [cw, ch] = m.canvas();
    let mut dst = Rgba::new(ow, oh);
    for l in layers {
        let Some(sheet) = m.sheet(&l.image.sheet) else {
            continue;
        };
        let scale = sheet.scale.unwrap_or(1);
        let n = l.image.index;
        if n >= sheet.count {
            continue;
        }
        let base_size = sheet
            .sizes
            .as_ref()
            .and_then(|s| s.get(n as usize))
            .copied()
            .unwrap_or(sheet.cell);
        let [lw, lh] = l
            .size
            .unwrap_or([base_size[0] / scale, base_size[1] / scale]);
        let (x, y) = (l.x as i64, l.y as i64);
        let x0 = trunc_div(x * ow as i64, cw as i64);
        let x1 = trunc_div((x + lw as i64) * ow as i64, cw as i64);
        let y0 = trunc_div(y * oh as i64, ch as i64);
        let y1 = trunc_div((y + lh as i64) * oh as i64, ch as i64);
        let (dw, dh) = (x1 - x0, y1 - y0);
        if dw <= 0 || dh <= 0 {
            continue;
        }
        let key = pick_density(sheet, sheets, base_size, dw, dh);
        let Some(Ok(img)) = sheets.files.get(&(sheet.id.clone(), key)) else {
            continue;
        };
        let k = key as u64;
        let cell = [sheet.cell[0] as u64 * k, sheet.cell[1] as u64 * k];
        let cols = img.width as u64 / cell[0];
        if cols == 0 {
            continue;
        }
        let (sw, sh) = (base_size[0] as u64 * k, base_size[1] as u64 * k);
        let (px, py) = ((n as u64 % cols) * cell[0], (n as u64 / cols) * cell[1]);
        if px + sw > img.width as u64 || py + sh > img.height as u64 || sw == 0 || sh == 0 {
            continue;
        }
        let (px, py, sw, sh) = (px as u32, py as u32, sw as u32, sh as u32);
        let identity = dw == sw as i64 && dh == sh as i64;
        // Clip to the canvas.
        let dx_lo = (-x0).max(0);
        let dx_hi = dw.min(ow as i64 - x0);
        let dy_lo = (-y0).max(0);
        let dy_hi = dh.min(oh as i64 - y0);
        if dx_lo >= dx_hi || dy_lo >= dy_hi {
            continue;
        }
        let cols_map: Vec<u32> = (dx_lo..dx_hi)
            .map(|dx| {
                if identity {
                    dx as u32
                } else {
                    ((2 * dx + 1) * sw as i64 / (2 * dw)) as u32
                }
            })
            .collect();
        for dy in dy_lo..dy_hi {
            let sy = if identity {
                dy as u32
            } else {
                ((2 * dy + 1) * sh as i64 / (2 * dh)) as u32
            };
            let row = ((y0 + dy) as usize) * ow as usize;
            for (k, dx) in (dx_lo..dx_hi).enumerate() {
                let s = img.pixel(px + cols_map[k], py + sy);
                let a = s[3] as u32;
                if a == 0 {
                    continue;
                }
                let di = (row + (x0 + dx) as usize) * 4;
                let d = &mut dst.data[di..di + 4];
                if a == 255 {
                    d.copy_from_slice(&s);
                } else {
                    blend(d, s);
                }
            }
        }
    }
    dst
}

/// Source-over with straight alpha, rounded (character-format 5.3 step 4); 0 < alpha < 255.
fn blend(d: &mut [u8], s: [u8; 4]) {
    let a = s[3] as u64;
    let da = d[3] as u64;
    let wa = a * 255;
    let wd = da * (255 - a);
    let den = wa + wd;
    for c in 0..3 {
        let num = s[c] as u64 * wa + d[c] as u64 * wd;
        d[c] = ((2 * num + den) / (2 * den)) as u8;
    }
    d[3] = ((2 * den + 255) / 510) as u8;
}

/// The file for one layer: the smallest density key whose picture is at least as large as the
/// destination rectangle `dw x dh`, else the densest decoded file (A-CHAR-3).
fn pick_density(sheet: &Sheet, sheets: &SheetSet, base: [u32; 2], dw: i64, dh: i64) -> u32 {
    if sheet.hidpi.is_empty() {
        return 1;
    }
    let mut keys: Vec<u32> = std::iter::once(1)
        .chain(sheet.hidpi.keys().copied())
        .filter(|k| matches!(sheets.files.get(&(sheet.id.clone(), *k)), Some(Ok(_))))
        .collect();
    keys.sort_unstable();
    keys.iter()
        .copied()
        .find(|&k| base[0] as i64 * k as i64 >= dw && base[1] as i64 * k as i64 >= dh)
        .or_else(|| keys.last().copied())
        .unwrap_or(1)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_partial_alpha_vectors() {
        // Opaque destination: plain mix, alpha stays 255.
        let mut d = [0, 0, 0, 255];
        blend(&mut d, [255, 255, 255, 128]);
        assert_eq!(d, [128, 128, 128, 255]);
        // Transparent destination: source colour, source alpha.
        let mut d = [9, 9, 9, 0];
        blend(&mut d, [200, 100, 50, 77]);
        assert_eq!(d, [200, 100, 50, 77]);
        // Half over half: out_a = round((128*255 + 128*127)/255) = 192.
        let mut d = [0, 0, 255, 128];
        blend(&mut d, [255, 0, 0, 128]);
        assert_eq!(d[3], 192);
        assert_eq!(d[0], 170); // 255*128*255 / (128*255 + 128*127) = 170.3
        assert_eq!(d[2], 85);
    }
}
