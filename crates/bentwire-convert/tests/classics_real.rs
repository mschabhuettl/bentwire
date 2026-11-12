//! The classics pack (A-PACK-2, A-THEME-1): every character loads completely, and regenerating
//! from the sources gives byte-identical files. For the vector actors the committed `.bwc`
//! sheets are the golden images of our rasteriser (A-TEST-2); this test compares against them.

mod oracle_common;

use std::path::PathBuf;

use bentwire_character::Character;
use bentwire_convert::classics::{Outcome, generate, parse_overrides};

/// The classics pack (its own repository): `BENTWIRE_CLASSICS`, else `../bentwire-classics`
/// next to this repository. A relative value is taken from the repository root.
fn pack() -> PathBuf {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let p = root.join(
        std::env::var_os("BENTWIRE_CLASSICS").unwrap_or_else(|| "../bentwire-classics".into()),
    );
    if !p.join("pack.toml").exists() {
        panic!("NOT RUN: no classics pack at {}", p.display());
    }
    p
}

#[test]
#[ignore]
fn classics_pack_loads_and_is_reproducible() {
    agent_fixtures::real::sources_dirs();
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
    // The source list and palette location belong to the pack (its sources.sh).
    let run = |args: &[&str]| -> Vec<PathBuf> {
        let out = std::process::Command::new("bash")
            .arg(pack.join("sources.sh"))
            .args(args)
            .output()
            .expect("bash");
        // sources.sh exits 77 with "NOT RUN: ..." when a source is missing.
        assert!(
            out.status.success(),
            "sources.sh failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        );
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

/// A-CONV-7: the same picture gets the same colours in every actor that shows it. Pairs of ACT
/// bitmap actors of the pack that share at least `MIN_PAIRS` pictures (same size and
/// transparency mask, found without names) must agree in colour at the same pixels, with the
/// palette each is converted with. Catches an actor converted with the wrong palette.
#[test]
#[ignore]
fn a_conv_7_same_picture_same_colours() {
    use bentwire_convert::sibling::{MIN_PAIRS, bitmaps, same_pictures};
    agent_fixtures::real::sources_dirs();
    let pack = pack();
    let sh = |args: &[&str]| -> Vec<PathBuf> {
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
    let opts = bentwire_convert::ConvertOptions {
        office_palette: sh(&["--palette"]).into_iter().next(),
        ..Default::default()
    };
    let office =
        bentwire_convert::read_office_palette(opts.office_palette.as_ref().unwrap()).unwrap();
    let overrides = parse_overrides(&std::fs::read_to_string(pack.join("options.txt")).unwrap());
    let (_, planned) = bentwire_convert::classics::plan(&sh(&[]), &opts, &overrides).unwrap();
    let mut actors = Vec::new();
    for p in &planned {
        let data = std::fs::read(&p.path).unwrap();
        let Ok(a) = agent_formats::act::parse(&data) else {
            continue;
        };
        if !bitmaps(&a).is_empty() {
            actors.push((p.id.clone(), a, p.options.palette.unwrap_or(office)));
        }
    }
    let mut checked = 0;
    let mut bad = Vec::new();
    for (i, (ia, a, pa)) in actors.iter().enumerate() {
        for (ib, b, pb) in &actors[i + 1..] {
            let pairs = same_pictures(&bitmaps(a), &bitmaps(b));
            if pairs.len() < MIN_PAIRS {
                continue;
            }
            let (mut sum, mut n) = (0f64, 0u64);
            for (x, y) in &pairs {
                for (&p, &q) in x.pixels.iter().zip(&y.pixels) {
                    if p == 10 {
                        continue;
                    }
                    let (c, d) = (pa[p as usize], pb[q as usize]);
                    sum += (0..3)
                        .map(|k| (c[k] as f64 - d[k] as f64).powi(2))
                        .sum::<f64>()
                        .sqrt();
                    n += 1;
                }
            }
            let mean = sum / n as f64;
            println!(
                "{ia} / {ib}: {} shared pictures, mean colour distance {mean:.1}",
                pairs.len()
            );
            checked += 1;
            if mean > MAX_MEAN_DISTANCE {
                bad.push(format!("{ia} / {ib}: {mean:.1}"));
            }
        }
    }
    assert!(checked > 0, "no two actors share pictures");
    assert!(bad.is_empty(), "same pictures, different colours: {bad:?}");
    println!("{checked} pairs of actors with shared pictures checked");
}

/// Mean RGB distance (0 to 441) allowed between two actors' colours of the same pictures. A
/// palette read from a sibling (A-CONV-7) lands near 6 (the sibling was requantised); a wrong
/// palette near 140.
const MAX_MEAN_DISTANCE: f64 = 24.0;
