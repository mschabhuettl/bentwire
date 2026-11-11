//! ACS reader vs the Python research parser `acs.py` (A-TEST-1). Run with `-- --ignored`.

mod oracle;

use std::path::PathBuf;

use agent_formats::acs::Acs;
use oracle::{acs_json, assert_same, python, temp_dir};

fn compare(files: &[PathBuf]) {
    let py = python("acs_dump.py", files, &[]);
    for (f, p) in files.iter().zip(&py) {
        let data = std::fs::read(f).unwrap();
        let acs = Acs::parse(&data).unwrap_or_else(|e| panic!("{}: {e}", f.display()));
        assert_same(&acs_json(&acs), p, &f.display().to_string());
    }
}

/// The fixture writer is checked by an independent reader, so writer and Rust reader cannot
/// share one misreading.
#[test]
#[ignore]
fn acs_fixtures_match_python() {
    let dir = temp_dir("acs-fixtures");
    let mut variants = vec![agent_fixtures::acs::sample()];
    let mut s = agent_fixtures::acs::sample();
    s.voice = None;
    s.balloon = None;
    s.tray_icon = None;
    s.palette.truncate(255);
    variants.push(s);
    let mut s = agent_fixtures::acs::sample();
    s.voice.as_mut().unwrap().extra = None;
    variants.push(s);
    let files: Vec<PathBuf> = variants
        .iter()
        .enumerate()
        .map(|(i, v)| {
            let p = dir.join(format!("fixture{i}.acs"));
            std::fs::write(&p, v.write()).unwrap();
            p
        })
        .collect();
    compare(&files);
    std::fs::remove_dir_all(&dir).ok();
}

/// Every ACS sample parses completely and agrees with acs.py on structure, pixel, region and
/// sound hashes.
#[test]
#[ignore]
fn acs_real_data_matches_python() {
    agent_fixtures::real::reference_root();
    let files = agent_fixtures::real::locate_section("1.1");
    compare(&files);
    eprintln!("{} ACS files agree", files.len());
}
