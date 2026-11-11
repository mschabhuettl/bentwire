//! The classics pack (A-PACK-2, A-THEME-1): every character loads completely, and regenerating
//! from the sources gives byte-identical files. For the vector actors the committed `.bwc`
//! sheets are the golden images of our rasteriser (A-TEST-2); this test compares against them.

mod oracle_common;

use std::path::PathBuf;

use bentwire_character::Character;
use bentwire_convert::classics::{Outcome, generate, parse_overrides};

/// The classics pack: `BENTWIRE_CLASSICS`, else `$BENTWIRE_PACKS/classics` (the pack may live
/// outside this repository).
fn pack() -> PathBuf {
    let p = match (
        std::env::var_os("BENTWIRE_CLASSICS"),
        std::env::var_os("BENTWIRE_PACKS"),
    ) {
        (Some(c), _) => PathBuf::from(c),
        (None, Some(packs)) => PathBuf::from(packs).join("classics"),
        (None, None) => panic!("NOT RUN: set BENTWIRE_CLASSICS or BENTWIRE_PACKS"),
    };
    if !p.join("pack.toml").exists() {
        panic!("NOT RUN: no classics pack at {}", p.display());
    }
    p
}

#[test]
#[ignore]
fn classics_pack_loads_and_is_reproducible() {
    let _ = (oracle_common::research(), oracle_common::sources());
    let pack = pack();
    let mut committed: Vec<PathBuf> = std::fs::read_dir(&pack)
        .unwrap()
        .map(|e| e.unwrap().path())
        .filter(|p| p.extension().is_some_and(|e| e == "bwc"))
        .collect();
    committed.sort();
    for p in &committed {
        let c = Character::load(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        assert!(c.warnings.is_empty(), "{}: {:?}", p.display(), c.warnings);
        assert!(c.manifest.missing_animations().is_empty());
        for a in &c.manifest.animations {
            assert!(c.is_available(&a.name), "{}: {}", p.display(), a.name);
        }
    }
    let theme = pack.join("balloon-themes/office/theme.toml");
    assert!(theme.exists(), "balloon theme missing");

    let tmp = oracle_common::temp_dir("classics");
    // The source list and palette location belong to the pack (characters/classics/sources.sh).
    let run = |args: &[&str]| -> Vec<PathBuf> {
        let out = std::process::Command::new("bash")
            .arg(pack.join("sources.sh"))
            .args(args)
            .output()
            .expect("bash");
        assert!(out.status.success(), "sources.sh failed");
        String::from_utf8(out.stdout)
            .unwrap()
            .lines()
            .map(PathBuf::from)
            .collect()
    };
    let sources = run(&[]);
    let opts = bentwire_convert::ConvertOptions {
        office_palette: run(&["--palette"]).into_iter().next(),
        ..Default::default()
    };
    let overrides = parse_overrides(&std::fs::read_to_string(pack.join("options.txt")).unwrap());
    let res = generate(&sources, &tmp, &opts, &overrides).unwrap();
    let ids: Vec<&String> = res
        .iter()
        .filter_map(|r| match r {
            Outcome::Included { id, .. } => Some(id),
            _ => None,
        })
        .collect();
    if ids.len() != committed.len() {
        let why: Vec<String> = res
            .iter()
            .filter_map(|r| match r {
                Outcome::Excluded { path, reason } => Some(format!("{}: {reason}", path.display())),
                _ => None,
            })
            .collect();
        panic!(
            "included {} of {} committed characters (a full $TMPDIR is a common cause; set \
             TMPDIR to a disk directory). Excluded:\n{}",
            ids.len(),
            committed.len(),
            why.join("\n")
        );
    }
    for id in ids {
        let a = std::fs::read(tmp.join(format!("{id}.bwc"))).unwrap();
        let b = std::fs::read(pack.join(format!("{id}.bwc")))
            .unwrap_or_else(|_| panic!("{id}.bwc not committed"));
        assert!(a == b, "{id}.bwc differs from a fresh conversion");
    }
    println!(
        "classics: {} characters load and regenerate byte-identically",
        committed.len()
    );
}
