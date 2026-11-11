//! bentwire's own artwork for the Office balloon look-alike (B-OB-1): push buttons,
//! check boxes, label bullets and 32 x 32 icons. No Microsoft bitmaps.
//!
//! All functions take logical coordinates and the buffer scale `k`.

use bentwire_core::api::{ControlState, IconKind};

use crate::canvas::{Canvas, Rect, Rgb};
use crate::font::{FontSize, FontSpec, Fonts};

/// Fills a logical rectangle at scale k.
pub fn fill_rect(c: &mut Canvas, k: u32, x: i32, y: i32, w: i32, h: i32, col: Rgb) {
    let k = k as i32;
    c.fill_rect(x * k, y * k, w * k, h * k, col);
}

/// Antialiased shape over a logical box; `f` gets logical coordinates.
pub fn shape(c: &mut Canvas, k: u32, r: Rect, mut f: impl FnMut(f32, f32) -> Option<Rgb>) {
    let kf = k as f32;
    let ki = k as i32;
    c.fill_aa(r.x * ki, r.y * ki, r.w * ki, r.h * ki, |x, y| {
        f(x / kf, y / kf)
    });
}

/// A 1 px dotted rectangle outline (R-OB-10, DrawFocusRect-like), dots every other px.
pub fn dotted_rect(c: &mut Canvas, k: u32, r: Rect, col: Rgb) {
    let mut put = |x: i32, y: i32| {
        if (x + y) % 2 == 0 {
            fill_rect(c, k, x, y, 1, 1, col);
        }
    };
    for x in r.x..r.right() {
        put(x, r.y);
        put(x, r.bottom() - 1);
    }
    for y in r.y..r.bottom() {
        put(r.x, y);
        put(r.right() - 1, y);
    }
}

/// Outline with 2 px corner cuts (B-OB-1 button shape).
fn cut_outline(c: &mut Canvas, k: u32, r: Rect, col: Rgb) {
    fill_rect(c, k, r.x + 2, r.y, r.w - 4, 1, col);
    fill_rect(c, k, r.x + 2, r.bottom() - 1, r.w - 4, 1, col);
    fill_rect(c, k, r.x, r.y + 2, 1, r.h - 4, col);
    fill_rect(c, k, r.right() - 1, r.y + 2, 1, r.h - 4, col);
    for (x, y) in [
        (r.x + 1, r.y + 1),
        (r.right() - 2, r.y + 1),
        (r.x + 1, r.bottom() - 2),
        (r.right() - 2, r.bottom() - 2),
    ] {
        fill_rect(c, k, x, y, 1, 1, col);
    }
}

pub const GREY: Rgb = Rgb(0x80, 0x80, 0x80);

/// Push button frame (B-OB-1, colours from R-OB-11). Returns the caption colour
/// and the caption offset (pressed buttons shift by 1 px).
pub fn button_frame(
    c: &mut Canvas,
    k: u32,
    r: Rect,
    state: ControlState,
    focused: bool,
) -> (Rgb, i32) {
    let (outline, caption, shift) = match state {
        ControlState::Normal if focused => (Rgb::BLACK, Rgb::BLACK, 0),
        ControlState::Normal => (Rgb(0x86, 0x86, 0x86), Rgb::BLACK, 0),
        ControlState::Hot => (Rgb::BLACK, Rgb::BLACK, 0),
        ControlState::Pressed => (Rgb::BLACK, Rgb::BLACK, 1),
        ControlState::Disabled => (Rgb(0xC0, 0xC0, 0xC0), GREY, 0),
    };
    match state {
        ControlState::Hot => {
            // Raised: white top/left, light grey bottom/right inside the outline.
            fill_rect(c, k, r.x + 2, r.y + 1, r.w - 4, 1, Rgb::WHITE);
            fill_rect(c, k, r.x + 1, r.y + 2, 1, r.h - 4, Rgb::WHITE);
            let g = Rgb(0xC0, 0xC0, 0xC0);
            fill_rect(c, k, r.x + 2, r.bottom() - 2, r.w - 4, 1, g);
            fill_rect(c, k, r.right() - 2, r.y + 2, 1, r.h - 4, g);
        }
        ControlState::Pressed => {
            fill_rect(
                c,
                k,
                r.x + 1,
                r.y + 1,
                r.w - 2,
                r.h - 2,
                Rgb(0xE7, 0xE7, 0xD6),
            );
            fill_rect(c, k, r.x + 2, r.y + 1, r.w - 4, 1, Rgb(0x86, 0x86, 0x86));
            fill_rect(c, k, r.x + 1, r.y + 2, 1, r.h - 4, Rgb(0x86, 0x86, 0x86));
        }
        _ => {}
    }
    cut_outline(c, k, r, outline);
    if focused && state != ControlState::Disabled {
        dotted_rect(
            c,
            k,
            Rect::new(r.x + 3, r.y + 3, r.w - 6, r.h - 6),
            Rgb::BLACK,
        );
    }
    (caption, shift)
}

