//! Batch conversion into a pack (A-PACK-2, A-ID-1, A-CONV-5): a list of source files in, one
//! `<id>.bwc` per source that converts completely, plus a report of exclusions. Generic: the
//! list of sources and every source-specific path live with the pack that uses it
//! (`characters/classics/sources.sh`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::{ConvertOptions, Error, convert_to_package_with, sha16};

/// Outcome per source.
#[derive(Debug)]
pub enum Outcome {
    Included {
        id: String,
        path: PathBuf,
        size: u64,
    },
    Excluded {
        path: PathBuf,
        reason: String,
    },
    Duplicate {
        path: PathBuf,
        of: PathBuf,
    },
}

/// `NAME(Qualifier).ext` -> `name-qualifier` (`qualified`) or `name`; other characters
/// outside `[a-z0-9]` become `-`.
pub fn stem_id(path: &Path, qualified: bool) -> String {
    let mut stem = path
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_lowercase();
    if !qualified && let Some(i) = stem.find('(') {
        stem.truncate(i);
    }
    let mut out = String::new();
    for c in stem.chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if !out.ends_with('-') && !out.is_empty() {
            out.push('-');
        }
    }
    out.trim_end_matches('-').to_owned()
}

/// Pack id for a source by its format (A-ID-1).
fn id_for(path: &Path, stem: &str, data: &[u8]) -> Result<String, Error> {
    let stem = stem.to_owned();
    let magic = data
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    if magic == Some(agent_formats::acs::MAGIC) {
        return Ok(stem);
    }
    if magic == Some(agent_formats::acf::MAGIC_20) || magic == Some(agent_formats::acf::MAGIC_15) {
        return Ok(format!("{stem}-web"));
    }
    if agent_formats::agent15::is_ole(data) {
        return Ok(format!("{stem}-15"));
    }
    if data.starts_with(b"LP") {
        let a = agent_formats::act::parse(data).map_err(|e| Error::Source(e.to_string()))?;
        let suffix = if a.major == 1 {
            "bob"
        } else if a.header.kind == 7 {
            "97"
        } else {
            "act"
        };
        return Ok(format!("{stem}-{suffix}"));
    }
    Err(Error::Unsupported(format!(
        "{}: unknown format",
        path.display()
    )))
}

/// Per-id converter options (A-CONV-3): `id lang codepage` per line, `#` comments. The pack
/// script keeps them in `characters/classics/options.txt`.
pub fn parse_overrides(text: &str) -> Vec<(String, String, String)> {
    text.lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .filter_map(|l| {
            let f: Vec<&str> = l.split_whitespace().collect();
            (f.len() == 3).then(|| (f[0].to_owned(), f[1].to_owned(), f[2].to_owned()))
        })
        .collect()
}

fn options(
    id: &str,
    base: &ConvertOptions,
    overrides: &[(String, String, String)],
) -> ConvertOptions {
    let mut o = base.clone();
    if let Some((_, lang, cp)) = overrides.iter().find(|(i, _, _)| i == id) {
        o.lang = Some(lang.clone());
        o.codepage = Some(cp.clone());
    }
    o
}

/// Convert every source into `out/<id>.bwc`. Sources with identical bytes are converted once;
/// two different sources with the same id are an error (A-ID-1).
pub fn generate(
    sources: &[PathBuf],
    out: &Path,
    base: &ConvertOptions,
    overrides: &[(String, String, String)],
) -> Result<Vec<Outcome>, Error> {
    std::fs::create_dir_all(out)?;
    let mut by_hash: BTreeMap<String, PathBuf> = BTreeMap::new();
    let mut res = Vec::new();
    // Pass 1: deduplicate by content and compute both id forms.
    let mut cands: Vec<(PathBuf, String, String)> = Vec::new();
    for p in sources {
        let data = std::fs::read(p)?;
        let h = sha16(&data);
        if let Some(first) = by_hash.get(&h) {
            res.push(Outcome::Duplicate {
                path: p.clone(),
                of: first.clone(),
            });
            continue;
        }
        by_hash.insert(h, p.clone());
        let ids = id_for(p, &stem_id(p, false), &data)
            .and_then(|a| Ok((a, id_for(p, &stem_id(p, true), &data)?)));
        match ids {
            Ok((plain, qualified)) => cands.push((p.clone(), plain, qualified)),
            Err(e) => res.push(Outcome::Excluded {
                path: p.clone(),
                reason: e.to_string(),
            }),
        }
    }
    // Pass 2: the plain id unless another source has it too (A-CONV-5); collisions fail.
    let mut by_id: BTreeMap<String, PathBuf> = BTreeMap::new();
    for (p, plain, qualified) in &cands {
        let shared = cands.iter().filter(|c| &c.1 == plain).count() > 1;
        let id = if shared {
            qualified.clone()
        } else {
            plain.clone()
        };
        if let Some(other) = by_id.get(&id) {
            return Err(Error::Source(format!(
                "id {id} of {} collides with {}",
                p.display(),
                other.display()
            )));
        }
        by_id.insert(id.clone(), p.clone());
        let p = p.clone();
        match convert_to_package_with(&p, &options(&id, base, overrides)) {
            Err(e) => res.push(Outcome::Excluded {
                path: p.clone(),
                reason: e.to_string(),
            }),
            Ok(pkg) => {
                let missing = pkg.manifest.missing_animations();
                if !missing.is_empty() {
                    res.push(Outcome::Excluded {
                        path: p.clone(),
                        reason: format!(
                            "incomplete: {} animations missing (source.missing_animations)",
                            missing.len()
                        ),
                    });
                    continue;
                }
                let c = pkg.load()?;
                let visible = c.manifest.animations.iter().any(|a| {
                    a.frames.iter().any(|f| {
                        c.compose_frame(f, None, 1)
                            .data
                            .chunks(4)
                            .any(|p| p[3] != 0)
                    })
                });
                if !visible {
                    res.push(Outcome::Excluded {
                        path: p.clone(),
                        reason: "no visible picture in any frame (helper actor)".into(),
                    });
                    continue;
                }
                let dest = out.join(format!("{id}.bwc"));
                pkg.write_bwc(&dest)?;
                let size = std::fs::metadata(&dest)?.len();
                res.push(Outcome::Included {
                    id,
                    path: p.clone(),
                    size,
                });
            }
        }
    }
    Ok(res)
}
