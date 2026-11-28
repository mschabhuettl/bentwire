//! Character picture to wl_shm buffer (B-WL-6), input shape (B-PIX-3) and mouth
//! clipping (B-PIX-2).

use crate::canvas::{Canvas, Rect, Region, RgbaImage};

impl From<&bentwire_character::Rgba> for RgbaImage {
    fn from(p: &bentwire_character::Rgba) -> Self {
        RgbaImage {
            width: p.width,
            height: p.height,
            data: p.data.clone(),
        }
    }
}

/// A wl_shm ARGB8888 buffer plus the input region in logical coordinates.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShmBuffer {
    pub width: u32,
    pub height: u32,
    pub stride: u32,
    /// Premultiplied, little-endian B, G, R, A.
    pub data: Vec<u8>,
    /// Input region (alpha > 0), logical px relative to the surface (B-WL-6).
    pub input_region: Vec<Rect>,
}

/// Converts a composed picture (straight RGBA) to a buffer at `scale` for a surface of
/// logical size `w x h`. A picture already at `w*scale x h*scale` (a `hidpi` sheet,
/// B-PIX-1) is used as is; any other size is mapped with nearest-neighbour pixel-centre
/// sampling (integer upscaling for a 1x picture).
pub fn picture_buffer(pic: &RgbaImage, w: u32, h: u32, scale: u32) -> ShmBuffer {
    let (bw, bh) = (w * scale, h * scale);
    let mut c = Canvas::new(bw, bh);
    c.draw_image_part(pic, 0, 0, pic.width, pic.height, 0, 0, bw, bh);
    ShmBuffer {
        width: bw,
        height: bh,
        stride: bw * 4,
        data: c.to_argb8888(),
        input_region: logical_shape(&c.to_rgba(), w, h).rects(),
    }
}

/// The shape of a picture at logical size `w x h`: a logical pixel belongs to it when
/// any picture pixel it covers has alpha > 0 (bentwire decision for hidpi pictures;
/// for a 1x picture this is exactly B-PIX-3).
pub fn logical_shape(pic: &RgbaImage, w: u32, h: u32) -> Region {
    if pic.width == w && pic.height == h {
        return Region::from_alpha(pic);
    }
    let mut small = RgbaImage::new(w, h);
    for y in 0..h {
        let (sy0, sy1) = (y * pic.height / h, ((y + 1) * pic.height).div_ceil(h));
        for x in 0..w {
            let (sx0, sx1) = (x * pic.width / w, ((x + 1) * pic.width).div_ceil(w));
            let any = (sy0..sy1.min(pic.height))
                .any(|sy| (sx0..sx1.min(pic.width)).any(|sx| pic.alpha(sx, sy) > 0));
            if any {
                small.data[((y * w + x) * 4 + 3) as usize] = 255;
            }
        }
    }
    Region::from_alpha(&small)
}

/// B-SMOOTH-1: `pic` reduced by `n` on both axes, each pixel the box average of an `n x n`
/// block: alpha = round(sum a / n^2), colour = round(sum c * a / sum a) (premultiplied
/// average, back to straight alpha), transparent black where the block is. Halves round up.
/// The sides of `pic` are multiples of `n`.
pub fn downsample(pic: &RgbaImage, n: u32) -> RgbaImage {
    let (w, h) = (pic.width / n, pic.height / n);
    let mut out = RgbaImage::new(w, h);
    let nn = u64::from(n * n);
    for y in 0..h {
        for x in 0..w {
            let (mut sa, mut sc) = (0u64, [0u64; 3]);
            for sy in y * n..(y + 1) * n {
                for sx in x * n..(x + 1) * n {
                    let o = ((sy * pic.width + sx) * 4) as usize;
                    let p = &pic.data[o..o + 4];
                    let a = u64::from(p[3]);
                    sa += a;
                    for k in 0..3 {
                        sc[k] += u64::from(p[k]) * a;
                    }
                }
            }
            if sa == 0 {
                continue;
            }
            let o = ((y * w + x) * 4) as usize;
            for (d, c) in out.data[o..o + 3].iter_mut().zip(sc) {
                *d = ((2 * c + sa) / (2 * sa)) as u8;
            }
            out.data[o + 3] = ((2 * sa + nn) / (2 * nn)) as u8;
        }
    }
    out
}

