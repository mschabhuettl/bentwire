//! Source sounds (RIFF WAVE) to PCM. MS ADPCM is decoded by Microsoft's rule of
//! character-format 6 (written from that description; Python reference
//! `common.msadpcm_decode_ms`).

use bentwire_character::Pcm;
use bentwire_character::audio::wav_chunks;

use crate::Error;

const ADAPT: [i32; 16] = [
    230, 230, 230, 230, 307, 409, 512, 614, 768, 614, 512, 409, 307, 230, 230, 230,
];

/// Result of decoding one source sound.
#[derive(Debug, PartialEq, Eq)]
pub enum Decoded {
    Pcm(Pcm),
    /// Agent cannot load it (MS ADPCM `data` shorter than one block, rule 2).
    Unplayable,
}

/// MS ADPCM `fmt ` fields the rule needs.
struct AdpcmFmt {
    channels: usize,
    rate: u32,
    block_align: usize,
    samples_per_block: usize,
    coefs: Vec<(i32, i32)>,
}

fn u16le(b: &[u8], p: usize) -> Option<u16> {
    b.get(p..p + 2).map(|s| u16::from_le_bytes([s[0], s[1]]))
}

fn i16le(b: &[u8], p: usize) -> i32 {
    i16::from_le_bytes([b[p], b[p + 1]]) as i32
}

fn parse_adpcm_fmt(fmt: &[u8]) -> Result<AdpcmFmt, Error> {
    let bad = || Error::Source("MS ADPCM fmt chunk too short".into());
    let channels = u16le(fmt, 2).ok_or_else(bad)? as usize;
    let rate = fmt
        .get(4..8)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(bad)?;
    let block_align = u16le(fmt, 12).ok_or_else(bad)? as usize;
    let samples_per_block = u16le(fmt, 18).ok_or_else(bad)? as usize;
    let ncoef = u16le(fmt, 20).ok_or_else(bad)? as usize;
    if fmt.len() < 22 + 4 * ncoef {
        return Err(bad());
    }
    let coefs = (0..ncoef)
        .map(|i| (i16le(fmt, 22 + 4 * i), i16le(fmt, 24 + 4 * i)))
        .collect();
    if channels == 0 || block_align == 0 {
        return Err(Error::Source(
            "MS ADPCM with 0 channels or block size".into(),
        ));
    }
    Ok(AdpcmFmt {
        channels,
        rate,
        block_align,
        samples_per_block,
        coefs,
    })
}

/// Decode MS ADPCM `data` by the normative rule (character-format 6, steps 1 to 5).
/// `short_ok`: a sound shorter than one block is decoded and padded to one block instead of
/// being unplayable (the converter's choice for Office 97 sounds, section 6 "Limits").
fn msadpcm(f: &AdpcmFmt, data: &[u8], short_ok: bool) -> Result<Decoded, Error> {
    let nch = f.channels;
    if data.len() < f.block_align && !short_ok {
        return Ok(Decoded::Unplayable);
    }
    let blocks = data.len().div_ceil(f.block_align).max(1);
    let total = blocks * f.samples_per_block * nch;
    let mut out: Vec<i16> = Vec::with_capacity(total);
    let mut pos = 0;
    'blocks: while data.len() - pos >= 7 * nch {
        let end = (pos + f.block_align).min(data.len());
        let blk = &data[pos..end];
        pos = end;
        let pred: Vec<usize> = blk[..nch].iter().map(|&b| b as usize).collect();
        if pred.iter().any(|&p| p >= f.coefs.len()) {
            break 'blocks;
        }
        let mut p = nch;
        let mut delta: Vec<i32> = (0..nch).map(|i| i16le(blk, p + 2 * i)).collect();
        p += 2 * nch;
        let mut s1: Vec<i32> = (0..nch).map(|i| i16le(blk, p + 2 * i)).collect();
        p += 2 * nch;
        let mut s2: Vec<i32> = (0..nch).map(|i| i16le(blk, p + 2 * i)).collect();
        p += 2 * nch;
        out.extend(s2.iter().map(|&v| v as i16));
        out.extend(s1.iter().map(|&v| v as i16));
        let mut c = 0;
        for &byte in &blk[p..] {
            for nib in [byte >> 4, byte & 15] {
                let (k1, k2) = f.coefs[pred[c]];
                let signed = if nib & 8 != 0 {
                    nib as i32 - 16
                } else {
                    nib as i32
                };
                // 32-bit signed arithmetic shift (rounds toward minus infinity).
                let pr = s1[c].wrapping_mul(k1).wrapping_add(s2[c].wrapping_mul(k2)) >> 8;
                let v = (pr + signed * delta[c]).clamp(-32768, 32767);
                s2[c] = s1[c];
                s1[c] = v;
                delta[c] = ((ADAPT[nib as usize] * delta[c]) >> 8).max(16);
                out.push(v as i16);
                c = (c + 1) % nch;
            }
        }
    }
    if out.len() > total {
        return Err(Error::Source(
            "MS ADPCM block decodes to more samples than wSamplesPerBlock".into(),
        ));
    }
    out.resize(total, 0);
    Ok(Decoded::Pcm(Pcm {
        channels: nch as u16,
        rate: f.rate,
        bits: 16,
        data: out.iter().flat_map(|s| s.to_le_bytes()).collect(),
    }))
}

