//! Canonical dump (character-format 11.1) and interpreter B (11.3), as the prototype
//! `reader.py` produces them, so dumps compare equal as JSON with the Python oracle.

use std::cell::RefCell;
use std::collections::HashMap;

use serde_json::{Value, json};

use crate::model::*;
use crate::{Character, Layer, sha16};

/// One step of a trace: (picture key, ms, sound hash), or a terminator.
#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    /// Agent: picture hash; office97: pose id (or null).
    Frame(Value, u32, Option<String>),
    End,
    /// Exit was requested on a frame with `exit = "end"`.
    Cut,
    Limit,
}

impl Step {
    pub fn to_json(&self) -> Value {
        match self {
            Step::Frame(p, ms, s) => json!([p, ms, s]),
            Step::End => json!("END"),
            Step::Cut => json!("CUT"),
            Step::Limit => json!("LIMIT"),
        }
    }
}

/// Hash caches for one character.
pub struct Dumper<'a> {
    pub c: &'a Character,
    pics: RefCell<HashMap<Vec<Layer>, String>>,
}

impl<'a> Dumper<'a> {
    pub fn new(c: &'a Character) -> Self {
        Dumper {
            c,
            pics: RefCell::new(HashMap::new()),
        }
    }

    /// Hash of the composited picture of `layers` at display size.
    pub fn picture_hash(&self, layers: &[Layer]) -> String {
        if let Some(h) = self.pics.borrow().get(layers) {
            return h.clone();
        }
        let h = sha16(&self.c.compose(layers, 1).data);
        self.pics.borrow_mut().insert(layers.to_vec(), h.clone());
        h
    }

    fn frame_hash(&self, f: &Frame, mouth: Option<usize>) -> String {
        self.picture_hash(&self.c.frame_layers(f, mouth))
    }

    fn sound_hash(&self, s: &Option<String>) -> Option<String> {
        s.as_ref().and_then(|id| self.c.sound(id)).map(|p| p.hash())
    }

    /// `[count, sha of RGB triples, transparent index]` of the (common) sheet palette, or null
    /// when any sheet is RGBA. Several different palettes give a string describing the conflict.
    fn palette(&self) -> Value {
        let mut out: Vec<Value> = Vec::new();
        for s in &self.c.manifest.sheets {
            let Some(img) = self.c.sheets.base(&s.id) else {
                return json!("undecodable sheet");
            };
            match img.palette_info() {
                None => return Value::Null,
                Some(Err(e)) => return json!(e.to_string()),
                Some(Ok((n, rgb, t))) => {
                    let v = json!([n, sha16(&rgb), t]);
                    if !out.contains(&v) {
                        out.push(v);
                    }
                }
            }
        }
        if out.len() == 1 {
            out.pop().unwrap()
        } else {
            json!(format!("{} different palettes", out.len()))
        }
    }

