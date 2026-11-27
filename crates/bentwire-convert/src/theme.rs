//! Office balloon theme (A-THEME-1) from the Office balloon DLL's bitmap and icon resources, as the
//! layout of bentwire-render's theme loader (`balloon-themes/<id>/theme.toml`). Resource ids and
//! strip layouts are office-assistant R-OB-5, R-OB-8, R-OB-9, R-OB-11 and R-OB-36 (A-THEME-2). Read at pack generation time
//! only; nothing of it is embedded in code.

use std::path::Path;

use bentwire_character::write::encode_png_rgba;

use crate::Error;
use crate::build::dib_decode;

fn bad(m: impl Into<String>) -> Error {
    Error::Source(format!("balloon DLL: {}", m.into()))
}

fn u16_at(d: &[u8], o: usize) -> Result<u16, Error> {
    d.get(o..o + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| bad("truncated"))
}

fn u32_at(d: &[u8], o: usize) -> Result<u32, Error> {
    d.get(o..o + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| bad("truncated"))
}

/// Resources of a PE file: (type id, name id, data) for integer ids, first language only.
pub fn pe_resources(d: &[u8]) -> Result<Vec<(u32, u32, Vec<u8>)>, Error> {
    if d.get(..2) != Some(b"MZ") {
        return Err(bad("not a PE file"));
    }
    let pe = u32_at(d, 0x3C)? as usize;
    if d.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err(bad("no PE signature"));
    }
    let nsec = u16_at(d, pe + 6)? as usize;
    let optsz = u16_at(d, pe + 20)? as usize;
    let opt = pe + 24;
    let dd = match u16_at(d, opt)? {
        0x10B => opt + 96,
        0x20B => opt + 112,
        _ => return Err(bad("unknown optional header")),
    };
    let rsrc_rva = u32_at(d, dd + 2 * 8)?;
    let secs: Vec<(u32, u32, u32)> = (0..nsec)
        .map(|k| {
            let s = opt + optsz + 40 * k;
            Ok((
                u32_at(d, s + 12)?,
                u32_at(d, s + 8)?.max(u32_at(d, s + 16)?),
                u32_at(d, s + 20)?,
            ))
        })
        .collect::<Result<_, Error>>()?;
    let off = |rva: u32| -> Result<usize, Error> {
        secs.iter()
            .find(|(va, size, _)| rva >= *va && rva - va < *size)
            .map(|(va, _, raw)| *raw as usize + (rva - va) as usize)
            .ok_or_else(|| bad(format!("RVA {rva:#x} not in a section")))
    };
    let root = off(rsrc_rva)?;
    let entries = |dir: usize| -> Result<Vec<(u32, u32)>, Error> {
        let n = u16_at(d, dir + 12)? as usize + u16_at(d, dir + 14)? as usize;
        (0..n)
            .map(|i| Ok((u32_at(d, dir + 16 + 8 * i)?, u32_at(d, dir + 20 + 8 * i)?)))
            .collect()
    };
    let mut out = Vec::new();
    for (ty, tdir) in entries(root)? {
        if ty & 0x8000_0000 != 0 || tdir & 0x8000_0000 == 0 {
            continue;
        }
        for (name, ndir) in entries(root + (tdir & 0x7FFF_FFFF) as usize)? {
            if name & 0x8000_0000 != 0 || ndir & 0x8000_0000 == 0 {
                continue;
            }
            let langs = entries(root + (ndir & 0x7FFF_FFFF) as usize)?;
            let Some(&(_, leaf)) = langs.first() else {
                continue;
            };
            let e = root + leaf as usize;
            let (rva, size) = (u32_at(d, e)?, u32_at(d, e + 4)? as usize);
            let o = off(rva)?;
            let data = d
                .get(o..o + size)
                .ok_or_else(|| bad("truncated resource"))?;
            out.push((ty, name, data.to_vec()));
        }
    }
    Ok(out)
}

fn resource(res: &[(u32, u32, Vec<u8>)], ty: u32, id: u32) -> Result<&[u8], Error> {
    res.iter()
        .find(|r| r.0 == ty && r.1 == id)
        .map(|r| r.2.as_slice())
        .ok_or_else(|| bad(format!("resource type {ty} id {id} missing")))
}

