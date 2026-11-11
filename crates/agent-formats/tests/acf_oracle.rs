//! ACF/ACA reader vs the Python research parser `acfparse.py` (A-TEST-1). Run with
//! `-- --ignored`.

mod oracle;

use std::path::PathBuf;

use agent_formats::acf::read_web_character;
use oracle::{assert_same, env_dir, python, temp_dir, web_json};

fn compare(dirs: &[PathBuf]) {
    let py = python("acf_dump.py", dirs, &[]);
    for (d, p) in dirs.iter().zip(&py) {
        let acf = std::fs::read_dir(d)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case("acf")))
            .unwrap();
        let w = read_web_character(&acf).unwrap_or_else(|e| panic!("{}: {e}", d.display()));
        assert_same(&web_json(&w), p, &d.display().to_string());
    }
}

#[test]
#[ignore]
fn acf_fixtures_match_python() {
    let root = temp_dir("acf-fixtures");
    let mut dirs = Vec::new();
    for v in agent_formats::acf::VERSIONS {
        for variant in 0..2 {
            let mut s = agent_fixtures::acf::sample(v);
            if variant == 1 {
                s.voice.as_mut().unwrap().extra = None;
                s.tray_icon = None;
                s.animations[0].headerless = true;
                s.animations[0].compression = agent_fixtures::common::Compression::Raw;
            }
            let d = root.join(format!("c{v:x}_{variant}"));
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(d.join("char.acf"), s.write_acf()).unwrap();
            // leave the last animation out: a missing file
            for i in 0..s.animations.len() - 1 {
                std::fs::write(d.join(&s.animations[i].file_name), s.write_aca(i)).unwrap();
            }
            dirs.push(d);
        }
    }
    compare(&dirs);
    std::fs::remove_dir_all(&root).ok();
}

/// The four web characters: every ACF and every archived ACA parses and agrees with
/// acfparse.py.
#[test]
#[ignore]
fn acf_real_data_matches_python() {
    let r = env_dir("BENTWIRE_REFERENCE");
    let dirs: Vec<PathBuf> = ["merlin", "genie", "peedy", "robby"]
        .iter()
        .map(|c| r.join("acf/downloads").join(c))
        .collect();
    compare(&dirs);
}
