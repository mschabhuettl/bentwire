//! Locating the inputs of the real-data tests (architecture A-TEST-1). Used by the
//! `#[ignore]` tests only.
//!
//! - `BENTWIRE_SOURCES`: one or more directories (colon-separated) with your own copies of the
//!   original files, in any layout. A file is found by its file name and checked by the
//!   SHA-256 listed for it in `docs/sources.md`.
//! - `BENTWIRE_REFERENCE`: the directory with the Python reference parsers and prototype;
//!   the files are found by file name below it.
//!
//! An unset variable or a missing file panics with "NOT RUN", so a test can never pass
//! without its data.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Mutex, OnceLock};

use sha2::{Digest, Sha256};

/// One file listed in `docs/sources.md`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Listed {
    pub name: String,
    pub sha256: String,
}

fn env_dirs(var: &str) -> Vec<PathBuf> {
    match std::env::var_os(var) {
        Some(v) if !v.is_empty() => std::env::split_paths(&v).collect(),
        _ => panic!("NOT RUN: set {var} (see docs/architecture.md A-TEST-1)"),
    }
}

/// Every file below `roots`, by file name, in sorted path order. Symlinked files count,
/// symlinked directories below a root are not followed.
fn index(roots: &[PathBuf]) -> HashMap<String, Vec<PathBuf>> {
    fn walk(d: &Path, out: &mut HashMap<String, Vec<PathBuf>>) {
        let Ok(rd) = std::fs::read_dir(d) else {
            return;
        };
        let mut es: Vec<_> = rd.filter_map(Result::ok).collect();
        es.sort_by_key(|e| e.file_name());
        for e in es {
            let p = e.path();
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                walk(&p, out);
            } else if p.is_file() {
                out.entry(e.file_name().to_string_lossy().into_owned())
                    .or_default()
                    .push(p);
            }
        }
    }
    let mut out = HashMap::new();
    for r in roots {
        walk(r, &mut out);
    }
    out
}

fn sources_index() -> &'static HashMap<String, Vec<PathBuf>> {
    static I: OnceLock<HashMap<String, Vec<PathBuf>>> = OnceLock::new();
    let roots = env_dirs("BENTWIRE_SOURCES");
    I.get_or_init(|| index(&roots))
}

pub fn sha256_file(p: &Path) -> String {
    static C: OnceLock<Mutex<HashMap<PathBuf, String>>> = OnceLock::new();
    let c = C.get_or_init(Default::default);
    if let Some(s) = c.lock().unwrap().get(p) {
        return s.clone();
    }
    let b = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    let s: String = Sha256::digest(&b)
        .iter()
        .map(|x| format!("{x:02x}"))
        .collect();
    c.lock().unwrap().insert(p.to_owned(), s.clone());
    s
}

fn sources_md() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../docs/sources.md")
}

/// The files listed in the tables of `docs/sources.md` section `section` (a heading such
/// as `### 1.1 ...`), in table order: rows whose first cell is a file name in backticks
/// and that carry a SHA-256 in backticks.
pub fn listed(section: &str) -> Vec<Listed> {
    let text = std::fs::read_to_string(sources_md()).unwrap();
    let mut inside = false;
    let mut out = Vec::new();
    for line in text.lines() {
        if line.starts_with('#') {
            inside = line.trim_start_matches('#').trim().split(' ').next() == Some(section);
            continue;
        }
        if !inside || !line.starts_with("| `") {
            continue;
        }
        let cells: Vec<&str> = line.trim_matches('|').split('|').map(str::trim).collect();
        let unquote = |c: &str| {
            c.strip_prefix('`')
                .and_then(|c| c.strip_suffix('`'))
                .map(str::to_owned)
        };
        let sha = cells.iter().filter_map(|c| unquote(c)).find(|c| {
            c.len() == 64
                && c.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if let (Some(name), Some(sha256)) = (unquote(cells[0]), sha) {
            out.push(Listed { name, sha256 });
        }
    }
    assert!(
        !out.is_empty(),
        "docs/sources.md: no files listed in section {section}"
    );
    out
}

fn candidates(l: &Listed) -> Vec<PathBuf> {
    let found: Vec<PathBuf> = sources_index()
        .get(&l.name)
        .into_iter()
        .flatten()
        .filter(|p| sha256_file(p) == l.sha256)
        .cloned()
        .collect();
    if found.is_empty() {
        panic!(
            "NOT RUN: {} (SHA-256 {}) not found below BENTWIRE_SOURCES",
            l.name, l.sha256
        );
    }
    found
}

/// The file `l` below `BENTWIRE_SOURCES` (the first match in path order).
pub fn locate(l: &Listed) -> PathBuf {
    candidates(l).swap_remove(0)
}

/// The folder of a web character whose definition file is `l` (ACF next to its animation
/// files). Of several identical copies, the folder with the most files wins.
pub fn locate_dir(l: &Listed) -> PathBuf {
    let count = |p: &Path| std::fs::read_dir(p).map_or(0, |r| r.count());
    candidates(l)
        .into_iter()
        .map(|p| p.parent().unwrap().to_owned())
        .max_by_key(|d| count(d))
        .unwrap()
}

/// The file named `name` in `section`.
pub fn locate_name(section: &str, name: &str) -> PathBuf {
    let l = listed(section)
        .into_iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("docs/sources.md section {section} lists no {name}"));
    locate(&l)
}

/// The directories of `BENTWIRE_SOURCES` (panics with "NOT RUN" when unset).
pub fn sources_dirs() -> Vec<PathBuf> {
    env_dirs("BENTWIRE_SOURCES")
}

/// Every file of `section`, located; identical files listed twice resolve to one path.
pub fn locate_section(section: &str) -> Vec<PathBuf> {
    let mut out: Vec<PathBuf> = Vec::new();
    for l in listed(section) {
        let p = locate(&l);
        if !out.contains(&p) {
            out.push(p);
        }
    }
    out
}

/// The directory below `BENTWIRE_REFERENCE` that holds all of `names` (the first in path
/// order). Caches and virtual environments are skipped.
pub fn reference_dir(names: &[&str]) -> PathBuf {
    fn walk(d: &Path, names: &[&str]) -> Option<PathBuf> {
        if names.iter().all(|n| d.join(n).is_file()) {
            return Some(d.to_owned());
        }
        let mut es: Vec<_> = std::fs::read_dir(d).ok()?.filter_map(Result::ok).collect();
        es.sort_by_key(|e| e.file_name());
        es.into_iter()
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter(|e| {
                let n = e.file_name();
                !matches!(
                    n.to_str(),
                    Some("cache" | "__pycache__" | "venv" | ".venv" | "site-packages")
                )
            })
            .find_map(|e| walk(&e.path(), names))
    }
    env_dirs("BENTWIRE_REFERENCE")
        .iter()
        .find_map(|r| walk(r, names))
        .unwrap_or_else(|| panic!("NOT RUN: {names:?} not found below BENTWIRE_REFERENCE"))
}

/// The root of `BENTWIRE_REFERENCE` (panics with "NOT RUN" when unset).
pub fn reference_root() -> PathBuf {
    env_dirs("BENTWIRE_REFERENCE").swap_remove(0)
}

/// The prototype directory (the one holding `oracle.py` and `reader.py`).
pub fn prototype_dir() -> PathBuf {
    reference_dir(&["oracle.py", "reader.py", "sources.py"])
}
