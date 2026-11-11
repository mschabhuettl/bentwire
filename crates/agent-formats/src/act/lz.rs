//! Bit-level LZ77 decoder shared by ACS images and ACT bitmap blobs (act-format 4.8).

/// Upper bound for a declared decompressed size; real blobs are below 1 MiB.
pub const MAX_DECLARED: usize = 256 << 20;

struct Bits<'a> {
    src: &'a [u8],
    pos: usize,
}

impl Bits<'_> {
    fn bit(&mut self) -> Option<u32> {
        let byte = *self.src.get(self.pos >> 3)?;
        let b = (byte >> (self.pos & 7)) & 1;
        self.pos += 1;
        Some(u32::from(b))
    }

    fn bits(&mut self, n: u32) -> Option<u32> {
        let mut v = 0;
        for i in 0..n {
            v |= self.bit()? << i;
        }
        Some(v)
    }
}

/// Decompress `src` (first byte must be 0) into exactly `declared` bytes.
pub fn decompress(src: &[u8], declared: usize) -> Result<Vec<u8>, &'static str> {
    if declared > MAX_DECLARED {
        return Err("declared size too large");
    }
    if src.len() < 8 {
        return Err("stream shorter than 8 bytes");
    }
    if src[0] != 0 {
        return Err("first byte not 0");
    }
    let mut r = Bits { src, pos: 8 };
    let mut out: Vec<u8> = Vec::new();
    const EOS: &str = "stream ended before end marker";
    loop {
        if r.bit().ok_or(EOS)? == 0 {
            out.push(r.bits(8).ok_or(EOS)? as u8);
        } else {
            let mut ones = 0;
            while ones < 3 && r.bit().ok_or(EOS)? == 1 {
                ones += 1;
            }
            let dist = match ones {
                0 => r.bits(6).ok_or(EOS)? as usize + 1,
                1 => r.bits(9).ok_or(EOS)? as usize + 65,
                2 => r.bits(12).ok_or(EOS)? as usize + 577,
                _ => {
                    let v = r.bits(20).ok_or(EOS)?;
                    if v == 0xFFFFF {
                        break;
                    }
                    v as usize + 4673
                }
            };
            let mut len: usize = if ones == 3 { 3 } else { 2 };
            let mut j = 0;
            while r.bit().ok_or(EOS)? == 1 {
                j += 1;
                if j > 11 {
                    return Err("bad length prefix");
                }
            }
            len += r.bits(j).ok_or(EOS)? as usize + (1usize << j) - 1;
            if dist > out.len() {
                return Err("distance before start of output");
            }
            if out.len() + len > declared {
                return Err("output exceeds declared size");
            }
            for _ in 0..len {
                out.push(out[out.len() - dist]);
            }
        }
        if out.len() > declared {
            return Err("output exceeds declared size");
        }
    }
    if out.len() != declared {
        return Err("output size differs from declared size");
    }
    Ok(out)
}
