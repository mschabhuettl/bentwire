//! Fixture round trip for the ACT parser: synthetic actors from `agent-fixtures` parse
//! back to exactly what was written (act-format 4).

mod act_common;

use agent_fixtures::act::{self as fx, Actor, PayloadSpec};
use agent_fixtures::common::lz_compress;
use agent_formats::act::{self, ActError, Payload, lz};
use serde_json::Value;

fn check(spec: &Actor) -> act::ActFile {
    let bytes = spec.to_bytes();
    let f = act::parse(&bytes).expect("fixture parses");
    assert_eq!((f.major, f.minor), (spec.major, spec.minor));
    assert_eq!(f.item_name, spec.name);
    let h = &f.header;
    assert_eq!(
        (h.kind, h.rest_pose, h.idle_anim, h.logical_w, h.logical_h),
        (
            spec.kind,
            spec.rest_pose,
            spec.idle_anim,
            spec.logical.0,
            spec.logical.1
        )
    );
    assert_eq!((h.default_px_w, h.default_px_h), spec.default_px);
    assert_eq!([h.unk10, h.unk12, h.unk14, h.unk16], spec.unk);
    assert_eq!(h.rect, spec.rect);
    assert_eq!(h.nohit_image, spec.nohit_image);
    assert_eq!((h.stretch_x0, h.stretch_x1), spec.stretch);
    assert_eq!(f.color_table, spec.color_table);
    let tr: Vec<_> = f
        .transitions
        .iter()
        .map(|t| (t.from_pose, t.to_pose, t.anim_id))
        .collect();
    assert_eq!(tr, spec.transitions);
    if spec.major == 2 && spec.minor >= 2 {
        assert_eq!(f.transition_rest_pose, Some(spec.rest_pose));
    } else {
        assert_eq!(f.transition_rest_pose, None);
    }
    // payloads, one id per bitmap image
    let mut got = f.payloads.iter();
    for p in &spec.payloads {
        match p {
            PayloadSpec::Composite(cs) => {
                let Some(Payload::Composite(g)) = got.next() else {
                    panic!("composite expected")
                };
                let g: Vec<_> = g
                    .iter()
                    .map(|c| (c.id, c.left, c.top, c.right, c.bottom))
                    .collect();
                assert_eq!(&g, cs);
            }
            PayloadSpec::Wmf(b) => {
                let Some(Payload::Wmf(w)) = got.next() else {
                    panic!("wmf expected")
                };
                assert_eq!(&w.bytes, b);
                assert_eq!(w.records.last().unwrap().function, 0);
                assert_eq!(w.records[0].function, 0x0103);
            }
            PayloadSpec::Bitmaps(ims, _) => {
                for im in ims {
                    let Some(Payload::Bitmap(b)) = got.next() else {
                        panic!("bitmap expected")
                    };
                    assert_eq!((b.width, b.height), (im.width, im.height));
                    assert_eq!(b.pixels, im.pixels);
                }
            }
        }
    }
    assert!(got.next().is_none());
    let snd: Vec<_> = f.sounds.iter().map(|s| s.bytes.clone()).collect();
    assert_eq!(snd, spec.sounds);
    let anims: Vec<_> = f
        .animations
        .iter()
        .map(|g| {
            let vs: Vec<Vec<_>> = g
                .variants
                .iter()
                .map(|v| v.commands.iter().map(|c| (c.kind, c.a, c.b)).collect())
                .collect();
            (g.id, vs)
        })
        .collect();
    assert_eq!(anims, spec.animations);
    assert!(
        f.animations
            .iter()
            .all(|g| g.variants.iter().all(|v| v.header == 0x0100))
    );
    let strs: Vec<_> = f
        .strings
        .iter()
        .map(|g| (g.id, g.variants.clone()))
        .collect();
    assert_eq!(strs, spec.strings);
    assert_eq!(
        (f.unused_animation_entries, f.unused_string_entries),
        (0, 0)
    );
    f
}

