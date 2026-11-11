//! `character.toml` text to [`Manifest`]. Type and range errors are hard errors; unknown keys
//! (not `x-`) are collected in `Manifest::unknown_keys`.

use std::collections::BTreeMap;

use toml::{Table, Value};

use crate::Error;
use crate::model::*;

type R<T> = Result<T, Error>;

fn err<T>(path: &str, msg: impl std::fmt::Display) -> R<T> {
    Err(Error::Manifest(format!("{path}: {msg}")))
}

/// A table plus the keys a reader consumed, so the rest can be reported.
struct Obj<'a> {
    t: &'a Table,
    path: String,
    used: Vec<&'static str>,
}

impl<'a> Obj<'a> {
    fn new(t: &'a Table, path: impl Into<String>) -> Self {
        Obj {
            t,
            path: path.into(),
            used: Vec::new(),
        }
    }

    fn p(&self, k: &str) -> String {
        if self.path.is_empty() {
            k.to_owned()
        } else {
            format!("{}.{k}", self.path)
        }
    }

    fn get(&mut self, k: &'static str) -> Option<&'a Value> {
        self.used.push(k);
        self.t.get(k)
    }

    fn req(&mut self, k: &'static str) -> R<&'a Value> {
        match self.get(k) {
            Some(v) => Ok(v),
            None => err(&self.p(k), "required key missing"),
        }
    }

    fn opt_str(&mut self, k: &'static str) -> R<Option<String>> {
        self.get(k)
            .map(|v| as_str(v, &self.p(k)).map(str::to_owned))
            .transpose()
    }

    fn opt_int(&mut self, k: &'static str) -> R<Option<i64>> {
        self.get(k).map(|v| as_int(v, &self.p(k))).transpose()
    }

    fn opt_bool(&mut self, k: &'static str) -> R<Option<bool>> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::Boolean(b)) => Ok(Some(*b)),
            Some(_) => err(&self.p(k), "expected a boolean"),
        }
    }

    fn opt_table(&mut self, k: &'static str) -> R<Option<&'a Table>> {
        match self.get(k) {
            None => Ok(None),
            Some(Value::Table(t)) => Ok(Some(t)),
            Some(_) => err(&self.p(k), "expected a table"),
        }
    }

    fn finish(self, unknown: &mut Vec<String>) {
        for k in self.t.keys() {
            if !self.used.contains(&k.as_str()) && !k.starts_with("x-") {
                unknown.push(if self.path.is_empty() {
                    k.clone()
                } else {
                    format!("{}.{k}", self.path)
                });
            }
        }
    }
}

fn as_str<'a>(v: &'a Value, path: &str) -> R<&'a str> {
    v.as_str()
        .map_or_else(|| err(path, "expected a string"), Ok)
}

fn as_int(v: &Value, path: &str) -> R<i64> {
    v.as_integer()
        .map_or_else(|| err(path, "expected an integer"), Ok)
}

fn as_u32(v: &Value, path: &str) -> R<u32> {
    let i = as_int(v, path)?;
    u32::try_from(i).or_else(|_| err(path, format!("{i} out of range")))
}

fn as_i32(v: &Value, path: &str) -> R<i32> {
    let i = as_int(v, path)?;
    i32::try_from(i).or_else(|_| err(path, format!("{i} out of range")))
}

fn as_arr<'a>(v: &'a Value, path: &str) -> R<&'a Vec<Value>> {
    v.as_array()
        .map_or_else(|| err(path, "expected an array"), Ok)
}

fn as_table<'a>(v: &'a Value, path: &str) -> R<&'a Table> {
    v.as_table()
        .map_or_else(|| err(path, "expected a table"), Ok)
}

fn size(v: &Value, path: &str) -> R<[u32; 2]> {
    let a = as_arr(v, path)?;
    if a.len() != 2 {
        return err(path, "expected [w, h]");
    }
    let w = as_u32(&a[0], path)?;
    let h = as_u32(&a[1], path)?;
    if w == 0 || h == 0 {
        return err(path, "sizes must be at least 1");
    }
    Ok([w, h])
}

