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

/// CRC-8 (polynomial 0x07) of a FLAC frame header.
fn crc8(b: &[u8]) -> u8 {
    b.iter().fold(0u8, |mut c, &x| {
        c ^= x;
        for _ in 0..8 {
            c = if c & 0x80 != 0 {
                (c << 1) ^ 0x07
            } else {
                c << 1
            };
        }
        c
    })
}

/// Frame number and block size of every frame header (sync code and CRC-8 checked) in a
/// FLAC stream with a single STREAMINFO block, in order.
fn frame_headers(f: &[u8]) -> Vec<(u64, u64)> {
    let mut out = Vec::new();
    let mut p = 4 + 4 + 34;
    while p + 6 <= f.len() {
        if f[p] != 0xFF || f[p + 1] != 0xF8 {
            p += 1;
            continue;
        }
        let bs = f[p + 2] >> 4;
        let sr = f[p + 2] & 15;
        // UTF-8-like coded frame number.
        let mut q = p + 4;
        let lead = f[q];
        let extra = lead.leading_ones() as usize;
        let extra = extra.saturating_sub(1);
        let mut num = u64::from(lead & (0x7F >> extra.min(6)));
        for i in 1..=extra {
            num = (num << 6) | u64::from(f[q + i] & 0x3F);
        }
        q += 1 + extra;
        let size = match bs {
            1 => 192,
            2..=5 => 576 << (bs - 2),
            6 => {
                q += 1;
                u64::from(f[q - 1]) + 1
            }
            7 => {
                q += 2;
                u64::from(u16::from_be_bytes([f[q - 2], f[q - 1]])) + 1
            }
            8..=15 => 256 << (bs - 8),
            _ => 0,
        };
        q += match sr {
            12 => 1,
            13 | 14 => 2,
            _ => 0,
        };
        if q < f.len() && crc8(&f[p..q]) == f[q] && size > 0 {
            out.push((num, size));
            p = q + 1;
        } else {
            p += 1;
        }
    }
    out
}

/// libFLAC's rule for a fixed-blocksize stream: STREAMINFO min block size == max block
/// size (the last block may be shorter), frame n starts at sample n * block size; else
/// `flac -t` warns "sample or frame number does not increase correctly".
#[test]
fn flac_frame_numbers_increase_with_fixed_block_size() {
    for frames in [1u32, 4096, 4096 * 2 + 100, 7300] {
        let pcm = Pcm {
            channels: 1,
            rate: 22050,
            bits: 16,
            data: (0..frames)
                .flat_map(|i| ((i * 37 % 5000) as i16 - 2500).to_le_bytes())
                .collect(),
        };
        let f = encode_flac(&pcm).unwrap();
        let min = u64::from(u16::from_be_bytes([f[8], f[9]]));
        let max = u64::from(u16::from_be_bytes([f[10], f[11]]));
        let h = frame_headers(&f);
        assert_eq!(
            h.iter().map(|x| x.1).sum::<u64>(),
            u64::from(frames),
            "{frames}: {h:?}"
        );
        let mut at = 0;
        for (i, &(num, size)) in h.iter().enumerate() {
            assert_eq!(num, i as u64, "{frames}: {h:?}");
            assert_eq!(min, max, "{frames}: STREAMINFO {min}..{max}, frames {h:?}");
            assert_eq!(num * max, at, "{frames}: {h:?}");
            at += size;
        }
        assert_eq!(audio::decode_sound(&f).unwrap().data, pcm.data, "{frames}");
    }
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

/// Issue #21: indexed sheets are written at the smallest bit depth that holds the palette,
/// RGBA with the better of two filter choices; decoding gives back the same indices,
/// palette and tRNS, or the same RGBA.
#[test]
fn png_round_trips_at_every_bit_depth() {
    for (colours, depth) in [(2usize, 1u8), (4, 2), (5, 4), (16, 4), (17, 8), (256, 8)] {
        let (w, h) = (13u32, 7u32);
        let plte: Vec<[u8; 3]> = (0..colours).map(|i| [i as u8, 7, 255 - i as u8]).collect();
        let idx: Vec<u8> = (0..w * h)
            .map(|i| ((i * 7 + i / 5) as usize % colours) as u8)
            .collect();
        let png = encode_png_indexed(w, h, &idx, &plte, Some(&[255, 0])).unwrap();
        assert_eq!(png[24], depth, "{colours} colours");
        let img = image::decode_png(&png).unwrap();
        assert_eq!((img.width, img.height), (w, h));
        match img.pixels {
            image::Pixels::Indexed {
                data,
                plte: p,
                trns,
                ..
            } => {
                assert_eq!(&data[..], &idx[..], "{colours} colours");
                assert_eq!(p, plte);
                assert_eq!(trns.as_deref(), Some(&[255u8, 0][..]));
            }
            _ => panic!("indexed"),
        }
    }
    // Noise (no filter wins) and a gradient (a per-row filter wins).
    let (w, h) = (64u32, 9u32);
    let noise: Vec<u8> = (0..w * h * 4).map(|i| (i * 37 % 251) as u8).collect();
    let ramp: Vec<u8> = (0..w * h * 4)
        .map(|i| (i / 4 % w + i / (4 * w)) as u8)
        .collect();
    for rgba in [noise, ramp] {
        let img = image::decode_png(&encode_png_rgba(w, h, &rgba).unwrap()).unwrap();
        match img.pixels {
            image::Pixels::Rgba(d) => assert_eq!(&d[..], &rgba[..]),
            _ => panic!("rgba"),
        }
    }
    assert!(encode_png_indexed(0, 1, &[], &[[0; 3]], None).is_err());
    assert!(encode_png_rgba(2, 2, &[0; 15]).is_err());
}
