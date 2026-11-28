//! Writer: manifest text, PNG and FLAC encoding, folder and `.bwc` output. Everything is
//! deterministic: fixed key order, fixed PNG/FLAC/deflate settings, fixed ZIP timestamps.

use std::fmt::Write as _;
use std::path::Path;

use crate::model::*;
use crate::{Error, Pcm};

/// TOML basic string.
pub fn tstr(s: &str) -> String {
    let mut o = String::with_capacity(s.len() + 2);
    o.push('"');
    for c in s.chars() {
        match c {
            '"' => o.push_str("\\\""),
            '\\' => o.push_str("\\\\"),
            '\n' => o.push_str("\\n"),
            '\t' => o.push_str("\\t"),
            '\r' => o.push_str("\\r"),
            c if (c as u32) < 0x20 || c as u32 == 0x7f => {
                let _ = write!(o, "\\u{:04X}", c as u32);
            }
            c => o.push(c),
        }
    }
    o.push('"');
    o
}

/// TOML key: bare when possible, quoted otherwise.
pub fn tkey(k: &str) -> String {
    if !k.is_empty()
        && k.bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
    {
        k.to_owned()
    } else {
        tstr(k)
    }
}

fn tvalue(v: &toml::Value) -> String {
    match v {
        toml::Value::String(s) => tstr(s),
        toml::Value::Integer(i) => i.to_string(),
        toml::Value::Float(f) if f.is_nan() => "nan".into(),
        toml::Value::Float(f) if f.is_infinite() => (if *f > 0.0 { "inf" } else { "-inf" }).into(),
        toml::Value::Float(f) => {
            let s = f.to_string();
            if s.contains('.') || s.contains('e') || s.contains("inf") || s.contains("nan") {
                s
            } else {
                format!("{s}.0")
            }
        }
        toml::Value::Boolean(b) => b.to_string(),
        toml::Value::Datetime(d) => d.to_string(),
        toml::Value::Array(a) => {
            format!("[{}]", a.iter().map(tvalue).collect::<Vec<_>>().join(", "))
        }
        toml::Value::Table(t) => {
            if t.is_empty() {
                return "{}".into();
            }
            let inner: Vec<String> = t
                .iter()
                .map(|(k, v)| format!("{} = {}", tkey(k), tvalue(v)))
                .collect();
            format!("{{ {} }}", inner.join(", "))
        }
    }
}

fn layer(l: &Layer) -> String {
    let r = tstr(&format!("{}:{}", l.image.sheet, l.image.index));
    match l.size {
        Some([w, h]) => format!("[{r}, {}, {}, {w}, {h}]", l.x, l.y),
        None if l.x == 0 && l.y == 0 => r,
        None => format!("[{r}, {}, {}]", l.x, l.y),
    }
}

fn layers(ls: &[Layer]) -> String {
    format!("[{}]", ls.iter().map(layer).collect::<Vec<_>>().join(", "))
}

fn branch(b: &Branch) -> String {
    match *b {
        Branch::Random { target, p } => format!("[{target}, {p}]"),
        Branch::Daypart { target, value } => format!("[{target}, \"daypart\", {value}]"),
        Branch::Host { target, value } => format!("[{target}, \"host\", {value}]"),
    }
}

fn frame(f: &Frame) -> String {
    let mut p = vec![format!("ms = {}", f.ms)];
    if let Some(pose) = f.pose {
        p.push(format!("pose = {pose}"));
    } else if !f.layers.is_empty() {
        p.push(format!("layers = {}", layers(&f.layers)));
    }
    if let Some(s) = &f.sound {
        p.push(format!("sound = {}", tstr(s)));
    }
    if let Some(a) = f.sound_arg {
        p.push(format!("sound_arg = {a}"));
    }
    match f.exit {
        Some(Exit::End) => p.push("exit = \"end\"".into()),
        Some(Exit::Frame(i)) => p.push(format!("exit = {i}")),
        None => {}
    }
    if !f.branch.is_empty() {
        let b: Vec<String> = f.branch.iter().map(branch).collect();
        p.push(format!("branch = [{}]", b.join(", ")));
    }
    if !f.mouths.is_empty() {
        let ms: Vec<String> = f
            .mouths
            .iter()
            .map(|m| {
                let mut q = vec![
                    format!("type = {}", m.kind),
                    format!("layer = {}", layer(&m.layer)),
                ];
                if let Some([x, y]) = m.anchor {
                    q.push(format!("anchor = [{x}, {y}]"));
                }
                if let Some(b) = &m.base {
                    q.push(format!("base = {}", layers(b)));
                }
                format!("{{ {} }}", q.join(", "))
            })
            .collect();
        p.push(format!("mouths = [{}]", ms.join(", ")));
    }
    format!("{{ {} }}", p.join(", "))
}

