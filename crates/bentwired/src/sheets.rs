//! Cache of decoded sheet files (B-CHAR-7): one entry per sheet file in
//! `$XDG_CACHE_HOME/bentwire/sheets/`, mapped read-only instead of decoding the PNG again.
//!
//! Entry: magic, version, key, width, height, kind (0 RGBA, 1 indexed), PLTE entry count,
//! tRNS length (0xFFFF: none), then PLTE, tRNS and the pixels exactly as `decode_png` gave them.

use bentwire_character::Error;
use bentwire_character::image::{self, Bytes, Image, Pixels};
use sha2::{Digest, Sha256};
use std::io::{ErrorKind, Write};
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::SystemTime;

/// Bumped when the entry layout or the output of `decode_png` changes.
const VERSION: u32 = 1;
const MAGIC: &[u8; 8] = b"BWSHEET\0";
const HEADER: usize = 8 + 4 + 32 + 4 + 4 + 1 + 2 + 2;
/// The cache is trimmed to this size after each write, least recently used entries first.
const LIMIT: u64 = 512 << 20;

static WARNED: AtomicBool = AtomicBool::new(false);
static TMP_SEQ: AtomicU64 = AtomicU64::new(0);

#[derive(Clone)]
pub struct SheetCache {
    pub dir: PathBuf,
}

/// A mapped cache entry from byte `.1` on: the pixels.
struct Mapped(memmap2::Mmap, usize);

impl AsRef<[u8]> for Mapped {
    fn as_ref(&self) -> &[u8] {
        &self.0[self.1..]
    }
}

impl SheetCache {
    /// The decoded sheet file `file` (bytes `png`) of the character at `source`: the cache
    /// entry when it is valid, else `decode_png`, written to the cache in the background.
    pub fn decode(&self, source: &Path, file: &str, png: &[u8]) -> Result<Image, Error> {
        let key = key(source, file, png);
        let path = self.entry(&key);
        match read(&path, &key) {
            Ok(img) => {
                // Recently used for trim(); best effort.
                let _ = std::fs::File::options()
                    .write(true)
                    .open(&path)
                    .and_then(|f| f.set_modified(SystemTime::now()));
                return Ok(img);
            }
            Err(e) if e.kind() != ErrorKind::NotFound => {
                log::debug!("sheet cache {}: {e}; decoding", path.display());
            }
            Err(_) => {}
        }
        let img = image::decode_png(png)?;
        let (cache, copy) = (self.clone(), img.clone());
        std::thread::spawn(move || cache.store(&key, &copy));
        Ok(img)
    }

    fn entry(&self, key: &[u8; 32]) -> PathBuf {
        let hex: String = key.iter().map(|b| format!("{b:02x}")).collect();
        self.dir.join(format!("{hex}.sheet"))
    }

    /// Write the entry for `key` (temp file, then rename) and trim the cache. An unwritable
    /// cache only logs, once per process; a file larger than the whole cache is not kept.
    fn store(&self, key: &[u8; 32], img: &Image) {
        let (Pixels::Indexed { data, .. } | Pixels::Rgba(data)) = &img.pixels;
        if data.len() as u64 > LIMIT {
            return;
        }
        let path = self.entry(key);
        match write(&self.dir, &path, key, img) {
            Ok(()) => trim(&self.dir, &path, LIMIT),
            Err(e) => {
                if !WARNED.swap(true, Ordering::Relaxed) {
                    log::warn!("sheet cache {}: {e}; not caching", self.dir.display());
                }
            }
        }
    }
}

/// Version, character path, file name, length and CRC-32 of the PNG bytes: a changed file
/// gets another key, so an entry never goes stale.
fn key(source: &Path, file: &str, png: &[u8]) -> [u8; 32] {
    let mut h = Sha256::new();
    h.update(VERSION.to_le_bytes());
    h.update(source.as_os_str().as_bytes());
    h.update([0]);
    h.update(file.as_bytes());
    h.update([0]);
    h.update((png.len() as u64).to_le_bytes());
    h.update(crc32fast::hash(png).to_le_bytes());
    h.finalize().into()
}

