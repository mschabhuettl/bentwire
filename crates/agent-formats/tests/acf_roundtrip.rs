//! ACF + ACA/AAF fixture -> parser round trip at every accepted version (acf-aaf-format 5-7,
//! agent15-format 4).

use agent_fixtures::acf as fx;
use agent_fixtures::common::Compression;
use agent_formats::acf::{Aca, AcaHeader, Acf, AnimationFile, BaseImage, read_web_character};
use agent_formats::common::Region;

fn mask(r: &Region, w: u16, h: u16) -> Vec<bool> {
    let mut m = vec![false; w as usize * h as usize];
    for x in &r.rects {
        for y in x.top..x.bottom {
            for xx in x.left..x.right {
                m[y as usize * w as usize + xx as usize] = true;
            }
        }
    }
    m
}

fn check(spec: &fx::Acf) {
    let v = spec.version;
    let acf = Acf::parse(&spec.write_acf()).expect("ACF parses");
    assert_eq!(acf.magic, spec.magic);
    let c = &acf.character;
    assert_eq!(c.version, v);
    assert_eq!(c.guid.0, spec.guid);
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
    if v >= 0x2_0001 {
        assert_eq!(loc, spec.localized);
    } else {
        let (_, n, d, e) = spec.localized[0].clone();
        assert_eq!(loc, vec![(9, n, d, e)]);
    }
    assert_eq!(
        (c.width, c.height, c.transparent, c.flags),
        (
            spec.width,
            spec.height,
            spec.transparent,
            spec.flags_written()
        )
    );
    assert_eq!(
        c.animation_set,
        (v >= 0x2_0001).then_some(spec.animation_set)
    );
    let sv = spec.voice.as_ref().unwrap();
    let cv = c.voice.as_ref().unwrap();
    assert_eq!(
        (cv.engine.0, cv.mode.0, cv.speed, cv.pitch),
        (sv.engine, sv.mode, sv.speed, sv.pitch)
    );
    let extra = cv.extra.as_ref().map(|e| {
        (
            e.language,
            e.dialect.clone(),
            e.gender,
            e.age,
            e.style.clone(),
        )
    });
    assert_eq!(extra, if v > 0x2_0000 { sv.extra.clone() } else { None });
    let b = c.balloon.as_ref().unwrap();
    let sb = spec.balloon.as_ref().unwrap();
    assert_eq!(
        (
            b.lines,
            b.chars_per_line,
            b.foreground,
            b.background,
            b.border
        ),
        (
            sb.lines,
            sb.chars_per_line,
            sb.foreground,
            sb.background,
            sb.border
        )
    );
    assert_eq!(
        (&b.font[..], b.font_height, b.font_weight, b.italic),
        (&sb.font[..], sb.font_height, sb.font_weight, sb.italic)
    );
    assert_eq!(b.flag2, (v > 0x1_001E).then_some(sb.flag2));
    let pal: Vec<[u8; 4]> = c
        .palette
        .iter()
        .map(|q| [q.b, q.g, q.r, q.reserved])
        .collect();
    assert_eq!(pal, spec.palette);
    let icon = c
        .tray_icon
        .as_ref()
        .map(|t| (t.mask_dib.clone(), t.color_dib.clone()));
    assert_eq!(
        icon,
        if v > 0x2_0000 {
            spec.tray_icon.clone()
        } else {
            None
        }
    );
    let states: Vec<_> = c
        .states
        .iter()
        .map(|s| (s.name.clone(), s.animations.clone()))
        .collect();
    assert_eq!(states, spec.states);
    assert_eq!(c.animations.len(), spec.animations.len());
    for (i, (e, sa)) in c.animations.iter().zip(&spec.animations).enumerate() {
        assert_eq!(
            (&e.name, &e.file_name, &e.return_animation),
            (&sa.name, &sa.file_name, &sa.return_animation)
        );
        assert_eq!(e.checksum, (v > 0x1_001D).then_some(sa.checksum));
        let aca =
            Aca::parse(&spec.write_aca(i), c).unwrap_or_else(|e| panic!("{v:#x} {}: {e}", sa.name));
        aca.check_checksum(e).unwrap();
        match aca.header {
            AcaHeader::Absent => assert!(v < 0x1_001D),
            AcaHeader::Present { version, checksum } => {
                assert_eq!((version, checksum), (v, sa.checksum))
            }
            AcaHeader::Unrecognized { .. } => assert!(sa.headerless),
        }
        assert_eq!(aca.compressed, sa.compression != Compression::Raw);
        let an = &aca.animation;
        assert_eq!(an.sounds, sa.sounds);
        assert_eq!(an.transition, (v >= 0x1_0020).then_some(sa.transition));
        assert_eq!(an.images.len(), sa.images.len());
        for (img, sp) in an.images.iter().zip(&sa.images) {
            match (img, sp) {
                (None, None) => {}
                (Some(img), Some(p)) => {
                    assert_eq!(img.unknown, sa.image_unknown);
                    assert_eq!(img.bitmap.top_down(), p.pixels);
                    let want: Vec<bool> = p.pixels.iter().map(|&x| x != spec.transparent).collect();
                    assert_eq!(mask(img.region.as_ref().unwrap(), p.width, p.height), want);
                }
                _ => panic!("image presence"),
            }
        }
        assert_eq!(an.frames.len(), sa.frames.len());
        for (f, sf) in an.frames.iter().zip(&sa.frames) {
            assert_eq!(
                (f.image, f.sound, f.duration, f.offset_x, f.offset_y),
                (sf.image, sf.sound, sf.duration, sf.offset.0, sf.offset.1)
            );
            assert_eq!(f.exit, (v >= 0x1_0020).then_some(sf.exit));
            let br: Vec<_> = f
                .branches
                .iter()
                .map(|b| (b.target, b.probability))
                .collect();
            assert_eq!(br, sf.branches);
            let has_base = v > 0x1_001F && !sf.overlays.is_empty();
            match (&f.base, &sf.base) {
                (None, _) => assert!(!has_base),
                (Some(BaseImage::Absent), fx::Base::Absent)
                | (Some(BaseImage::Empty), fx::Base::Empty) => assert!(has_base),
                (Some(BaseImage::Image { bitmap, region }), fx::Base::Picture(p)) => {
                    assert!(has_base);
                    assert_eq!(bitmap.top_down(), p.pixels);
                    assert!(region.is_some());
                }
                (a, b) => panic!("base {a:?} vs {b:?}"),
            }
            assert_eq!(f.overlays.len(), sf.overlays.len());
            for (o, so) in f.overlays.iter().zip(&sf.overlays) {
                assert_eq!(o.mouth, so.mouth);
                match (&o.data, &so.picture) {
                    (None, None) => {}
                    (Some(d), Some((x, y, p))) => {
                        assert_eq!((d.x, d.y, d.reserved), (*x, *y, so.reserved));
                        assert_eq!((d.bitmap.width, d.bitmap.height), (p.width, p.height));
                        assert_eq!(d.bitmap.top_down(), p.pixels);
                        assert_eq!(d.replace_base, (v > 0x1_001F).then_some(so.replace_base));
                        assert_eq!(d.region.is_some(), v > 0x1_001F && so.replace_base != 0);
                    }
                    _ => panic!("overlay presence"),
                }
            }
        }
    }
}