fn point(v: &Value, path: &str) -> R<[i32; 2]> {
    let a = as_arr(v, path)?;
    if a.len() != 2 {
        return err(path, "expected [x, y]");
    }
    Ok([as_i32(&a[0], path)?, as_i32(&a[1], path)?])
}

/// `"sheet:n"`.
pub fn parse_ref(s: &str, path: &str) -> R<ImageRef> {
    let Some((sheet, n)) = s.rsplit_once(':') else {
        return err(path, format!("bad image reference {s:?}"));
    };
    let ok_id = !sheet.is_empty()
        && sheet
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    match (ok_id, n.parse::<u32>()) {
        (true, Ok(index)) if !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()) => {
            Ok(ImageRef {
                sheet: sheet.to_owned(),
                index,
            })
        }
        _ => err(path, format!("bad image reference {s:?}")),
    }
}

fn layer(v: &Value, path: &str) -> R<Layer> {
    if let Value::String(s) = v {
        return Ok(Layer {
            image: parse_ref(s, path)?,
            x: 0,
            y: 0,
            size: None,
        });
    }
    let a = as_arr(v, path)?;
    if a.len() != 3 && a.len() != 5 {
        return err(
            path,
            "layer must be \"s:n\", [ref, x, y] or [ref, x, y, w, h]",
        );
    }
    let image = parse_ref(as_str(&a[0], path)?, path)?;
    let x = as_i32(&a[1], path)?;
    let y = as_i32(&a[2], path)?;
    let size = if a.len() == 5 {
        let w = as_u32(&a[3], path)?;
        let h = as_u32(&a[4], path)?;
        if w == 0 || h == 0 {
            return err(path, "layer size must be at least 1");
        }
        Some([w, h])
    } else {
        None
    };
    Ok(Layer { image, x, y, size })
}

fn layers(v: &Value, path: &str) -> R<Vec<Layer>> {
    as_arr(v, path)?
        .iter()
        .enumerate()
        .map(|(i, l)| layer(l, &format!("{path}[{i}]")))
        .collect()
}

fn branch(v: &Value, path: &str) -> R<Branch> {
    let a = as_arr(v, path)?;
    match a.len() {
        2 => {
            let target = as_u32(&a[0], path)?;
            let p = as_u32(&a[1], path)?;
            if p > 65535 {
                return err(path, "probability above 65535");
            }
            Ok(Branch::Random { target, p })
        }
        3 => {
            let target = as_u32(&a[0], path)?;
            let value = as_u32(&a[2], path)?;
            match as_str(&a[1], path)? {
                "daypart" => Ok(Branch::Daypart { target, value }),
                "host" => Ok(Branch::Host { target, value }),
                k => err(path, format!("unknown branch kind {k:?}")),
            }
        }
        _ => err(path, "branch must be [target, p] or [target, kind, v]"),
    }
}

