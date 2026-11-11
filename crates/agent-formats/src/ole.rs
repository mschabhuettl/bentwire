//! Minimal read-only OLE compound file (MS-CFB) reader for Agent 1.5 `.acs` files.
//! Owner: formats teammate.

use crate::common::{Error, Guid, Result};

pub const SIGNATURE: [u8; 8] = [0xD0, 0xCF, 0x11, 0xE0, 0xA1, 0xB1, 0x1A, 0xE1];
const ENDOFCHAIN: u32 = 0xFFFF_FFFE;
const FREESECT: u32 = 0xFFFF_FFFF;
const NOSTREAM: u32 = 0xFFFF_FFFF;

/// A directory entry.
#[derive(Debug, Clone)]
pub struct Entry {
    pub name: String,
    /// 1 storage, 2 stream, 5 root.
    pub kind: u8,
    pub clsid: Guid,
    left: u32,
    right: u32,
    child: u32,
    start: u32,
    pub size: u64,
}

pub struct CompoundFile<'a> {
    data: &'a [u8],
    sector: usize,
    mini_sector: usize,
    cutoff: u64,
    fat: Vec<u32>,
    minifat: Vec<u32>,
    ministream: Vec<u8>,
    pub entries: Vec<Entry>,
}

fn err(what: impl Into<String>) -> Error {
    Error::invalid(format!("compound file: {}", what.into()), 0)
}

fn u16_at(b: &[u8], o: usize) -> u16 {
    u16::from_le_bytes([b[o], b[o + 1]])
}

fn u32_at(b: &[u8], o: usize) -> u32 {
    u32::from_le_bytes([b[o], b[o + 1], b[o + 2], b[o + 3]])
}

impl<'a> CompoundFile<'a> {
    pub fn parse(data: &'a [u8]) -> Result<Self> {
        if data.len() < 512 || data[..8] != SIGNATURE {
            return Err(err("no compound file signature"));
        }
        let h = &data[..512];
        let major = u16_at(h, 0x1A);
        let shift = u16_at(h, 0x1E);
        let mini_shift = u16_at(h, 0x20);
        if u16_at(h, 0x1C) != 0xFFFE
            || !matches!((major, shift), (3, 9) | (4, 12))
            || mini_shift != 6
        {
            return Err(err("unsupported header"));
        }
        let sector = 1usize << shift;
        let nsect = data.len().saturating_sub(sector) / sector;
        let nfat = u32_at(h, 0x2C) as usize;
        let dir_start = u32_at(h, 0x30);
        let cutoff = u32_at(h, 0x38) as u64;
        let minifat_start = u32_at(h, 0x3C);
        let nminifat = u32_at(h, 0x40);
        let mut difat_sect = u32_at(h, 0x44);
        let ndifat = u32_at(h, 0x48) as usize;
        if nfat > nsect {
            return Err(err("FAT larger than the file"));
        }
        let mut cf = CompoundFile {
            data,
            sector,
            mini_sector: 64,
            cutoff,
            fat: Vec::new(),
            minifat: Vec::new(),
            ministream: Vec::new(),
            entries: Vec::new(),
        };
        let mut fat_sects: Vec<u32> = (0..109).map(|i| u32_at(h, 0x4C + 4 * i)).collect();
        for _ in 0..ndifat.min(nsect) {
            if difat_sect == ENDOFCHAIN || difat_sect == FREESECT {
                break;
            }
            let s = cf.sector(difat_sect)?;
            let k = sector / 4 - 1;
            fat_sects.extend((0..k).map(|i| u32_at(s, 4 * i)));
            difat_sect = u32_at(s, 4 * k);
        }
        let fat_sects: Vec<u32> = fat_sects
            .into_iter()
            .filter(|&s| s != FREESECT)
            .take(nfat)
            .collect();
        if fat_sects.len() != nfat {
            return Err(err("DIFAT lists fewer FAT sectors than the header"));
        }
        let mut fat = Vec::with_capacity(nfat * sector / 4);
        for s in fat_sects {
            let b = cf.sector(s)?;
            fat.extend((0..sector / 4).map(|i| u32_at(b, 4 * i)));
        }
        cf.fat = fat;
        let dir = cf.chain_bytes(dir_start, false)?;
        for e in dir.as_chunks::<128>().0 {
            let nl = (u16_at(e, 64) as usize).min(64);
            let units: Vec<u16> = (0..nl.saturating_sub(2) / 2)
                .map(|i| u16_at(e, 2 * i))
                .collect();
            let mut clsid = [0; 16];
            clsid.copy_from_slice(&e[80..96]);
            let mut size = u32_at(e, 120) as u64 | (u32_at(e, 124) as u64) << 32;
            if major == 3 {
                size &= 0xFFFF_FFFF;
            }
            cf.entries.push(Entry {
                name: String::from_utf16_lossy(&units),
                kind: e[66],
                clsid: Guid(clsid),
                left: u32_at(e, 68),
                right: u32_at(e, 72),
                child: u32_at(e, 76),
                start: u32_at(e, 116),
                size,
            });
        }
        let root = cf
            .entries
            .first()
            .ok_or_else(|| err("empty directory"))?
            .clone();
        if root.kind != 5 {
            return Err(err("first directory entry is not the root"));
        }
        if nminifat > 0 {
            let mf = cf.chain_bytes(minifat_start, false)?;
            cf.minifat = mf
                .as_chunks::<4>()
                .0
                .iter()
                .map(|c| u32::from_le_bytes(*c))
                .collect();
        }
        if root.start != ENDOFCHAIN && root.size > 0 {
            let mut ms = cf.chain_bytes(root.start, false)?;
            if (ms.len() as u64) < root.size {
                return Err(err("mini stream shorter than declared"));
            }
            ms.truncate(root.size as usize);
            cf.ministream = ms;
        }
        Ok(cf)
    }

