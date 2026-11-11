//! Shared helpers of the real-data tests (architecture A-TEST-1, A-TEST-2).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use bentwire_character::Character;
use bentwire_character::dump::{Dumper, all_traces, diff};
use serde_json::Value;

/// The Office palette source for bitmap actors in the tests: `CLIPPIT.ACS`.
pub fn office_palette() -> PathBuf {
    agent_fixtures::real::locate_name("1.1", "CLIPPIT.ACS")
}

/// A fresh temp directory (removed by the OS, never inside the repo).
pub fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bentwire-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Scratch copy of the prototype (it writes pickles into `cache/`). `BENTWIRE_PROTO_COPY`
/// reuses an existing copy (scripts/oracle-check.sh makes one); otherwise one is made here.
pub fn proto_copy() -> &'static Path {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| {
        if let Some(p) = std::env::var_os("BENTWIRE_PROTO_COPY") {
            return PathBuf::from(p);
        }
        let src = agent_fixtures::real::prototype_dir();
        let dst = temp_dir("proto");
        let st = Command::new("rsync")
            .args([
                "-a",
                "--exclude",
                "msadp32/",
                "--exclude",
                "out/",
                "--exclude",
                "__pycache__/",
            ])
            .arg(format!("{}/", src.display()))
            .arg(format!("{}/", dst.display()))
            .status()
            .expect("rsync");
        assert!(st.success(), "copying the prototype failed");
        dst
    })
}

/// Repo root (two levels above this crate).
pub fn repo() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

/// Run scripts/oracle/dump.py with `args`; returns the JSON it wrote.
pub fn oracle_json(args: &[&str], out: &Path) -> Value {
    let st = Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg("-I")
        .arg(repo().join("scripts/oracle/dump.py"))
        .arg("--proto")
        .arg(proto_copy())
        .args(args)
        .arg(out)
        .status()
        .expect("python3");
    assert!(st.success(), "dump.py {args:?} failed");
    serde_json::from_slice(&std::fs::read(out).unwrap()).unwrap()
}

/// ACT actors converted fully (bitmap actors and one vector actor); every other actor of
/// docs/sources.md 1.4 is kind `actg` (animation graph only, pictures replaced by 1x1
/// placeholders).
const ACT_FULL: &[&str] = &[
    "GENIUS.act",
    "MNATURE.act",
    "EARL.act",
    "ROBOT.act",
    "ROCKY(Office).act",
    "OFFLADY.act",
    "TOFFLOGO.act",
    "TUTOR.ACT",
    "ROCKY(Greetings).ACT",
    "ROCKY(EarlyGreetings).ACT",
    "CLIPPIT.ACT",
];

/// The round-trip samples as (kind, path), from docs/sources.md: `acs` 1.1, `acf` 1.2 (the
/// character folders), `ole15` 1.3, `act` and `actg` 1.4.
pub fn samples() -> Vec<(String, PathBuf)> {
    use agent_fixtures::real::{listed, locate, locate_dir, locate_section};
    let mut out = Vec::new();
    let mut add = |kind: &str, ps: Vec<PathBuf>| {
        out.extend(ps.into_iter().map(|p| (kind.to_owned(), p)));
    };
    add("acs", locate_section("1.1"));
    add("acf", listed("1.2").iter().map(locate_dir).collect());
    add("ole15", locate_section("1.3"));
    let act = listed("1.4");
    let (full, graph): (Vec<_>, Vec<_>) = act
        .iter()
        .partition(|l| ACT_FULL.contains(&l.name.as_str()));
    add("act", full.into_iter().map(locate).collect());
    add("actg", graph.into_iter().map(locate).collect());
    out
}

/// Rust dump and traces of a converted character.
pub fn rust_json(c: &Character) -> Value {
    let d = Dumper::new(c);
    serde_json::json!({ "dump": d.dump(), "traces": all_traces(&d) })
}

/// Compare oracle and reader JSON: dump and traces. `Err` describes the first difference.
pub fn compare(oracle: &Value, got: &Value) -> Result<(usize, usize), String> {
    if let Some(d) = diff(&oracle["dump"], &got["dump"], "dump") {
        return Err(d);
    }
    if let Some(d) = diff(&oracle["traces"], &got["traces"], "traces") {
        return Err(d);
    }
    let traces = got["traces"].as_object().map_or(0, |o| {
        o.values().map(|v| v.as_array().map_or(0, Vec::len)).sum()
    });
    let anims = got["dump"]["animations"].as_array().map_or(0, Vec::len);
    Ok((anims, traces))
}

