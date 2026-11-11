//! The Agent bit-level LZ77 decompressor (acs-format 4), used by ACS images and regions and
//! by whole ACF/ACA/AAF bodies. Owner: formats teammate.

/// Why a stream failed to decode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LzError {
    #[error("stream shorter than 8 bytes")]
    TooShort,
    #[error("first byte is not 0")]
    BadLeadByte,
    #[error("stream does not end in six 0xFF bytes")]
    BadTrailer,
    #[error("output larger than {0} bytes")]
    Overflow(usize),
    #[error("back-reference distance {dist} beyond the {have} bytes decoded")]
    BadDistance { dist: usize, have: usize },
    #[error("length prefix longer than 11 bits")]
    BadLength,
    #[error("decoded {got} bytes, expected {want}")]
    LengthMismatch { got: usize, want: usize },
}

struct Bits<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    /// Reads `n <= 24` bits, least significant first. Bytes past the end read as 0xFF, which
    /// always decodes as the end marker (the runtime reads a few bytes past the end too).
    fn read(&mut self, n: u32) -> u32 {
        let mut v = 0u32;
        for i in 0..n {
            let byte = self.src.get(self.pos >> 3).copied().unwrap_or(0xFF);
            v |= (((byte >> (self.pos & 7)) & 1) as u32) << i;
            self.pos += 1;
        }
        v
    }
}

/// Decodes `src` into at most `capacity` bytes. The result may be shorter than `capacity`;
/// callers that know the exact size use [`decompress_exact`].
pub fn decompress(src: &[u8], capacity: usize) -> Result<Vec<u8>, LzError> {
    if src.len() < 8 {
        return Err(LzError::TooShort);
    }
    if src[src.len() - 6..].iter().any(|&b| b != 0xFF) {
        return Err(LzError::BadTrailer);
    }
    if src[0] != 0 {
        return Err(LzError::BadLeadByte);
    }
    let mut out: Vec<u8> = Vec::with_capacity(capacity.min(src.len().saturating_mul(8)));
    let mut bits = Bits { src, pos: 8 };
    loop {
        if bits.read(1) == 0 {
            if out.len() == capacity {
                return Err(LzError::Overflow(capacity));
            }
            out.push(bits.read(8) as u8);
            continue;
        }
        let (dist, bonus) = if bits.read(1) == 0 {
            (bits.read(6) as usize + 1, 0)
        } else if bits.read(1) == 0 {
            (bits.read(9) as usize + 65, 0)
        } else if bits.read(1) == 0 {
            (bits.read(12) as usize + 577, 0)
        } else {
            let v = bits.read(20);
            if v == 0xFFFFF {
                return Ok(out);
            }
            (v as usize + 4673, 1)
        };
        let mut n = 0;
        while bits.read(1) == 1 {
            n += 1;
            if n == 12 {
                return Err(LzError::BadLength);
            }
        }
        let length = 2 + bonus + ((1usize << n) - 1) + bits.read(n) as usize;
        if length > capacity - out.len() {
            return Err(LzError::Overflow(capacity));
        }
        if dist > out.len() {
            return Err(LzError::BadDistance {
                dist,
                have: out.len(),
            });
        }
        for _ in 0..length {
            out.push(out[out.len() - dist]);
        }
    }
}

/// Decodes `src` and requires exactly `size` output bytes.
pub fn decompress_exact(src: &[u8], size: usize) -> Result<Vec<u8>, LzError> {
    let out = decompress(src, size)?;
    if out.len() != size {
        return Err(LzError::LengthMismatch {
            got: out.len(),
            want: size,
        });
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// LSB-first bit writer for hand-built streams.
    struct W {
        bytes: Vec<u8>,
        n: usize,
    }

    impl W {
        fn new() -> Self {
            W {
                bytes: vec![0],
                n: 8,
            }
        }
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
        fn lit(&mut self, b: u8) {
            self.put(0, 1);
            self.put(b as u32, 8);
        }
        fn end(mut self) -> Vec<u8> {
            self.put(0xFFFFFF, 24);
            while !self.n.is_multiple_of(8) {
                self.put(1, 1);
            }
            self.bytes.extend_from_slice(&[0xFF; 3]);
            self.bytes
        }
    }

    #[test]
    fn literals_and_short_copy() {
        let mut w = W::new();
        w.lit(b'a');
        w.lit(b'b');
        // back-reference, 6-bit class, distance 2, length 2 + (2^1 - 1) + 1 = 4
        w.put(1, 1);
        w.put(0, 1);
        w.put(1, 6);
        w.put(1, 1);
        w.put(0, 1);
        w.put(1, 1);
        let s = w.end();
        assert_eq!(decompress(&s, 100).unwrap(), b"ababab");
        assert_eq!(decompress_exact(&s, 6).unwrap(), b"ababab");
        assert_eq!(
            decompress_exact(&s, 7),
            Err(LzError::LengthMismatch { got: 6, want: 7 })
        );
        assert_eq!(decompress(&s, 5), Err(LzError::Overflow(5)));
    }

    #[test]
    fn far_class_has_length_bonus() {
        let mut w = W::new();
        for i in 0..4673u32 {
            w.lit((i % 251) as u8);
        }
        // 20-bit class, distance 4673, minimal length 3
        w.put(0b1111, 4);
        w.put(0, 20);
        w.put(0, 1);
        let s = w.end();
        let out = decompress(&s, 10_000).unwrap();
        assert_eq!(out.len(), 4676);
        assert_eq!(&out[4673..], &out[..3]);
    }

    #[test]
    fn validation() {
        assert_eq!(decompress(&[0; 7], 10), Err(LzError::TooShort));
        assert_eq!(
            decompress(&[1, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], 10),
            Err(LzError::BadLeadByte)
        );
        assert_eq!(
            decompress(&[0, 0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0], 10),
            Err(LzError::BadTrailer)
        );
        // empty stream: just the end marker
        assert_eq!(
            decompress(&[0, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF, 0xFF], 10).unwrap(),
            b""
        );
        // copy before any output
        let mut w = W::new();
        w.put(1, 1);
        w.put(0, 1);
        w.put(0, 6);
        w.put(0, 1);
        assert_eq!(
            decompress(&w.end(), 10),
            Err(LzError::BadDistance { dist: 1, have: 0 })
        );
    }
}
