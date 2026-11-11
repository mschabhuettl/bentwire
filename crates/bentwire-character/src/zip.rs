//! Minimal ZIP reader and writer for `.bwc` (character-format 3): methods 0 and 8 only, no
//! encryption, no ZIP64, UTF-8 names. The writer is reproducible: entries in the given order,
//! one fixed timestamp, no extra fields.

use std::io::{Read, Write};

use crate::Error;

fn bad(msg: impl Into<String>) -> Error {
    Error::Zip(msg.into())
}

fn u16_at(b: &[u8], p: usize) -> Result<u16, Error> {
    b.get(p..p + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| bad("truncated"))
}

fn u32_at(b: &[u8], p: usize) -> Result<u32, Error> {
    b.get(p..p + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| bad("truncated"))
}

/// One entry of an archive held in memory.
#[derive(Clone, Debug)]
pub struct Entry {
    pub name: String,
    /// 0 stored, 8 deflated.
    pub method: u16,
    data_start: usize,
    comp_size: usize,
    size: usize,
    crc: u32,
}

/// An archive read fully into memory; entries are decoded on demand.
pub struct Archive {
    bytes: Vec<u8>,
    pub entries: Vec<Entry>,
}

impl Archive {
    pub fn new(bytes: Vec<u8>) -> Result<Self, Error> {
        // End of central directory: last 22..22+65535 bytes.
        let n = bytes.len();
        if n < 22 {
            return Err(bad("too short for a ZIP"));
        }
        let lo = n.saturating_sub(22 + 65535);
        let eocd = (lo..=n - 22)
            .rev()
            .find(|&p| bytes[p..p + 4] == [0x50, 0x4b, 0x05, 0x06])
            .ok_or_else(|| bad("no end of central directory"))?;
        let count = u16_at(&bytes, eocd + 10)? as usize;
        let cd_size = u32_at(&bytes, eocd + 12)?;
        let cd_off = u32_at(&bytes, eocd + 16)?;
        if count == 0xFFFF || cd_size == 0xFFFF_FFFF || cd_off == 0xFFFF_FFFF {
            return Err(bad("ZIP64 is not allowed"));
        }
        let mut p = cd_off as usize;
        let mut entries = Vec::with_capacity(count);
        for _ in 0..count {
            if u32_at(&bytes, p)? != 0x0201_4b50 {
                return Err(bad("bad central directory entry"));
            }
            let flags = u16_at(&bytes, p + 8)?;
            let method = u16_at(&bytes, p + 10)?;
            let crc = u32_at(&bytes, p + 16)?;
            let comp_size = u32_at(&bytes, p + 20)?;
            let size = u32_at(&bytes, p + 24)?;
            let name_len = u16_at(&bytes, p + 28)? as usize;
            let extra_len = u16_at(&bytes, p + 30)? as usize;
            let comment_len = u16_at(&bytes, p + 32)? as usize;
            let local = u32_at(&bytes, p + 42)?;
            if comp_size == 0xFFFF_FFFF || size == 0xFFFF_FFFF || local == 0xFFFF_FFFF {
                return Err(bad("ZIP64 is not allowed"));
            }
            if flags & 1 != 0 || flags & 0x40 != 0 {
                return Err(bad("encrypted entries are not allowed"));
            }
            let raw = bytes
                .get(p + 46..p + 46 + name_len)
                .ok_or_else(|| bad("truncated"))?;
            let name = std::str::from_utf8(raw)
                .map_err(|_| bad("entry name is not UTF-8"))?
                .to_owned();
            if method != 0 && method != 8 {
                return Err(bad(format!(
                    "{name}: compression method {method} not allowed"
                )));
            }
            let l = local as usize;
            if u32_at(&bytes, l)? != 0x0403_4b50 {
                return Err(bad(format!("{name}: bad local header")));
            }
            let lname = u16_at(&bytes, l + 26)? as usize;
            let lextra = u16_at(&bytes, l + 28)? as usize;
            let data_start = l + 30 + lname + lextra;
            if data_start + comp_size as usize > n {
                return Err(bad(format!("{name}: truncated data")));
            }
            entries.push(Entry {
                name,
                method,
                data_start,
                comp_size: comp_size as usize,
                size: size as usize,
                crc,
            });
            p += 46 + name_len + extra_len + comment_len;
        }
        Ok(Archive { bytes, entries })
    }

