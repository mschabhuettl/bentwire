//! A-CONV-7: the palette of a bitmap actor whose own palette is in no source file, read from a
//! sibling actor that shows the same pictures in a known palette. Generic: which actor is
//! whose sibling is a per-id option of the pack.

use std::collections::BTreeMap;

use agent_formats::act::{ActFile, Bitmap, Payload};

/// The transparent palette index of ACT bitmaps.
const KEY: u8 = 10;

/// A derivation needs at least this many pictures shown by both actors.
pub const MIN_PAIRS: usize = 16;

/// The bitmaps of an actor in payload order.
pub fn bitmaps(a: &ActFile) -> Vec<&Bitmap> {
    a.payloads
        .iter()
        .filter_map(|p| match p {
            Payload::Bitmap(b) => Some(b),
            _ => None,
        })
        .collect()
}

/// Bitmaps by (width, height, transparency mask), for masks with both transparent and
/// opaque pixels; `None` where one mask comes with two different pixel contents.
fn by_mask<'a>(list: &[&'a Bitmap]) -> BTreeMap<(u32, u32, Vec<bool>), Option<&'a Bitmap>> {
    let mut m: BTreeMap<(u32, u32, Vec<bool>), Option<&'a Bitmap>> = BTreeMap::new();
    for &b in list {
        let mask: Vec<bool> = b.pixels.iter().map(|&p| p == KEY).collect();
        if !mask.contains(&true) || !mask.contains(&false) {
            continue;
        }
        m.entry((b.width, b.height, mask))
            .and_modify(|e| {
                if e.is_some_and(|o| o.pixels != b.pixels) {
                    *e = None;
                }
            })
            .or_insert(Some(b));
    }
    m
}

/// Pairs of bitmaps of `a` and `b` that show the same picture: same size and the same
/// transparency mask (with transparent and opaque pixels), a mask that comes with one pixel
/// content only on each side. In `a`'s mask order.
pub fn same_pictures<'a>(a: &[&'a Bitmap], b: &[&'a Bitmap]) -> Vec<(&'a Bitmap, &'a Bitmap)> {
    let mb = by_mask(b);
    by_mask(a)
        .into_iter()
        .filter_map(|(k, x)| Some((x?, (*mb.get(&k)?)?)))
        .collect()
}

/// The palette for `targets` (one bitmap list per actor; all share one palette) from the
/// `sibling` bitmaps drawn in `sibling_palette`: each index takes the sibling colour it
/// covers most often in the same pictures (ties: the lower sibling index). Indices never
/// seen in a shared picture, and the transparent index, keep the sibling palette's entry.
/// Returns the palette and the number of shared pictures; fewer than [`MIN_PAIRS`] is an
/// error.
pub fn derive_palette(
    sibling: &[&Bitmap],
    sibling_palette: &[[u8; 3]; 256],
    targets: &[Vec<&Bitmap>],
) -> Result<([[u8; 3]; 256], usize), String> {
    let mut counts = vec![[0u64; 256]; 256];
    let mut pairs = 0;
    for t in targets {
        for (a, b) in same_pictures(t, sibling) {
            pairs += 1;
            for (&i, &j) in a.pixels.iter().zip(&b.pixels) {
                counts[i as usize][j as usize] += 1;
            }
        }
    }
    if pairs < MIN_PAIRS {
        return Err(format!(
            "{pairs} pictures shared with the sibling actor, at least {MIN_PAIRS} needed"
        ));
    }
    let mut pal = *sibling_palette;
    for (i, row) in counts.iter().enumerate() {
        if i == KEY as usize || row.iter().all(|&c| c == 0) {
            continue;
        }
        // max_by_key returns the last maximum; scan reversed so ties go to the lower index.
        let (j, _) = row
            .iter()
            .enumerate()
            .rev()
            .max_by_key(|(_, c)| **c)
            .unwrap();
        pal[i] = sibling_palette[j];
    }
    Ok((pal, pairs))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bm(w: u32, px: &[u8]) -> Bitmap {
        Bitmap {
            width: w,
            height: px.len() as u32 / w,
            flags: 0,
            pixels: px.to_vec(),
        }
    }

    #[test]
    fn a_conv_7_palette_from_sibling() {
        let mut sib_pal = [[0u8; 3]; 256];
        for (i, c) in sib_pal.iter_mut().enumerate() {
            *c = [i as u8, 0, 0];
        }
        // The same pictures with indices i -> i + 100 on the sibling side; picture k differs
        // from the others by its size, so every mask is unique.
        let sib: Vec<Bitmap> = (0..20u32)
            .map(|k| {
                bm(
                    k + 3,
                    &[&[10u8, 101, 102][..], &vec![103; k as usize]].concat(),
                )
            })
            .collect();
        let tgt: Vec<Bitmap> = (0..20u32)
            .map(|k| bm(k + 3, &[&[10u8, 1, 2][..], &vec![3; k as usize]].concat()))
            .collect();
        let s: Vec<&Bitmap> = sib.iter().collect();
        let t: Vec<&Bitmap> = tgt.iter().collect();
        assert_eq!(same_pictures(&t, &s).len(), 20);
        let (pal, n) = derive_palette(&s, &sib_pal, std::slice::from_ref(&t)).unwrap();
        assert_eq!(n, 20);
        assert_eq!(
            (pal[1], pal[2], pal[3]),
            ([101, 0, 0], [102, 0, 0], [103, 0, 0])
        );
        // Unseen indices and the transparent index keep the sibling entry.
        assert_eq!((pal[4], pal[10]), ([4, 0, 0], [10, 0, 0]));
        // Too few shared pictures.
        assert!(derive_palette(&s[..5], &sib_pal, std::slice::from_ref(&t)).is_err());
        // A fully opaque picture is no evidence; two contents for one mask drop that mask.
        let opaque = bm(2, &[1, 2]);
        let other = bm(2, &[103, 103]);
        assert!(same_pictures(&[&opaque], &[&other]).is_empty());
        let dup = bm(3, &[10, 9, 9]);
        assert_eq!(same_pictures(&[t[0], &dup], &[s[0]]).len(), 0);
    }
}