/// 13 x 13 check box glyph (R-OB-12), drawn in the XP look.
pub fn checkbox(c: &mut Canvas, k: u32, r: Rect, checked: bool, state: ControlState) {
    let (border, fill, mark) = match state {
        ControlState::Disabled => (Rgb(0xCA, 0xC8, 0xBB), Rgb::WHITE, Rgb(0xCA, 0xC8, 0xBB)),
        ControlState::Pressed => (
            Rgb(0x1C, 0x51, 0x80),
            Rgb(0xB0, 0xB0, 0xA7),
            Rgb(0x21, 0xA1, 0x21),
        ),
        _ => (
            Rgb(0x1C, 0x51, 0x80),
            Rgb(0xF4, 0xF4, 0xF0),
            Rgb(0x21, 0xA1, 0x21),
        ),
    };
    fill_rect(c, k, r.x, r.y, r.w, r.h, border);
    fill_rect(c, k, r.x + 1, r.y + 1, r.w - 2, r.h - 2, fill);
    if state == ControlState::Hot {
        let o = Rgb(0xF8, 0xB3, 0x30);
        fill_rect(c, k, r.x + 1, r.y + 1, r.w - 2, 1, o);
        fill_rect(c, k, r.x + 1, r.bottom() - 2, r.w - 2, 1, o);
        fill_rect(c, k, r.x + 1, r.y + 1, 1, r.h - 2, o);
        fill_rect(c, k, r.right() - 2, r.y + 1, 1, r.h - 2, o);
    }
    if checked {
        // A 7 x 7 tick, 3 px thick, centred.
        let (x0, y0) = (r.x + (r.w - 7) / 2, r.y + (r.h - 7) / 2);
        let col = [2, 3, 4, 3, 2, 1, 0];
        for (i, &top) in col.iter().enumerate() {
            fill_rect(c, k, x0 + i as i32, y0 + top, 1, 3, mark);
        }
    }
}