    pub fn entry(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name == name)
    }

    /// Decoded bytes of entry `name`, CRC checked.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, Error> {
        let e = self
            .entry(name)
            .ok_or_else(|| Error::MissingFile(name.to_owned()))?;
        let raw = &self.bytes[e.data_start..e.data_start + e.comp_size];
        let out = if e.method == 0 {
            raw.to_vec()
        } else {
            let mut v = Vec::with_capacity(e.size);
            flate2::read::DeflateDecoder::new(raw)
                .take(e.size as u64 + 1)
                .read_to_end(&mut v)
                .map_err(|x| bad(format!("{name}: {x}")))?;
            v
        };
        if out.len() != e.size || crc32fast::hash(&out) != e.crc {
            return Err(bad(format!("{name}: size or CRC mismatch")));
        }
        Ok(out)
    }
}

/// Fixed DOS timestamp for every entry: 1980-01-01 00:00:00.
const DOS_TIME: u16 = 0;
const DOS_DATE: u16 = (1 << 5) | 1;

/// Write `entries` (name, data, deflate?) as a ZIP, in the given order.
pub fn write(entries: &[(&str, &[u8], bool)]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data, deflate) in entries {
        let crc = crc32fast::hash(data);
        let (method, body): (u16, Vec<u8>) = if *deflate {
            let mut enc =
                flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::best());
            enc.write_all(data).map_err(|e| bad(e.to_string()))?;
            (8, enc.finish().map_err(|e| bad(e.to_string()))?)
        } else {
            (0, data.to_vec())
        };
        let too_big = |x: usize| x >= 0xFFFF_FFFF;
        if too_big(out.len()) || too_big(data.len()) || too_big(body.len()) {
            return Err(bad("entry too large for a ZIP without ZIP64"));
        }
        let offset = out.len() as u32;
        let nb = name.as_bytes();
        // Local header.
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes()); // version needed
        out.extend_from_slice(&0x0800u16.to_le_bytes()); // UTF-8 names
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&DOS_TIME.to_le_bytes());
        out.extend_from_slice(&DOS_DATE.to_le_bytes());
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(body.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(nb.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(nb);
        out.extend_from_slice(&body);
        // Central directory entry.
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&0x031Eu16.to_le_bytes()); // made by: Unix, 3.0
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&0x0800u16.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&DOS_TIME.to_le_bytes());
        central.extend_from_slice(&DOS_DATE.to_le_bytes());
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(body.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(nb.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 8]); // extra, comment, disk, internal attributes
        central.extend_from_slice(&(0o100644u32 << 16).to_le_bytes()); // external: regular file
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(nb);
    }
    if entries.len() >= 0xFFFF {
        return Err(bad("too many entries for a ZIP without ZIP64"));
    }
    let cd_off = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(entries.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_off.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zip_round_trip_and_rules() {
        let z = write(&[
            ("character.toml", b"format = 1\n", true),
            ("a/b.png", b"xyz", false),
        ])
        .unwrap();
        let a = Archive::new(z.clone()).unwrap();
        assert_eq!(a.entries.len(), 2);
        assert_eq!(a.entry("character.toml").unwrap().method, 8);
        assert_eq!(a.entry("a/b.png").unwrap().method, 0);
        assert_eq!(a.read("character.toml").unwrap(), b"format = 1\n");
        assert_eq!(a.read("a/b.png").unwrap(), b"xyz");
        assert_eq!(
            z,
            write(&[
                ("character.toml", b"format = 1\n", true),
                ("a/b.png", b"xyz", false)
            ])
            .unwrap()
        );
        // Corrupt the stored entry: CRC must catch it.
        let mut c = z.clone();
        let pos = c.windows(3).position(|w| w == b"xyz").unwrap();
        c[pos] = b'q';
        assert!(Archive::new(c).unwrap().read("a/b.png").is_err());
        assert!(Archive::new(b"not a zip at all, definitely not".to_vec()).is_err());
    }
}
