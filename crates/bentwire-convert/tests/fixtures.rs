//! Every converter path on synthetic files from agent-fixtures (runs without real data): convert,
//! write folder and `.bwc`, load both without warnings, equal dumps, byte-identical reruns.

use std::path::{Path, PathBuf};

use bentwire_character::Character;
use bentwire_character::dump::{Dumper, all_traces};
use bentwire_convert::{ConvertOptions, convert_to_package_with};

fn tmp(tag: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("bentwire-fix-{tag}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Convert `src`, check folder/.bwc equivalence and reproducibility; returns the character.
fn check(src: &Path, opts: &ConvertOptions) -> Character {
    let pkg =
        convert_to_package_with(src, opts).unwrap_or_else(|e| panic!("{}: {e}", src.display()));
    let again = convert_to_package_with(src, opts).unwrap();
    assert_eq!(
        pkg.to_bwc().unwrap(),
        again.to_bwc().unwrap(),
        "not reproducible"
    );
    let d = src.with_extension("out");
    pkg.write_folder(&d).unwrap();
    let z = src.with_extension("bwc");
    pkg.write_bwc(&z).unwrap();
    let a = Character::load(&d).unwrap();
    let b = Character::load(&z).unwrap();
    assert!(
        a.warnings.is_empty() && b.warnings.is_empty(),
        "{:?}",
        a.warnings
    );
    let (da, db) = (Dumper::new(&a), Dumper::new(&b));
    assert_eq!(da.dump(), db.dump());
    assert_eq!(all_traces(&da), all_traces(&db));
    a
}

/// The fixture contains branch data outside character-format 7.2's constraints, which the
/// converter must reject (character-format 10 "ACS"); the cleaned copy converts.
#[test]
fn acs_fixture_converts() {
    let d = tmp("acs");
    let mut f = agent_fixtures::acs::sample();
    let p = d.join("raw.acs");
    std::fs::write(&p, f.write()).unwrap();
    let Err(e) = convert_to_package_with(&p, &ConvertOptions::default()) else {
        panic!("invalid branches accepted");
    };
    assert!(e.to_string().contains("branch"), "{e}");
    for a in &mut f.animations {
        let n = a.frames.len();
        for fr in &mut a.frames {
            fr.branches.retain(|&(t, pr)| (t as usize) < n && pr > 0);
            if fr.branches.iter().map(|b| b.1 as u32).sum::<u32>() > 100 {
                fr.branches.clear();
            }
            if fr.exit >= n as i16 || fr.exit < -2 {
                fr.exit = -2;
            }
        }
    }
    let p = d.join("sample.acs");
    std::fs::write(&p, f.write()).unwrap();
    let c = check(&p, &ConvertOptions::default());
    assert_eq!(c.manifest.animations.len(), f.animations.len());
}

/// Nonzero ACA frame offsets are not representable (character-format 9.2: the converter
/// fails); the fixture uses one, so it is cleared here.
fn clean_offsets(f: &mut agent_fixtures::acf::Acf) {
    for a in &mut f.animations {
        for fr in &mut a.frames {
            fr.offset = (0, 0);
        }
    }
}

#[test]
fn web_acf_fixtures_convert() {
    for (version, magic, ext) in [
        (0x2_0001, agent_formats::acf::MAGIC_20, "acf-aca-2.0"),
        (0x1_001F, agent_formats::acf::MAGIC_15, "acf-aaf-1.5"),
    ] {
        let d = tmp(&format!("acf{version:x}"));
        let mut f = agent_fixtures::acf::sample(version);
        f.magic = magic;
        clean_offsets(&mut f);
        std::fs::write(d.join("sample.acf"), f.write_acf()).unwrap();
        for (i, a) in f.animations.iter().enumerate() {
            std::fs::write(d.join(&a.file_name), f.write_aca(i)).unwrap();
        }
        let c = check(&d.join("sample.acf"), &ConvertOptions::default());
        let src = c.manifest.source.as_ref().unwrap();
        assert_eq!(src["format"].as_str(), Some(ext));
        assert_eq!(c.manifest.animations.len(), f.animations.len());
    }
}

#[test]
fn agent15_ole_fixture_converts() {
    let d = tmp("ole");
    let mut f = agent_fixtures::agent15::sample(0x1_001F);
    clean_offsets(&mut f);
    let p = d.join("sample.acs");
    std::fs::write(&p, agent_fixtures::agent15::write_ole(&f)).unwrap();
    let c = check(&p, &ConvertOptions::default());
    // 1.5 defaults (character-format 9.3): transition return, exit end on every frame.
    for a in &c.manifest.animations {
        assert_eq!(a.transition, Some(bentwire_character::Transition::Return));
        assert!(
            a.frames
                .iter()
                .all(|f| f.exit == Some(bentwire_character::Exit::End))
        );
    }
}

#[test]
fn act_fixtures_convert() {
    let d = tmp("act");
    // Office palette source: a synthetic ACS with 256 palette entries.
    let mut pal = agent_fixtures::acs::sample();
    pal.palette = agent_fixtures::acs::palette(256);
    let pal_path = d.join("palette.acs");
    std::fs::write(&pal_path, pal.write()).unwrap();
    let opts = ConvertOptions {
        office_palette: Some(pal_path),
        ..Default::default()
    };
    for (name, actor) in [
        ("bitmap", agent_fixtures::act::bitmap_actor()),
        ("wmf", agent_fixtures::act::wmf_actor()),
        ("bob", agent_fixtures::act::bob_actor()),
    ] {
        let p = d.join(format!("{name}.act"));
        std::fs::write(&p, actor.to_bytes()).unwrap();
        let c = check(&p, &opts);
        assert_eq!(c.family(), bentwire_character::Family::Office97);
        assert!(c.manifest.office97.is_some());
        if name != "bitmap" {
            assert!(
                c.manifest.sheets[0].hidpi.contains_key(&2),
                "{name}: 2x sheet"
            );
        }
    }
    // Without a palette source a bitmap actor is an error, not a guess.
    let p = d.join("bitmap.act");
    assert!(convert_to_package_with(&p, &ConvertOptions::default()).is_err());
}