fn frame(v: &Value, path: &str, unknown: &mut Vec<String>) -> R<Frame> {
    let mut o = Obj::new(as_table(v, path)?, path);
    let ms = as_u32(o.req("ms")?, &o.p("ms"))?;
    if ms > 655_350 {
        return err(&o.p("ms"), "duration above 655350");
    }
    let mut f = Frame {
        ms,
        ..Frame::default()
    };
    if let Some(v) = o.get("layers") {
        f.layers = layers(v, &o.p("layers"))?;
    }
    if let Some(v) = o.get("pose") {
        if o.t.contains_key("layers") {
            return err(path, "a frame has either layers or pose");
        }
        f.pose = Some(as_u32(v, &o.p("pose"))?);
    }
    f.sound = o.opt_str("sound")?;
    if let Some(v) = o.get("sound_arg") {
        let a = as_u32(v, &o.p("sound_arg"))?;
        if a > 65535 {
            return err(&o.p("sound_arg"), "above 65535");
        }
        f.sound_arg = Some(a);
    }
    if let Some(v) = o.get("branch") {
        let p = o.p("branch");
        let a = as_arr(v, &p)?;
        if a.is_empty() {
            return err(&p, "empty branch list");
        }
        f.branch = a
            .iter()
            .enumerate()
            .map(|(i, b)| branch(b, &format!("{p}[{i}]")))
            .collect::<R<_>>()?;
    }
    if let Some(v) = o.get("exit") {
        let p = o.p("exit");
        f.exit = Some(match v {
            Value::String(s) if s == "end" => Exit::End,
            Value::Integer(_) => Exit::Frame(as_u32(v, &p)?),
            _ => return err(&p, "exit must be \"end\" or a frame index"),
        });
    }
    if let Some(v) = o.get("mouths") {
        let p = o.p("mouths");
        let a = as_arr(v, &p)?;
        if a.is_empty() || a.len() > 7 {
            return err(&p, "1 to 7 mouths");
        }
        for (i, m) in a.iter().enumerate() {
            let mp = format!("{p}[{i}]");
            let mut mo = Obj::new(as_table(m, &mp)?, mp.clone());
            let kind = as_u32(mo.req("type")?, &mo.p("type"))?;
            if kind > 6 {
                return err(&mo.p("type"), "mouth type must be 0..6");
            }
            let layer = layer(mo.req("layer")?, &mo.p("layer"))?;
            let anchor = mo
                .get("anchor")
                .map(|v| point(v, &format!("{mp}.anchor")))
                .transpose()?;
            let base = mo
                .get("base")
                .map(|v| layers(v, &format!("{mp}.base")))
                .transpose()?;
            mo.finish(unknown);
            f.mouths.push(Mouth {
                kind: kind as u8,
                layer,
                anchor,
                base,
            });
        }
    }
    o.finish(unknown);
    Ok(f)
}

fn colour(v: Option<String>, path: &str) -> R<Option<String>> {
    if let Some(s) = &v {
        let ok = s.len() == 7
            && s.starts_with('#')
            && s[1..]
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'A'..=b'F').contains(&b));
        if !ok {
            return err(path, format!("colour {s:?} is not #RRGGBB"));
        }
    }
    Ok(v)
}

fn byte_range(v: Option<i64>, path: &str) -> R<Option<i64>> {
    match v {
        Some(x) if !(0..=255).contains(&x) => err(path, "must be 0..255"),
        _ => Ok(v),
    }
}

/// Paths inside a package: relative, `/`-separated, no `.`/`..` segments, no backslash.
pub fn check_path(s: &str, path: &str) -> R<()> {
    let bad = s.is_empty()
        || s.starts_with('/')
        || s.contains('\\')
        || s.split('/')
            .any(|seg| seg.is_empty() || seg == "." || seg == "..");
    if bad {
        return err(path, format!("bad package path {s:?}"));
    }
    Ok(())
}

fn file_path(v: &Value, path: &str) -> R<String> {
    let s = as_str(v, path)?;
    check_path(s, path)?;
    Ok(s.to_owned())
}

