//! Agent balloon (R-BAL-4) and Office balloon (B-OB-1, B-OB-2) drawing.
//!
//! Scenes come from the core in global logical coordinates. A balloon surface is a
//! canvas whose `origin` is the window's top-left times the buffer scale.

use bentwire_core::api::{
    AgentBalloonScene, ControlState, LabelPrefix, OfficeBalloonScene, TextLine,
};

use crate::art;
use crate::canvas::{Canvas, Rect, Region, Rgb};
use crate::font::{FontSpec, Fonts};
use crate::theme::Theme;

pub(crate) fn rect(r: bentwire_core::api::Rect) -> Rect {
    Rect::new(r.x, r.y, r.w, r.h)
}

pub(crate) fn rgb(c: [u8; 3]) -> Rgb {
    Rgb(c[0], c[1], c[2])
}

/// Office balloon colours (R-OB-1, R-OB-2).
pub const OFFICE_BG: Rgb = Rgb(0xFF, 0xFF, 0xCC);
pub const LABEL_TEXT: Rgb = Rgb(0x00, 0x00, 0x80);

/// Agent balloon outline in logical coordinates: the 16 x 20 rounded body plus the
/// tail triangle (R-BAL-4). Also the surface's input region (B-WL-8).
pub fn agent_balloon_region(s: &AgentBalloonScene) -> Region {
    let tail = s.tail.map(|p| (p.0 as f64, p.1 as f64));
    Region::round_rect(rect(s.body), 16, 20).union(&Region::polygon(&tail))
}

/// Office balloon outline (R-OB-23): body region from (B.left + 1, B.top + 1) to
/// (B.right, B.bottom) with a 16 x 16 corner ellipse, merged with the tail.
pub fn office_balloon_region(s: &OfficeBalloonScene) -> Region {
    let b = rect(s.body);
    let body = Region::round_rect(Rect::new(b.x + 1, b.y + 1, b.w - 1, b.h - 1), 16, 16);
    let tail = s.tail.map(|p| (p.0 as f64, p.1 as f64));
    body.union(&Region::polygon(&tail))
}

/// A canvas for a balloon window at buffer scale k.
pub fn window_canvas(window: bentwire_core::api::Rect, k: u32) -> Canvas {
    let mut c = Canvas::new(window.w as u32 * k, window.h as u32 * k);
    c.origin = (window.x * k as i32, window.y * k as i32);
    c
}

fn fill_outline(c: &mut Canvas, k: u32, shape: &Region, fill: Rgb, border: Rgb) {
    let ki = k as i32;
    c.fill_region(&shape.scale(ki), fill);
    c.fill_region(&shape.frame().scale(ki), border);
}

/// Draws the Agent balloon (R-BAL-4, R-BAL-9) onto `c`.
pub fn draw_agent_balloon(c: &mut Canvas, k: u32, fonts: &Fonts, s: &AgentBalloonScene) {
    fill_outline(c, k, &agent_balloon_region(s), rgb(s.bg), rgb(s.border));
    let font = crate::core_font(&s.font);
    let body = rect(s.body);
    let (tx, ty) = s.text_origin;
    let (ix, iy) = (tx - body.x, ty - body.y);
    let ki = k as i32;
    let saved = c.clip;
    c.clip = Some(Rect::new(
        tx * ki,
        ty * ki,
        (body.w - 2 * ix) * ki,
        (body.h - 2 * iy) * ki,
    ));
    for (i, line) in s.lines.iter().enumerate() {
        let y = ty + i as i32 * s.line_pitch + s.scroll_px;
        fonts.draw_text(c, k, tx, y, &font, line, rgb(s.fg), false);
    }
    c.clip = saved;
}

fn draw_lines(
    c: &mut Canvas,
    k: u32,
    fonts: &Fonts,
    font: &FontSpec,
    lines: &[TextLine],
    over: Option<Rgb>,
    underline_all: bool,
) {
    for l in lines {
        for (i, r) in l.runs.iter().enumerate() {
            let col = over.unwrap_or(rgb(r.color));
            let ul = r.underline || underline_all;
            let w = fonts.draw_text(c, k, r.x, l.y, font, &r.text, col, ul);
            // An underline continues across the gap to the next underlined run of the
            // same colour (runs are split at spaces).
            if let Some(n) = l.runs.get(i + 1)
                && ul
                && (n.underline || underline_all)
                && over.unwrap_or(rgb(n.color)) == col
                && n.x > r.x + w
            {
                fonts.draw_underline(c, k, r.x + w, n.x, l.y, font, col);
            }
        }
    }
}

