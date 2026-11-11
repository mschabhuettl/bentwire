//! Real data (A-TEST-1): every character the Python prototype wrote loads and validates.

use bentwire_character::*;

#[test]
#[ignore]
fn prototype_output_loads() {
    let out = agent_fixtures::real::prototype_dir().join("out");
    let mut n = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&out)
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    entries.sort();
    for p in entries {
        if !(p.is_dir() || p.extension().is_some_and(|e| e == "bwc")) || p.ends_with("variants") {
            continue;
        }
        let c = Character::load(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
        let bad: Vec<_> = c
            .warnings
            .iter()
            .filter(|w| !w.contains("is not stored"))
            .collect();
        assert!(bad.is_empty(), "{}: {bad:?}", p.display());
        for a in &c.manifest.animations {
            assert!(
                c.is_available(&a.name) || c.manifest.is_missing(&a.name),
                "{} {}",
                p.display(),
                a.name
            );
            for f in &a.frames {
                let _ = c.compose_frame(f, None, 1);
            }
        }
        n += 1;
    }
    assert!(n >= 126, "only {n} characters");
}