fn lerp(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    Rgb(m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

/// 15 x 15 round label bullet (R-OB-8): a cyan-to-navy glass ball, own artwork.
pub fn label_bullet(c: &mut Canvas, k: u32, x: i32, y: i32, state: ControlState) {
    let (light, dark) = match state {
        ControlState::Normal => (Rgb(0xB8, 0xF0, 0xFF), Rgb(0x00, 0x20, 0x90)),
        ControlState::Hot => (Rgb(0xE0, 0xFF, 0xFF), Rgb(0x10, 0x50, 0xD0)),
        ControlState::Pressed => (Rgb(0x60, 0xA0, 0xE0), Rgb(0x00, 0x10, 0x60)),
        ControlState::Disabled => (Rgb(0xF0, 0xF0, 0xF0), Rgb(0x80, 0x80, 0x80)),
    };
    let (cx, cy) = (x as f32 + 7.5, y as f32 + 7.5);
    shape(c, k, Rect::new(x, y, 15, 15), |px, py| {
        let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
        if d > 7.0 {
            return None;
        }
        if d > 6.2 {
            return Some(dark);
        }
        let hl = ((px - (cx - 2.0)).powi(2) + (py - (cy - 2.5)).powi(2)).sqrt();
        Some(lerp(light, dark, hl / 8.0))
    });
}

fn glyph(c: &mut Canvas, k: u32, fonts: &Fonts, box_: Rect, text: &str, em: u32, col: Rgb) {
    let spec = FontSpec::tahoma(FontSize::Em(em), true);
    let m = fonts.metrics(&spec);
    let w = fonts.text_width(&spec, text);
    let x = box_.x + (box_.w - w) / 2;
    let y = box_.y + (box_.h - m.ascent) / 2 - 1;
    fonts.draw_text(c, k, x, y, &spec, text, col, false);
}

/// 32 x 32 balloon icon (R-OB-5), own artwork.
pub fn icon(c: &mut Canvas, k: u32, fonts: &Fonts, kind: IconKind, x: i32, y: i32) {
    let r = Rect::new(x, y, 32, 32);
    let (cx, cy) = (x as f32 + 16.0, y as f32 + 16.0);
    let disc = |c: &mut Canvas, rim: Rgb, light: Rgb, dark: Rgb| {
        shape(c, k, r, |px, py| {
            let d = ((px - cx).powi(2) + (py - cy).powi(2)).sqrt();
            if d > 15.0 {
                None
            } else if d > 14.0 {
                Some(rim)
            } else {
                let hl = ((px - cx + 5.0).powi(2) + (py - cy + 6.0).powi(2)).sqrt();
                Some(lerp(light, dark, hl / 24.0))
            }
        });
    };
    match kind {
        IconKind::Info | IconKind::Query => {
            disc(
                c,
                Rgb(0x10, 0x30, 0x90),
                Rgb(0x70, 0xA8, 0xFF),
                Rgb(0x10, 0x40, 0xC0),
            );
            let t = if kind == IconKind::Info { "i" } else { "?" };
            glyph(c, k, fonts, r, t, 22, Rgb::WHITE);
        }
        IconKind::Critical => {
            disc(
                c,
                Rgb(0x80, 0x10, 0x10),
                Rgb(0xFF, 0x80, 0x70),
                Rgb(0xC8, 0x20, 0x18),
            );
            shape(c, k, r, |px, py| {
                let (u, v) = (px - cx, py - cy);
                let near = |a: f32, b: f32| (a - b).abs() / std::f32::consts::SQRT_2 < 2.0;
                (u.abs() < 7.0 && v.abs() < 7.0 && (near(u, v) || near(u, -v)))
                    .then_some(Rgb::WHITE)
            });
        }
        IconKind::Warning | IconKind::Alert => {
            // Rounded-off triangle: inside all three edges.
            let tri = |px: f32, py: f32, grow: f32| {
                let (u, v) = (px - cx, py - (y as f32 + 2.0));
                v >= -grow && v <= 28.0 + grow && u.abs() <= v * 0.55 + grow
            };
            shape(c, k, r, |px, py| {
                if tri(px, py, 0.0) {
                    Some(Rgb(0xFF, 0xD8, 0x20))
                } else if tri(px, py, 1.2) {
                    Some(Rgb(0x60, 0x40, 0x00))
                } else {
                    None
                }
            });
            glyph(
                c,
                k,
                fonts,
                Rect::new(x, y + 5, 32, 27),
                "!",
                20,
                Rgb::BLACK,
            );
        }
        IconKind::Tip => {
            // Light bulb: glass and a grey screw base.
            let (gx, gy) = (cx, y as f32 + 12.0);
            shape(c, k, r, |px, py| {
                let d = ((px - gx).powi(2) + (py - gy).powi(2)).sqrt();
                let neck = py >= gy && py < y as f32 + 23.0 && (px - gx).abs() < 5.5;
                if d <= 10.0 || neck {
                    let hl = ((px - gx + 3.0).powi(2) + (py - gy + 4.0).powi(2)).sqrt();
                    Some(lerp(
                        Rgb(0xFF, 0xFF, 0xD0),
                        Rgb(0xF0, 0xC8, 0x00),
                        hl / 14.0,
                    ))
                } else if d <= 11.0 {
                    Some(Rgb(0x80, 0x60, 0x00))
                } else {
                    None
                }
            });
            for (i, col) in [
                Rgb(0x90, 0x90, 0x90),
                Rgb(0x60, 0x60, 0x60),
                Rgb(0x90, 0x90, 0x90),
                Rgb(0x60, 0x60, 0x60),
            ]
            .into_iter()
            .enumerate()
            {
                fill_rect(c, k, x + 11, y + 23 + 2 * i as i32, 10, 2, col);
            }
            fill_rect(c, k, x + 13, y + 31, 6, 1, Rgb(0x40, 0x40, 0x40));
        }
    }
}