    fn sector(&self, n: u32) -> Result<&'a [u8]> {
        let o = (n as usize)
            .checked_add(1)
            .and_then(|k| k.checked_mul(self.sector));
        match o {
            Some(o) if o + self.sector <= self.data.len() => Ok(&self.data[o..o + self.sector]),
            _ => Err(err(format!("sector {n:#x} beyond the end of the file"))),
        }
    }

    /// Bytes of the chain starting at `start` in the FAT (or the mini FAT).
    fn chain_bytes(&self, start: u32, mini: bool) -> Result<Vec<u8>> {
        let fat = if mini { &self.minifat } else { &self.fat };
        // a chain longer than the number of addressable sectors must contain a cycle
        let max = if mini {
            self.ministream.len() / self.mini_sector
        } else {
            self.data.len().saturating_sub(self.sector) / self.sector
        };
        let mut out = Vec::new();
        let mut s = start;
        let mut n = 0;
        while s != ENDOFCHAIN {
            n += 1;
            if s as usize >= fat.len() || n > max {
                return Err(err(format!("bad sector chain at {s:#x}")));
            }
            if mini {
                let o = s as usize * self.mini_sector;
                let b = self
                    .ministream
                    .get(o..o + self.mini_sector)
                    .ok_or_else(|| err("mini sector out of range"))?;
                out.extend_from_slice(b);
            } else {
                out.extend_from_slice(self.sector(s)?);
            }
            s = fat[s as usize];
        }
        Ok(out)
    }

    /// Contents of stream entry `e`.
    pub fn stream(&self, e: &Entry) -> Result<Vec<u8>> {
        if e.size > self.data.len() as u64 {
            return Err(err(format!("stream {:?} larger than the file", e.name)));
        }
        if e.size == 0 {
            return Ok(Vec::new());
        }
        let mut b = self.chain_bytes(e.start, e.size < self.cutoff)?;
        if (b.len() as u64) < e.size {
            return Err(err(format!("stream {:?} shorter than its size", e.name)));
        }
        b.truncate(e.size as usize);
        Ok(b)
    }

    /// The direct children of the root storage, in tree order.
    pub fn root_children(&self) -> Result<Vec<&Entry>> {
        let mut out = Vec::new();
        let mut seen = vec![false; self.entries.len()];
        let mut stack = vec![self.entries[0].child];
        while let Some(i) = stack.pop() {
            if i == NOSTREAM {
                continue;
            }
            let e = self
                .entries
                .get(i as usize)
                .ok_or_else(|| err("bad directory link"))?;
            if std::mem::replace(&mut seen[i as usize], true) {
                return Err(err("directory tree has a cycle"));
            }
            out.push(e);
            stack.push(e.left);
            stack.push(e.right);
        }
        Ok(out)
    }

    /// A root-level stream by name, case-insensitively (as OLE compares names).
    pub fn root_stream(&self, name: &str) -> Result<Option<Vec<u8>>> {
        let found = self
            .root_children()?
            .into_iter()
            .find(|e| e.kind == 2 && e.name.eq_ignore_ascii_case(name));
        found.map(|e| self.stream(e)).transpose()
    }
}
