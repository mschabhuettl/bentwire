//! Agent 1.5 OLE fixture -> parser round trip (agent15-format 4).

use agent_fixtures::agent15 as fx;
use agent_formats::acf::{Aca, Acf};
use agent_formats::agent15::{OleAcs, is_ole};
use agent_formats::ole::CompoundFile;

#[test]
fn ole_matches_flat_files_at_every_15_version() {
    for v in [0x1_001F, 0x1_001E, 0x1_001C] {
        let spec = fx::sample(v);
        let bytes = fx::write_ole(&spec);
        assert!(is_ole(&bytes));
        let ole = OleAcs::parse(&bytes).unwrap();
        let flat = Acf::parse(&spec.write_acf()).unwrap().character;
        assert_eq!(ole.character, flat);
        assert_eq!(ole.animations.len(), spec.animations.len());
        for (i, a) in ole.animations.iter().enumerate() {
            assert_eq!(
                a.as_ref().unwrap(),
                &Aca::parse(&spec.write_aca(i), &flat).unwrap()
            );
        }
    }
}

#[test]
fn damaged_and_missing_streams_fail_per_animation() {
    let spec = fx::sample(0x1_001F);
    let mut streams = vec![("char.acf".to_string(), fx::char_acf(&spec))];
    let mut damaged = spec.write_aca(0);
    // declared size 2 bytes larger than the data decodes to (like Ozzar anim60.aaf)
    let ulen = u32::from_le_bytes(damaged[9..13].try_into().unwrap()) + 2;
    damaged[9..13].copy_from_slice(&ulen.to_le_bytes());
    streams.push(("ANIM1.AAF".into(), damaged));
    streams.push(("Anim2.aaf".into(), spec.write_aca(1))); // case differs: still found
    // anim3.aaf missing
    let ole = OleAcs::parse(&fx::compound_file(&streams, [7; 16])).unwrap();
    assert_eq!(ole.root_clsid.0, [7; 16]);
    assert!(ole.animations[0].is_err());
    assert!(ole.animations[1].is_ok());
    assert!(
        ole.animations[2]
            .as_ref()
            .unwrap_err()
            .to_string()
            .contains("anim3.aaf")
    );
}

#[test]
fn compound_file_streams() {
    let big: Vec<u8> = (0..20_000u32).map(|i| (i * 31 % 251) as u8).collect();
    let streams = vec![
        ("small".to_string(), vec![1, 2, 3]),
        ("big".to_string(), big.clone()),
        ("empty".to_string(), vec![]),
        ("edge".to_string(), vec![9; 4096]),
        ("mini".to_string(), vec![5; 4095]),
    ];
    let f = fx::compound_file(&streams, [0; 16]);
    let cf = CompoundFile::parse(&f).unwrap();
    for (name, data) in &streams {
        assert_eq!(&cf.root_stream(name).unwrap().unwrap(), data, "{name}");
    }
    assert!(cf.root_stream("nope").unwrap().is_none());
    // no char.acf stream
    assert!(OleAcs::parse(&f).is_err());
}

/// Review regression: OLE streams take any header version (agent15-format 4.5); the web
/// rule (seek back, read headerless) applies to flat files only.
#[test]
fn ole_header_version_is_not_checked() {
    let spec = fx::sample(0x1_001F);
    let mut aaf = spec.write_aca(0);
    aaf[..4].copy_from_slice(&0xDEAD_BEEFu32.to_le_bytes());
    let streams = vec![
        ("char.acf".to_string(), fx::char_acf(&spec)),
        ("anim1.aaf".to_string(), aaf.clone()),
    ];
    let ole = OleAcs::parse(&fx::compound_file(&streams, [0; 16])).unwrap();
    let a = ole.animations[0].as_ref().unwrap();
    assert!(matches!(
        a.header,
        agent_formats::acf::AcaHeader::Present {
            version: 0xDEAD_BEEF,
            ..
        }
    ));
    let flat = Acf::parse(&spec.write_acf()).unwrap().character;
    assert!(Aca::parse(&aaf, &flat).is_err());
}
