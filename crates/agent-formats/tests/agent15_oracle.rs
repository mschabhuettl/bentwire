//! Agent 1.5 reader vs the Python research parsers (A-TEST-1). Run with `-- --ignored`.

mod oracle;

use std::path::{Path, PathBuf};

use agent_formats::acf::{AnimationFile, read_web_character};
use agent_formats::agent15::OleAcs;
use oracle::{assert_same, env_dir, ole15_json, python, temp_dir, web_json};

fn compare(files: &[PathBuf]) -> usize {
    let py = python("agent15_dump.py", files, &[]);
    let mut failed = 0;
    for (f, p) in files.iter().zip(&py) {
        let o = OleAcs::parse(&std::fs::read(f).unwrap())
            .unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        for (a, e) in o.animations.iter().zip(&o.character.animations) {
            if let Err(err) = a {
                eprintln!("{}: {} failed: {err}", f.display(), e.name);
                failed += 1;
            }
        }
        assert_same(&ole15_json(&o), p, &f.display().to_string());
    }
    failed
}

#[test]
#[ignore]
fn agent15_fixtures_match_python() {
    let dir = temp_dir("ole15-fixtures");
    let files: Vec<PathBuf> = [0x1_001F, 0x1_001E, 0x1_001C]
        .iter()
        .map(|&v| {
            let p = dir.join(format!("f{v:x}.acs"));
            std::fs::write(
                &p,
                agent_fixtures::agent15::write_ole(&agent_fixtures::agent15::sample(v)),
            )
            .unwrap();
            p
        })
        .collect();
    assert_eq!(compare(&files), 0);
    std::fs::remove_dir_all(&dir).ok();
}

/// Every OLE sample (prototype/sources.py SAMPLES["ole15"]) parses and agrees with the
/// research parsers; exactly the two damaged Ozzar streams fail, each on its own.
#[test]
#[ignore]
fn agent15_ole_real_data_matches_python() {
    let r = env_dir("BENTWIRE_REFERENCE");
    let d = r.join("agent15/downloads");
    let mut files = vec![d.join("Characters/merlin.acs")];
    let mut ia: Vec<PathBuf> = std::fs::read_dir(d.join("source-files"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    ia.sort();
    files.extend(ia);
    assert_eq!(
        compare(&files),
        2,
        "expected exactly the two damaged Ozzar streams to fail"
    );
}

/// Copies a Wayback download directory (`name.ext.<timestamp>`) to `out` with plain names,
/// taking the first copy of each name (the duplicate captures are byte-identical).
fn strip_timestamps(src: &Path, out: &Path) {
    std::fs::create_dir_all(out).unwrap();
    let mut files: Vec<PathBuf> = std::fs::read_dir(src)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    for f in files {
        let n = f.file_name().unwrap().to_string_lossy().to_string();
        let plain = n.rsplit_once('.').map_or(n.clone(), |(a, _)| a.to_string());
        let dst = out.join(plain);
        if !dst.exists() {
            std::fs::copy(&f, dst).unwrap();
        }
    }
}

/// The Agent 1.5 web characters (flat ACF magic 0xABCDABC2 + .aaf) agree with acfparse.py.
#[test]
#[ignore]
fn agent15_web_real_data_matches_python() {
    let r = env_dir("BENTWIRE_REFERENCE");
    let tmp = temp_dir("web15");
    let dirs: Vec<PathBuf> = ["genie", "merlin", "merlinsfx", "robby"]
        .iter()
        .map(|c| {
            let d = tmp.join(c);
            strip_timestamps(&r.join("web").join(c), &d);
            d
        })
        .collect();
    let py = python("acf_dump.py", &dirs, &[]);
    let mut loaded = 0;
    for (d, p) in dirs.iter().zip(&py) {
        let w = read_web_character(
            &d.join(format!("{}.acf", d.file_name().unwrap().to_string_lossy())),
        )
        .unwrap();
        assert_eq!(w.acf.magic, agent_formats::acf::MAGIC_15);
        loaded += w
            .animations
            .iter()
            .filter(|a| matches!(a, AnimationFile::Loaded(_)))
            .count();
        assert_same(&web_json(&w), p, &d.display().to_string());
    }
    std::fs::remove_dir_all(&tmp).ok();
    assert_eq!(loaded, 12, "12 distinct AAF files (agent15-format 8.1)");
}