/// Decode a source RIFF WAVE: PCM `data` unchanged, MS ADPCM by Microsoft's rule.
pub fn decode_source_sound(riff: &[u8], short_ok: bool) -> Result<Decoded, Error> {
    let chunks = wav_chunks(riff).map_err(|e| Error::Source(e.to_string()))?;
    let get = |id: &[u8; 4]| chunks.iter().find(|(k, _)| k == id).map(|(_, v)| *v);
    let fmt = get(b"fmt ").ok_or_else(|| Error::Source("sound without fmt chunk".into()))?;
    let data = get(b"data").ok_or_else(|| Error::Source("sound without data chunk".into()))?;
    let tag = u16le(fmt, 0).ok_or_else(|| Error::Source("short fmt chunk".into()))?;
    match tag {
        1 => {
            if fmt.len() < 16 {
                return Err(Error::Source("short fmt chunk".into()));
            }
            Ok(Decoded::Pcm(Pcm {
                channels: u16le(fmt, 2).unwrap(),
                rate: u32::from_le_bytes([fmt[4], fmt[5], fmt[6], fmt[7]]),
                bits: u16le(fmt, 14).unwrap(),
                data: data.to_vec(),
            }))
        }
        2 => msadpcm(&parse_adpcm_fmt(fmt)?, data, short_ok),
        t => Err(Error::Source(format!("unsupported WAVE format tag {t}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STD_COEFS: [(i32, i32); 7] = [
        (256, 0),
        (512, -256),
        (0, 0),
        (192, 64),
        (240, 0),
        (460, -208),
        (392, -232),
    ];

    fn fmt(nch: usize, align: usize, spb: usize) -> AdpcmFmt {
        AdpcmFmt {
            channels: nch,
            rate: 11025,
            block_align: align,
            samples_per_block: spb,
            coefs: STD_COEFS.to_vec(),
        }
    }

    fn samples(d: Decoded) -> Vec<i16> {
        match d {
            Decoded::Pcm(p) => p
                .data
                .chunks(2)
                .map(|c| i16::from_le_bytes([c[0], c[1]]))
                .collect(),
            Decoded::Unplayable => panic!("unplayable"),
        }
    }

    /// Mono header: predictor, delta, sample1, sample2.
    fn header(pred: u8, delta: i16, s1: i16, s2: i16) -> Vec<u8> {
        let mut v = vec![pred];
        v.extend_from_slice(&delta.to_le_bytes());
        v.extend_from_slice(&s1.to_le_bytes());
        v.extend_from_slice(&s2.to_le_bytes());
        v
    }

    #[test]
    fn arithmetic_shift_rounds_toward_minus_infinity() {
        // Predictor 3 (192, 64): s1 = -1, s2 = 0: (-192) >> 8 = -1 (a truncating division
        // would give 0). Nibble 0 adds nothing. Output: s2, s1, then -1.
        let mut b = header(3, 16, -1, 0);
        b.push(0x00);
        let out = samples(msadpcm(&fmt(1, 8, 4), &b, false).unwrap());
        assert_eq!(out, vec![0, -1, -1, -1]);
    }

    #[test]
    fn high_nibble_first_clamp_and_delta_floor() {
        // Predictor 0 (256, 0): prediction = s1. Delta 1000; nibble 7 adds 7000 each step.
        // s1 = 30000: 37000 clamps to 32767. Delta: 614*1000>>8 = 2398. Then nibble 1 (low):
        // 32767 + 2398 clamps again.
        let mut b = header(0, 1000, 30000, 0);
        b.push(0x71);
        let out = samples(msadpcm(&fmt(1, 8, 4), &b, false).unwrap());
        assert_eq!(out, vec![0, 30000, 32767, 32767]);
        // Delta floor: delta 16, nibble 0 -> 230*16>>8 = 14 -> 16; nibble 8 (-8) uses 16 and
        // sets 768*16>>8 = 48; nibble 0 sets 230*48>>8 = 43; nibble 8: -128 - 8*43 = -472.
        let mut b = header(0, 16, 0, 0);
        b.extend_from_slice(&[0x08, 0x08]);
        let out = samples(msadpcm(&fmt(1, 9, 6), &b, false).unwrap());
        assert_eq!(out, vec![0, 0, 0, -128, -128, -472]);
    }

    #[test]
    fn short_final_block_is_padded_with_zeros() {
        // Block size 9 (2 data bytes, spb 6). Data: one full block plus 8 bytes (7 header + 1).
        let mut b = header(0, 16, 5, 4);
        b.extend_from_slice(&[0x00, 0x00]);
        b.extend(header(0, 16, 7, 6));
        b.push(0x00);
        let out = samples(msadpcm(&fmt(1, 9, 6), &b, false).unwrap());
        // ceil(17 / 9) * 6 = 12 sample frames; the short block gives 4, the rest stays zero.
        assert_eq!(out, vec![4, 5, 5, 5, 5, 5, 6, 7, 7, 7, 0, 0]);
        // A tail shorter than 7 bytes starts no block.
        let mut c = header(0, 16, 5, 4);
        c.extend_from_slice(&[0x00, 0x00, 1, 2, 3]);
        let out = samples(msadpcm(&fmt(1, 9, 6), &c, false).unwrap());
        assert_eq!(out, vec![4, 5, 5, 5, 5, 5, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn shorter_than_one_block_is_unplayable() {
        let b = header(0, 16, 5, 4);
        assert_eq!(
            msadpcm(&fmt(1, 9, 6), &b, false).unwrap(),
            Decoded::Unplayable
        );
        // Office 97 path: decoded and padded to one block.
        let out = samples(msadpcm(&fmt(1, 9, 6), &b, true).unwrap());
        assert_eq!(out, vec![4, 5, 0, 0, 0, 0]);
    }

    #[test]
    fn bad_predictor_stops_decoding() {
        let mut b = header(0, 16, 5, 4);
        b.extend_from_slice(&[0x00, 0x00]);
        b.extend(header(7, 16, 7, 6)); // index 7 >= wNumCoef 7
        b.extend_from_slice(&[0x00, 0x00]);
        let out = samples(msadpcm(&fmt(1, 9, 6), &b, false).unwrap());
        assert_eq!(out, vec![4, 5, 5, 5, 5, 5, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn stereo_interleaves_high_nibble_left() {
        // Header: preds, deltas, s1s, s2s (per channel each).
        let mut b = vec![0u8, 0];
        for v in [16i16, 16, 100, 200, 10, 20] {
            b.extend_from_slice(&v.to_le_bytes());
        }
        b.push(0x10); // left +1*16, right +0
        let out = samples(msadpcm(&fmt(2, 15, 3), &b, false).unwrap());
        assert_eq!(out, vec![10, 20, 100, 200, 116, 200]);
    }
}