/// B-SMOOTH-1: `pic` reduced by `n` for smooth edges. An aliased picture (every pixel fully
/// opaque or fully transparent) first gets its thin lines fitted to the `n x n` grid
/// ([`fit_thin_lines`]), so a line one output pixel wide keeps full coverage; then
/// [`downsample`].
pub fn smooth(pic: &RgbaImage, n: u32) -> RgbaImage {
    if n > 1 && pic.data.chunks(4).all(|p| p[3] == 0 || p[3] == 255) {
        downsample(&fit_thin_lines(pic, n), n)
    } else {
        downsample(pic, n)
    }
}

/// B-SMOOTH-1 line fitting: along each column, then along each row of the result, every
/// thin run (B-SMOOTH-1 states the conditions) is moved onto the `n` grid.
pub fn fit_thin_lines(pic: &RgbaImage, n: u32) -> RgbaImage {
    fit_axis(&fit_axis(pic, n, true), n, false)
}

/// One pass of [`fit_thin_lines`] along columns (`vertical`) or rows. Runs are maximal
/// stretches of equal pixels in one line. A run is thin when it is not transparent, at most
/// `n` long, not exactly one block of the grid, bounded on both sides by runs at least `n`
/// long, and its colour touches it in both neighbouring lines (within one pixel of its
/// ends). It is moved into the block that holds most of it (on a tie, the lower one) and
/// fills that block; the pixels it leaves take the colour of the run beyond them.
fn fit_axis(pic: &RgbaImage, n: u32, vertical: bool) -> RgbaImage {
    let (w, h, n) = (pic.width as usize, pic.height as usize, n as usize);
    let (lines, len) = if vertical { (w, h) } else { (h, w) };
    let at = |l: usize, i: usize| {
        let o = if vertical { i * w + l } else { l * w + i } * 4;
        <[u8; 4]>::try_from(&pic.data[o..o + 4]).unwrap()
    };
    let mut out = pic.clone();
    for l in 0..lines {
        let mut runs = Vec::new();
        let mut s = 0;
        while s < len {
            let mut e = s;
            while e + 1 < len && at(l, e + 1) == at(l, s) {
                e += 1;
            }
            runs.push((s, e));
            s = e + 1;
        }
        for k in 1..runs.len().saturating_sub(1) {
            let ((s, e), c) = (runs[k], at(l, runs[k].0));
            let rl = e - s + 1;
            let long = |r: (usize, usize)| r.1 - r.0 + 1 >= n;
            if c[3] == 0
                || rl > n
                || (s % n == 0 && rl == n)
                || !long(runs[k - 1])
                || !long(runs[k + 1])
            {
                continue;
            }
            let goes_on =
                |m: usize| (s.saturating_sub(1)..=(e + 1).min(len - 1)).any(|i| at(m, i) == c);
            if l == 0 || l + 1 == lines || !goes_on(l - 1) || !goes_on(l + 1) {
                continue;
            }
            let b = s / n;
            let first = (b + 1) * n - s;
            let b = if e / n == b || first >= rl - first {
                b
            } else {
                b + 1
            };
            let (t0, t1) = (b * n, (b * n + n).min(len) - 1);
            let (before, after) = (at(l, s - 1), at(l, e + 1));
            let mut set = |i: usize, c: [u8; 4]| {
                let o = if vertical { i * w + l } else { l * w + i } * 4;
                out.data[o..o + 4].copy_from_slice(&c);
            };
            for i in s..t0 {
                set(i, before);
            }
            for i in t1 + 1..=e {
                set(i, after);
            }
            for i in t0..=t1 {
                set(i, c);
            }
        }
    }
    out
}