macro_rules! opt {
    ($o:expr, $t:expr, $name:literal, str) => {
        if let Some(v) = &$t {
            let _ = writeln!($o, "{} = {}", $name, tstr(v));
        }
    };
    ($o:expr, $t:expr, $name:literal, plain) => {
        if let Some(v) = &$t {
            let _ = writeln!($o, "{} = {}", $name, v);
        }
    };
}

/// Manifest as `character.toml` text. `header` lines (without `#`) are written as comments.
pub fn manifest_to_toml(m: &Manifest, header: &[&str]) -> String {
    let mut o = String::new();
    for h in header {
        let _ = writeln!(o, "# {h}");
    }
    let _ = writeln!(o, "format = {}", m.format);
    let _ = writeln!(o, "family = {}", tstr(m.family.as_str()));

    let c = &m.character;
    o.push_str("\n[character]\n");
    opt!(o, c.id, "id", str);
    let _ = writeln!(o, "canvas = [{}, {}]", c.canvas[0], c.canvas[1]);
    if let Some([w, h]) = c.display {
        let _ = writeln!(o, "display = [{w}, {h}]");
    }
    if c.standard_animation_set {
        o.push_str("standard_animation_set = true\n");
    }
    for i in &c.info {
        o.push_str("\n[[character.info]]\n");
        let _ = writeln!(o, "lang = {}", tstr(&i.lang));
        let _ = writeln!(o, "name = {}", tstr(&i.name));
        opt!(o, i.description, "description", str);
        opt!(o, i.extra, "extra", str);
    }
    if let Some(b) = &m.balloon {
        o.push_str("\n[balloon]\n");
        opt!(o, b.enabled, "enabled", plain);
        opt!(o, b.lines, "lines", plain);
        opt!(o, b.chars_per_line, "chars_per_line", plain);
        opt!(o, b.fg, "fg", str);
        opt!(o, b.bg, "bg", str);
        opt!(o, b.border, "border", str);
        opt!(o, b.font, "font", str);
        opt!(o, b.font_height, "font_height", plain);
        opt!(o, b.font_weight, "font_weight", plain);
        opt!(o, b.italic, "italic", plain);
        opt!(o, b.font_flag2, "font_flag2", plain);
        opt!(o, b.size_to_text, "size_to_text", plain);
        opt!(o, b.auto_hide, "auto_hide", plain);
        opt!(o, b.auto_pace, "auto_pace", plain);
    }
    if let Some(v) = &m.voice {
        o.push_str("\n[voice]\n");
        opt!(o, v.engine, "engine", str);
        opt!(o, v.mode, "mode", str);
        opt!(o, v.speed, "speed", plain);
        opt!(o, v.pitch, "pitch", plain);
        opt!(o, v.language, "language", str);
        opt!(o, v.dialect, "dialect", str);
        opt!(o, v.gender, "gender", plain);
        opt!(o, v.age, "age", plain);
        opt!(o, v.style, "style", str);
    }
    if let Some(i) = &m.icon {
        let _ = writeln!(o, "\n[icon]\nfile = {}", tstr(&i.file));
    }
    for s in &m.sheets {
        o.push_str("\n[[sheets]]\n");
        let _ = writeln!(o, "id = {}", tstr(&s.id));
        let _ = writeln!(o, "file = {}", tstr(&s.file));
        let _ = writeln!(o, "cell = [{}, {}]", s.cell[0], s.cell[1]);
        let _ = writeln!(o, "count = {}", s.count);
        if let Some(sz) = &s.sizes {
            let v: Vec<String> = sz.iter().map(|[w, h]| format!("[{w}, {h}]")).collect();
            let _ = writeln!(o, "sizes = [{}]", v.join(", "));
        }
        opt!(o, s.scale, "scale", plain);
        if !s.hidpi.is_empty() {
            o.push_str("[sheets.hidpi]\n");
            for (k, f) in &s.hidpi {
                let _ = writeln!(o, "{k} = {}", tstr(f));
            }
        }
    }
    if !m.sounds.is_empty() {
        o.push_str("\n[sounds]\n");
        for (k, f) in &m.sounds {
            let _ = writeln!(o, "{} = {}", tkey(k), tstr(f));
        }
    }
    if !m.states.is_empty() {
        o.push_str("\n[states]\n");
        for (k, v) in &m.states {
            let list: Vec<String> = v.iter().map(|s| tstr(s)).collect();
            let _ = writeln!(o, "{} = [{}]", tkey(k), list.join(", "));
        }
    }
    if !m.poses.is_empty() {
        o.push_str("\n[poses]\n");
        for (k, v) in &m.poses {
            let _ = writeln!(o, "{k} = {}", layers(v));
        }
    }
    if let Some(q) = &m.office97 {
        o.push_str("\n[office97]\n");
        opt!(o, q.rest_pose, "rest_pose", plain);
        let _ = writeln!(o, "idle = {}", tstr(&q.idle));
        let t: Vec<String> = q
            .transitions
            .iter()
            .map(|(a, b, s)| format!("[{a}, {b}, {}]", tstr(s)))
            .collect();
        let _ = writeln!(o, "transitions = [{}]", t.join(", "));
        if !q.ids.is_empty() {
            o.push_str("\n[office97.ids]\n");
            for (k, v) in &q.ids {
                let _ = writeln!(o, "{} = {v}", tkey(k));
            }
        }
        if !q.strings.is_empty() {
            o.push_str("\n[office97.strings]\n");
            for (k, v) in &q.strings {
                let list: Vec<String> = v.iter().map(|s| tstr(s)).collect();
                let _ = writeln!(o, "{k} = [{}]", list.join(", "));
            }
        }
    }
    for a in &m.animations {
        o.push_str("\n[[animations]]\n");
        let _ = writeln!(o, "name = {}", tstr(&a.name));
        if let Some(t) = a.transition {
            let _ = writeln!(o, "transition = {}", tstr(t.as_str()));
        }
        opt!(o, a.return_animation, "return", str);
        o.push_str("frames = [\n");
        for f in &a.frames {
            let _ = writeln!(o, "  {},", frame(f));
        }
        o.push_str("]\n");
    }
    if let Some(s) = &m.source {
        o.push_str("\n[source]\n");
        for (k, v) in s {
            let _ = writeln!(o, "{} = {}", tkey(k), tvalue(v));
        }
    }
    o
}

