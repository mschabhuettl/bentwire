//! Sounds in a package: FLAC or RIFF WAVE with PCM (character-format 6, "Storage").

use crate::Error;

/// Decoded PCM. `data` uses the WAVE sample layout: interleaved, little endian, 8-bit
/// unsigned, wider depths signed, `(bits + 7) / 8` bytes per sample.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Pcm {
    pub channels: u16,
    pub rate: u32,
    pub bits: u16,
    pub data: Vec<u8>,
}

impl Pcm {
    pub fn bytes_per_sample(&self) -> usize {
        (self.bits as usize).div_ceil(8)
    }

    /// Number of sample frames.
    pub fn frames(&self) -> usize {
        let per = self.bytes_per_sample() * self.channels.max(1) as usize;
        self.data.len() / per
    }

    /// Duration in milliseconds, rounded up.
    pub fn duration_ms(&self) -> u64 {
        if self.rate == 0 {
            return 0;
        }
        (self.frames() as u64 * 1000).div_ceil(self.rate as u64)
    }

    /// Sample `i` (interleaved index) as a signed value.
    pub fn sample(&self, i: usize) -> i32 {
        let n = self.bytes_per_sample();
        let b = &self.data[i * n..i * n + n];
        if n == 1 {
            return b[0] as i32 - 128;
        }
        let mut v: i32 = 0;
        for (k, x) in b.iter().enumerate() {
            v |= (*x as i32) << (8 * k);
        }
        let shift = 32 - 8 * n as u32;
        (v << shift) >> shift
    }

    /// All samples as i16 (8-bit scaled up, wider depths cut to the top 16 bits); for playback.
    pub fn to_i16(&self) -> Vec<i16> {
        let n = self.data.len() / self.bytes_per_sample();
        let bits = 8 * self.bytes_per_sample() as u32;
        (0..n)
            .map(|i| {
                let s = self.sample(i);
                if bits >= 16 {
                    (s >> (bits - 16)) as i16
                } else {
                    (s << (16 - bits)) as i16
                }
            })
            .collect()
    }

    /// Container-independent identity: first 16 hex digits of SHA-256 over
    /// `<channels u16><rate u32><bits u16>` (little endian) plus the PCM bytes
    /// (the prototype's `common.pcm_hash`).
    pub fn hash(&self) -> String {
        let mut h = Vec::with_capacity(8 + self.data.len());
        h.extend_from_slice(&self.channels.to_le_bytes());
        h.extend_from_slice(&self.rate.to_le_bytes());
        h.extend_from_slice(&self.bits.to_le_bytes());
        h.extend_from_slice(&self.data);
        crate::sha16(&h)
    }
}

/// A RIFF chunk: id and body.
pub type Chunk<'a> = ([u8; 4], &'a [u8]);

/// RIFF chunks of a WAVE file: the first occurrence of each id.
pub fn wav_chunks(b: &[u8]) -> Result<Vec<Chunk<'_>>, Error> {
    if b.len() < 12 || &b[..4] != b"RIFF" || &b[8..12] != b"WAVE" {
        return Err(Error::Sound("not a RIFF WAVE file".into()));
    }
    let mut out: Vec<Chunk<'_>> = Vec::new();
    let mut p = 12;
    while p + 8 <= b.len() {
        let id = [b[p], b[p + 1], b[p + 2], b[p + 3]];
        let n = u32::from_le_bytes([b[p + 4], b[p + 5], b[p + 6], b[p + 7]]) as usize;
        let end = (p + 8).saturating_add(n).min(b.len());
        if !out.iter().any(|(k, _)| *k == id) {
            out.push((id, &b[p + 8..end]));
        }
        p = (p + 8).saturating_add(n).saturating_add(n & 1);
    }
    Ok(out)
}

/// Decode a package sound: FLAC, or WAVE with PCM (format tag 1).
pub fn decode_sound(bytes: &[u8]) -> Result<Pcm, Error> {
    if bytes.starts_with(b"fLaC") {
        return decode_flac(bytes);
    }
    let chunks = wav_chunks(bytes)?;
    let get = |id: &[u8; 4]| chunks.iter().find(|(k, _)| k == id).map(|(_, v)| *v);
    let fmt = get(b"fmt ").ok_or_else(|| Error::Sound("no fmt chunk".into()))?;
    let data = get(b"data").ok_or_else(|| Error::Sound("no data chunk".into()))?;
    if fmt.len() < 16 {
        return Err(Error::Sound("short fmt chunk".into()));
    }
    let tag = u16::from_le_bytes([fmt[0], fmt[1]]);
    if tag != 1 {
        return Err(Error::Sound(format!(
            "WAVE format tag {tag} not allowed in a package (PCM only)"
        )));
    }
    let channels = u16::from_le_bytes([fmt[2], fmt[3]]);
    let rate = u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]);
    let bits = u16::from_le_bytes([fmt[14], fmt[15]]);
    if channels == 0 || bits == 0 || bits > 32 {
        return Err(Error::Sound("bad WAVE format".into()));
    }
    Ok(Pcm {
        channels,
        rate,
        bits,
        data: data.to_vec(),
    })
}

fn decode_flac(bytes: &[u8]) -> Result<Pcm, Error> {
    let e = |x: claxon::Error| Error::Sound(format!("FLAC: {x}"));
    let mut r = claxon::FlacReader::new(std::io::Cursor::new(bytes)).map_err(e)?;
    let info = r.streaminfo();
    let bits = info.bits_per_sample as u16;
    let n = (bits as usize).div_ceil(8);
    let mut data = Vec::new();
    for s in r.samples() {
        let s = s.map_err(e)?;
        if n == 1 {
            data.push((s + 128) as u8);
        } else {
            data.extend_from_slice(&s.to_le_bytes()[..n]);
        }
    }
    Ok(Pcm {
        channels: info.channels as u16,
        rate: info.sample_rate,
        bits,
        data,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wav(tag: u16, ch: u16, rate: u32, bits: u16, data: &[u8]) -> Vec<u8> {
        let mut fmt = Vec::new();
        fmt.extend_from_slice(&tag.to_le_bytes());
        fmt.extend_from_slice(&ch.to_le_bytes());
        fmt.extend_from_slice(&rate.to_le_bytes());
        fmt.extend_from_slice(&(rate * ch as u32 * bits as u32 / 8).to_le_bytes());
        fmt.extend_from_slice(&(ch * bits / 8).to_le_bytes());
        fmt.extend_from_slice(&bits.to_le_bytes());
        let mut body = b"WAVEfmt ".to_vec();
        body.extend_from_slice(&(fmt.len() as u32).to_le_bytes());
        body.extend_from_slice(&fmt);
        body.extend_from_slice(b"data");
        body.extend_from_slice(&(data.len() as u32).to_le_bytes());
        body.extend_from_slice(data);
        let mut out = b"RIFF".to_vec();
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&body);
        out
    }

    #[test]
    fn wave_pcm_and_rejects_adpcm() {
        let p = decode_sound(&wav(1, 1, 8000, 16, &[1, 0, 0xff, 0xff])).unwrap();
        assert_eq!((p.channels, p.rate, p.bits, p.frames()), (1, 8000, 16, 2));
        assert_eq!(p.sample(1), -1);
        assert!(decode_sound(&wav(2, 1, 8000, 4, &[0; 8])).is_err());
        let p8 = decode_sound(&wav(1, 1, 11025, 8, &[0, 128, 255])).unwrap();
        assert_eq!(p8.to_i16(), vec![-32768, 0, 127 << 8]);
    }
}
