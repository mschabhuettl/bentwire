//! Character discovery: search roots, packs and the cheap check (B-CHAR-1, A-PACK-1).
//! Display names and startup selection are the core's (B-CHAR-3, B-CHAR-4).

use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// Cheap-check results by entry path, valid while the manifest file (`character.toml` or
/// the `.bwc`) keeps its modification time and size. Rescans on use (B-CHAR-1) read only
/// what changed.
pub type Cache = HashMap<PathBuf, (Option<SystemTime>, u64, Option<Entry>)>;

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub id: String,
    pub path: PathBuf,
    pub info: Vec<Info>,
}

#[derive(Debug, Clone, Default, Deserialize, PartialEq)]
#[serde(default)]
pub struct Info {
    pub lang: String,
    pub name: String,
    pub description: String,
}

#[derive(Deserialize)]
struct CheapManifest {
    format: i64,
    family: String,
    #[serde(default)]
    character: CheapCharacter,
}

#[derive(Deserialize, Default)]
struct CheapCharacter {
    #[serde(default)]
    info: Vec<Info>,
}

/// Search roots in order (A-PACK-1): `--characters`, config `character_paths`,
/// `$XDG_DATA_HOME/bentwire`, each `$XDG_DATA_DIRS` entry plus `/bentwire`.
pub fn roots(cli: &[PathBuf], config_paths: &[PathBuf]) -> Vec<PathBuf> {
    let mut r: Vec<PathBuf> = cli.iter().chain(config_paths).cloned().collect();
    r.push(crate::config::data_home().join("bentwire"));
    r.extend(
        crate::config::data_dirs()
            .into_iter()
            .map(|d| d.join("bentwire")),
    );
    r
}

/// All characters under the roots, sorted by id; the first entry of an id wins.
#[cfg(test)]
pub fn discover(roots: &[PathBuf]) -> Vec<Entry> {
    discover_cached(roots, &mut Cache::new())
}

/// All characters under the roots, sorted by id; the first entry of an id wins. `cache`
/// is kept between scans; entries no longer found are dropped from it.
pub fn discover_cached(roots: &[PathBuf], cache: &mut Cache) -> Vec<Entry> {
    let mut next = Cache::new();
    let mut found: BTreeMap<String, Entry> = BTreeMap::new();
    for root in roots {
        for e in scan_root(root, cache, &mut next) {
            found.entry(e.id.clone()).or_insert(e);
        }
    }
    *cache = next;
    found.into_values().collect()
}

fn scan_root(root: &Path, old: &Cache, new: &mut Cache) -> Vec<Entry> {
    if root.join("character.toml").is_file() {
        return check_cached(root, old, new).into_iter().collect();
    }
    if root.join("pack.toml").is_file() {
        return scan_dir(root, false, old, new);
    }
    let chars = root.join("characters");
    if chars.is_dir() {
        return scan_dir(&chars, true, old, new);
    }
    if root.file_name().is_some_and(|n| n == "characters") && root.is_dir() {
        return scan_dir(root, true, old, new);
    }
    Vec::new()
}

fn sorted_children(dir: &Path) -> Vec<PathBuf> {
    let mut v: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd.filter_map(|e| e.ok().map(|e| e.path())).collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v
}

/// `allow_packs`: a `characters/` dir may hold packs; a pack's children may not.
fn scan_dir(dir: &Path, allow_packs: bool, old: &Cache, new: &mut Cache) -> Vec<Entry> {
    let mut out = Vec::new();
    for p in sorted_children(dir) {
        let is_bwc = p.extension().is_some_and(|e| e == "bwc") && p.is_file();
        if p.is_dir() && p.join("character.toml").is_file() || is_bwc {
            out.extend(check_cached(&p, old, new));
        } else if allow_packs && p.is_dir() && p.join("pack.toml").is_file() {
            out.extend(scan_dir(&p, false, old, new));
        }
    }
    out
}

fn check_cached(path: &Path, old: &Cache, new: &mut Cache) -> Option<Entry> {
    let file = if path.is_dir() {
        path.join("character.toml")
    } else {
        path.to_path_buf()
    };
    let key = std::fs::metadata(&file).map_or((None, 0), |m| (m.modified().ok(), m.len()));
    let e = match old.get(path).or_else(|| new.get(path)) {
        Some((t, n, e)) if key.0.is_some() && (*t, *n) == key => e.clone(),
        _ => check(path),
    };
    new.insert(path.to_path_buf(), (key.0, key.1, e.clone()));
    e
}

/// A-THEME-1: the first `balloon-themes/<id>/theme.toml` of a pack under the roots,
/// visited in the same order as characters.
pub fn find_theme(roots: &[PathBuf]) -> Option<PathBuf> {
    let theme_in = |pack: &Path| {
        sorted_children(&pack.join("balloon-themes"))
            .into_iter()
            .find(|d| d.join("theme.toml").is_file())
    };
    for root in roots {
        if root.join("pack.toml").is_file() {
            if let Some(t) = theme_in(root) {
                return Some(t);
            }
            continue;
        }
        let chars = if root.file_name().is_some_and(|n| n == "characters") {
            root.clone()
        } else {
            root.join("characters")
        };
        for p in sorted_children(&chars) {
            if p.join("pack.toml").is_file()
                && let Some(t) = theme_in(&p)
            {
                return Some(t);
            }
        }
    }
    None
}