/// Run `f` over `items` on a few threads, keeping order.
pub fn par_map<T: Sync, R: Send>(items: &[T], f: impl Fn(&T) -> R + Sync) -> Vec<R> {
    let n = std::thread::available_parallelism()
        .map_or(4, |n| n.get())
        .min(8);
    let next = std::sync::atomic::AtomicUsize::new(0);
    let out: Vec<std::sync::Mutex<Option<R>>> =
        items.iter().map(|_| std::sync::Mutex::new(None)).collect();
    std::thread::scope(|s| {
        for _ in 0..n {
            s.spawn(|| {
                loop {
                    let i = next.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    if i >= items.len() {
                        break;
                    }
                    *out[i].lock().unwrap() = Some(f(&items[i]));
                }
            });
        }
    });
    out.into_iter()
        .map(|m| m.into_inner().unwrap().unwrap())
        .collect()
}

/// Map the Rust JSON onto the oracle's naming before comparing. ACS, ACF and 1.5 use the same
/// names on both sides. ACT: the oracle names groups after the Office 97 type library, the
/// converter after Office XP (D6); both are mapped through `office97.ids` (A-TEST-2), and the
/// D6 alias states are removed after checking that each equals its group.
pub fn normalize(kind: &str, oracle: &Value, mut got: Value) -> Value {
    if kind != "act" && kind != "actg" {
        return got;
    }
    use std::collections::HashMap;
    let ids = |v: &Value| -> Vec<(String, u64)> {
        v["dump"]["office97"]["ids"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| (e[0].as_str().unwrap().to_owned(), e[1].as_u64().unwrap()))
            .collect()
    };
    let theirs: HashMap<u64, String> = ids(oracle).into_iter().map(|(n, g)| (g, n)).collect();
    let mine = ids(&got);
    let map: HashMap<String, String> = mine
        .iter()
        .filter_map(|(n, g)| theirs.get(g).map(|t| (n.clone(), t.clone())))
        .collect();
    let lower: HashMap<String, String> = map
        .iter()
        .map(|(a, b)| (a.to_lowercase(), b.to_lowercase()))
        .collect();
    let ren = |name: &str, m: &HashMap<String, String>| -> String {
        if let Some(t) = m.get(name) {
            return t.clone();
        }
        if let Some((base, k)) = name.rsplit_once('.')
            && k.bytes().all(|b| b.is_ascii_digit())
            && let Some(t) = m.get(base)
        {
            return format!("{t}.{k}");
        }
        name.to_owned()
    };
    let d = &mut got["dump"];
    for a in d["animations"].as_array_mut().unwrap() {
        a[0] = Value::from(ren(a[0].as_str().unwrap(), &map));
    }
    // Alias states (D6): each must list exactly its group's members; then they are dropped.
    let alias_of: HashMap<String, u64> = bentwire_convert::act::STATE_ALIASES
        .iter()
        .map(|(a, g)| (a.to_lowercase(), *g as u64))
        .collect();
    let group_of: HashMap<u64, String> = mine.iter().map(|(n, g)| (*g, n.to_lowercase())).collect();
    let states = d["states"].as_array().unwrap().clone();
    let members = |name: &str| states.iter().find(|s| s[0] == name).map(|s| s[1].clone());
    let mut kept = Vec::new();
    for s in &states {
        let n = s[0].as_str().unwrap();
        if let Some(g) = alias_of.get(n) {
            let group = group_of.get(g).expect("alias state without its group");
            assert_eq!(
                Some(s[1].clone()),
                members(group),
                "alias state {n} differs from its group"
            );
            continue;
        }
        let ms: Vec<Value> = s[1]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| Value::from(ren(x.as_str().unwrap(), &lower)))
            .collect();
        kept.push(serde_json::json!([ren(n, &lower), ms]));
    }
    d["states"] = Value::Array(kept);
    let o = &mut d["office97"];
    o["idle"] = Value::from(ren(o["idle"].as_str().unwrap(), &map));
    for t in o["transitions"].as_array_mut().unwrap() {
        t[2] = Value::from(ren(t[2].as_str().unwrap(), &map));
    }
    let mut new_ids: Vec<(String, u64)> = mine.iter().map(|(n, g)| (ren(n, &map), *g)).collect();
    new_ids.sort();
    o["ids"] = serde_json::json!(new_ids);
    let traces = got["traces"].as_object().unwrap().clone();
    got["traces"] = Value::Object(
        traces
            .into_iter()
            .map(|(k, v)| (ren(&k, &map), v))
            .collect(),
    );
    if oracle["is_wmf"].as_bool() == Some(true) {
        // No reference rasteriser (character-format 11): pose ids only, no palette.
        let d = &mut got["dump"];
        d["palette"] = Value::Null;
        if let Some(pp) = d["pose_pixels"].as_object_mut() {
            for v in pp.values_mut() {
                *v = Value::Null;
            }
        }
    }
    got
}

