//! Reading the Office palette from a user's MSO97.DLL at conversion time (act-format 4.7).
//! No palette data is embedded here; only the location of the LOGPALETTE in MSO97.DLL
//! 8.0.3507 is known.

use super::ActError;

/// Virtual address of the Office palette LOGPALETTE in MSO97.DLL 8.0.3507.
pub const MSO97_PALETTE_VA: u32 = 0x309D_9560;

fn err(detail: &str) -> ActError {
    ActError::Invalid {
        what: "MSO97.DLL",
        detail: detail.into(),
    }
}

fn rd16(d: &[u8], o: usize) -> Result<u16, ActError> {
    d.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| err("truncated PE"))
}

fn rd32(d: &[u8], o: usize) -> Result<u32, ActError> {
    d.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| err("truncated PE"))
}

/// Read the 256-entry Office palette (R, G, B) from MSO97.DLL. Checks the PE layout
/// and the LOGPALETTE header (version 0x300, 256 entries).
pub fn office_palette_from_mso97(dll: &[u8]) -> Result<[[u8; 3]; 256], ActError> {
    if dll.get(..2) != Some(b"MZ") {
        return Err(err("not a PE file"));
    }
    let pe = rd32(dll, 0x3C)? as usize;
    if dll.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(err("no PE signature"));
    }
    let nsec = usize::from(rd16(dll, pe + 6)?);
    let opt = pe + 24;
    let optsz = usize::from(rd16(dll, pe + 20)?);
    if rd16(dll, opt)? != 0x10B {
        return Err(err("not a PE32 image"));
    }
    let base = rd32(dll, opt + 28)?;
    let rva = MSO97_PALETTE_VA
        .checked_sub(base)
        .ok_or_else(|| err("unexpected image base"))?;
    let mut off = None;
    for k in 0..nsec {
        let s = opt + optsz + 40 * k;
        let vsize = rd32(dll, s + 8)?;
        let va = rd32(dll, s + 12)?;
        let raw = rd32(dll, s + 20)?;
        if rva >= va && rva - va < vsize {
            off = Some((raw + (rva - va)) as usize);
        }
    }
    let off = off.ok_or_else(|| err("palette address not in any section"))?;
    if rd16(dll, off)? != 0x300 || rd16(dll, off + 2)? != 256 {
        return Err(err(
            "no LOGPALETTE at the expected address (other MSO97 build?)",
        ));
    }
    let mut pal = [[0u8; 3]; 256];
    for (i, e) in pal.iter_mut().enumerate() {
        let p = off + 4 + 4 * i;
        let s = dll.get(p..p + 3).ok_or_else(|| err("truncated palette"))?;
        *e = [s[0], s[1], s[2]];
    }
    Ok(pal)
}
