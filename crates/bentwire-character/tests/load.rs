//! Loader, validation and compositor on hand-made characters.

use std::path::Path;

use bentwire_character::*;

fn png_indexed(w: u32, h: u32, idx: &[u8], plte: &[[u8; 3]], trns: Option<&[u8]>) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Indexed);
        e.set_depth(png::BitDepth::Eight);
        e.set_palette(plte.iter().flatten().copied().collect::<Vec<_>>());
        if let Some(t) = trns {
            e.set_trns(t.to_vec());
        }
        let mut wr = e.write_header().unwrap();
        wr.write_image_data(idx).unwrap();
    }
    out
}

fn png_rgba(w: u32, h: u32, px: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, w, h);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut wr = e.write_header().unwrap();
        wr.write_image_data(px).unwrap();
    }
    out
}

fn wav16(samples: &[i16]) -> Vec<u8> {
    let data: Vec<u8> = samples.iter().flat_map(|s| s.to_le_bytes()).collect();
    let mut v = b"RIFF".to_vec();
    v.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    v.extend_from_slice(b"WAVEfmt ");
    v.extend_from_slice(&16u32.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&1u16.to_le_bytes());
    v.extend_from_slice(&8000u32.to_le_bytes());
    v.extend_from_slice(&16000u32.to_le_bytes());
    v.extend_from_slice(&2u16.to_le_bytes());
    v.extend_from_slice(&16u16.to_le_bytes());
    v.extend_from_slice(b"data");
    v.extend_from_slice(&(data.len() as u32).to_le_bytes());
    v.extend_from_slice(&data);
    v
}

const MANIFEST: &str = r##"
format = 1
family = "agent"
x-tool = "test"

[character]
canvas = [4, 4]

[[character.info]]
lang = "en-US"
name = "Pebble"
description = "Round"

[[character.info]]
lang = "de-DE"
name = "Kiesel"

[[sheets]]
id = "a"
file = "sheets/a.png"
cell = [2, 2]
count = 3
sizes = [[2, 2], [1, 1], [2, 2]]
[sheets.hidpi]
2 = "sheets/a@2x.png"

[sounds]
pop = "sounds/pop.wav"
bad = "sounds/bad.wav"

[states]
Showing = ["Show"]

[[animations]]
name = "Show"
transition = "none"
frames = [
  { ms = 100, layers = ["a:0", ["a:1", 2, 2]], sound = "pop" },
  { ms = 100, layers = [["a:0", -1, 0]], exit = "end", mouths = [{ type = 2, layer = ["a:2", 1, 1], anchor = [2, 2] }] },
]

[[animations]]
name = "Broken"
frames = [{ ms = 10, sound = "bad" }]
"##;

/// Writes the test character into `dir`.
fn write_char(dir: &Path, manifest: &str) {
    std::fs::create_dir_all(dir.join("sheets")).unwrap();
    std::fs::create_dir_all(dir.join("sounds")).unwrap();
    std::fs::write(dir.join("character.toml"), manifest).unwrap();
    // 1x sheet: 3 columns of 2x2 cells = 6x2. Index 0 transparent, 1 red, 2 green, 3 blue.
    let plte = [[255, 0, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255]];
    #[rustfmt::skip]
    let idx = [
        1, 1, 2, 0, 3, 3,
        1, 0, 0, 0, 3, 3,
    ];
    std::fs::write(
        dir.join("sheets/a.png"),
        png_indexed(6, 2, &idx, &plte, Some(&[0])),
    )
    .unwrap();
    // 2x sheet: 12x4, all opaque white.
    std::fs::write(
        dir.join("sheets/a@2x.png"),
        png_rgba(12, 4, &[255; 12 * 4 * 4]),
    )
    .unwrap();
    std::fs::write(dir.join("sounds/pop.wav"), wav16(&[1, -1, 3])).unwrap();
    std::fs::write(dir.join("sounds/bad.wav"), b"RIFF0000WAVEjunk").unwrap();
}