/// The file the Rust converter takes for a sample: the `.acf` inside a web character folder,
/// the sample itself otherwise.
pub fn converter_input(path: &Path) -> PathBuf {
    if path.is_dir() {
        let mut acf: Vec<PathBuf> = std::fs::read_dir(path)
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("acf")))
            .collect();
        acf.sort();
        return acf.into_iter().next().expect("no .acf in folder");
    }
    path.to_owned()
}

/// Round trip of every sample of `kind` against the oracle, folder and `.bwc`. Returns
/// (characters, animations, traces) and panics with every difference.
pub fn round_trip_kind(kind: &str) -> (usize, usize, usize) {
    round_trip_with(kind, &bentwire_convert::ConvertOptions::default())
}

/// [`round_trip_kind`] with converter options.
pub fn round_trip_with(
    kind: &str,
    opts: &bentwire_convert::ConvertOptions,
) -> (usize, usize, usize) {
    let list: Vec<PathBuf> = samples()
        .into_iter()
        .filter(|(k, _)| k == kind)
        .map(|(_, p)| p)
        .collect();
    assert!(!list.is_empty(), "no samples of kind {kind}");
    let tmp = temp_dir(&format!("{kind}-rt"));
    // The prototype caches by file name: a second file of the same name (two copies of
    // Ozzar.acs) runs from a renamed copy.
    let mut seen = std::collections::HashSet::new();
    let list: Vec<PathBuf> = list
        .into_iter()
        .enumerate()
        .map(|(i, p)| {
            if seen.insert(p.file_name().unwrap().to_owned()) || p.is_dir() {
                return p;
            }
            let q = tmp.join(format!(
                "{}-{i}.{}",
                p.file_stem().unwrap().to_string_lossy(),
                p.extension().unwrap().to_string_lossy()
            ));
            std::fs::copy(&p, &q).unwrap();
            q
        })
        .collect();
    let list: Vec<(PathBuf, String)> = list.iter().cloned().zip(output_stems(&list)).collect();
    let results = par_map(&list, |(path, stem)| {
        let stem = stem.clone();
        let oracle = oracle_json(
            &["oracle", kind, path.to_str().unwrap()],
            &tmp.join(format!("{stem}.oracle.json")),
        );
        let pkg = bentwire_convert::convert_to_package_with(&converter_input(path), opts)
            .map_err(|e| format!("{stem}: {e}"))?;
        let folder = tmp.join(&stem);
        let bwc = tmp.join(format!("{stem}.bwc"));
        pkg.write_folder(&folder).unwrap();
        pkg.write_bwc(&bwc).unwrap();
        let mut counts = (0, 0);
        for p in [&folder, &bwc] {
            let c = Character::load(p).map_err(|e| format!("{stem}: {e}"))?;
            if !c.warnings.is_empty() {
                return Err(format!("{stem}: warnings {:?}", c.warnings));
            }
            let got = normalize(kind, &oracle, rust_json(&c));
            counts = compare(&oracle, &got).map_err(|d| format!("{} : {d}", p.display()))?;
        }
        let missing = c_missing(&pkg);
        Ok::<_, String>((stem, counts, missing))
    });
    let mut bad = Vec::new();
    let (mut n, mut anims, mut traces) = (0, 0, 0);
    for r in results {
        match r {
            Ok((s, (a, t), missing)) => {
                println!(
                    "{kind} {s}: {a} animations, {t} traces equal (folder and .bwc){}",
                    if missing.is_empty() {
                        String::new()
                    } else {
                        format!("; {} listed in source.missing_animations", missing.len())
                    }
                );
                n += 1;
                anims += a;
                traces += t;
            }
            Err(e) => bad.push(e),
        }
    }
    println!("{kind}: {n} characters, {anims} animations, {traces} traces");
    assert!(bad.is_empty(), "{bad:#?}");
    (n, anims, traces)
}

/// Output names under the round trip's temp dir, one per sample and unique: sources with
/// the same base name (two ACF folders named `data`) would otherwise share and overwrite
/// their outputs while the samples run in parallel.
pub fn output_stems(paths: &[PathBuf]) -> Vec<String> {
    let mut seen = std::collections::HashSet::new();
    paths
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let stem = p
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .to_lowercase()
                .replace(['(', ')', ' '], "_");
            if seen.insert(stem.clone()) {
                stem
            } else {
                format!("{stem}-{i}")
            }
        })
        .collect()
}

fn c_missing(p: &bentwire_convert::PackageData) -> Vec<String> {
    p.manifest.missing_animations()
}