/// Indexed PNG: `PLTE` as given (source order), optional `tRNS`. The bit depth is the
/// smallest of 1, 2, 4, 8 that holds the palette and every index (issue #21); the indices,
/// and so the decoded picture, are the same at any depth.
pub fn encode_png_indexed(
    w: u32,
    h: u32,
    idx: &[u8],
    plte: &[[u8; 3]],
    trns: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    png_size(w, h, idx.len(), 1)?;
    let n = plte
        .len()
        .max(idx.iter().max().map_or(0, |&m| m as usize + 1));
    let depth: u8 = match n {
        0..=2 => 1,
        3..=4 => 2,
        5..=16 => 4,
        _ => 8,
    };
    let per = 8 / depth as usize;
    let line = (w as usize).div_ceil(per);
    let mut raw = Vec::with_capacity((line + 1) * h as usize);
    for row in idx.chunks_exact(w as usize).take(h as usize) {
        raw.push(0);
        for px in row.chunks(per) {
            let mut b = 0u8;
            for (i, &v) in px.iter().enumerate() {
                b |= v << (8 - depth as usize * (i + 1));
            }
            raw.push(b);
        }
    }
    let mut out = png_start(w, h, depth, 3);
    png_chunk(&mut out, b"PLTE", plte.as_flattened());
    if let Some(t) = trns {
        png_chunk(&mut out, b"tRNS", t);
    }
    png_end(out, &raw)
}

