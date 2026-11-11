//! Round trip of the ACT actors against the Python oracle (character-format 11, A-TEST-2).
//! The oracle reads the Office palette from CLIPPIT.ACS, so the converter does too here.

mod oracle_common;

use bentwire_convert::ConvertOptions;

fn opts() -> ConvertOptions {
    ConvertOptions {
        office_palette: Some(oracle_common::office_palette()),
        ..Default::default()
    }
}

#[test]
#[ignore]
fn act_round_trip_against_oracle() {
    oracle_common::round_trip_with("act", &opts());
}

/// Vector (WMF) actors: graph and traces against the oracle (pixels have no reference
/// rasteriser, character-format 11), plus structural checks of the rasterised poses: every
/// pose picture is non-empty, 1x and 2x exist, transparency is exactly index 0.
#[test]
#[ignore]
fn act_vector_round_trip_and_structure() {
    oracle_common::round_trip_with("actg", &opts());
    let mut n = 0;
    for (kind, path) in oracle_common::samples() {
        if kind != "actg" && !(kind == "act" && path.ends_with("CLIPPIT.ACT")) {
            continue;
        }
        let pkg = bentwire_convert::convert_to_package_with(&path, &opts()).unwrap();
        let c = pkg.load().unwrap();
        let sheet = &c.manifest.sheets[0];
        assert!(
            sheet.hidpi.contains_key(&2),
            "{}: no 2x sheet",
            path.display()
        );
        let img = c.sheets.base("a").unwrap();
        let (_, _, t) = img.palette_info().unwrap().unwrap();
        assert_eq!(t, Some(0), "{}: transparent index", path.display());
        let act = agent_formats::act::parse(&std::fs::read(&path).unwrap()).unwrap();
        // A-CHAR-3: at density 2 every pose shows its 2x cell unscaled.
        let two = c.sheets.file("a", 2).expect("2x file");
        let [cw, ch] = [sheet.cell[0] * 2, sheet.cell[1] * 2];
        let cols = two.width / cw;
        for layers in c.manifest.poses.values() {
            let n = layers[0].image.index;
            let (ox, oy) = ((n % cols) * cw, (n / cols) * ch);
            let pic = c.compose(layers, 2);
            for y in 0..ch {
                for x in 0..cw {
                    let p = two.pixel(ox + x, oy + y);
                    let want = if p[3] == 0 { [0; 4] } else { p };
                    let i = ((y * cw + x) * 4) as usize;
                    assert_eq!(
                        pic.data[i..i + 4],
                        want,
                        "{}: 2x pose differs",
                        path.display()
                    );
                }
            }
        }
        for (pid, layers) in &c.manifest.poses {
            for k in [1, 2] {
                let pic = c.compose(layers, k);
                // An empty picture is only allowed when the pose's metafiles draw nothing
                // (ZVISIBLE, ZSPEAKER: invisible Bob helper actors).
                assert!(
                    pic.data.chunks(4).any(|p| p[3] == 255) || !draws(&act, *pid as usize),
                    "{}: pose {pid} empty at {k}x",
                    path.display()
                );
            }
        }
        n += 1;
    }
    println!("vector actors with structural checks: {n}");
    assert_eq!(n, 27);
}

/// Whether a pose's metafiles contain any drawing record.
fn draws(act: &agent_formats::act::ActFile, pose: usize) -> bool {
    use agent_formats::act::Payload;
    const DRAW: [u16; 12] = [
        0x0324, 0x0325, 0x0418, 0x0538, 0x0521, 0x061C, 0x041B, 0x0213, 0x0A32, 0x0817, 0x081A,
        0x0830,
    ];
    let wmf_draws = |i: usize| match act.payloads.get(i) {
        Some(Payload::Wmf(w)) => w.records.iter().any(|r| DRAW.contains(&r.function)),
        _ => false,
    };
    match act.payloads.get(pose) {
        Some(Payload::Composite(c)) => c.iter().any(|ch| wmf_draws(ch.id as usize)),
        _ => wmf_draws(pose),
    }
}
