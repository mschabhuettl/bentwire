//! Building blocks shared by the ACS, ACF/ACA and Agent 1.5 writers: pictures, DIB rows,
//! RGNDATA regions, the Agent LZ encoder and RIFF/WAVE sounds. Written independently of the
//! readers in `agent-formats`. Owner: formats teammate.

/// Little-endian byte writer.
#[derive(Default)]
pub struct Out(pub Vec<u8>);

impl Out {
    pub fn u8(&mut self, v: u8) {
        self.0.push(v);
    }
    pub fn u16(&mut self, v: u16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i16(&mut self, v: i16) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn u32(&mut self, v: u32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn i32(&mut self, v: i32) {
        self.0.extend_from_slice(&v.to_le_bytes());
    }
    pub fn bytes(&mut self, b: &[u8]) {
        self.0.extend_from_slice(b);
    }
    fn units(&mut self, s: &str) -> u32 {
        let u: Vec<u16> = s.encode_utf16().collect();
        self.u32(u.len() as u32);
        for c in &u {
            self.u16(*c);
        }
        u.len() as u32
    }
    /// ACS string: count, units, 0 terminator; just a zero count when empty.
    pub fn acs_string(&mut self, s: &str) {
        if self.units(s) > 0 {
            self.u16(0);
        }
    }
    /// ACF string: count, units, no terminator.
    pub fn acf_string(&mut self, s: &str) {
        self.units(s);
    }
    pub fn len(&self) -> usize {
        self.0.len()
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// An 8 bpp palette-indexed picture, top-down, `width * height` bytes, no padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picture {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

impl Picture {
    /// A deterministic test picture: a filled ellipse with stripes in colours derived from
    /// `seed`, `transparent` outside. Never uses `transparent` inside the shape.
    pub fn pattern(width: u16, height: u16, seed: u32, transparent: u8) -> Picture {
        let (w, h) = (width as i64, height as i64);
        let mut pixels = Vec::with_capacity((w * h) as usize);
        for y in 0..h {
            for x in 0..w {
                // ellipse inscribed in the rectangle, with a hole for region variety
                let dx = 2 * x + 1 - w;
                let dy = 2 * y + 1 - h;
                let inside = dx * dx * h * h + dy * dy * w * w <= w * w * h * h;
                let hole = (dx.abs() < w / 6) && (dy.abs() < h / 6);
                if inside && !hole {
                    let mut c =
                        (((x / 3 + y / 2) as u32).wrapping_add(seed.wrapping_mul(37)) % 200) as u8;
                    if c == transparent {
                        c = c.wrapping_add(1);
                    }
                    pixels.push(c);
                } else {
                    pixels.push(transparent);
                }
            }
        }
        Picture {
            width,
            height,
            pixels,
        }
    }

    /// The picture as bottom-up DIB rows padded to 4 bytes.
    pub fn dib(&self) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let s = (w + 3) & !3;
        let mut out = vec![0u8; s * h];
        for y in 0..h {
            let dst = (h - 1 - y) * s;
            out[dst..dst + w].copy_from_slice(&self.pixels[y * w..y * w + w]);
        }
        out
    }

    /// RGNDATA (RDH_RECTANGLES) covering exactly the pixels that are not `transparent`, as
    /// one rectangle per horizontal run, top-down.
    pub fn region(&self, transparent: u8) -> Vec<u8> {
        let (w, h) = (self.width as usize, self.height as usize);
        let mut rects = Vec::new();
        for y in 0..h {
            let row = &self.pixels[y * w..y * w + w];
            let mut x = 0;
            while x < w {
                if row[x] == transparent {
                    x += 1;
                    continue;
                }
                let start = x;
                while x < w && row[x] != transparent {
                    x += 1;
                }
                rects.push([start as i32, y as i32, x as i32, y as i32 + 1]);
            }
        }
        rgndata(&rects)
    }
}

/// RGNDATA from rectangles [left, top, right, bottom].
pub fn rgndata(rects: &[[i32; 4]]) -> Vec<u8> {
    let mut bound = [0i32; 4];
    if let Some(first) = rects.first() {
        bound = *first;
        for r in rects {
            bound[0] = bound[0].min(r[0]);
            bound[1] = bound[1].min(r[1]);
            bound[2] = bound[2].max(r[2]);
            bound[3] = bound[3].max(r[3]);
        }
    }
    let mut o = Out::default();
    o.u32(32);
    o.u32(1);
    o.u32(rects.len() as u32);
    o.u32(16 * rects.len() as u32);
    for v in bound {
        o.i32(v);
    }
    for r in rects {
        for v in r {
            o.i32(*v);
        }
    }
    o.0
}

/// How a block is stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Compression {
    /// Not compressed (where the format allows it).
    Raw,
    /// Agent LZ with literals only.
    Literals,
    /// Agent LZ with back-references (greedy matching).
    Lz,
}

struct Bits {
    bytes: Vec<u8>,
    n: usize,
}

impl Bits {
    fn put(&mut self, v: u32, width: u32) {
        for i in 0..width {
            if self.n.is_multiple_of(8) {
                self.bytes.push(0);
            }
            let last = self.bytes.len() - 1;
            self.bytes[last] |= (((v >> i) & 1) as u8) << (self.n % 8);
            self.n += 1;
        }
    }
}

const MAX_DIST: usize = 4673 + 0xFFFFE;

/// Agent LZ encoding (acs-format 4): lead byte 0, LSB-first bit codes, end marker, padding
/// so that the stream ends in six 0xFF bytes with three bytes after the marker's last byte.
pub fn lz_compress(data: &[u8], backrefs: bool) -> Vec<u8> {
    let mut b = Bits {
        bytes: vec![0],
        n: 8,
    };
    let mut heads: std::collections::HashMap<[u8; 2], Vec<usize>> = Default::default();
    let mut i = 0;
    while i < data.len() {
        let mut best = (0usize, 0usize); // (length, distance)
        if backrefs
            && i + 2 <= data.len()
            && let Some(cands) = heads.get(&[data[i], data[i + 1]])
        {
            {
                for &j in cands.iter().rev().take(32) {
                    let dist = i - j;
                    if dist > MAX_DIST {
                        break;
                    }
                    let far = dist >= 4673;
                    let max = if far { 4097 } else { 4096 };
                    let mut l = 0;
                    while l < max && i + l < data.len() && data[j + l] == data[i + l] {
                        l += 1;
                    }
                    if l >= 2 + far as usize && l > best.0 {
                        best = (l, dist);
                    }
                }
            }
        }
        let step = if best.0 == 0 {
            b.put(0, 1);
            b.put(data[i] as u32, 8);
            1
        } else {
            let (len, dist) = best;
            b.put(1, 1);
            let bonus = if dist <= 64 {
                b.put(0, 1);
                b.put((dist - 1) as u32, 6);
                0
            } else if dist <= 576 {
                b.put(0b01, 2);
                b.put((dist - 65) as u32, 9);
                0
            } else if dist <= 4672 {
                b.put(0b011, 3);
                b.put((dist - 577) as u32, 12);
                0
            } else {
                b.put(0b111, 3);
                b.put((dist - 4673) as u32, 20);
                1
            };
            let v = (len - 2 - bonus) as u32; // 0..=4094
            let n = 31 - (v + 1).leading_zeros(); // 2^n - 1 <= v < 2^(n+1) - 1
            b.put((1 << n) - 1, n);
            b.put(0, 1);
            b.put(v - ((1 << n) - 1), n);
            len
        };
        for k in i..i + step {
            if k + 2 <= data.len() {
                heads.entry([data[k], data[k + 1]]).or_default().push(k);
            }
        }
        i += step;
    }
    b.put(0xFF_FFFF, 24);
    while !b.n.is_multiple_of(8) {
        b.put(1, 1);
    }
    b.bytes.extend_from_slice(&[0xFF; 3]);
    // the decoder needs at least 8 bytes (only an empty input is shorter)
    b.bytes.resize(b.bytes.len().max(8), 0xFF);
    b.bytes
}

/// A PCM RIFF/WAVE file with a deterministic saw-tooth signal.
pub fn wav_pcm(rate: u32, channels: u16, bits: u16, frames: u32, seed: u32) -> Vec<u8> {
    let block = channels * bits / 8;
    let mut data = Out::default();
    for i in 0..frames * channels as u32 {
        let v = i.wrapping_mul(7).wrapping_add(seed) % 256;
        if bits == 8 {
            data.u8(v as u8);
        } else {
            data.i16(((v as i32 - 128) * 200) as i16);
        }
    }
    let mut o = Out::default();
    o.bytes(b"RIFF");
    o.u32(4 + 8 + 16 + 8 + data.len() as u32);
    o.bytes(b"WAVEfmt ");
    o.u32(16);
    o.u16(1);
    o.u16(channels);
    o.u32(rate);
    o.u32(rate * block as u32);
    o.u16(block);
    o.u16(bits);
    o.bytes(b"data");
    o.u32(data.len() as u32);
    o.bytes(&data.0);
    if data.len() % 2 == 1 {
        // RIFF pad byte; keep RIFF length + 8 == file size
        o.u8(0);
        let n = o.len() as u32 - 8;
        o.0[4..8].copy_from_slice(&n.to_le_bytes());
    }
    o.0
}

/// A packed DIB (BITMAPINFOHEADER, colour table of `1 << bpp` entries, bottom-up bits) for
/// tray icons. `index(x, y)` gives the colour index at top-down (x, y).
pub fn packed_dib(width: u32, height: u32, bpp: u16, index: impl Fn(u32, u32) -> u8) -> Vec<u8> {
    let mut o = Out::default();
    let row = (width * bpp as u32).div_ceil(32) * 4;
    o.u32(40);
    o.i32(width as i32);
    o.i32(height as i32);
    o.u16(1);
    o.u16(bpp);
    o.u32(0);
    o.u32(row * height);
    o.u32(0);
    o.u32(0);
    o.u32(0);
    o.u32(0);
    for c in 0..(1u32 << bpp) {
        let v = (c * 255 / ((1 << bpp) - 1)) as u8;
        o.bytes(&[v, v / 2, 255 - v, 0]);
    }
    for y in (0..height).rev() {
        let mut bits = vec![0u8; row as usize];
        for x in 0..width {
            let v = index(x, y) as u32 & ((1 << bpp) - 1);
            let bit = x * bpp as u32;
            bits[(bit / 8) as usize] |= (v << (8 - bpp as u32 - bit % 8)) as u8;
        }
        o.bytes(&bits);
    }
    o.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lz_trailer_shape() {
        for data in [&b""[..], b"a", b"abababababababab", &[7u8; 5000][..]] {
            for backrefs in [false, true] {
                let s = lz_compress(data, backrefs);
                assert_eq!(s[0], 0);
                assert!(s.len() >= 8 && s[s.len() - 6..].iter().all(|&b| b == 0xFF));
            }
        }
        assert!(lz_compress(&[7u8; 5000], true).len() < 100);
    }

    #[test]
    fn region_rows() {
        let p = Picture {
            width: 3,
            height: 2,
            pixels: vec![0, 1, 1, 1, 0, 1],
        };
        let r = p.region(0);
        assert_eq!(&r[8..12], &3u32.to_le_bytes());
        assert_eq!(r.len(), 32 + 3 * 16);
        assert_eq!(p.dib(), vec![1, 0, 1, 0, 0, 1, 1, 0]);
    }
}
