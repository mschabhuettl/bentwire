//! Round trip of the 12 ACS files against the Python oracle (character-format 11, A-TEST-2).
//! `cargo test -p bentwire-convert --test acs_real -- --ignored` (or scripts/oracle-check.sh).

mod oracle_common;

use bentwire_character::Character;
use oracle_common::*;

#[test]
#[ignore]
fn acs_round_trip_against_oracle() {
    let _ = sources();
    let acs: Vec<_> = samples().into_iter().filter(|(k, _)| k == "acs").collect();
    assert_eq!(acs.len(), 12, "expected the 12 ACS files");
    let tmp = temp_dir("acs-rt");
    let results = par_map(&acs, |(_, path)| {
        let stem = path.file_stem().unwrap().to_string_lossy().to_lowercase();
        let oracle = oracle_json(
            &["oracle", "acs", path.to_str().unwrap()],
            &tmp.join(format!("{stem}.oracle.json")),
        );
        let pkg = bentwire_convert::convert_to_package(path).map_err(|e| format!("{stem}: {e}"))?;
        let folder = tmp.join(&stem);
        let bwc = tmp.join(format!("{stem}.bwc"));
        pkg.write_folder(&folder).unwrap();
        pkg.write_bwc(&bwc).unwrap();
        let mut counts = (0, 0);
        for p in [&folder, &bwc] {
            let c = Character::load(p).map_err(|e| format!("{stem}: {e}"))?;
            assert!(c.warnings.is_empty(), "{stem}: {:?}", c.warnings);
            counts =
                compare(&oracle, &rust_json(&c)).map_err(|d| format!("{} : {d}", p.display()))?;
        }
        Ok::<_, String>((stem, counts))
    });
    let mut bad = Vec::new();
    let (mut anims, mut traces) = (0, 0);
    for r in results {
        match r {
            Ok((s, (a, t))) => {
                println!("{s}: {a} animations, {t} traces equal (folder and .bwc)");
                anims += a;
                traces += t;
            }
            Err(e) => bad.push(e),
        }
    }
    println!("ACS: {anims} animations, {traces} traces");
    assert!(bad.is_empty(), "{bad:#?}");
}