/// RGBA 8-bit PNG: no filter or the per-row filter with the smallest sum of absolute
/// values, whichever compresses smaller.
pub fn encode_png_rgba(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, Error> {
    png_size(w, h, rgba.len(), 4)?;
    let line = w as usize * 4;
    let mut none = Vec::with_capacity((line + 1) * h as usize);
    let mut adaptive = Vec::with_capacity(none.capacity());
    let zero = vec![0u8; line];
    let mut prev: &[u8] = &zero;
    for row in rgba.chunks_exact(line).take(h as usize) {
        none.push(0);
        none.extend_from_slice(row);
        let best = (0..5u8)
            .map(|f| (f, png_filter(f, row, prev)))
            .min_by_key(|(_, v)| {
                v.iter()
                    .map(|&b| (b as i8).unsigned_abs() as u64)
                    .sum::<u64>()
            })
            .expect("five filters");
        adaptive.push(best.0);
        adaptive.extend_from_slice(&best.1);
        prev = row;
    }
    let a = png_end(png_start(w, h, 8, 6), &none)?;
    let b = png_end(png_start(w, h, 8, 6), &adaptive)?;
    Ok(if b.len() < a.len() { b } else { a })
}

/// PNG filter `f` of `row` (4 bytes per pixel) against the row above.
fn png_filter(f: u8, row: &[u8], up: &[u8]) -> Vec<u8> {
    (0..row.len())
        .map(|i| {
            let a = if i >= 4 { row[i - 4] } else { 0 };
            let b = up[i];
            let c = if i >= 4 { up[i - 4] } else { 0 };
            let p = match f {
                0 => 0,
                1 => a,
                2 => b,
                3 => ((a as u16 + b as u16) / 2) as u8,
                _ => {
                    let (pa, pb, pc) = (
                        (b as i16 - c as i16).abs(),
                        (a as i16 - c as i16).abs(),
                        (a as i16 + b as i16 - 2 * c as i16).abs(),
                    );
                    if pa <= pb && pa <= pc {
                        a
                    } else if pb <= pc {
                        b
                    } else {
                        c
                    }
                }
            };
            row[i].wrapping_sub(p)
        })
        .collect()
}

/// PNG signature and `IHDR` (no interlace).
fn png_start(w: u32, h: u32, depth: u8, color: u8) -> Vec<u8> {
    let mut out = b"\x89PNG\r\n\x1a\n".to_vec();
    let mut ihdr = Vec::with_capacity(13);
    ihdr.extend_from_slice(&w.to_be_bytes());
    ihdr.extend_from_slice(&h.to_be_bytes());
    ihdr.extend_from_slice(&[depth, color, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &ihdr);
    out
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

fn png_size(w: u32, h: u32, len: usize, bpp: usize) -> Result<(), Error> {
    if w == 0 || h == 0 || len != w as usize * h as usize * bpp {
        return Err(Error::Image(format!(
            "{w}x{h} PNG with {len} bytes of pixels"
        )));
    }
    Ok(())
}

/// One `IDAT` with the filtered rows `raw`, deflated by libdeflate at its strongest level
/// (deterministic for a given libdeflate version), then `IEND`.
fn png_end(mut out: Vec<u8>, raw: &[u8]) -> Result<Vec<u8>, Error> {
    let mut c = libdeflater::Compressor::new(
        libdeflater::CompressionLvl::new(12).expect("level 12 exists"),
    );
    let mut z = vec![0; c.zlib_compress_bound(raw.len())];
    let n = c
        .zlib_compress(raw, &mut z)
        .map_err(|e| Error::Image(format!("deflate: {e}")))?;
    png_chunk(&mut out, b"IDAT", &z[..n]);
    png_chunk(&mut out, b"IEND", &[]);
    Ok(out)
}

/// FLAC of `pcm` with fixed settings (the bit-exact PCM of character-format 6).
pub fn encode_flac(pcm: &Pcm) -> Result<Vec<u8>, Error> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let n = pcm.data.len() / pcm.bytes_per_sample();
    let samples: Vec<i32> = (0..n).map(|i| pcm.sample(i)).collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|e| Error::Sound(format!("FLAC config: {e:?}")))?;
    let block = config.block_size;
    let src = flacenc::source::MemSource::from_samples(
        &samples,
        pcm.channels as usize,
        pcm.bits as usize,
        pcm.rate as usize,
    );
    let mut stream = flacenc::encode_with_fixed_block_size(&config, src, block)
        .map_err(|e| Error::Sound(format!("FLAC: {e:?}")))?;
    // flacenc counts the shorter last block in STREAMINFO's minimum block size; a fixed
    // block size stream needs min = max (the last block excluded), or libFLAC finds the
    // frame numbers not increasing.
    stream
        .stream_info_mut()
        .set_block_sizes(block, block)
        .map_err(|e| Error::Sound(format!("FLAC: {e:?}")))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream
        .write(&mut sink)
        .map_err(|e| Error::Sound(format!("FLAC: {e:?}")))?;
    Ok(sink.as_slice().to_vec())
}

/// A package in memory: the manifest plus every other file by package path.
pub struct PackageData {
    pub manifest: Manifest,
    /// Comment lines written at the top of `character.toml`.
    pub header: Vec<String>,
    /// Package path to bytes (sheets, sounds, icon). Written in this order after the manifest.
    pub files: Vec<(String, Vec<u8>)>,
}

impl PackageData {
    pub fn manifest_text(&self) -> String {
        let h: Vec<&str> = self.header.iter().map(String::as_str).collect();
        manifest_to_toml(&self.manifest, &h)
    }

    /// Entries in the fixed order: manifest, then files sorted by path.
    fn ordered(&self) -> Vec<(&str, &[u8])> {
        let mut f: Vec<(&str, &[u8])> = self
            .files
            .iter()
            .map(|(p, b)| (p.as_str(), b.as_slice()))
            .collect();
        f.sort_by(|a, b| a.0.cmp(b.0));
        f
    }

    /// `.bwc` bytes: manifest first and deflated, media stored (character-format 3).
    pub fn to_bwc(&self) -> Result<Vec<u8>, Error> {
        let text = self.manifest_text();
        let mut entries: Vec<(&str, &[u8], bool)> = vec![("character.toml", text.as_bytes(), true)];
        entries.extend(self.ordered().into_iter().map(|(p, b)| (p, b, false)));
        crate::zip::write(&entries)
    }

    /// Write a folder (created; existing files with the same names are replaced).
    pub fn write_folder(&self, dir: &Path) -> Result<(), Error> {
        std::fs::create_dir_all(dir)?;
        std::fs::write(dir.join("character.toml"), self.manifest_text())?;
        for (p, b) in self.ordered() {
            crate::parse::check_path(p, "file")?;
            let full = dir.join(p);
            if let Some(parent) = full.parent() {
                std::fs::create_dir_all(parent)?;
            }
            std::fs::write(full, b)?;
        }
        Ok(())
    }

    /// Write a `.bwc` file.
    pub fn write_bwc(&self, path: &Path) -> Result<(), Error> {
        std::fs::write(path, self.to_bwc()?)?;
        Ok(())
    }

    /// Folder when `dest` does not end in `.bwc`, else `.bwc`.
    pub fn write(&self, dest: &Path) -> Result<(), Error> {
        if dest
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case("bwc"))
        {
            self.write_bwc(dest)
        } else {
            self.write_folder(dest)
        }
    }

    /// Load the in-memory package as a character (no disk round trip).
    pub fn load(&self) -> Result<crate::Character, Error> {
        crate::Character::from_package(&crate::Package::Zip(crate::zip::Archive::new(
            self.to_bwc()?,
        )?))
    }
}
