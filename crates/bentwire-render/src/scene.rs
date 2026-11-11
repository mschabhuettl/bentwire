//! Headless scene compositor (T-4 `--dump-frames`): one image per output with the
//! character, balloons and menu, in that z-order. The background is transparent
//! (bentwire decision: the dump shows only what bentwire draws).

use std::path::Path;

use bentwire_core::api::{OutputInfo, Scene};

use crate::balloon::{draw_agent_balloon, draw_office_balloon};
use crate::canvas::{Canvas, RgbaImage};
use crate::font::Fonts;
use crate::menu::draw_menu;
use crate::theme::Theme;

/// Buffer scale for an output: ceil(output scale) (B-WL-6).
pub fn buffer_scale(scale: f64) -> u32 {
    (scale.ceil() as u32).max(1)
}

/// Composes the whole output. `picture` is the character's composed picture (straight
/// RGBA, at display size or display size times the buffer scale). The character is
/// drawn only when it is mapped and lives on this output (B-WL-2, B-WL-4); balloons
/// and menu belong to the character surface and follow it.
pub fn compose_output(
    scene: &Scene,
    output: &OutputInfo,
    picture: Option<&RgbaImage>,
    fonts: &Fonts,
    theme: Option<&Theme>,
) -> RgbaImage {
    let k = buffer_scale(output.scale);
    let ki = k as i32;
    let mut c = Canvas::new(output.rect.w as u32 * k, output.rect.h as u32 * k);
    c.origin = (output.rect.x * ki, output.rect.y * ki);
    let Some(ch) = scene
        .character
        .as_ref()
        .filter(|c| c.mapped && c.output == Some(output.id))
    else {
        return c.to_rgba();
    };
    if let Some(pic) = picture {
        c.draw_image_part(
            pic,
            0,
            0,
            pic.width,
            pic.height,
            ch.x * ki,
            ch.y * ki,
            ch.width * k,
            ch.height * k,
        );
    }
    if let Some(b) = &scene.agent_balloon {
        draw_agent_balloon(&mut c, k, fonts, b);
    }
    if let Some(b) = &scene.office_balloon {
        draw_office_balloon(&mut c, k, fonts, theme, b);
    }
    if let Some(m) = &scene.menu {
        draw_menu(&mut c, k, fonts, m);
    }
    c.to_rgba()
}

/// Encodes a straight RGBA image as PNG.
pub fn encode_png(img: &RgbaImage) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut enc = png::Encoder::new(&mut out, img.width, img.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        let mut w = enc.write_header().expect("png header");
        w.write_image_data(&img.data).expect("png data");
    }
    out
}

pub fn write_png(path: &Path, img: &RgbaImage) -> std::io::Result<()> {
    std::fs::write(path, encode_png(img))
}