fn invalid(msg: &str) -> std::io::Error {
    std::io::Error::new(ErrorKind::InvalidData, msg)
}

/// A cache entry, checked against `key` and its own header (dimensions, exact length).
fn read(path: &Path, key: &[u8; 32]) -> std::io::Result<Image> {
    let f = std::fs::File::open(path)?;
    // SAFETY: entries are never changed in place: written to a temp file and renamed,
    // removed only by unlink, so a live mapping keeps its file.
    let map = unsafe { memmap2::Mmap::map(&f)? };
    if map.len() < HEADER || &map[..8] != MAGIC {
        return Err(invalid("not a sheet cache entry"));
    }
    let u16_at = |p: usize| u16::from_le_bytes([map[p], map[p + 1]]) as usize;
    let u32_at = |p: usize| u32::from_le_bytes(map[p..p + 4].try_into().unwrap());
    if u32_at(8) != VERSION || map[12..44] != key[..] {
        return Err(invalid("other version or key"));
    }
    let (w, h, kind) = (u32_at(44), u32_at(48), map[52]);
    let (plte_n, trns_n) = (u16_at(53), u16_at(55));
    let trns_len = if trns_n == 0xFFFF { 0 } else { trns_n };
    let (iw, ih) = (w as u64, h as u64);
    if w == 0
        || h == 0
        || iw > image::MAX_FILE_SIDE
        || ih > image::MAX_FILE_SIDE
        || iw * ih > image::MAX_FILE_PIXELS
        || kind > 1
        || plte_n > 256
        || trns_len > 256
        || (kind == 0 && (plte_n != 0 || trns_n != 0xFFFF))
    {
        return Err(invalid("bad header"));
    }
    let start = HEADER + 3 * plte_n + trns_len;
    let px = (iw * ih) as usize * if kind == 1 { 1 } else { 4 };
    if map.len() != start + px {
        return Err(invalid("wrong length"));
    }
    let plte: Vec<[u8; 3]> = map[HEADER..HEADER + 3 * plte_n].as_chunks::<3>().0.to_vec();
    let trns = (trns_n != 0xFFFF).then(|| map[start - trns_len..start].to_vec());
    let data = Bytes(Arc::new(Mapped(map, start)));
    let pixels = if kind == 1 {
        Pixels::Indexed {
            data,
            table: image::palette_table(&plte, trns.as_deref()),
            plte,
            trns,
        }
    } else {
        Pixels::Rgba(data)
    };
    Ok(Image {
        width: w,
        height: h,
        pixels,
    })
}

fn write(dir: &Path, path: &Path, key: &[u8; 32], img: &Image) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)?;
    let (kind, data) = match &img.pixels {
        Pixels::Indexed { data, .. } => (1, data),
        Pixels::Rgba(data) => (0, data),
    };
    let (plte, trns) = match &img.pixels {
        Pixels::Indexed { plte, trns, .. } => (&plte[..], trns.as_deref()),
        Pixels::Rgba(_) => (&[][..], None),
    };
    let mut head = Vec::new();
    head.extend_from_slice(MAGIC);
    head.extend_from_slice(&VERSION.to_le_bytes());
    head.extend_from_slice(key);
    head.extend_from_slice(&img.width.to_le_bytes());
    head.extend_from_slice(&img.height.to_le_bytes());
    head.push(kind);
    head.extend_from_slice(&(plte.len() as u16).to_le_bytes());
    head.extend_from_slice(&trns.map_or(0xFFFF, |t| t.len() as u16).to_le_bytes());
    head.extend(plte.iter().flatten());
    head.extend_from_slice(trns.unwrap_or_default());
    let seq = TMP_SEQ.fetch_add(1, Ordering::Relaxed);
    let tmp = path.with_extension(format!("{}-{seq}.tmp", std::process::id()));
    let r = std::fs::File::create(&tmp).and_then(|mut f| {
        f.write_all(&head)?;
        f.write_all(data)?;
        f.sync_all()?;
        std::fs::rename(&tmp, path)
    });
    if r.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    r
}