/// Underlines the character at (line, run, char index) (CheckboxScene.mnemonic_at).
#[allow(clippy::too_many_arguments)]
fn underline_at(
    c: &mut Canvas,
    k: u32,
    fonts: &Fonts,
    font: &FontSpec,
    lines: &[TextLine],
    (li, ri, ci): (usize, usize, usize),
    col: Rgb,
) {
    let Some(l) = lines.get(li) else { return };
    let Some(r) = l.runs.get(ri) else { return };
    let before: String = r.text.chars().take(ci).collect();
    let Some(ch) = r.text.chars().nth(ci) else {
        return;
    };
    let x0 = r.x + fonts.text_width(font, &before);
    let x1 = x0 + fonts.text_width(font, &ch.to_string());
    fonts.draw_underline(c, k, x0, x1, l.y, font, col);
}

/// Draws the Office balloon (B-OB-1, B-OB-2) onto `c`. With a theme, buttons,
/// bullets and icons come from it where it has them (A-THEME-1).
pub fn draw_office_balloon(
    c: &mut Canvas,
    k: u32,
    fonts: &Fonts,
    theme: Option<&Theme>,
    s: &OfficeBalloonScene,
) {
    fill_outline(c, k, &office_balloon_region(s), OFFICE_BG, Rgb::BLACK);
    let body_font = crate::core_font(&s.body_font);
    let heading_font = crate::core_font(&s.heading_font);

    if let Some((kind, r)) = s.icon
        && !theme.is_some_and(|t| t.draw_icon(c, k, kind, r.x, r.y))
    {
        art::icon(c, k, fonts, kind, r.x, r.y);
    }
    draw_lines(c, k, fonts, &heading_font, &s.heading, None, false);
    draw_lines(c, k, fonts, &body_font, &s.text, None, false);

    for l in &s.labels {
        let disabled = l.state == ControlState::Disabled;
        let over = disabled.then_some(art::GREY);
        let pr = rect(l.prefix_rect);
        match &l.prefix {
            LabelPrefix::Bullet => {
                let (x, y) = (pr.x + (pr.w - 15) / 2, pr.y + (pr.h - 15) / 2);
                if !theme.is_some_and(|t| t.draw_bullet(c, k, l.state, x, y)) {
                    art::label_bullet(c, k, x, y, l.state);
                }
            }
            LabelPrefix::Char(t) | LabelPrefix::Number(t) => {
                let col = over.unwrap_or(
                    l.lines
                        .first()
                        .and_then(|ln| ln.runs.first())
                        .map_or(Rgb::BLACK, |r| rgb(r.color)),
                );
                let y = l.lines.first().map_or(pr.y, |ln| ln.y);
                fonts.draw_text(c, k, pr.x, y, &body_font, t, col, false);
            }
        }
        // Hot labels use the underlined font (R-OB-4).
        let hot = l.clickable && l.state == ControlState::Hot;
        draw_lines(c, k, fonts, &body_font, &l.lines, over, hot);
        if l.focused {
            art::dotted_rect(c, k, rect(l.rect), art::GREY);
        }
    }

    for cb in &s.checkboxes {
        art::checkbox(c, k, rect(cb.box_rect), cb.checked, cb.state);
        let disabled = cb.state == ControlState::Disabled;
        let over = disabled.then_some(art::GREY);
        draw_lines(c, k, fonts, &body_font, &cb.lines, over, false);
        if let Some(at) = cb.mnemonic_at {
            let col = over.unwrap_or(Rgb::BLACK);
            underline_at(c, k, fonts, &body_font, &cb.lines, at, col);
        }
        if cb.focused {
            let r = rect(cb.rect);
            let b = rect(cb.box_rect);
            let x = b.right() + 1;
            art::dotted_rect(c, k, Rect::new(x, r.y, r.right() - x, r.h), art::GREY);
        }
    }

    let m = fonts.metrics(&body_font);
    for b in &s.buttons {
        let r = rect(b.rect);
        let themed = theme.and_then(|t| t.draw_button(c, k, r, b.state, b.focused));
        let (col, shift) = themed.unwrap_or_else(|| art::button_frame(c, k, r, b.state, b.focused));
        let w = fonts.text_width(&body_font, &b.caption);
        let x = r.x + (r.w - w) / 2 + shift;
        let y = r.y + (r.h - m.height) / 2 + shift;
        fonts.draw_text(c, k, x, y, &body_font, &b.caption, col, false);
        if let Some(i) = b.mnemonic_index {
            let before: String = b.caption.chars().take(i).collect();
            let letter: String = b.caption.chars().skip(i).take(1).collect();
            let x0 = x + fonts.text_width(&body_font, &before);
            let x1 = x0 + fonts.text_width(&body_font, &letter);
            fonts.draw_underline(c, k, x0, x1, y, &body_font, col);
        }
    }
}