#[test]
fn act_fixture_bitmap_actor_round_trips() {
    let f = check(&fx::bitmap_actor());
    assert_eq!(
        f.string_text(&f.string_group(40).unwrap().variants[1])
            .as_deref(),
        Some("Hi there")
    );
    assert_eq!(f.sounds[1].bits_per_sample, 16);
    assert_eq!(f.sounds[1].sample_rate, 22050);
    match &f.payloads[3] {
        Payload::Bitmap(b) => assert_eq!(b.flags, 2),
        p => panic!("{p:?}"),
    }
}

#[test]
fn act_fixture_wmf_actor_round_trips() {
    let f = check(&fx::wmf_actor());
    let Payload::Wmf(w) = &f.payloads[0] else {
        panic!()
    };
    assert_eq!(w.bbox, [0, 0, 1000, 1000]);
    assert_eq!(w.inch, 1440);
    let poly = w.records.iter().find(|r| r.function == 0x0324).unwrap();
    assert_eq!(
        (poly.word(0), poly.int(1), poly.int(2)),
        (Some(3), Some(100), Some(100))
    );
}

#[test]
fn act_fixture_bob_actor_round_trips() {
    let f = check(&fx::bob_actor());
    assert_eq!(f.string_text(&f.strings[0].variants[0]), None);
    assert_eq!(f.strings[1].variants[0], b"Caf\xe9");
}

#[test]
fn act_lz_covers_all_distance_classes_and_long_lengths() {
    // long run (j = 11 lengths), near/mid/far repeats, then a block repeated 5000+ bytes back
    let mut data = vec![7u8; 9000];
    let mut x = 1u32;
    let noise: Vec<u8> = (0..6000)
        .map(|_| {
            x = x.wrapping_mul(1_103_515_245).wrapping_add(12345);
            (x >> 16) as u8
        })
        .collect();
    data.extend_from_slice(&noise);
    data.extend_from_slice(&noise[100..140]); // distance > 4673
    data.extend_from_slice(&noise[5900..5950]); // distance ~150
    data.extend_from_slice(&noise[5000..5100]); // distance ~1100
    data.extend_from_within(data.len() - 30..); // distance 30
    let c = lz_compress(&data, true);
    assert_eq!(lz::decompress(&c, data.len()).unwrap(), data);
    // wrong declared size, truncation and a bad lead byte are errors
    assert!(lz::decompress(&c, data.len() - 1).is_err());
    assert!(lz::decompress(&c, data.len() + 1).is_err());
    assert!(lz::decompress(&c[..c.len() / 2], data.len()).is_err());
    let mut bad = c.clone();
    bad[0] = 1;
    assert!(lz::decompress(&bad, data.len()).is_err());
}

#[test]
fn act_rejects_bad_container() {
    let b = fx::bitmap_actor().to_bytes();
    assert_eq!(act::parse(b"XX"), Err(ActError::BadMagic));
    let mut v = b.clone();
    v[2] = 3;
    assert!(matches!(
        act::parse(&v),
        Err(ActError::Version { major: 3, .. })
    ));
    assert!(act::parse(&b[..b.len() - 1]).is_err());
}

#[test]
#[ignore = "real data: needs BENTWIRE_REFERENCE (research parser validates the fixtures)"]
fn act_fixtures_accepted_by_research_parser() {
    let research = act_common::research();
    let dir = act_common::temp_dir("act-fixtures");
    let mut files = Vec::new();
    for (name, a) in [
        ("bitmap.act", fx::bitmap_actor()),
        ("wmf.act", fx::wmf_actor()),
        ("bob.act", fx::bob_actor()),
    ] {
        let p = dir.join(name);
        std::fs::write(&p, a.to_bytes()).unwrap();
        files.push(p);
    }
    let (list, out) = act_common::run_oracle(&research, &files);
    assert_eq!(list.len(), 3);
    for (key, path) in &list {
        let f = act::parse(&std::fs::read(path).unwrap()).unwrap();
        let want: Value =
            serde_json::from_slice(&std::fs::read(out.join(format!("json/{key}.json"))).unwrap())
                .unwrap();
        assert_eq!(act_common::dump(&f), want, "{key}");
    }
    std::fs::remove_dir_all(&dir).ok();
    std::fs::remove_dir_all(&out).ok();
}
