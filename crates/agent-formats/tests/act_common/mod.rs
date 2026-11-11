//! Shared helpers for the ACT tests: oracle driver and the canonical JSON dump.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use agent_formats::act::{self, Payload};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

pub fn sha(b: &[u8]) -> String {
    Sha256::digest(b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect()
}

pub fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

/// The ACT and ACP sample set: the actor collection (docs/sources.md 1.4) and the actors
/// and previews of the Office 97 CD (1.5), in table order.
pub fn samples() -> Vec<PathBuf> {
    use agent_fixtures::real::{listed, locate};
    let mut out: Vec<PathBuf> = listed("1.4").iter().map(locate).collect();
    out.extend(
        listed("1.5")
            .iter()
            .filter(|l| l.name.ends_with(".ACT") || l.name.ends_with(".ACP"))
            .map(locate),
    );
    out
}

pub fn temp_dir(tag: &str) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let d = std::env::temp_dir().join(format!("bentwire-{tag}-{}-{nanos}", std::process::id()));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Run the oracle driver; returns (key, path) pairs and the output directory.
pub fn run_oracle(files: &[PathBuf]) -> (Vec<(String, PathBuf)>, PathBuf) {
    let out = temp_dir("act-oracle");
    let script = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/oracle/act_dump.py");
    let st = Command::new("python3")
        .arg("-I")
        .arg(&script)
        .arg(agent_fixtures::real::reference_root())
        .arg(out.join("json"))
        .args(files)
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .current_dir(&out)
        .status()
        .expect("python3");
    assert!(st.success(), "oracle driver failed");
    let idx: Value =
        serde_json::from_slice(&std::fs::read(out.join("json/index.json")).unwrap()).unwrap();
    let list = idx
        .as_array()
        .unwrap()
        .iter()
        .map(|e| {
            (
                e[0].as_str().unwrap().to_string(),
                PathBuf::from(e[1].as_str().unwrap()),
            )
        })
        .collect();
    (list, out)
}

/// The same JSON shape as `act_dump.py`.
pub fn dump(f: &act::ActFile) -> Value {
    let h = &f.header;
    let header = json!([
        h.kind,
        h.rest_pose,
        h.idle_anim,
        h.logical_w,
        h.logical_h,
        h.default_px_w,
        h.default_px_h,
        h.unk10,
        h.unk12,
        h.unk14,
        h.unk16,
        h.rect[0],
        h.rect[1],
        h.rect[2],
        h.rect[3],
        h.nohit_image,
        h.stretch_x0,
        h.stretch_x1
    ]);
    let payloads: Vec<Value> = f
        .payloads
        .iter()
        .map(|p| match p {
            Payload::Composite(cs) => json!({"kind": "composite", "children":
                cs.iter().map(|c| json!([c.id, c.left, c.top, c.right, c.bottom])).collect::<Vec<_>>()}),
            Payload::Wmf(w) => {
                let fns: Vec<u8> = w.records.iter().flat_map(|r| r.function.to_le_bytes()).collect();
                json!({"kind": "wmf", "sha256": sha(&w.bytes), "records": w.records.len(),
                       "functions_sha256": sha(&fns)})
            }
            Payload::Bitmap(b) => json!({"kind": "bitmap", "width": b.width, "height": b.height,
                                         "pixels_sha256": sha(&b.pixels)}),
        })
        .collect();
    json!({
        "major": f.major, "minor": f.minor, "item_name": hex(&f.item_name),
        "header": header,
        "color_table": f.color_table,
        "transitions": f.transitions.iter().map(|t| json!([t.from_pose, t.to_pose, t.anim_id])).collect::<Vec<_>>(),
        "transition_rest_pose": f.transition_rest_pose,
        "payloads": payloads,
        "sounds": f.sounds.iter().map(|s| json!({"sha256": sha(&s.bytes), "format_tag": s.format_tag,
            "channels": s.channels, "rate": s.sample_rate, "bits": s.bits_per_sample})).collect::<Vec<_>>(),
        "animations": f.animations.iter().map(|g| json!([g.id, g.variants.iter().map(|v| json!({
            "header": v.header,
            "commands": v.commands.iter().map(|c| json!([c.kind, c.a, c.b])).collect::<Vec<_>>()
        })).collect::<Vec<_>>()])).collect::<Vec<_>>(),
        "strings": f.strings.iter().map(|g| json!([g.id, g.variants.iter().map(|v| hex(v)).collect::<Vec<_>>()]))
            .collect::<Vec<_>>(),
    })
}
