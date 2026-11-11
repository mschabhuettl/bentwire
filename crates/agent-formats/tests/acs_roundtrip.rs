//! ACS fixture -> parser round trip (acs-format 3).

use agent_fixtures::acs as fx;
use agent_fixtures::common::{Compression, Picture};
use agent_formats::acs::Acs;

fn check(spec: &fx::Acs) {
    let bytes = spec.write();
    let acs = Acs::parse(&bytes).expect("fixture parses");
    let c = &acs.character;
    assert_eq!(c.version, spec.version);
    assert_eq!(c.guid.0, spec.guid);
    assert_eq!(
        (c.width, c.height, c.transparent),
        (spec.width, spec.height, spec.transparent)
    );
    assert_eq!(c.flags, spec.flags_written());
    assert_eq!(c.animation_set, spec.animation_set);
    match (&c.voice, &spec.voice) {
        (None, None) => {}
        (Some(v), Some(s)) => {
            assert_eq!(
                (v.engine.0, v.mode.0, v.speed, v.pitch),
                (s.engine, s.mode, s.speed, s.pitch)
            );
            let e = v.extra.as_ref().map(|e| {
                (
                    e.language,
                    e.dialect.clone(),
                    e.gender,
                    e.age,
                    e.style.clone(),
                )
            });
            assert_eq!(e, s.extra);
        }
        _ => panic!("voice presence differs"),
    }
    match (&c.balloon, &spec.balloon) {
        (None, None) => {}
        (Some(b), Some(s)) => {
            assert_eq!(
                (
                    b.lines,
                    b.chars_per_line,
                    b.foreground,
                    b.background,
                    b.border
                ),
                (
                    s.lines,
                    s.chars_per_line,
                    s.foreground,
                    s.background,
                    s.border
                )
            );
            assert_eq!(
                (&b.font[..], b.font_height, b.font_weight, b.italic),
                (&s.font[..], s.font_height, s.font_weight, s.italic)
            );
            assert_eq!(b.flag2, Some(s.flag2));
        }
        _ => panic!("balloon presence differs"),
    }
    let pal: Vec<[u8; 4]> = c
        .palette
        .iter()
        .map(|q| [q.b, q.g, q.r, q.reserved])
        .collect();
    assert_eq!(pal, spec.palette);
    assert_eq!(
        c.tray_icon
            .as_ref()
            .map(|t| (t.mask_dib.clone(), t.color_dib.clone())),
        spec.tray_icon
    );
    let states: Vec<_> = c
        .states
        .iter()
        .map(|s| (s.name.clone(), s.animations.clone()))
        .collect();
    assert_eq!(states, spec.states);
    let loc: Vec<_> = c
        .localized
        .iter()
        .map(|l| {
            (
                l.langid,
                l.name.clone(),
                l.description.clone(),
                l.extra.clone(),
            )
        })
        .collect();
    assert_eq!(loc, spec.localized);

    assert_eq!(acs.animations.len(), spec.animations.len());
    for (a, s) in acs.animations.iter().zip(&spec.animations) {
        assert_eq!(a.name, s.name);
        assert_eq!(a.record_name, s.name.to_uppercase());
        assert_eq!(
            (a.transition, &a.return_animation),
            (s.transition, &s.return_animation)
        );
        assert_eq!(a.frames.len(), s.frames.len());
        for (f, sf) in a.frames.iter().zip(&s.frames) {
            let imgs: Vec<_> = f.images.iter().map(|i| (i.image, i.x, i.y)).collect();
            assert_eq!(imgs, sf.images);
            assert_eq!(
                (f.sound, f.duration, f.exit),
                (sf.sound, sf.duration, sf.exit)
            );
            let br: Vec<_> = f
                .branches
                .iter()
                .map(|b| (b.target, b.probability))
                .collect();
            assert_eq!(br, sf.branches);
            let ov: Vec<_> = f
                .overlays
                .iter()
                .map(|o| {
                    (
                        o.mouth,
                        o.replace_top,
                        o.image,
                        o.x,
                        o.y,
                        o.point_x,
                        o.point_y,
                    )
                })
                .collect();
            let so: Vec<_> = sf
                .overlays
                .iter()
                .map(|o| {
                    (
                        o.mouth,
                        o.replace_top,
                        o.image,
                        o.x,
                        o.y,
                        o.point_x,
                        o.point_y,
                    )
                })
                .collect();
            assert_eq!(ov, so);
        }
    }
    assert!(acs.animation("greet").is_some() && acs.animation("nope").is_none());

    assert_eq!(acs.images.len(), spec.images.len());
    for (i, (e, s)) in acs.images.iter().zip(&spec.images).enumerate() {
        assert_eq!(e.checksum, fx::checksum(i));
        match (&e.image, &s.picture) {
            (None, None) => {}
            (Some(img), Some(p)) => {
                assert_eq!((img.bitmap.width, img.bitmap.height), (p.width, p.height));
                assert_eq!(img.bitmap.top_down(), p.pixels, "image {i} pixels");
                assert_eq!(img.compressed, s.pixels != Compression::Raw);
                assert_eq!(img.region_compressed, s.region != Compression::Raw);
                if s.empty_region {
                    assert!(img.region.is_none());
                } else {
                    // the region covers exactly the non-transparent pixels
                    let rgn = img.region.as_ref().expect("region");
                    let mut mask = vec![false; p.pixels.len()];
                    for r in &rgn.rects {
                        for y in r.top..r.bottom {
                            for x in r.left..r.right {
                                mask[y as usize * p.width as usize + x as usize] = true;
                            }
                        }
                    }
                    let want: Vec<bool> = p.pixels.iter().map(|&v| v != spec.transparent).collect();
                    assert_eq!(mask, want, "image {i} region");
                }
            }
            _ => panic!("image {i} presence differs"),
        }
    }
    assert_eq!(acs.sounds.len(), spec.sounds.len());
    for (i, (e, s)) in acs.sounds.iter().zip(&spec.sounds).enumerate() {
        assert_eq!((e.checksum, &e.data), (fx::checksum(i), s));
    }
}

