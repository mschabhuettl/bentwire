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
    fn b_pix_1_blend_partial_alpha_matches_5_3() {
        let mut d = [100, 100, 100, 255];
        blend_straight(&mut d, &[200, 0, 0, 128]);
        // out_c = round((200*128*255 + 100*255*127) / (128*255 + 255*127)) = 150
        assert_eq!(d, [150, 50, 50, 255]);
    }
}