const RT_BITMAP: u32 = 2;
const RT_ICON: u32 = 3;
const RT_GROUP_ICON: u32 = 14;

/// A strip of `n` equal frames: RGB per pixel.
struct Strip {
    w: u32,
    h: u32,
    rgb: Vec<[u8; 3]>,
}

fn strip(res: &[(u32, u32, Vec<u8>)], id: u32) -> Result<Strip, Error> {
    let (w, h, pal, px) = dib_decode(resource(res, RT_BITMAP, id)?)?;
    Ok(Strip {
        w,
        h,
        rgb: px.iter().map(|&i| pal[i as usize]).collect(),
    })
}

/// Frames `images` of a vertical strip of `total` equal frames (stacked top to bottom), with
/// alpha from mask frames `masks` (a black mask pixel is opaque, white transparent). Returns
/// an RGBA strip with the frames side by side, as render's loader expects.
fn fold(
    s: &Strip,
    total: u32,
    masks: &[u32],
    images: &[u32],
) -> Result<(u32, u32, Vec<u8>), Error> {
    if !s.h.is_multiple_of(total) {
        return Err(bad("strip height is not a multiple of its frame count"));
    }
    let (fw, fh) = (s.w, s.h / total);
    let n = images.len() as u32;
    let mut out = vec![0u8; fw as usize * n as usize * fh as usize * 4];
    for (k, (&m, &im)) in masks.iter().zip(images).enumerate() {
        for y in 0..fh {
            for x in 0..fw {
                let mk = s.rgb[((m * fh + y) * fw + x) as usize];
                let c = s.rgb[((im * fh + y) * fw + x) as usize];
                let o = ((y * fw * n + k as u32 * fw + x) * 4) as usize;
                if mk == [0, 0, 0] {
                    out[o..o + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
                }
            }
        }
    }
    Ok((fw * n, fh, out))
}

/// The 32x32 icon of group `id`, deepest colour depth, AND mask folded into alpha.
fn icon(res: &[(u32, u32, Vec<u8>)], id: u32) -> Result<(u32, u32, Vec<u8>), Error> {
    let g = resource(res, RT_GROUP_ICON, id)?;
    let n = u16_at(g, 4)? as usize;
    let mut best: Option<(u16, u32)> = None;
    for i in 0..n {
        let e = 6 + 14 * i;
        let (w, h) = match g.get(e..e + 2) {
            Some(b) => (b[0], b[1]),
            None => return Err(bad("truncated icon group")),
        };
        let bits = u16_at(g, e + 6)?;
        let icon_id = u16_at(g, e + 12)? as u32;
        if w == 32 && h == 32 && best.is_none_or(|(b, _)| bits > b) {
            best = Some((bits, icon_id));
        }
    }
    let (_, iid) = best.ok_or_else(|| bad(format!("icon group {id} has no 32x32 image")))?;
    let d = resource(res, RT_ICON, iid)?;
    // DIB with doubled height: colour bits then the 1 bpp AND mask.
    let w = u32_at(d, 4)? as usize;
    let h2 = u32_at(d, 8)? as usize;
    let bpp = u16_at(d, 14)? as usize;
    let h = h2 / 2;
    let mut colour = d.to_vec();
    colour[8..12].copy_from_slice(&(h as u32).to_le_bytes());
    let (_, _, pal, px) = dib_decode(&colour)?;
    let ncol = 1usize << bpp;
    let xor_size = (w * bpp).div_ceil(32) * 4 * h;
    let mask_off = 40 + 4 * ncol + xor_size;
    let stride = w.div_ceil(32) * 4;
    let mut out = vec![0u8; w * h * 4];
    for y in 0..h {
        let row = mask_off + (h - 1 - y) * stride;
        for x in 0..w {
            let bit = (d
                .get(row + x / 8)
                .ok_or_else(|| bad("truncated icon mask"))?
                >> (7 - x % 8))
                & 1;
            if bit == 0 {
                let c = pal[px[y * w + x] as usize];
                out[(y * w + x) * 4..(y * w + x) * 4 + 4].copy_from_slice(&[c[0], c[1], c[2], 255]);
            }
        }
    }
    Ok((w as u32, h as u32, out))
}

// R-OB-36: the strips are bottom-up DIBs indexed from the bottom, so strip index s of an
// n-frame strip is frame n - 1 - s counted from the top.

/// Button strip frames as RGBA, side by side in the order [`BUTTON_FRAMES`] names: state s
/// is image s and mask s + 5 (R-OB-11), frames 9 - s and 4 - s from the top.
fn button_frames(s: &Strip) -> Result<(u32, u32, Vec<u8>), Error> {
    fold(s, 10, &[4, 3, 2, 1, 0], &[9, 8, 7, 6, 5])
}

/// Bullet strip frames as RGBA, side by side in the order [`BULLET_FRAMES`] names: state s
/// is image s (frame 4 - s from the top), all with mask index 4, the top frame (R-OB-8).
fn bullet_frames(s: &Strip) -> Result<(u32, u32, Vec<u8>), Error> {
    fold(s, 5, &[0; 4], &[4, 3, 2, 1])
}

const BUTTON_FRAMES: &str = r#"["normal", "hot", "pressing", "pressed", "disabled"]"#;
const BULLET_FRAMES: &str = r#"["normal", "hot", "pressing", "pressed"]"#;

fn theme_toml(name: &str, source: &str, generator: &str) -> String {
    let frames = BUTTON_FRAMES;
    let bullet_frames = BULLET_FRAMES;
    format!(
        r#"# Office balloon theme, extracted from {source} at pack generation time
# ({generator}). Frame names follow office-assistant R-OB-8, R-OB-11, R-OB-36.
format = 1
name = "{name}"

[buttons.small]
left = "button-small-left.png"
middle = "button-small-mid.png"
right = "button-small-right.png"
frames = {frames}

[buttons.large]
left = "button-large-left.png"
middle = "button-large-mid.png"
right = "button-large-right.png"
frames = {frames}

[bullets]
label = "bullet.png"
label_frames = {bullet_frames}

[icons]
tip = "tip.png"
"#
    )
}

/// Write `balloon-themes/<id>/` files (theme.toml and PNGs) for render's theme loader.
/// `source` is the DLL's file name, recorded in the theme.toml comment; `generator` names the
/// script that ran the extraction.
pub fn extract_theme(
    dll: &[u8],
    dir: &Path,
    name: &str,
    source: &str,
    generator: &str,
) -> Result<(), Error> {
    let res = pe_resources(dll)?;
    std::fs::create_dir_all(dir)?;
    let png = |file: &str, (w, h, d): (u32, u32, Vec<u8>)| -> Result<(), Error> {
        std::fs::write(dir.join(file), encode_png_rgba(w, h, &d)?)?;
        Ok(())
    };
    // R-OB-11: button strips of 10 frames, 5 masks then 5 images (top to bottom).
    for (size, ids) in [("small", [105, 106, 107]), ("large", [108, 109, 110])] {
        for (part, id) in ["left", "mid", "right"].iter().zip(ids) {
            png(
                &format!("button-{size}-{part}.png"),
                button_frames(&strip(&res, id)?)?,
            )?;
        }
    }
    // R-OB-8: bullets, 5 frames of 15x15: mask, then 4 states (top to bottom).
    png("bullet.png", bullet_frames(&strip(&res, 101)?)?)?;
    png("bullet-tip.png", bullet_frames(&strip(&res, 102)?)?)?;
    // R-OB-9: arrows, 5 frames of 17x9 in the bullet layout and order.
    png("arrow-up.png", bullet_frames(&strip(&res, 103)?)?)?;
    png("arrow-down.png", bullet_frames(&strip(&res, 104)?)?)?;
    // R-OB-5: tip icon, group 100.
    png("tip.png", icon(&res, 100)?)?;
    let toml = theme_toml(name, source, generator);
    std::fs::write(dir.join("theme.toml"), toml)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A vertical strip of `n` frames of `fw` x `fh`, listed top to bottom. Image frame `f` has
    /// the colour [f, 100, 200]; mask frame `f` (in `masks`) is black (opaque) except column
    /// `f`, which is white (transparent).
    fn synthetic(fw: u32, fh: u32, n: u32, masks: &[u32]) -> Strip {
        let mut rgb = Vec::new();
        for f in 0..n {
            for _ in 0..fh {
                for x in 0..fw {
                    rgb.push(if !masks.contains(&f) {
                        [f as u8, 100, 200]
                    } else if x == f {
                        [255, 255, 255]
                    } else {
                        [0, 0, 0]
                    });
                }
            }
        }
        Strip {
            w: fw,
            h: fh * n,
            rgb,
        }
    }

    /// Output frame `k` of `fw` x `fh`: (colour of an opaque pixel, transparent columns).
    fn frame(out: &(u32, u32, Vec<u8>), fw: u32, k: u32) -> ([u8; 3], Vec<u32>) {
        let (_, _, d) = out;
        let px = |x: u32| {
            let o = ((k * fw + x) * 4) as usize;
            [d[o], d[o + 1], d[o + 2], d[o + 3]]
        };
        let clear: Vec<u32> = (0..fw).filter(|&x| px(x)[3] == 0).collect();
        let opaque = (0..fw).find(|&x| px(x)[3] == 255).unwrap();
        let c = px(opaque);
        (c[..3].try_into().unwrap(), clear)
    }

    // R-OB-11, R-OB-36: strip index s is frame 9 - s from the top; image s, mask s + 5.
    #[test]
    fn r_ob_11_button_strip_state_order() {
        let out = button_frames(&synthetic(11, 22, 10, &[0, 1, 2, 3, 4])).unwrap();
        assert_eq!((out.0, out.1), (55, 22));
        // normal, hot, pressing, pressed, disabled: (image frame, mask frame) from the top.
        for (k, (image, mask)) in [(9, 4), (8, 3), (7, 2), (6, 1), (5, 0)].iter().enumerate() {
            let (c, clear) = frame(&out, 11, k as u32);
            assert_eq!(clear, vec![*mask], "state {k}: mask");
            assert_eq!(c, [*image as u8, 100, 200], "state {k}: image");
        }
        assert_eq!(
            BUTTON_FRAMES,
            r#"["normal", "hot", "pressing", "pressed", "disabled"]"#
        );
        let t: toml::Value = toml::from_str(&theme_toml("Office", "x.dll", "test")).unwrap();
        for size in ["small", "large"] {
            assert_eq!(
                t["buttons"][size]["frames"].as_array().unwrap().len(),
                5,
                "[buttons.{size}]"
            );
        }
    }

    // R-OB-8, R-OB-9, R-OB-36: mask index 4 (frame 0 from the top); image s is frame 4 - s.
    #[test]
    fn r_ob_8_r_ob_9_bullet_strip_state_order() {
        let bullet = bullet_frames(&synthetic(15, 15, 5, &[0])).unwrap();
        assert_eq!((bullet.0, bullet.1), (60, 15));
        // Arrows 103/104: 17 x 45, 5 frames of 17 x 9, bullet order.
        let arrow = bullet_frames(&synthetic(17, 9, 5, &[0])).unwrap();
        assert_eq!((arrow.0, arrow.1), (68, 9));
        for (out, fw) in [(&bullet, 15), (&arrow, 17)] {
            // normal, hot, pressing, pressed.
            for (k, image) in [4u8, 3, 2, 1].iter().enumerate() {
                let (c, clear) = frame(out, fw, k as u32);
                assert_eq!(clear, vec![0], "frame {k}: mask is frame 0");
                assert_eq!(c, [*image, 100, 200], "frame {k}: image");
            }
        }
        assert_eq!(BULLET_FRAMES, r#"["normal", "hot", "pressing", "pressed"]"#);
        let t: toml::Value = toml::from_str(&theme_toml("Office", "x.dll", "test")).unwrap();
        assert_eq!(t["bullets"]["label_frames"].as_array().unwrap().len(), 4);
    }
}