    /// The canonical dump.
    pub fn dump(&self) -> Value {
        let m = &self.c.manifest;
        let c = &m.character;
        let info: Vec<Value> = c
            .info
            .iter()
            .map(|i| {
                json!([
                    i.lang,
                    i.name,
                    i.description.clone().unwrap_or_default(),
                    i.extra.clone().unwrap_or_default()
                ])
            })
            .collect();
        let states: Vec<Value> = m
            .states
            .iter()
            .map(|(k, v)| {
                json!([
                    k.to_lowercase(),
                    v.iter().map(|a| a.to_lowercase()).collect::<Vec<_>>()
                ])
            })
            .collect();
        let mut d = serde_json::Map::new();
        d.insert("family".into(), json!(m.family.as_str()));
        d.insert("canvas".into(), json!(m.canvas()));
        d.insert("display".into(), json!(m.display()));
        d.insert("guid".into(), json!(c.id));
        d.insert("info".into(), json!(info));
        d.insert(
            "standard_animation_set".into(),
            json!(c.standard_animation_set),
        );
        d.insert(
            "balloon".into(),
            m.balloon.as_ref().map_or(Value::Null, balloon_json),
        );
        d.insert(
            "voice".into(),
            m.voice.as_ref().map_or(Value::Null, voice_json),
        );
        d.insert("palette".into(), self.palette());
        d.insert(
            "icon".into(),
            match (&m.icon, &self.c.icon) {
                (Some(_), Some(i)) => json!(sha16(&i.data)),
                (Some(_), None) => json!("undecodable icon"),
                _ => Value::Null,
            },
        );
        d.insert("states".into(), json!(states));
        let mut anims = Vec::new();
        for a in &m.animations {
            let mut frames = Vec::new();
            for f in &a.frames {
                let mut e = serde_json::Map::new();
                e.insert("ms".into(), json!(f.ms));
                e.insert("sound".into(), json!(self.sound_hash(&f.sound)));
                e.insert(
                    "branch".into(),
                    json!(f.branch.iter().map(branch_json).collect::<Vec<_>>()),
                );
                match m.family {
                    Family::Agent => {
                        e.insert("image".into(), json!(self.frame_hash(f, None)));
                        e.insert("exit".into(), exit_json(f.exit));
                        let mouths: Vec<Value> = f
                            .mouths
                            .iter()
                            .enumerate()
                            .map(|(i, mo)| json!([mo.kind, self.frame_hash(f, Some(i)), mo.anchor]))
                            .collect();
                        e.insert("mouths".into(), json!(mouths));
                    }
                    Family::Office97 => {
                        e.insert("pose".into(), json!(f.pose));
                        e.insert("sound_arg".into(), json!(f.sound_arg));
                    }
                }
                frames.push(Value::Object(e));
            }
            anims.push(json!([
                a.name,
                a.transition.map(|t| t.as_str()),
                a.return_animation,
                frames
            ]));
        }
        d.insert("animations".into(), json!(anims));
        if let Some(o) = &m.office97 {
            let mut ids: Vec<(String, u32)> = o.ids.clone();
            ids.sort();
            let transitions: Vec<Value> = o
                .transitions
                .iter()
                .map(|(a, b, s)| json!([a, b, s]))
                .collect();
            let strings: Vec<Value> = o.strings.iter().map(|(k, v)| json!([k, v])).collect();
            d.insert(
                "office97".into(),
                json!({
                    "rest_pose": o.rest_pose,
                    "idle": o.idle,
                    "transitions": transitions,
                    "ids": ids.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
                    "strings": strings,
                }),
            );
            let mut pp = serde_json::Map::new();
            for (k, v) in &m.poses {
                pp.insert(k.to_string(), json!(self.picture_hash(v)));
            }
            d.insert("pose_pixels".into(), Value::Object(pp));
        }
        Value::Object(d)
    }