#[test]
fn every_version_round_trips() {
    for v in agent_formats::acf::VERSIONS {
        check(&fx::sample(v));
        let mut s = fx::sample(v);
        s.voice.as_mut().unwrap().extra = None;
        s.tray_icon = None;
        s.animations[0].headerless = true;
        s.animations[0].compression = Compression::Raw;
        check(&s);
    }
}

#[test]
fn web_directory_loader() {
    let s = fx::sample(0x2_0001);
    let dir = std::env::temp_dir().join(format!("bentwire-acf-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("webby.acf"), s.write_acf()).unwrap();
    // case differs from the ACF table; animation 1 missing; animation 2 stale
    std::fs::write(dir.join("SHOW.ACA"), s.write_aca(0)).unwrap();
    let mut stale = s.clone();
    stale.animations[2].checksum ^= 1;
    std::fs::write(dir.join("BlinkReturn.aca"), stale.write_aca(2)).unwrap();
    let w = read_web_character(&dir.join("webby.acf")).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(w.animations[0], AnimationFile::Loaded(_)));
    assert_eq!(w.animations[1], AnimationFile::Missing);
    match &w.animations[2] {
        AnimationFile::Failed(e) => assert!(matches!(
            e.root(),
            agent_formats::Error::ChecksumMismatch { .. }
        )),
        a => panic!("{a:?}"),
    }
}

#[test]
fn rejects_bad_input() {
    use agent_formats::Error;
    let s = fx::sample(0x2_0001);
    let mut b = s.write_acf();
    b[0] = 0xC3;
    assert!(matches!(Acf::parse(&b), Err(Error::BadMagic(_))));
    let mut s2 = s.clone();
    s2.version = 0x1_001D;
    assert_eq!(
        Acf::parse(&s2.write_acf()).unwrap_err().root(),
        &Error::UnsupportedVersion(0x1_001D)
    );
    let c = Acf::parse(&s.write_acf()).unwrap().character;
    let mut s3 = s.clone();
    s3.animations[0].frames.clear();
    assert!(Aca::parse(&s3.write_aca(0), &c).is_err());
    let mut s4 = s.clone();
    s4.animations[0].frames[2].overlays[0].mouth = 9;
    assert!(Aca::parse(&s4.write_aca(0), &c).is_err());
}

/// Review regression: an ACF path without a directory part reads its animations from ".".
#[test]
fn web_loader_relative_path() {
    let s = fx::sample(0x2_0001);
    let dir = std::env::temp_dir().join(format!("bentwire-acf-rel-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("rel.acf"), s.write_acf()).unwrap();
    std::fs::write(dir.join("Show.aca"), s.write_aca(0)).unwrap();
    // a bare file name has an empty parent; every other test here uses absolute paths, so
    // changing this process's working directory is safe
    std::env::set_current_dir(&dir).unwrap();
    let w = read_web_character(std::path::Path::new("rel.acf")).unwrap();
    std::fs::remove_dir_all(&dir).ok();
    assert!(matches!(w.animations[0], AnimationFile::Loaded(_)));
}
