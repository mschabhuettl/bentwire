//! Writer: manifest text round trip, reproducible packages, FLAC bit exactness, interpreter B.

use bentwire_character::dump::{Dumper, SplitMix64, Step, next_agent};
use bentwire_character::write::{encode_flac, encode_png_indexed, encode_png_rgba};
use bentwire_character::*;

const M: &str = r##"
format = 1
family = "agent"

[character]
id = "{0B5E7C1A-4D2F-4E8B-9A6C-3F1D2E4B5A60}"
canvas = [4, 4]

[[character.info]]
lang = "en"
name = "Pe\"bb\\le"
extra = "tab\there"

[balloon]
enabled = true
font = "Tahoma"
font_height = -13

[[sheets]]
id = "a"
file = "sheets/a.png"
cell = [2, 2]
count = 3
[sheets.hidpi]
2 = "sheets/a@2x.png"

[sounds]
s00 = "sounds/s00.flac"
s01 = "sounds/s01.flac"

[states]
Showing = ["Show"]
"Odd.Name" = ["Show"]

[[animations]]
name = "Show"
transition = "return"
return = "Show"
frames = [
  { ms = 100, layers = ["a:0", ["a:1", 2, 2], ["a:2", -1, 0, 3, 3]], sound = "s00" },
  { ms = 0 },
  { ms = 100, layers = ["a:0"], exit = 0, branch = [[0, 30], [2, 50]], mouths = [{ type = 6, layer = ["a:2", 1, 1], anchor = [2, 2], base = [] }] },
  { ms = 50, layers = ["a:1"], exit = "end", sound = "s01" },
]

[source]
format = "test"
header = { kind = 7, name = "x" }
missing_animations = []
"##;

fn package() -> PackageData {
    let manifest = parse::parse_manifest(M).unwrap();
    let plte = [[255, 0, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255]];
    let idx = [1, 1, 2, 0, 3, 3, 1, 0, 0, 0, 3, 3];
    let s00 = Pcm {
        channels: 2,
        rate: 22050,
        bits: 16,
        data: (0..2000i32)
            .flat_map(|i| (((i * 37) % 6000 - 3000) as i16).to_le_bytes())
            .collect(),
    };
    let s01 = Pcm {
        channels: 1,
        rate: 11025,
        bits: 8,
        data: (0..777u32).map(|i| (i * 7 % 256) as u8).collect(),
    };
    let mut hi = vec![0u8; 12 * 4 * 4];
    for (i, p) in hi.chunks_mut(4).enumerate() {
        p.copy_from_slice(&[i as u8, 0, 0, 128]);
    }
    PackageData {
        manifest,
        header: vec!["test character".into()],
        files: vec![
            ("sounds/s01.flac".into(), encode_flac(&s01).unwrap()),
            (
                "sheets/a.png".into(),
                encode_png_indexed(6, 2, &idx, &plte, Some(&[0])).unwrap(),
            ),
            (
                "sheets/a@2x.png".into(),
                encode_png_rgba(12, 4, &hi).unwrap(),
            ),
            ("sounds/s00.flac".into(), encode_flac(&s00).unwrap()),
        ],
    }
}

#[test]
fn manifest_text_round_trips() {
    let p = package();
    let text = p.manifest_text();
    let back = parse::parse_manifest(&text).unwrap();
    assert_eq!(back, p.manifest);
    assert!(validate(&back).is_empty(), "{:?}", validate(&back));
}

#[test]
fn bwc_and_folder_are_reproducible() {
    let a = package().to_bwc().unwrap();
    let b = package().to_bwc().unwrap();
    assert_eq!(a, b, "two runs give byte-identical .bwc");
    let d = std::env::temp_dir().join(format!("bwc-write-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    package().write_folder(&d.join("x")).unwrap();
    package().write_bwc(&d.join("x.bwc")).unwrap();
    let f = Character::load(d.join("x")).unwrap();
    let z = Character::load(d.join("x.bwc")).unwrap();
    assert!(
        f.warnings.is_empty() && z.warnings.is_empty(),
        "{:?}",
        z.warnings
    );
    let df = Dumper::new(&f).dump();
    let dz = Dumper::new(&z).dump();
    assert_eq!(df, dz);
    // The ZIP rules: manifest first and deflated, media stored.
    let ar = zip::Archive::new(a).unwrap();
    assert_eq!(ar.entries[0].name, "character.toml");
    assert_eq!(ar.entries[0].method, 8);
    assert!(ar.entries[1..].iter().all(|e| e.method == 0));
}

#[test]
fn flac_is_bit_exact() {
    let p = package();
    let c = p.load().unwrap();
    let s00 = c.sound("s00").unwrap();
    assert_eq!(
        (s00.channels, s00.rate, s00.bits, s00.frames()),
        (2, 22050, 16, 1000)
    );
    assert_eq!(s00.sample(3), 3 * 37 - 3000);
    let s01 = c.sound("s01").unwrap();
    assert_eq!(s01.bits, 8);
    assert_eq!(
        s01.data,
        (0..777u32).map(|i| (i * 7 % 256) as u8).collect::<Vec<_>>()
    );
}

/// spec R-ANIM-4 test vector: branches [[5, 30], [7, 50]].
#[test]
fn r_anim_4_cumulative_branch_walk() {
    let mut fr: Vec<Frame> = (0..9)
        .map(|_| Frame {
            ms: 10,
            layers: vec![Layer {
                image: ImageRef {
                    sheet: "a".into(),
                    index: 0,
                },
                x: 0,
                y: 0,
                size: None,
            }],
            ..Frame::default()
        })
        .collect();
    fr[0].branch = vec![
        Branch::Random { target: 5, p: 30 },
        Branch::Random { target: 7, p: 50 },
    ];
    for (r, want) in [(29, 5), (30, 7), (79, 7), (80, 1)] {
        assert_eq!(next_agent(&fr, 0, false, &mut |_| r), Some(want));
    }
    // R-ANIM-5: invisible branch points are walked through.
    fr[1].layers.clear();
    fr[1].ms = 0;
    assert_eq!(next_agent(&fr, 0, false, &mut |_| 99), Some(2));
}

#[test]
fn interpreter_b_exit_mode() {
    let c = package().load().unwrap();
    let d = Dumper::new(&c);
    // Normal play with draw 99: frame 0, (1 skipped), 2 (no branch taken), 3, END.
    let t = d.trace("Show", &mut |_| 99, None, 0, 0, 300);
    assert_eq!(t.len(), 4);
    assert_eq!(t[3], Step::End);
    // Exit requested at step 2 (frame 2, exit 0): jumps to frame 0, whose exit is absent, so it
    // continues normally: 0 -> 2 -> exit 0 ... until LIMIT.
    let t = d.trace("Show", &mut |_| 99, Some(2), 0, 0, 20);
    assert_eq!(t.last(), Some(&Step::Limit));
    // Exit requested on frame 3 (exit "end"): CUT.
    let t = d.trace("Show", &mut |_| 99, Some(3), 0, 0, 20);
    assert_eq!(t.last(), Some(&Step::Cut));
}

#[test]
fn splitmix64_reference_values() {
    // Reference values of splitmix64 seeded with 0 (Vigna's reference implementation).
    let mut g = SplitMix64(0);
    assert_eq!(g.next_u64(), 0xE220_A839_7B1D_CDAF);
    assert_eq!(g.next_u64(), 0x6E78_9E6A_A1B9_65F4);
}
