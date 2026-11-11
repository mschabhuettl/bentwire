//! Shared helpers for comparing the Rust readers with the Python research parsers
//! (architecture A-TEST-1). Used by the `#[ignore]` oracle tests only.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use agent_formats::common::{Region, Voice};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn env_dir(var: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => PathBuf::from(v),
        _ => panic!("NOT RUN: set {var} (see docs/architecture.md A-TEST-1)"),
    }
}

pub fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

/// A fresh, empty temp directory.
pub fn temp_dir(tag: &str) -> PathBuf {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("bentwire-{tag}-{}-{n}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Runs `scripts/oracle/<script>` on `files` and returns one JSON value per file, read from
/// `<out>/<basename>.json`.
pub fn python(script: &str, files: &[PathBuf], extra: &[&str]) -> Vec<Value> {
    let research = env_dir("BENTWIRE_REFERENCE");
    let out = temp_dir("oracle");
    let st = Command::new("python3")
        .arg("-I")
        .arg(repo().join("scripts/oracle").join(script))
        .arg("--research")
        .arg(&research)
        .arg("--out")
        .arg(&out)
        .args(extra)
        .args(files)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .status()
        .expect("run python3");
    assert!(st.success(), "{script} failed");
    let res = files
        .iter()
        .map(|f| {
            let p = out.join(format!("{}.json", f.file_name().unwrap().to_string_lossy()));
            serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
        })
        .collect();
    std::fs::remove_dir_all(&out).ok();
    res
}

/// Panics with the first differing JSON path.
pub fn assert_same(rust: &Value, python: &Value, what: &str) {
    fn walk(a: &Value, b: &Value, path: &mut String) -> Option<String> {
        match (a, b) {
            (Value::Object(x), Value::Object(y)) => {
                let mut keys: Vec<_> = x.keys().chain(y.keys()).collect();
                keys.sort();
                keys.dedup();
                for k in keys {
                    let n = path.len();
                    path.push_str(&format!(".{k}"));
                    let r = walk(
                        x.get(k).unwrap_or(&Value::Null),
                        y.get(k).unwrap_or(&Value::Null),
                        path,
                    );
                    if r.is_some() {
                        return r;
                    }
                    path.truncate(n);
                }
                None
            }
            (Value::Array(x), Value::Array(y)) => {
                if x.len() != y.len() {
                    return Some(format!("{path}: length {} vs {}", x.len(), y.len()));
                }
                for (i, (p, q)) in x.iter().zip(y).enumerate() {
                    let n = path.len();
                    path.push_str(&format!("[{i}]"));
                    let r = walk(p, q, path);
                    if r.is_some() {
                        return r;
                    }
                    path.truncate(n);
                }
                None
            }
            _ if a == b => None,
            _ => Some(format!("{path}: rust {a} vs python {b}")),
        }
    }
    if let Some(d) = walk(rust, python, &mut String::new()) {
        panic!("{what}: Rust and Python disagree at {d}");
    }
}

/// RGNDATA bytes rebuilt from a parsed region (so it can be hashed like the stored bytes).
pub fn region_bytes(r: &Region) -> Vec<u8> {
    let mut o = Vec::new();
    for v in [32u32, 1, r.rects.len() as u32, r.rgn_size] {
        o.extend_from_slice(&v.to_le_bytes());
    }
    for x in std::iter::once(&r.bound).chain(&r.rects) {
        for v in [x.left, x.top, x.right, x.bottom] {
            o.extend_from_slice(&v.to_le_bytes());
        }
    }
    o
}

pub fn voice_json(v: &Option<Voice>) -> Value {
    match v {
        None => Value::Null,
        Some(v) => json!({
            "engine": v.engine.to_string(), "mode": v.mode.to_string(), "speed": v.speed, "pitch": v.pitch,
            "extra": v.extra.as_ref().map(|e| json!([e.language, e.dialect, e.gender, e.age, e.style])),
        }),
    }
}

pub fn palette_bytes(p: &[agent_formats::common::Rgbquad]) -> Vec<u8> {
    p.iter().flat_map(|q| [q.b, q.g, q.r, q.reserved]).collect()
}

/// The layout written by `scripts/oracle/acs_dump.py`.
pub fn acs_json(a: &agent_formats::acs::Acs) -> Value {
    let c = &a.character;
    json!({
        "version": c.version,
        "guid": c.guid.to_string(),
        "width": c.width, "height": c.height, "transparent": c.transparent, "flags": c.flags,
        "animation_set": [c.animation_set.0, c.animation_set.1],
        "palette": [c.palette.len(), sha(&palette_bytes(&c.palette))],
        "tray_icon": c.tray_icon.as_ref().map(|t| json!([sha(&t.mask_dib), sha(&t.color_dib)])),
        "states": c.states.iter().map(|s| json!([s.name, s.animations])).collect::<Vec<_>>(),
        "localized": c.localized.iter().map(|l| json!([l.langid, l.name, l.description, l.extra])).collect::<Vec<_>>(),
        "voice": voice_json(&c.voice),
        "balloon": c.balloon.as_ref().map(|b| json!([b.lines, b.chars_per_line, b.foreground, b.background,
            b.border, b.font, b.font_height, b.font_weight, b.italic, b.flag2])),
        "animations": a.animations.iter().map(|an| json!({
            "name": an.name, "record_name": an.record_name, "transition": an.transition,
            "return": an.return_animation,
            "frames": an.frames.iter().map(|f| json!({
                "images": f.images.iter().map(|i| json!([i.image, i.x, i.y])).collect::<Vec<_>>(),
                "sound": f.sound, "duration": f.duration, "exit": f.exit,
                "branches": f.branches.iter().map(|b| json!([b.target, b.probability])).collect::<Vec<_>>(),
                "overlays": f.overlays.iter().map(|o| json!([o.mouth, o.replace_top, o.image, o.x, o.y,
                    o.point_x, o.point_y])).collect::<Vec<_>>(),
            })).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "images": a.images.iter().map(|e| {
            let i = e.image.as_ref().expect("kind-0 images are not comparable with acs.py");
            json!({"checksum": e.checksum, "width": i.bitmap.width, "height": i.bitmap.height,
                "pixels": sha(&i.bitmap.pixels), "region": i.region.as_ref().map(|r| sha(&region_bytes(r)))})
        }).collect::<Vec<_>>(),
        "sounds": a.sounds.iter().map(|s| json!([s.checksum, sha(&s.data)])).collect::<Vec<_>>(),
    })
}