#[test]
fn sample_round_trip() {
    check(&fx::sample());
}

#[test]
fn variants_round_trip() {
    // no voice, no balloon, no icon, kind-0 image, version 0x1001F, odd palette
    let mut s = fx::sample();
    s.voice = None;
    s.balloon = None;
    s.tray_icon = None;
    s.version = 0x1_001F;
    s.palette.truncate(255);
    s.images[4].picture = None;
    check(&s);
    // voice without extra block, empty strings everywhere
    let mut s = fx::sample();
    s.voice.as_mut().unwrap().extra = None;
    s.localized = vec![(9, String::new(), String::new(), String::new())];
    s.states.clear();
    check(&s);
    // a large picture needs far back-references
    let mut s = fx::sample();
    s.width = 300;
    s.height = 200;
    s.images[0].picture = Some(Picture::pattern(300, 200, 9, s.transparent));
    check(&s);
}

#[test]
fn rejects_bad_header_and_versions() {
    use agent_formats::Error;
    let bytes = fx::sample().write();
    let mut b = bytes.clone();
    b[0] ^= 1;
    assert!(matches!(Acs::parse(&b), Err(Error::BadMagic(_))));
    assert!(Acs::parse(&bytes[..20]).is_err());
    for v in [0x1_001E, 0x2_0002, 0] {
        let mut s = fx::sample();
        s.version = v;
        let e = Acs::parse(&s.write()).unwrap_err();
        assert_eq!(e.root(), &Error::UnsupportedVersion(v));
    }
    // zero frames and mouth type 7 are rejected
    let mut s = fx::sample();
    s.animations[0].frames.clear();
    assert!(Acs::parse(&s.write()).is_err());
    let mut s = fx::sample();
    s.animations[1].frames[3].overlays[0].mouth = 7;
    assert!(Acs::parse(&s.write()).is_err());
}