fn entry_id(path: &Path) -> String {
    let name = path.file_name().unwrap_or_default().to_string_lossy();
    name.strip_suffix(".bwc")
        .unwrap_or(&name)
        .to_ascii_lowercase()
}

pub fn read_manifest_text(path: &Path) -> Result<String, String> {
    if path.is_dir() {
        return std::fs::read_to_string(path.join("character.toml")).map_err(|e| e.to_string());
    }
    let f = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut z = zip::ZipArchive::new(f).map_err(|e| e.to_string())?;
    let mut m = z.by_name("character.toml").map_err(|e| e.to_string())?;
    let mut s = String::new();
    m.read_to_string(&mut s).map_err(|e| e.to_string())?;
    Ok(s)
}

/// The cheap check: the manifest parses, `format = 1`, `family` is known.
fn check(path: &Path) -> Option<Entry> {
    let res = read_manifest_text(path).and_then(|t| {
        let m: CheapManifest = toml::from_str(&t).map_err(|e| e.to_string())?;
        if m.format != 1 {
            return Err(format!("unsupported format {}", m.format));
        }
        if m.family != "agent" && m.family != "office97" {
            return Err(format!("unknown family {:?}", m.family));
        }
        Ok(m.character.info)
    });
    match res {
        Ok(info) => Some(Entry {
            id: entry_id(path),
            path: path.to_path_buf(),
            info,
        }),
        Err(e) => {
            log::warn!("skipping character {}: {e}", path.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::tests::tempdir;

    fn write_char(dir: &Path, name: &str, family: &str) {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        std::fs::write(
            d.join("character.toml"),
            format!(
                "format = 1\nfamily = \"{family}\"\n[character]\ncanvas = [8, 8]\n\
                 [[character.info]]\nlang = \"en\"\nname = \"N-{name}\"\n"
            ),
        )
        .unwrap();
    }

    #[test]
    fn b_char_1_a_pack_1_discovery() {
        let r1 = tempdir();
        let chars = r1.join("characters");
        write_char(&chars, "Zed", "agent");
        write_char(&chars, "bad", "klingon");
        let pack = chars.join("mypack");
        std::fs::create_dir_all(&pack).unwrap();
        std::fs::write(pack.join("pack.toml"), "name = \"p\"\nlicence = \"CC0\"\n").unwrap();
        write_char(&pack, "alpha", "office97");
        // A nested pack in a pack is not visited.
        let nested = pack.join("inner");
        std::fs::create_dir_all(&nested).unwrap();
        std::fs::write(nested.join("pack.toml"), "name = \"x\"").unwrap();
        write_char(&nested, "hidden", "agent");
        // Second root: a single pack folder that repeats "alpha".
        let r2 = tempdir();
        std::fs::write(r2.join("pack.toml"), "name = \"q\"").unwrap();
        write_char(&r2, "ALPHA", "agent");
        write_char(&r2, "beta", "agent");
        // Third root: a single character folder.
        let r3 = tempdir().join("gamma");
        write_char(r3.parent().unwrap(), "gamma", "agent");

        let e = discover(&[r1.clone(), r2.clone(), r3.clone()]);
        let ids: Vec<&str> = e.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids, ["alpha", "beta", "gamma", "zed"]);
        // B-CHAR-1 on use: a cached rescan gives the same result and sees changes.
        let mut cache = Cache::new();
        let roots = [r1.clone(), r2.clone(), r3.clone()];
        assert_eq!(discover_cached(&roots, &mut cache), e);
        assert_eq!(discover_cached(&roots, &mut cache), e);
        write_char(&chars, "new", "agent");
        std::fs::write(
            pack.join("alpha/character.toml"),
            "format = 1\nfamily = \"agent\"\n[[character.info]]\nlang = \"en\"\nname = \"Changed\"\n",
        )
        .unwrap();
        let e2 = discover_cached(&roots, &mut cache);
        let ids2: Vec<&str> = e2.iter().map(|e| e.id.as_str()).collect();
        assert_eq!(ids2, ["alpha", "beta", "gamma", "new", "zed"]);
        assert_eq!(e2[0].info[0].name, "Changed");
        std::fs::remove_dir_all(chars.join("new")).unwrap();
        assert_eq!(discover_cached(&roots, &mut cache).len(), 4);
        assert_eq!(e[0].path, pack.join("alpha"));
        assert_eq!(e[3].info[0].name, "N-Zed");
        // A-THEME-1: a theme in the first root's pack.
        assert_eq!(find_theme(&[r1.clone(), r2.clone()]), None);
        let t = pack.join("balloon-themes/office");
        std::fs::create_dir_all(&t).unwrap();
        std::fs::write(t.join("theme.toml"), "").unwrap();
        assert_eq!(find_theme(&[r2.clone(), r1.clone()]), Some(t));
    }
}