/// Straight-alpha source-over blend of character-format 5.3 step 4.
fn blend_straight(d: &mut [u8], s: &[u8]) {
    let a = s[3] as u32;
    let da = d[3] as u32;
    if a == 255 {
        d.copy_from_slice(s);
        return;
    }
    if a == 0 {
        return;
    }
    let den = a * 255 + da * (255 - a);
    for k in 0..3 {
        let num = s[k] as u32 * a * 255 + d[k] as u32 * da * (255 - a);
        d[k] = ((num + den / 2) / den) as u8;
    }
    d[3] = ((den + 127) / 255) as u8;
}

/// B-PIX-2: draws a mouth layer (already placed on a canvas-sized transparent image)
/// over the base picture, only where the base has alpha > 0. A mouth with its own
/// `base` list is not clipped; callers composite that case normally.
pub fn clip_mouth(base: &RgbaImage, mouth: &RgbaImage) -> RgbaImage {
    assert_eq!((base.width, base.height), (mouth.width, mouth.height));
    let mut out = base.clone();
    for (d, s) in out.data.chunks_mut(4).zip(mouth.data.chunks(4)) {
        if d[3] > 0 {
            blend_straight(d, s);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn square(w: u32, h: u32, x0: u32, y0: u32, n: u32, rgba: [u8; 4]) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for y in y0..y0 + n {
            for x in x0..x0 + n {
                let o = ((y * w + x) * 4) as usize;
                img.data[o..o + 4].copy_from_slice(&rgba);
            }
        }
        img
    }

    #[test]
    fn b_pix_2_mouth_clipped_to_base() {
        // Opaque 10x10 square, mouth overlapping it by 5 px: only overlap changes.
        let base = square(20, 20, 0, 0, 10, [255, 0, 0, 255]);
        let mouth = square(20, 20, 5, 0, 10, [0, 0, 255, 255]);
        let out = clip_mouth(&base, &mouth);
        let mut changed = 0;
        for i in 0..400 {
            let (b, o) = (&base.data[i * 4..i * 4 + 4], &out.data[i * 4..i * 4 + 4]);
            if b != o {
                changed += 1;
                let (x, y) = (i % 20, i / 20);
                assert!((5..10).contains(&x) && y < 10);
                assert_eq!(o, [0, 0, 255, 255]);
            }
        }
        assert_eq!(changed, 50);
    }

    #[test]
    fn b_wl_6_integer_upscale_and_byte_order() {
        let mut pic = RgbaImage::new(2, 1);
        pic.data = vec![255, 0, 0, 255, 0, 255, 0, 128];
        let b = picture_buffer(&pic, 2, 1, 2);
        assert_eq!((b.width, b.height, b.stride), (4, 2, 16));
        // Row 0: red, red, half green premultiplied (0,128,0,128) as B,G,R,A.
        assert_eq!(
            &b.data[..16],
            &[
                0, 0, 255, 255, 0, 0, 255, 255, 0, 128, 0, 128, 0, 128, 0, 128
            ]
        );
        assert_eq!(&b.data[..16], &b.data[16..]);
        assert_eq!(b.input_region, vec![Rect::new(0, 0, 2, 1)]);
    }

    #[test]
    fn b_ren_4_region_follows_displayed_pixels() {
        // A 2x1 picture squeezed into 1x1 samples only the transparent right pixel.
        let mut pic = RgbaImage::new(2, 1);
        pic.data[..4].copy_from_slice(&[1, 1, 1, 255]);
        let b = picture_buffer(&pic, 1, 1, 1);
        assert_eq!(b.data, vec![0, 0, 0, 0]);
        assert!(b.input_region.is_empty());
    }

    #[test]
    fn empty_picture_gives_transparent_buffer() {
        // character's compositor returns 0x0 above its size limit (A-CHAR-4).
        let b = picture_buffer(&RgbaImage::new(0, 0), 3, 2, 2);
        assert_eq!((b.width, b.height), (6, 4));
        assert!(b.data.iter().all(|&v| v == 0));
        assert!(b.input_region.is_empty());
    }

    #[test]
    fn b_pix_3_input_region_is_alpha_shape() {
        let pic = square(8, 8, 2, 3, 3, [1, 2, 3, 1]);
        let b = picture_buffer(&pic, 8, 8, 1);
        assert_eq!(b.input_region, vec![Rect::new(2, 3, 3, 3)]);
    }

    #[test]
    fn b_wl_6_hidpi_picture_used_as_is_region_logical() {
        // A 2x picture (4x4) for a 2x2 surface: one opaque buffer pixel marks its
        // logical pixel.
        let pic = square(4, 4, 3, 3, 1, [9, 9, 9, 255]);
        let b = picture_buffer(&pic, 2, 2, 2);
        assert_eq!((b.width, b.height), (4, 4));
        assert_eq!(
            &b.data[(3 * 4 + 3) * 4..(3 * 4 + 3) * 4 + 4],
            &[9, 9, 9, 255]
        );
        assert_eq!(b.input_region, vec![Rect::new(1, 1, 1, 1)]);
    }

    #[test]
    fn b_smooth_1_downsample_is_a_premultiplied_box_average() {
        // One 2x2 block: opaque red, opaque blue, half-transparent white, transparent.
        let mut pic = RgbaImage::new(4, 2);
        pic.data[..8].copy_from_slice(&[255, 0, 0, 255, 0, 0, 255, 255]);
        pic.data[16..24].copy_from_slice(&[255, 255, 255, 128, 9, 9, 9, 0]);
        // Right block: fully transparent (its colour bytes do not count).
        pic.data[8..12].copy_from_slice(&[7, 7, 7, 0]);
        let d = downsample(&pic, 2);
        assert_eq!((d.width, d.height), (2, 1));
        // sum a = 638: alpha round(638 / 4) = 160; red (255 * 255 + 255 * 128) / 638
        // = 153.08 -> 153; green 255 * 128 / 638 = 51.16 -> 51; blue as red.
        assert_eq!(&d.data[..4], &[153, 51, 153, 160]);
        assert_eq!(&d.data[4..], &[0, 0, 0, 0]);
        // An opaque single-colour block stays exact; n = 1 is the identity on it.
        let mut flat = RgbaImage::new(3, 3);
        for p in flat.data.chunks_mut(4) {
            p.copy_from_slice(&[10, 20, 30, 255]);
        }
        assert_eq!(downsample(&flat, 3).data, vec![10, 20, 30, 255]);
        assert_eq!(downsample(&flat, 1), flat);
    }

    const PAPER: [u8; 4] = [255, 255, 204, 255];
    const INK: [u8; 4] = [0, 0, 128, 255];

    /// A `w x h` picture of PAPER with INK on rows `rows` (all columns), or on those
    /// columns when `vertical`.
    fn ruled(w: u32, h: u32, rows: std::ops::Range<u32>, vertical: bool) -> RgbaImage {
        let mut img = RgbaImage::new(w, h);
        for y in 0..h {
            for x in 0..w {
                let on = rows.contains(if vertical { &x } else { &y });
                let o = ((y * w + x) * 4) as usize;
                img.data[o..o + 4].copy_from_slice(if on { &INK } else { &PAPER });
            }
        }
        img
    }

    /// The pixels of row `y` (or column, when `vertical`) of `img`, without the two at the
    /// ends (lines at the picture edge have no neighbour on one side and are not fitted).
    fn inner(img: &RgbaImage, y: u32, vertical: bool) -> Vec<[u8; 4]> {
        let n = if vertical { img.height } else { img.width };
        (1..n - 1)
            .map(|i| {
                let (x, y) = if vertical { (y, i) } else { (i, y) };
                let o = ((y * img.width + x) * 4) as usize;
                img.data[o..o + 4].try_into().unwrap()
            })
            .collect()
    }

    #[test]
    fn b_smooth_1_misaligned_2px_line_keeps_full_coverage() {
        for vertical in [false, true] {
            // 2 px at 2x on rows 3 and 4: across the 2 x 2 blocks of rows 1 and 2.
            let pic = ruled(12, 12, 3..5, vertical);
            // The box filter alone: two half-covered rows, lighter than the ink.
            let half = downsample(&pic, 2);
            assert!(
                inner(&half, 1, vertical)
                    .iter()
                    .all(|p| *p != INK && *p != PAPER)
            );
            // Fitted: the lower block (the tie) holds the whole line, the other is paper.
            let s = smooth(&pic, 2);
            assert_eq!((s.width, s.height), (6, 6));
            assert!(
                inner(&s, 1, vertical).iter().all(|p| *p == INK),
                "{vertical}"
            );
            assert!(
                inner(&s, 2, vertical).iter().all(|p| *p == PAPER),
                "{vertical}"
            );
            assert!(
                inner(&s, 0, vertical).iter().all(|p| *p == PAPER),
                "{vertical}"
            );
        }
    }

    #[test]
    fn b_smooth_1_one_pixel_line_at_2x_keeps_full_coverage() {
        for vertical in [false, true] {
            // A cosmetic one-pixel pen at 2x (row 3): half coverage with the box filter.
            let pic = ruled(12, 12, 3..4, vertical);
            assert!(
                inner(&downsample(&pic, 2), 1, vertical)
                    .iter()
                    .all(|p| *p != INK)
            );
            let s = smooth(&pic, 2);
            assert!(
                inner(&s, 1, vertical).iter().all(|p| *p == INK),
                "{vertical}"
            );
            assert!(
                inner(&s, 2, vertical).iter().all(|p| *p == PAPER),
                "{vertical}"
            );
        }
    }

    #[test]
    fn b_smooth_1_three_pixel_line_at_3x_goes_to_the_block_holding_most() {
        // Rows 4 to 6 at 3x: two rows in block 1 (rows 3 to 5), one in block 2.
        let s = smooth(&ruled(18, 18, 4..7, false), 3);
        assert!(inner(&s, 1, false).iter().all(|p| *p == INK));
        assert!(inner(&s, 2, false).iter().all(|p| *p == PAPER));
    }

    #[test]
    fn b_smooth_1_fitting_leaves_other_pictures_to_the_box_filter() {
        // Aligned lines and wide bands are already on the grid.
        for pic in [ruled(12, 12, 2..4, false), ruled(12, 12, 3..9, true)] {
            assert_eq!(smooth(&pic, 2), downsample(&pic, 2));
        }
        // An isolated misaligned dot is no line (its colour does not go on).
        let mut dot = ruled(12, 12, 0..0, false);
        for (x, y) in [(5, 5), (6, 5), (5, 6), (6, 6)] {
            let o = ((y * 12 + x) * 4) as usize;
            dot.data[o..o + 4].copy_from_slice(&INK);
        }
        assert_eq!(smooth(&dot, 2), downsample(&dot, 2));
        // A picture with partial alpha (anti-aliased already) is not fitted.
        let mut aa = ruled(12, 12, 3..5, false);
        aa.data[3] = 128;
        assert_eq!(smooth(&aa, 2), downsample(&aa, 2));
        // Factor 1 is the identity.
        let pic = ruled(6, 6, 3..4, false);
        assert_eq!(smooth(&pic, 1), pic);
    }

    #[test]
    fn b_pix_1_blend_partial_alpha_matches_5_3() {
        let mut d = [100, 100, 100, 255];
        blend_straight(&mut d, &[200, 0, 0, 128]);
        // out_c = round((200*128*255 + 100*255*127) / (128*255 + 255*127)) = 150
        assert_eq!(d, [150, 50, 50, 255]);
    }
}
