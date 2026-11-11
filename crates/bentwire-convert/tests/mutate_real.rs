//! Sensitivity of the round-trip check (character-format 11, the prototype's mutate.py): one small
//! mutation of a converted character must make its dump or traces differ from the oracle.

mod oracle_common;

use std::path::{Path, PathBuf};

use bentwire_character::image::{Pixels, decode_png};
use bentwire_character::write::{encode_flac, encode_png_indexed, manifest_to_toml};
use bentwire_character::*;
use oracle_common::*;

type Mutation = fn(&Path);

fn edit_manifest(dir: &Path, f: impl FnOnce(&mut Manifest)) {
    let p = dir.join("character.toml");
    let mut m = parse::parse_manifest(&std::fs::read_to_string(&p).unwrap()).unwrap();
    f(&mut m);
    std::fs::write(&p, manifest_to_toml(&m, &[])).unwrap();
}

fn edit_sheet(dir: &Path, f: impl FnOnce(&mut Vec<u8>, &mut Vec<[u8; 3]>, &[u8], u32)) {
    let p = dir.join("sheets/a.png");
    let img = decode_png(&std::fs::read(&p).unwrap()).unwrap();
    let Pixels::Indexed {
        mut data,
        mut plte,
        trns,
        ..
    } = img.pixels
    else {
        panic!("indexed sheet expected")
    };
    let trns = trns.unwrap();
    f(&mut data, &mut plte, &trns, img.width);
    let png = encode_png_indexed(img.width, img.height, &data, &plte, Some(&trns)).unwrap();
    std::fs::write(&p, png).unwrap();
}

fn pixel(d: &Path) {
    edit_sheet(d, |data, _, trns, w| {
        for y in 40..60 {
            for x in 40..60 {
                let i = (y * w + x) as usize;
                if trns.get(data[i] as usize) != Some(&0) {
                    data[i] = ((data[i] as u32 + 1) % 250) as u8;
                    return;
                }
            }
        }
        panic!("no opaque pixel found");
    });
}

fn palette(d: &Path) {
    edit_sheet(d, |_, plte, _, _| plte[40][0] ^= 1);
}

fn sound(d: &Path) {
    let p = d.join("sounds/s00.flac");
    let mut pcm = audio::decode_sound(&std::fs::read(&p).unwrap()).unwrap();
    let mid = pcm.data.len() / 2;
    pcm.data[mid] ^= 1;
    std::fs::write(&p, encode_flac(&pcm).unwrap()).unwrap();
}

fn frames_mut(m: &mut Manifest) -> impl Iterator<Item = &mut Frame> {
    m.animations.iter_mut().flat_map(|a| a.frames.iter_mut())
}

fn duration(d: &Path) {
    edit_manifest(d, |m| frames_mut(m).find(|f| f.ms == 100).unwrap().ms = 110);
}

fn probability(d: &Path) {
    edit_manifest(d, |m| {
        let f = frames_mut(m).find(|f| !f.branch.is_empty()).unwrap();
        if let Branch::Random { p, .. } = &mut f.branch[0] {
            *p += 1;
        }
    });
}

fn exit_target(d: &Path) {
    edit_manifest(d, |m| {
        let f = frames_mut(m)
            .find(|f| matches!(f.exit, Some(Exit::Frame(_))))
            .unwrap();
        if let Some(Exit::Frame(x)) = &mut f.exit {
            *x += 1;
        }
    });
}

fn state_member(d: &Path) {
    edit_manifest(d, |m| {
        let s = m
            .states
            .iter_mut()
            .find(|(k, _)| k == "IdlingLevel1")
            .unwrap();
        s.1.remove(0);
    });
}

fn layers_swapped(d: &Path) {
    edit_manifest(d, |m| {
        let f = frames_mut(m)
            .find(|f| f.layers.len() >= 2 && f.layers[0] != f.layers[1])
            .unwrap();
        f.layers.swap(0, 1);
    });
}

fn layer_offset(d: &Path) {
    edit_manifest(d, |m| {
        let f = frames_mut(m)
            .find(|f| f.layers.first().is_some_and(|l| l.x != 0))
            .unwrap();
        f.layers[0].x += 1;
    });
}

fn mouth_anchor(d: &Path) {
    edit_manifest(d, |m| {
        let f = frames_mut(m).find(|f| !f.mouths.is_empty()).unwrap();
        f.mouths[0].anchor.as_mut().unwrap()[1] += 1;
    });
}

fn transition(d: &Path) {
    edit_manifest(d, |m| {
        let a = m
            .animations
            .iter_mut()
            .find(|a| a.transition == Some(Transition::Exit))
            .unwrap();
        a.transition = Some(Transition::None);
    });
}

fn balloon(d: &Path) {
    edit_manifest(d, |m| m.balloon.as_mut().unwrap().chars_per_line = Some(29));
}

fn o97_probability(d: &Path) {
    edit_manifest(d, |m| {
        let b = frames_mut(m)
            .flat_map(|f| f.branch.iter_mut())
            .find(|b| matches!(b, Branch::Random { p, .. } if *p > 0 && *p < 32768))
            .unwrap();
        if let Branch::Random { p, .. } = b {
            *p -= 1;
        }
    });
}

fn o97_daypart(d: &Path) {
    edit_manifest(d, |m| {
        let b = frames_mut(m)
            .flat_map(|f| f.branch.iter_mut())
            .find(|b| matches!(b, Branch::Daypart { value: 0, .. }))
            .unwrap();
        if let Branch::Daypart { value, .. } = b {
            *value = 1;
        }
    });
}

