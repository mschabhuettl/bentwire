//! Writer for synthetic Agent 1.5 OLE `.acs` files: a version 3 compound file (512-byte
//! sectors, mini stream for streams below 4096 bytes) with `char.acf` and one stream per
//! animation (agent15-format 4.2, 4.3). Owner: formats teammate.

use crate::acf::Acf;
use crate::common::{Compression, Out, lz_compress};

const SECT: usize = 512;
const MINI: usize = 64;
const CUTOFF: usize = 4096;
const END: u32 = 0xFFFF_FFFE;
const FREE: u32 = 0xFFFF_FFFF;
const FATSECT: u32 = 0xFFFF_FFFD;

/// The `char.acf` stream: magic 0xABCDABC1 and the compressed ACF body.
pub fn char_acf(acf: &Acf) -> Vec<u8> {
    let body = acf.body();
    let z = lz_compress(&body, true);
    let mut o = Out::default();
    o.u32(0xABCD_ABC1);
    o.u32(body.len() as u32);
    o.u32(z.len() as u32);
    o.bytes(&z);
    o.0
}

/// The character as an OLE file: `char.acf` plus `animations[i].file_name` streams.
pub fn write_ole(acf: &Acf) -> Vec<u8> {
    let mut streams = vec![("char.acf".to_string(), char_acf(acf))];
    for (i, a) in acf.animations.iter().enumerate() {
        streams.push((a.file_name.clone(), acf.write_aca(i)));
    }
    compound_file(&streams, [0; 16])
}

/// [`crate::acf::sample`] with OLE stream names `anim1.aaf`, `anim2.aaf`, ...
pub fn sample(version: u32) -> Acf {
    let mut s = crate::acf::sample(version);
    for (i, a) in s.animations.iter_mut().enumerate() {
        a.file_name = format!("anim{}.aaf", i + 1);
        if a.compression == Compression::Raw {
            a.compression = Compression::Lz;
        }
    }
    s
}

/// A compound file whose root holds `streams` (name, bytes), in that order as a chain of
/// right siblings.
pub fn compound_file(streams: &[(String, Vec<u8>)], root_clsid: [u8; 16]) -> Vec<u8> {
    let mut fat: Vec<u32> = Vec::new();
    let mut sectors: Vec<Vec<u8>> = Vec::new();
    let mut alloc = |data: &[u8], fat: &mut Vec<u32>| -> u32 {
        if data.is_empty() {
            return END;
        }
        let first = sectors.len() as u32;
        for (k, c) in data.chunks(SECT).enumerate() {
            let mut s = c.to_vec();
            s.resize(SECT, 0);
            sectors.push(s);
            let last = (k + 1) * SECT >= data.len();
            fat.push(if last { END } else { first + k as u32 + 1 });
        }
        first
    };
    // mini stream and mini FAT
    let mut ministream = Vec::new();
    let mut minifat: Vec<u32> = Vec::new();
    let mut starts = Vec::new();
    for (_, d) in streams {
        if d.len() < CUTOFF && !d.is_empty() {
            let first = (ministream.len() / MINI) as u32;
            let n = d.len().div_ceil(MINI);
            for k in 0..n {
                minifat.push(if k + 1 == n {
                    END
                } else {
                    first + k as u32 + 1
                });
            }
            ministream.extend_from_slice(d);
            ministream.resize((first as usize + n) * MINI, 0);
            starts.push(first);
        } else {
            starts.push(u32::MAX); // filled below
        }
    }
    for (i, (_, d)) in streams.iter().enumerate() {
        if d.len() >= CUTOFF {
            starts[i] = alloc(d, &mut fat);
        } else if d.is_empty() {
            starts[i] = END;
        }
    }
    let ministream_start = alloc(&ministream, &mut fat);
    let mf: Vec<u8> = minifat.iter().flat_map(|v| v.to_le_bytes()).collect();
    let minifat_start = alloc(&mf, &mut fat);
    // directory: root + streams, 4 entries per sector
    let mut dir = Out::default();
    let entry = |o: &mut Out,
                 name: &str,
                 kind: u8,
                 right: u32,
                 child: u32,
                 clsid: [u8; 16],
                 start: u32,
                 size: u64| {
        let units: Vec<u16> = name.encode_utf16().collect();
        let mut nb = [0u8; 64];
        for (i, u) in units.iter().enumerate().take(31) {
            nb[2 * i..2 * i + 2].copy_from_slice(&u.to_le_bytes());
        }
        o.bytes(&nb);
        o.u16(((units.len().min(31) + 1) * 2) as u16);
        o.u8(kind);
        o.u8(1); // black
        o.u32(FREE); // left
        o.u32(right);
        o.u32(child);
        o.bytes(&clsid);
        o.u32(0); // state bits
        o.bytes(&[0; 16]); // times
        o.u32(start);
        o.u32(size as u32);
        o.u32(0);
    };
    entry(
        &mut dir,
        "Root Entry",
        5,
        FREE,
        if streams.is_empty() { FREE } else { 1 },
        root_clsid,
        ministream_start,
        ministream.len() as u64,
    );
    for (i, (name, d)) in streams.iter().enumerate() {
        let right = if i + 1 < streams.len() {
            i as u32 + 2
        } else {
            FREE
        };
        entry(
            &mut dir,
            name,
            2,
            right,
            FREE,
            [0; 16],
            starts[i],
            d.len() as u64,
        );
    }
    while dir.len() % SECT != 0 {
        // unused entries: empty name, type 0, no links
        let mut e = vec![0u8; 128];
        e[68..80].fill(0xFF);
        dir.bytes(&e);
    }
    let dir_start = alloc(&dir.0, &mut fat);
    // FAT sectors at the end; enough of them to cover themselves
    let mut nfat = 1;
    while nfat * (SECT / 4) < sectors.len() + nfat {
        nfat += 1;
    }
    assert!(nfat <= 109, "fixture too large for a header-only DIFAT");
    let fat_first = sectors.len() as u32;
    fat.extend(std::iter::repeat_n(FATSECT, nfat));
    fat.resize(nfat * SECT / 4, FREE);
    let fat_bytes: Vec<u8> = fat.iter().flat_map(|v| v.to_le_bytes()).collect();
    for c in fat_bytes.chunks(SECT) {
        sectors.push(c.to_vec());
    }
    let mut h = Out::default();
    h.bytes(&[0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1]);
    h.bytes(&[0; 16]);
    h.u16(0x3E);
    h.u16(3);
    h.u16(0xFFFE);
    h.u16(9);
    h.u16(6);
    h.bytes(&[0; 6]);
    h.u32(0); // directory sectors (0 for version 3)
    h.u32(nfat as u32);
    h.u32(dir_start);
    h.u32(0);
    h.u32(CUTOFF as u32);
    h.u32(minifat_start);
    h.u32(minifat.len().div_ceil(SECT / 4) as u32);
    h.u32(END); // DIFAT start
    h.u32(0);
    for i in 0..109 {
        h.u32(if i < nfat { fat_first + i as u32 } else { FREE });
    }
    let mut out = h.0;
    for s in sectors {
        out.extend_from_slice(&s);
    }
    out
}
