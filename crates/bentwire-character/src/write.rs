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

/// Indexed 8-bit PNG: `PLTE` as given (source order), optional `tRNS`.
pub fn encode_png_indexed(
    w: u32,
    h: u32,
    idx: &[u8],
    plte: &[[u8; 3]],
    trns: Option<&[u8]>,
) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        e.set_palette(plte.iter().flatten().copied().collect::<Vec<u8>>());
        if let Some(t) = trns {
            e.set_trns(t.to_vec());
        }
        png_settings(&mut e);
        let mut wr = e.write_header().map_err(|x| Error::Image(x.to_string()))?;
        wr.write_image_data(idx)
            .map_err(|x| Error::Image(x.to_string()))?;
    }
    Ok(out)
}

/// RGBA 8-bit PNG.
pub fn encode_png_rgba(w: u32, h: u32, rgba: &[u8]) -> Result<Vec<u8>, Error> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        png_settings(&mut e);
        let mut wr = e.write_header().map_err(|x| Error::Image(x.to_string()))?;
        wr.write_image_data(rgba)
            .map_err(|x| Error::Image(x.to_string()))?;
    }
    Ok(out)
}

fn png_settings<W: std::io::Write>(e: &mut png::Encoder<'_, W>) {
    e.set_compression(png::Compression::Best);
    e.set_filter(png::FilterType::NoFilter);
    e.set_adaptive_filter(png::AdaptiveFilterType::NonAdaptive);
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
    let stream = flacenc::encode_with_fixed_block_size(&config, src, block)
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
