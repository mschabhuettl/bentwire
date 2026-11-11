//! Agent 1.5 reader vs the Python research parsers (A-TEST-1). Run with `-- --ignored`.

mod oracle;

use std::path::PathBuf;

use agent_formats::acf::{AnimationFile, read_web_character};
use agent_formats::agent15::OleAcs;
use oracle::{assert_same, ole15_json, python, temp_dir, web_json};

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

/// Every Agent 1.5 OLE file of docs/sources.md section 1.3 parses and agrees with the
/// research parsers; exactly the two damaged Ozzar streams fail, each on its own.
#[test]
#[ignore]
fn agent15_ole_real_data_matches_python() {
    let files = agent_fixtures::real::locate_section("1.3");
    assert_eq!(
        compare(&files),
        2,
        "expected exactly the two damaged Ozzar streams to fail"
    );
}

/// The Agent 1.5 web characters (flat ACF magic 0xABCDABC2 + .aaf) agree with acfparse.py.
#[test]
#[ignore]
fn agent15_web_real_data_matches_python() {
    let acfs: Vec<PathBuf> = agent_fixtures::real::listed("1.7")
        .iter()
        .filter(|l| l.name.ends_with(".acf"))
        .map(agent_fixtures::real::locate)
        .collect();
    let dirs: Vec<PathBuf> = acfs.iter().map(|a| a.parent().unwrap().into()).collect();
    let py = python("acf_dump.py", &dirs, &[]);
    let mut loaded = 0;
    for ((a, d), p) in acfs.iter().zip(&dirs).zip(&py) {
        let w = read_web_character(a).unwrap();
        assert_eq!(w.acf.magic, agent_formats::acf::MAGIC_15);
        loaded += w
            .animations
            .iter()
            .filter(|a| matches!(a, AnimationFile::Loaded(_)))
            .count();
        assert_same(&web_json(&w), p, &d.display().to_string());
    }
    assert!(loaded > 0, "no AAF file loaded");
}