    /// Interpreter B over one animation (reader.py `trace`). `draw(scale)` returns a value in
    /// `[0, scale)`.
    pub fn trace(
        &self,
        name: &str,
        draw: &mut dyn FnMut(u32) -> u32,
        exit_at: Option<usize>,
        daypart: u32,
        host: u32,
        limit: usize,
    ) -> Vec<Step> {
        let Some(a) = self.c.manifest.animation(name) else {
            return vec![Step::End];
        };
        let fr = &a.frames;
        let n = fr.len() as u32;
        let mut out = Vec::new();
        match self.c.manifest.family {
            Family::Agent => {
                let mut cur = if fr[0].is_invisible_branch_point() {
                    next_agent(fr, 0, false, draw)
                } else {
                    Some(0)
                };
                let mut exiting = false;
                while let Some(i) = cur {
                    let f = &fr[i as usize];
                    out.push(Step::Frame(
                        json!(self.frame_hash(f, None)),
                        f.ms,
                        self.sound_hash(&f.sound),
                    ));
                    if out.len() >= limit {
                        out.push(Step::Limit);
                        return out;
                    }
                    if exit_at.is_some_and(|e| out.len() >= e) && !exiting {
                        if f.exit == Some(Exit::End) {
                            out.push(Step::Cut);
                            return out;
                        }
                        exiting = true;
                    }
                    cur = next_agent(fr, i, exiting, draw);
                }
                out.push(Step::End);
                out
            }
            Family::Office97 => {
                let mut i = 0u32;
                let mut exiting = false;
                while i < n {
                    let f = &fr[i as usize];
                    let ms = if exiting && f.ms > 99 { 100 } else { f.ms };
                    out.push(Step::Frame(json!(f.pose), ms, self.sound_hash(&f.sound)));
                    if out.len() >= limit {
                        out.push(Step::Limit);
                        return out;
                    }
                    if exit_at.is_some_and(|e| out.len() >= e) {
                        exiting = true;
                    }
                    let mut t = i + 1;
                    for b in &f.branch {
                        let take = if exiting {
                            b.target() > i
                        } else {
                            match *b {
                                Branch::Random { p, .. } => {
                                    p >= 32768 || (p > 0 && draw(32768) < p)
                                }
                                Branch::Daypart { value, .. } => daypart == value,
                                Branch::Host { value, .. } => host == value,
                            }
                        };
                        if take {
                            t = b.target();
                            break;
                        }
                    }
                    i = t;
                }
                out.push(Step::End);
                out
            }
        }
    }
}

/// character-format 7.2 `next()`; `None` = END.
pub fn next_agent(
    fr: &[Frame],
    mut cur: u32,
    exiting: bool,
    draw: &mut dyn FnMut(u32) -> u32,
) -> Option<u32> {
    let n = fr.len() as u32;
    let mut t = None;
    for _ in 0..32 {
        let f = &fr[cur as usize];
        t = None;
        if exiting {
            match f.exit {
                Some(Exit::End) => return None,
                Some(Exit::Frame(x)) => t = Some(x),
                None => {}
            }
        }
        if t.is_none() {
            if !f.branch.is_empty() {
                let mut r = draw(100);
                for b in &f.branch {
                    if let Branch::Random { target, p } = *b {
                        if r < p {
                            t = Some(target);
                            break;
                        }
                        r -= p;
                    }
                }
            }
            if t.is_none() {
                t = Some(cur + 1);
            }
        }
        let tt = t.unwrap();
        if tt >= n {
            return None;
        }
        if !fr[tt as usize].is_invisible_branch_point() {
            return Some(tt);
        }
        cur = tt;
    }
    t
}

fn exit_json(e: Option<Exit>) -> Value {
    match e {
        None => Value::Null,
        Some(Exit::End) => json!("end"),
        Some(Exit::Frame(i)) => json!(i),
    }
}

fn branch_json(b: &Branch) -> Value {
    match *b {
        Branch::Random { target, p } => json!([target, p]),
        Branch::Daypart { target, value } => json!([target, "daypart", value]),
        Branch::Host { target, value } => json!([target, "host", value]),
    }
}

fn put<T: serde::Serialize>(m: &mut serde_json::Map<String, Value>, k: &str, v: &Option<T>) {
    if let Some(v) = v {
        m.insert(k.into(), json!(v));
    }
}

fn balloon_json(b: &Balloon) -> Value {
    let mut m = serde_json::Map::new();
    put(&mut m, "enabled", &b.enabled);
    put(&mut m, "lines", &b.lines);
    put(&mut m, "chars_per_line", &b.chars_per_line);
    put(&mut m, "fg", &b.fg);
    put(&mut m, "bg", &b.bg);
    put(&mut m, "border", &b.border);
    put(&mut m, "font", &b.font);
    put(&mut m, "font_height", &b.font_height);
    put(&mut m, "font_weight", &b.font_weight);
    put(&mut m, "italic", &b.italic);
    put(&mut m, "font_flag2", &b.font_flag2);
    put(&mut m, "size_to_text", &b.size_to_text);
    put(&mut m, "auto_hide", &b.auto_hide);
    put(&mut m, "auto_pace", &b.auto_pace);
    Value::Object(m)
}