/// Remove the least recently used entries of `dir` (oldest modification time) until at most
/// `limit` bytes remain; never `keep`, nor the temp files of writes in progress.
fn trim(dir: &Path, keep: &Path, limit: u64) {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return;
    };
    let mut files: Vec<(SystemTime, u64, PathBuf)> = rd
        .flatten()
        .filter_map(|e| {
            let entry = e.path().extension() == Some("sheet".as_ref());
            let m = e.metadata().ok().filter(|m| m.is_file() && entry)?;
            Some((m.modified().ok()?, m.len(), e.path()))
        })
        .collect();
    let mut total: u64 = files.iter().map(|f| f.1).sum();
    files.sort();
    for (_, len, p) in files {
        if total <= limit {
            break;
        }
        if p != keep && std::fs::remove_file(&p).is_ok() {
            total -= len;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentwire_character::write::{encode_png_indexed, encode_png_rgba};

    /// `colour` changes one palette entry: same file size, other pixels.
    fn indexed_png(colour: u8) -> Vec<u8> {
        let idx: Vec<u8> = (0..12 * 5).map(|i| (i % 3) as u8).collect();
        let plte = [[255, 0, 255], [10, 20, colour], [200, 100, 50]];
        encode_png_indexed(12, 5, &idx, &plte, Some(&[0, 255])).unwrap()
    }

    fn rgba_png() -> Vec<u8> {
        let px: Vec<u8> = (0..7 * 3 * 4).map(|i| (i * 37) as u8).collect();
        encode_png_rgba(7, 3, &px).unwrap()
    }

    fn same(a: &Image, b: &Image) {
        assert_eq!((a.width, a.height), (b.width, b.height));
        match (&a.pixels, &b.pixels) {
            (
                Pixels::Indexed {
                    data: d1,
                    plte: p1,
                    trns: t1,
                    table: x1,
                },
                Pixels::Indexed {
                    data: d2,
                    plte: p2,
                    trns: t2,
                    table: x2,
                },
            ) => assert_eq!((&d1[..], p1, t1, x1), (&d2[..], p2, t2, x2)),
            (Pixels::Rgba(d1), Pixels::Rgba(d2)) => assert_eq!(&d1[..], &d2[..]),
            _ => panic!("pixel kinds differ"),
        }
    }

    /// The background write of `decode`, waited for: the entry of `png` reads back.
    fn wait_entry(c: &SheetCache, src: &Path, file: &str, png: &[u8]) -> Image {
        let k = key(src, file, png);
        for _ in 0..500 {
            if let Ok(img) = read(&c.entry(&k), &k) {
                return img;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        panic!("no cache entry for {file}");
    }

    fn cache() -> SheetCache {
        SheetCache {
            dir: crate::config::tests::tempdir().join("sheets"),
        }
    }

    #[test]
    fn b_char_7_entries_round_trip() {
        let c = cache();
        let src = Path::new("/x/a.bwc");
        for (file, png) in [
            ("sheets/i.png", indexed_png(0)),
            ("sheets/r.png", rgba_png()),
        ] {
            let fresh = image::decode_png(&png).unwrap();
            same(&c.decode(src, file, &png).unwrap(), &fresh);
            same(&wait_entry(&c, src, file, &png), &fresh);
            same(&c.decode(src, file, &png).unwrap(), &fresh);
        }
    }

    #[test]
    fn b_char_7_a_changed_file_never_uses_the_old_entry() {
        let c = cache();
        let src = Path::new("/x/a.bwc");
        let (old, new) = (indexed_png(0), indexed_png(1));
        assert_eq!(old.len(), new.len(), "same size, other pixels");
        c.decode(src, "s.png", &old).unwrap();
        wait_entry(&c, src, "s.png", &old);
        same(
            &c.decode(src, "s.png", &new).unwrap(),
            &image::decode_png(&new).unwrap(),
        );
        // Every key part counts.
        let k = key(src, "s.png", &old);
        assert_ne!(k, key(src, "s.png", &new));
        assert_ne!(k, key(Path::new("/x/b.bwc"), "s.png", &old));
        assert_ne!(k, key(src, "t.png", &old));
    }

    #[test]
    fn b_char_7_a_corrupt_entry_is_decoded_again_and_replaced() {
        let c = cache();
        let src = Path::new("/x/a.bwc");
        let png = indexed_png(0);
        let fresh = image::decode_png(&png).unwrap();
        c.decode(src, "s.png", &png).unwrap();
        wait_entry(&c, src, "s.png", &png);
        let path = c.entry(&key(src, "s.png", &png));
        let good = std::fs::read(&path).unwrap();
        let flip = |at: usize| {
            let mut b = good.clone();
            b[at] ^= 1;
            b
        };
        let mut longer = good.clone();
        longer.push(0);
        let corrupt = [
            Vec::new(),
            good[..good.len() - 1].to_vec(),
            good[..HEADER - 1].to_vec(),
            vec![0; good.len()],
            flip(8),  // version
            flip(20), // key
            flip(44), // width
            longer,
        ];
        for (n, bytes) in corrupt.iter().enumerate() {
            std::fs::write(&path, bytes).unwrap();
            same(&c.decode(src, "s.png", &png).unwrap(), &fresh);
            same(&wait_entry(&c, src, "s.png", &png), &fresh);
            assert_eq!(std::fs::read(&path).unwrap(), good, "case {n} replaced");
        }
    }

    #[test]
    fn b_char_7_an_unwritable_cache_dir_only_skips_caching() {
        let d = crate::config::tests::tempdir();
        std::fs::write(d.join("file"), "").unwrap();
        let c = SheetCache {
            dir: d.join("file/sheets"),
        };
        let png = rgba_png();
        let img = c.decode(Path::new("/x"), "s.png", &png).unwrap();
        same(&img, &image::decode_png(&png).unwrap());
        c.store(&key(Path::new("/x"), "s.png", &png), &img);
        assert!(!c.dir.exists());
    }

    #[test]
    fn b_char_7_trim_removes_the_least_recently_used() {
        let d = crate::config::tests::tempdir();
        let t0 = SystemTime::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000);
        // Oldest first; a temp file of a write in progress is never removed.
        let names = ["old.tmp", "c.sheet", "a.sheet", "keep.sheet", "b.sheet"];
        for (n, name) in names.iter().enumerate() {
            let p = d.join(name);
            std::fs::write(&p, [0; 100]).unwrap();
            let f = std::fs::File::options().write(true).open(&p).unwrap();
            f.set_modified(t0 + std::time::Duration::from_secs(n as u64))
                .unwrap();
        }
        trim(&d, &d.join("keep.sheet"), 250);
        let mut left: Vec<String> = std::fs::read_dir(&d)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        left.sort();
        assert_eq!(left, ["b.sheet", "keep.sheet", "old.tmp"]);
    }

    /// Composed frames of the builtin character are the same through a cold and a warm cache.
    #[test]
    fn b_char_7_cached_character_composes_the_same() {
        use bentwire_character::Character;
        let root = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../characters/builtin/bentwire-placeholder");
        let c = cache();
        let plain = Character::load(&root).unwrap();
        let load = || Character::load_with(&root, &mut |f, png| c.decode(&root, f, png)).unwrap();
        let cold = load();
        for s in &plain.manifest.sheets {
            let png = std::fs::read(root.join(&s.file)).unwrap();
            wait_entry(&c, &root, &s.file, &png);
        }
        let warm = load();
        for a in &plain.manifest.animations {
            for f in &a.frames {
                let want = plain.compose_frame(f, None, 2);
                assert_eq!(cold.compose_frame(f, None, 2), want, "{}", a.name);
                assert_eq!(warm.compose_frame(f, None, 2), want, "{}", a.name);
            }
        }
    }
}