fn o97_pose_rect(d: &Path) {
    edit_manifest(d, |m| {
        let l = m
            .poses
            .values_mut()
            .flat_map(|p| p.iter_mut())
            .find(|l| l.size.is_some_and(|[w, _]| w > 15))
            .unwrap();
        l.size.as_mut().unwrap()[0] -= 15;
    });
}

fn o97_transition_pose(d: &Path) {
    edit_manifest(d, |m| {
        m.office97.as_mut().unwrap().transitions[0].1 += 1;
    });
}

fn o97_exit_branch(d: &Path) {
    edit_manifest(d, |m| {
        for a in &mut m.animations {
            let n = a.frames.len() as u32;
            for f in &mut a.frames {
                for b in &mut f.branch {
                    if let Branch::Random { p: 0, target } = b
                        && *target + 1 < n
                    {
                        *target += 1;
                        return;
                    }
                }
            }
        }
        panic!("no p=0 branch to move");
    });
}

/// (sample stem, label, mutation); ACT cases use the Office 97 Genius bitmap actor.
const CASES: &[(&str, &str, Mutation)] = &[
    ("clippit", "one pixel index +1", pixel),
    ("clippit", "one palette entry, red +-1", palette),
    ("clippit", "one frame 100 ms -> 110 ms", duration),
    ("clippit", "one branch probability +1", probability),
    ("clippit", "one exit target +1", exit_target),
    ("clippit", "one sound sample byte", sound),
    ("clippit", "state member removed", state_member),
    ("dot", "two layers swapped", layers_swapped),
    ("dot", "one layer offset x+1", layer_offset),
    ("merlin", "mouth anchor y+1", mouth_anchor),
    ("merlin", "transition exit -> none", transition),
    ("merlin", "balloon chars_per_line", balloon),
    ("genius", "random branch p -1", o97_probability),
    ("genius", "daypart 0 -> 1", o97_daypart),
    ("genius", "pose rectangle width -15 units", o97_pose_rect),
    ("genius", "transition target pose", o97_transition_pose),
    ("genius", "p=0 exit branch target +1", o97_exit_branch),
];

fn source_of(stem: &str) -> (String, PathBuf) {
    samples()
        .into_iter()
        .find(|(k, p)| {
            (k == "acs" || k == "act")
                && p.file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .eq_ignore_ascii_case(stem)
        })
        .unwrap_or_else(|| panic!("sample {stem} not found"))
}

#[test]
#[ignore]
fn mutations_are_detected() {
    let tmp = temp_dir("mutate");
    let mut stems: Vec<&str> = CASES.iter().map(|c| c.0).collect();
    stems.dedup();
    let mut missed = Vec::new();
    let mut run = 0;
    for stem in stems {
        let (kind, path) = source_of(stem);
        let oracle = oracle_json(
            &["oracle", &kind, path.to_str().unwrap()],
            &tmp.join(format!("{stem}.json")),
        );
        let opts = bentwire_convert::ConvertOptions {
            office_palette: Some(office_palette()),
            ..Default::default()
        };
        let pkg = match bentwire_convert::convert_to_package_with(&path, &opts) {
            Ok(p) => p,
            Err(e) if kind == "act" => panic!("{stem}: ACT conversion failed: {e}"),
            Err(e) => panic!("{stem}: {e}"),
        };
        let clean = tmp.join(format!("{stem}-clean"));
        pkg.write_folder(&clean).unwrap();
        let base = rust_json(&Character::load(&clean).unwrap());
        let base = oracle_common::normalize(&kind, &oracle, base);
        compare(&oracle, &base).unwrap_or_else(|d| panic!("{stem} unmutated differs: {d}"));
        // Controls: the rewrite paths the mutations use, without a change, still match, so a
        // detection is never an artefact of re-encoding.
        let ctrl = tmp.join(format!("{stem}-control"));
        pkg.write_folder(&ctrl).unwrap();
        edit_manifest(&ctrl, |_| {});
        if kind == "acs" {
            edit_sheet(&ctrl, |_, _, _, _| {});
            let s = ctrl.join("sounds/s00.flac");
            if s.exists() {
                let pcm = audio::decode_sound(&std::fs::read(&s).unwrap()).unwrap();
                std::fs::write(&s, encode_flac(&pcm).unwrap()).unwrap();
            }
        }
        let got =
            oracle_common::normalize(&kind, &oracle, rust_json(&Character::load(&ctrl).unwrap()));
        compare(&oracle, &got).unwrap_or_else(|d| panic!("{stem} control rewrite differs: {d}"));
        println!(
            "{stem:8} {:40} equal (control)",
            "no-op rewrite of manifest, sheet, sound"
        );
        for (s, label, f) in CASES.iter().filter(|c| c.0 == stem) {
            let d = tmp.join(format!("{s}-{run}"));
            run += 1;
            pkg.write_folder(&d).unwrap();
            f(&d);
            let got = match Character::load(&d) {
                Ok(c) => oracle_common::normalize(&kind, &oracle, rust_json(&c)),
                Err(_) => {
                    println!("{s:8} {label:40} DETECTED (load fails)");
                    continue;
                }
            };
            match compare(&oracle, &got) {
                Err(_) => println!("{s:8} {label:40} DETECTED"),
                Ok(_) => {
                    println!("{s:8} {label:40} missed");
                    missed.push(*label);
                }
            }
        }
    }
    println!("mutations: {run}, detected: {}", run - missed.len());
    assert!(missed.is_empty(), "missed: {missed:?}");
}
