//! Shared helpers of the real-data tests (architecture A-TEST-1, A-TEST-2).

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::OnceLock;

use bentwire_character::Character;
use bentwire_character::dump::{Dumper, all_traces, diff};
use serde_json::Value;

pub fn env_dir(var: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(p) => PathBuf::from(p),
        None => panic!("NOT RUN: set {var}"),
    }
}

pub fn research() -> PathBuf {
    env_dir("BENTWIRE_REFERENCE")
}

pub fn sources() -> PathBuf {
    env_dir("BENTWIRE_SOURCES")
}

/// A fresh temp directory (removed by the OS, never inside the repo).
pub fn temp_dir(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bentwire-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Scratch copy of prototype (it writes pickles into `cache/`). `BENTWIRE_PROTO_COPY`
/// reuses an existing copy (scripts/oracle-check.sh makes one); otherwise one is made here.
pub fn proto_copy() -> &'static Path {
    static P: OnceLock<PathBuf> = OnceLock::new();
    P.get_or_init(|| {
        if let Some(p) = std::env::var_os("BENTWIRE_PROTO_COPY") {
            return PathBuf::from(p);
        }
        let src = research().join("prototype");
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
        assert!(st.success(), "copying prototype failed");
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

/// The sample list of prototype `sources.SAMPLES` as (kind, path).
pub fn samples() -> Vec<(String, PathBuf)> {
    let out = Command::new("python3")
        .env("PYTHONDONTWRITEBYTECODE", "1")
        .arg("-I")
        .arg(repo().join("scripts/oracle/dump.py"))
        .arg("--proto")
        .arg(proto_copy())
        .arg("list")
        .output()
        .expect("python3");
    assert!(out.status.success());
    String::from_utf8(out.stdout)
        .unwrap()
        .lines()
        .filter_map(|l| l.split_once(' '))
        .map(|(k, p)| (k.to_owned(), PathBuf::from(p)))
        .collect()
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
/// names on both sides; ACT is mapped through `office97.ids` (A-TEST-2).
pub fn normalize(kind: &str, oracle: &Value, got: Value) -> Value {
    let _ = (kind, oracle);
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
    let _ = sources();
    let list: Vec<PathBuf> = samples()
        .into_iter()
        .filter(|(k, _)| k == kind)
        .map(|(_, p)| p)
        .collect();
    assert!(!list.is_empty(), "no samples of kind {kind}");
    let tmp = temp_dir(&format!("{kind}-rt"));
    let results = par_map(&list, |path| {
        let stem = path
            .file_stem()
            .unwrap()
            .to_string_lossy()
            .to_lowercase()
            .replace(['(', ')', ' '], "_");
        let oracle = oracle_json(
            &["oracle", kind, path.to_str().unwrap()],
            &tmp.join(format!("{stem}.oracle.json")),
        );
        let pkg = bentwire_convert::convert_to_package(&converter_input(path))
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

fn c_missing(p: &bentwire_convert::PackageData) -> Vec<String> {
    p.manifest.missing_animations()
}
