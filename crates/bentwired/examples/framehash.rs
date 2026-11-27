//! Frame hashes of characters (issue #21): proves that a re-encoded pack shows the same pixels.
//! Per character one line: the hash of every decoded sheet file (indices, PLTE, tRNS or RGBA),
//! and the hash of every picture the daemon can draw (each frame, each mouth variant as
//! `library::picture` builds it, each pose) at densities 1 to 4, plus the smoothed picture
//! (B-SMOOTH-1) at density 1 when the character has denser sheets.
//! usage: framehash FILE...

use bentwire_character::image::Pixels;
use bentwire_character::{Character, Layer, Rgba};
use bentwire_render::canvas::RgbaImage;
use bentwire_render::picture::{clip_mouth, downsample};
use sha2::{Digest, Sha256};

fn img(i: Rgba) -> RgbaImage {
    RgbaImage {
        width: i.width,
        height: i.height,
        data: i.data,
    }
}

fn feed(h: &mut Sha256, p: &RgbaImage) {
    h.update(p.width.to_le_bytes());
    h.update(p.height.to_le_bytes());
    h.update(&p.data);
}

/// Every picture of `c` at `density`, in manifest order (mirrors `library::picture`).
fn pictures(c: &Character, density: u32, out: &mut dyn FnMut(RgbaImage)) {
    for a in &c.manifest.animations {
        for f in &a.frames {
            out(img(c.compose_frame(f, None, density)));
            for (i, m) in f.mouths.iter().enumerate() {
                out(match m.base {
                    None => clip_mouth(
                        &img(c.compose_frame(f, None, density)),
                        &img(c.compose(std::slice::from_ref(&m.layer), density)),
                    ),
                    Some(_) => img(c.compose_frame(f, Some(i), density)),
                });
            }
        }
    }
    for l in c.manifest.poses.values() {
        out(img(c.compose(l as &[Layer], density)));
    }
}

fn main() {
    for path in std::env::args().skip(1) {
        let c = Character::load(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
        let mut sheets = Sha256::new();
        for s in &c.manifest.sheets {
            for key in std::iter::once(1).chain(s.hidpi.keys().copied()) {
                let i = c.sheets.file(&s.id, key).expect("sheet decoded");
                sheets.update(format!("{}:{key}:{}x{}", s.id, i.width, i.height));
                match &i.pixels {
                    Pixels::Indexed {
                        data, plte, trns, ..
                    } => {
                        sheets.update(b"P");
                        sheets.update(&data[..]);
                        sheets.update(plte.as_flattened());
                        sheets.update(trns.as_deref().unwrap_or(b"-"));
                    }
                    Pixels::Rgba(d) => {
                        sheets.update(b"R");
                        sheets.update(&d[..]);
                    }
                }
            }
        }
        let mut frames = Sha256::new();
        let mut n = 0u64;
        for d in 1..=4 {
            pictures(&c, d, &mut |p| {
                feed(&mut frames, &p);
                n += 1;
            });
        }
        let k = c
            .manifest
            .sheets
            .iter()
            .flat_map(|s| s.hidpi.keys())
            .copied()
            .max();
        if let Some(k) = k.filter(|&k| k > 1) {
            pictures(&c, k, &mut |p| {
                feed(&mut frames, &downsample(&p, k));
                n += 1;
            });
        }
        println!(
            "{} sheets={:x} pictures={n} frames={:x}",
            std::path::Path::new(&path)
                .file_name()
                .unwrap()
                .to_string_lossy(),
            sheets.finalize(),
            frames.finalize()
        );
    }
}