fn voice_json(v: &Voice) -> Value {
    let mut m = serde_json::Map::new();
    put(&mut m, "engine", &v.engine);
    put(&mut m, "mode", &v.mode);
    put(&mut m, "speed", &v.speed);
    put(&mut m, "pitch", &v.pitch);
    put(&mut m, "language", &v.language);
    put(&mut m, "dialect", &v.dialect);
    put(&mut m, "gender", &v.gender);
    put(&mut m, "age", &v.age);
    put(&mut m, "style", &v.style);
    Value::Object(m)
}

/// Shared draw source of A-TEST-2: splitmix64, `draw(s) = next() % s`.
#[derive(Clone, Debug)]
pub struct SplitMix64(pub u64);

impl SplitMix64 {
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    pub fn draw(&mut self, s: u32) -> u32 {
        (self.next_u64() % s as u64) as u32
    }
}

/// The trace plan shared with `scripts/oracle/dump.py`: for every animation, `SEEDS` seeds times
/// 3 modes. Mode 0 runs normally, modes 1 and 2 enter exit mode at a drawn step.
pub const SEEDS: u64 = 12;
pub const TRACE_LIMIT: usize = 300;

/// (exit_at, daypart, host, draw seed) for one trace of an animation with `frames` frames.
pub fn trace_params(seed: u64, mode: u64, frames: usize) -> (Option<usize>, u32, u32, u64) {
    let mut p = SplitMix64(seed * 7919 + mode);
    let exit_at = if mode == 0 {
        None
    } else {
        Some(1 + p.draw(1 + 2 * frames as u32) as usize)
    };
    let dp = p.draw(4);
    let host = p.draw(7);
    (exit_at, dp, host, seed)
}

/// Every trace of the plan, as JSON: `{animation name: [[step, ...], ...]}` in plan order.
pub fn all_traces(d: &Dumper) -> Value {
    let mut out = serde_json::Map::new();
    for a in &d.c.manifest.animations {
        let mut list = Vec::new();
        for seed in 0..SEEDS {
            for mode in 0..3 {
                let (exit_at, dp, host, s) = trace_params(seed, mode, a.frames.len());
                let mut g = SplitMix64(s);
                let mut draw = |k: u32| g.draw(k);
                let t = d.trace(&a.name, &mut draw, exit_at, dp, host, TRACE_LIMIT);
                list.push(Value::Array(t.iter().map(Step::to_json).collect()));
            }
        }
        out.insert(a.name.clone(), Value::Array(list));
    }
    Value::Object(out)
}

/// First difference between two JSON values (for test messages), or `None`.
pub fn diff(a: &Value, b: &Value, path: &str) -> Option<String> {
    match (a, b) {
        (Value::Object(x), Value::Object(y)) => {
            let mut keys: Vec<&String> = x.keys().chain(y.keys()).collect();
            keys.sort();
            keys.dedup();
            for k in keys {
                match (x.get(k), y.get(k)) {
                    (Some(p), Some(q)) => {
                        if let Some(d) = diff(p, q, &format!("{path}.{k}")) {
                            return Some(d);
                        }
                    }
                    _ => return Some(format!("{path}.{k} missing on one side")),
                }
            }
            None
        }
        (Value::Array(x), Value::Array(y)) => {
            if x.len() != y.len() {
                return Some(format!("{path}: length {} != {}", x.len(), y.len()));
            }
            x.iter()
                .zip(y)
                .enumerate()
                .find_map(|(i, (p, q))| diff(p, q, &format!("{path}[{i}]")))
        }
        _ if a == b => None,
        _ => Some(format!("{path}: {a} != {b}")),
    }
}