/// Parse manifest text.
pub fn parse_manifest(text: &str) -> R<Manifest> {
    let root: Table = text
        .parse()
        .map_err(|e: toml::de::Error| Error::Manifest(format!("TOML: {e}")))?;
    let mut unknown = Vec::new();
    let mut o = Obj::new(&root, "");

    let format = as_int(o.req("format")?, "format")?;
    if format != 1 {
        return Err(Error::UnsupportedFormat(format));
    }
    let family = match as_str(o.req("family")?, "family")? {
        "agent" => Family::Agent,
        "office97" => Family::Office97,
        f => return Err(Error::UnknownFamily(f.to_owned())),
    };

    // [character]
    let ct = as_table(o.req("character")?, "character")?;
    let mut c = Obj::new(ct, "character");
    let id = c.opt_str("id")?;
    if let Some(g) = &id
        && !is_guid(g)
    {
        return err("character.id", format!("{g:?} is not a GUID {{...}}"));
    }
    let canvas = size(c.req("canvas")?, "character.canvas")?;
    let display = c
        .get("display")
        .map(|v| size(v, "character.display"))
        .transpose()?;
    let standard_animation_set = c.opt_bool("standard_animation_set")?.unwrap_or(false);
    let mut info = Vec::new();
    if let Some(v) = c.get("info") {
        for (i, e) in as_arr(v, "character.info")?.iter().enumerate() {
            let p = format!("character.info[{i}]");
            let mut io = Obj::new(as_table(e, &p)?, p.clone());
            let lang = as_str(io.req("lang")?, &io.p("lang"))?.to_owned();
            let name = as_str(io.req("name")?, &io.p("name"))?.to_owned();
            let description = io.opt_str("description")?;
            let extra = io.opt_str("extra")?;
            io.finish(&mut unknown);
            info.push(Info {
                lang,
                name,
                description,
                extra,
            });
        }
    }
    c.finish(&mut unknown);

    // [balloon]
    let balloon = match o.opt_table("balloon")? {
        None => None,
        Some(t) => {
            let mut b = Obj::new(t, "balloon");
            let r = Balloon {
                enabled: b.opt_bool("enabled")?,
                lines: byte_range(b.opt_int("lines")?, "balloon.lines")?,
                chars_per_line: byte_range(b.opt_int("chars_per_line")?, "balloon.chars_per_line")?,
                fg: colour(b.opt_str("fg")?, "balloon.fg")?,
                bg: colour(b.opt_str("bg")?, "balloon.bg")?,
                border: colour(b.opt_str("border")?, "balloon.border")?,
                font: b.opt_str("font")?,
                font_height: b.opt_int("font_height")?,
                font_weight: b.opt_int("font_weight")?,
                italic: b.opt_bool("italic")?,
                font_flag2: b.opt_int("font_flag2")?,
                size_to_text: b.opt_bool("size_to_text")?,
                auto_hide: b.opt_bool("auto_hide")?,
                auto_pace: b.opt_bool("auto_pace")?,
            };
            b.finish(&mut unknown);
            Some(r)
        }
    };

    // [voice]
    let voice = match o.opt_table("voice")? {
        None => None,
        Some(t) => {
            let mut v = Obj::new(t, "voice");
            let r = Voice {
                engine: v.opt_str("engine")?,
                mode: v.opt_str("mode")?,
                speed: v.opt_int("speed")?,
                pitch: v.opt_int("pitch")?,
                language: v.opt_str("language")?,
                dialect: v.opt_str("dialect")?,
                gender: v.opt_int("gender")?,
                age: v.opt_int("age")?,
                style: v.opt_str("style")?,
            };
            v.finish(&mut unknown);
            Some(r)
        }
    };

    // [icon]
    let icon = match o.opt_table("icon")? {
        None => None,
        Some(t) => {
            let mut i = Obj::new(t, "icon");
            let file = file_path(i.req("file")?, "icon.file")?;
            i.finish(&mut unknown);
            Some(IconRef { file })
        }
    };

    // [[sheets]]
    let mut sheets = Vec::new();
    for (i, e) in as_arr(o.req("sheets")?, "sheets")?.iter().enumerate() {
        let p = format!("sheets[{i}]");
        let mut s = Obj::new(as_table(e, &p)?, p.clone());
        let sid = as_str(s.req("id")?, &s.p("id"))?.to_owned();
        let ok_id = !sid.is_empty()
            && sid
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
        if !ok_id {
            return err(
                &s.p("id"),
                format!("sheet id {sid:?} must match [a-z0-9_]+"),
            );
        }
        let file = file_path(s.req("file")?, &s.p("file"))?;
        let cell = size(s.req("cell")?, &s.p("cell"))?;
        let count = as_u32(s.req("count")?, &s.p("count"))?;
        if count == 0 {
            return err(&s.p("count"), "must be at least 1");
        }
        let sizes = match s.get("sizes") {
            None => None,
            Some(v) => Some(
                as_arr(v, &format!("{p}.sizes"))?
                    .iter()
                    .enumerate()
                    .map(|(k, x)| size(x, &format!("{p}.sizes[{k}]")))
                    .collect::<R<Vec<_>>>()?,
            ),
        };
        let scale = match s.get("scale") {
            None => None,
            Some(v) => {
                let k = as_u32(v, &format!("{p}.scale"))?;
                if k == 0 {
                    return err(&format!("{p}.scale"), "must be at least 1");
                }
                Some(k)
            }
        };
        let mut hidpi = BTreeMap::new();
        if let Some(t) = s.opt_table("hidpi")? {
            for (k, v) in t {
                let hp = format!("{p}.hidpi.{k}");
                let key: u32 = match k.parse() {
                    Ok(n) if (2..=8).contains(&n) && k.len() == 1 => n,
                    _ => return err(&hp, "hidpi key must be 2..8"),
                };
                hidpi.insert(key, file_path(v, &hp)?);
            }
        }
        s.finish(&mut unknown);
        sheets.push(Sheet {
            id: sid,
            file,
            cell,
            count,
            sizes,
            scale,
            hidpi,
        });
    }
    if sheets.is_empty() {
        return err("sheets", "at least one sheet");
    }

    // [sounds]
    let mut sounds = Vec::new();
    if let Some(t) = o.opt_table("sounds")? {
        for (k, v) in t {
            sounds.push((k.clone(), file_path(v, &format!("sounds.{k}"))?));
        }
    }

    // [states]
    let mut states = Vec::new();
    if let Some(t) = o.opt_table("states")? {
        for (k, v) in t {
            let p = format!("states.{k}");
            let a = as_arr(v, &p)?;
            if a.is_empty() {
                return err(&p, "a state lists at least one animation");
            }
            let members = a
                .iter()
                .map(|x| as_str(x, &p).map(str::to_owned))
                .collect::<R<Vec<_>>>()?;
            states.push((k.clone(), members));
        }
    }

    // [poses]
    let mut poses = BTreeMap::new();
    if let Some(t) = o.opt_table("poses")? {
        for (k, v) in t {
            let p = format!("poses.{k}");
            let id: u32 = match k.parse() {
                Ok(n) if k.bytes().all(|b| b.is_ascii_digit()) => n,
                _ => return err(&p, "pose id must be a number"),
            };
            poses.insert(id, layers(v, &p)?);
        }
    }

    // [office97]
    let office97 = match o.opt_table("office97")? {
        None => None,
        Some(t) => {
            let mut q = Obj::new(t, "office97");
            let rest_pose = q
                .get("rest_pose")
                .map(|v| as_u32(v, "office97.rest_pose"))
                .transpose()?;
            let idle = as_str(q.req("idle")?, "office97.idle")?.to_owned();
            let mut transitions = Vec::new();
            if let Some(v) = q.get("transitions") {
                for (i, e) in as_arr(v, "office97.transitions")?.iter().enumerate() {
                    let p = format!("office97.transitions[{i}]");
                    let a = as_arr(e, &p)?;
                    if a.len() != 3 {
                        return err(&p, "expected [from, to, state]");
                    }
                    transitions.push((
                        as_u32(&a[0], &p)?,
                        as_u32(&a[1], &p)?,
                        as_str(&a[2], &p)?.to_owned(),
                    ));
                }
            }
            let mut ids = Vec::new();
            if let Some(t) = q.opt_table("ids")? {
                for (k, v) in t {
                    let p = format!("office97.ids.{k}");
                    let id = as_u32(v, &p)?;
                    if id > 65535 {
                        return err(&p, "above 65535");
                    }
                    ids.push((k.clone(), id));
                }
            }
            let mut strings = BTreeMap::new();
            if let Some(t) = q.opt_table("strings")? {
                for (k, v) in t {
                    let p = format!("office97.strings.{k}");
                    let id: u32 = match k.parse() {
                        Ok(n) if k.bytes().all(|b| b.is_ascii_digit()) => n,
                        _ => return err(&p, "string group id must be a number"),
                    };
                    let list = as_arr(v, &p)?
                        .iter()
                        .map(|x| as_str(x, &p).map(str::to_owned))
                        .collect::<R<Vec<_>>>()?;
                    strings.insert(id, list);
                }
            }
            q.finish(&mut unknown);
            Some(Office97 {
                rest_pose,
                idle,
                transitions,
                ids,
                strings,
            })
        }
    };

    // [[animations]]
    let mut animations = Vec::new();
    for (i, e) in as_arr(o.req("animations")?, "animations")?
        .iter()
        .enumerate()
    {
        let p = format!("animations[{i}]");
        let mut a = Obj::new(as_table(e, &p)?, p.clone());
        let name = as_str(a.req("name")?, &a.p("name"))?.to_owned();
        if name.is_empty() {
            return err(&a.p("name"), "empty animation name");
        }
        let transition = match a.opt_str("transition")?.as_deref() {
            None => None,
            Some("return") => Some(Transition::Return),
            Some("exit") => Some(Transition::Exit),
            Some("none") => Some(Transition::None),
            Some(t) => return err(&a.p("transition"), format!("unknown transition {t:?}")),
        };
        let return_animation = a.opt_str("return")?;
        let fp = a.p("frames");
        let fa = as_arr(a.req("frames")?, &fp)?;
        if fa.is_empty() {
            return err(&fp, "an animation has at least one frame");
        }
        let frames = fa
            .iter()
            .enumerate()
            .map(|(k, f)| frame(f, &format!("{name}[{k}]"), &mut unknown))
            .collect::<R<Vec<_>>>()?;
        a.finish(&mut unknown);
        animations.push(Animation {
            name,
            transition,
            return_animation,
            frames,
        });
    }
    if animations.is_empty() {
        return err("animations", "at least one animation");
    }

    let source = o.opt_table("source")?.cloned();
    o.finish(&mut unknown);

    Ok(Manifest {
        format,
        family,
        character: CharacterSection {
            id,
            canvas,
            display,
            standard_animation_set,
            info,
        },
        balloon,
        voice,
        icon,
        sheets,
        sounds,
        states,
        poses,
        office97,
        animations,
        source,
        unknown_keys: unknown,
    })
}

fn is_guid(s: &str) -> bool {
    let b = s.as_bytes();
    if b.len() != 38 || b[0] != b'{' || b[37] != b'}' {
        return false;
    }
    b[1..37].iter().enumerate().all(|(i, &c)| match i {
        8 | 13 | 18 | 23 => c == b'-',
        _ => c.is_ascii_digit() || (b'A'..=b'F').contains(&c),
    })
}

/// The cheap check of spec B-CHAR-1: the manifest parses as TOML, `format = 1`, family known.
pub fn cheap_check(text: &str) -> R<Family> {
    let root: Table = text
        .parse()
        .map_err(|e: toml::de::Error| Error::Manifest(format!("TOML: {e}")))?;
    match root.get("format").and_then(Value::as_integer) {
        Some(1) => {}
        Some(n) => return Err(Error::UnsupportedFormat(n)),
        None => return err("format", "required integer missing"),
    }
    match root.get("family").and_then(Value::as_str) {
        Some("agent") => Ok(Family::Agent),
        Some("office97") => Ok(Family::Office97),
        Some(f) => Err(Error::UnknownFamily(f.to_owned())),
        None => err("family", "required string missing"),
    }
}