fn tmp(name: &str) -> std::path::PathBuf {
    let d = std::env::temp_dir().join(format!("bwc-test-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    d
}

const T: [u8; 4] = [0, 0, 0, 0];
const RED: [u8; 4] = [255, 0, 0, 255];
const GREEN: [u8; 4] = [0, 255, 0, 255];
const BLUE: [u8; 4] = [0, 0, 255, 255];

fn px(r: &Rgba) -> Vec<[u8; 4]> {
    r.data.chunks(4).map(|c| [c[0], c[1], c[2], c[3]]).collect()
}

#[test]
fn load_folder_compose_sizes_offsets_mouths() {
    let d = tmp("folder");
    write_char(&d, MANIFEST);
    let c = Character::load(&d).unwrap();
    assert_eq!(c.family(), Family::Agent);
    let show = c.manifest.animation("show").unwrap(); // lookups ignore ASCII case
    // Frame 0: a:0 at 0,0 (red L, transparent hole), then a:1 (1x1 green per `sizes`) at 2,2.
    let f0 = c.compose_frame(&show.frames[0], None, 1);
    assert_eq!((f0.width, f0.height), (4, 4));
    #[rustfmt::skip]
    assert_eq!(px(&f0), vec![
        RED, RED, T, T,
        RED, T,   T, T,
        T,   T,   GREEN, T,
        T,   T,   T, T,
    ]);
    // Frame 1: a:0 at -1,0 (clipped), mouth a:2 (blue) at 1,1 on top.
    let f1 = c.compose_frame(&show.frames[1], None, 1);
    assert_eq!(px(&f1)[0], RED);
    assert_eq!(px(&f1).iter().filter(|p| p[3] != 0).count(), 1);
    let m = c.compose_frame(&show.frames[1], Some(0), 1);
    #[rustfmt::skip]
    assert_eq!(px(&m), vec![
        RED, T,    T,    T,
        T,   BLUE, BLUE, T,
        T,   BLUE, BLUE, T,
        T,   T,    T,    T,
    ]);
    // Density 2 picks the hidpi sheet (all white): a:0 covers 4x4 pixels at the origin.
    let hi = c.compose_frame(&show.frames[0], None, 2);
    assert_eq!((hi.width, hi.height), (8, 8));
    assert_eq!(px(&hi)[0], [255, 255, 255, 255]);
    // Sounds; the broken one only disables its animation (B-CHAR-2).
    assert_eq!(c.sound("pop").unwrap().frames(), 3);
    assert!(c.sound("bad").is_none());
    assert!(c.is_available("Show"));
    assert!(!c.is_available("Broken"));
    assert!(c.warnings.iter().any(|w| w.contains("sound bad")));
    // B-CHAR-3 display names.
    assert_eq!(c.display_name("de-AT").0, "Kiesel");
    assert_eq!(c.display_name("de-DE").1, ""); // step 2 takes the last German entry even if empty
    assert_eq!(c.display_name("fr").0, "Pebble");
}

#[test]
fn load_bwc_same_as_folder() {
    let d = tmp("zip");
    write_char(&d, MANIFEST);
    let names = [
        "character.toml",
        "sheets/a.png",
        "sheets/a@2x.png",
        "sounds/pop.wav",
        "sounds/bad.wav",
    ];
    let files: Vec<(String, Vec<u8>)> = names
        .iter()
        .map(|n| (n.to_string(), std::fs::read(d.join(n)).unwrap()))
        .collect();
    let entries: Vec<(&str, &[u8], bool)> = files
        .iter()
        .map(|(n, b)| (n.as_str(), b.as_slice(), n.ends_with(".toml")))
        .collect();
    let z = d.with_extension("bwc");
    std::fs::write(&z, zip::write(&entries).unwrap()).unwrap();
    let a = Character::load(&d).unwrap();
    let b = Character::load(&z).unwrap();
    let fa = &a.manifest.animations[0].frames[0];
    assert_eq!(a.compose_frame(fa, None, 1), b.compose_frame(fa, None, 1));
    assert_eq!(cheap_check_path(&z).unwrap(), Family::Agent);
}

#[test]
fn validation_rejects_bad_references_and_reports_unknown_keys() {
    let bad = MANIFEST.replace("exit = \"end\"", "exit = 7");
    let m = parse::parse_manifest(&bad).unwrap();
    assert!(
        validate(&m)
            .iter()
            .any(|e| e.contains("exit target out of range"))
    );
    let bad = MANIFEST.replace("[\"a:1\", 2, 2]", "[\"a:9\", 2, 2]");
    assert!(
        validate(&parse::parse_manifest(&bad).unwrap())
            .iter()
            .any(|e| e.contains("bad image reference"))
    );
    let bad = MANIFEST.replace("Showing = [\"Show\"]", "Showing = [\"Nope\"]");
    assert!(!validate(&parse::parse_manifest(&bad).unwrap()).is_empty());
    let bad = MANIFEST.replace("x-tool", "tool");
    let m = parse::parse_manifest(&bad).unwrap();
    assert_eq!(validate(&m), vec!["unknown key tool".to_string()]);
    // The loader only warns about it.
    let d = tmp("unknown");
    write_char(&d, &bad);
    assert!(
        Character::load(&d)
            .unwrap()
            .warnings
            .iter()
            .any(|w| w == "unknown key tool")
    );
    // Agent probabilities must sum to at most 100.
    let bad = MANIFEST.replace(
        "exit = \"end\",",
        "exit = \"end\", branch = [[0, 60], [1, 50]],",
    );
    assert!(
        validate(&parse::parse_manifest(&bad).unwrap())
            .iter()
            .any(|e| e.contains("sum"))
    );
    // Version and family gates (B-CHAR-1).
    assert!(matches!(
        cheap_check("format = 2\nfamily = \"agent\""),
        Err(Error::UnsupportedFormat(2))
    ));
    assert!(matches!(
        cheap_check("format = 1\nfamily = \"x\""),
        Err(Error::UnknownFamily(_))
    ));
    assert!(parse::parse_manifest(&MANIFEST.replace("sheets/a.png", "../a.png")).is_err());
}

#[test]
fn scaled_layers_use_pixel_centre_sampling() {
    // canvas 4x4 shown at display 8x8 (office97-like scaling): a 2x2 picture becomes 4x4.
    let m = MANIFEST
        .replace("canvas = [4, 4]", "canvas = [4, 4]\ndisplay = [8, 8]")
        .replace("[sheets.hidpi]\n2 = \"sheets/a@2x.png\"\n", "");
    let d = tmp("scaled");
    write_char(&d, &m);
    let c = Character::load(&d).unwrap();
    let f = c.compose(&[c.manifest.animations[0].frames[0].layers[0].clone()], 1);
    let p = px(&f);
    assert_eq!((f.width, f.height), (8, 8));
    // Each source pixel becomes a 2x2 block.
    assert_eq!(p[0], RED);
    assert_eq!(p[3], RED);
    assert_eq!(p[2 * 8 + 2], T);
    assert_eq!(p[3 * 8 + 3], T);
    assert_eq!(p[3 * 8 + 1], RED);
}
