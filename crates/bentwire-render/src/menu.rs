//! XP-style character menu (B-MENU-1, B-WL-9). The renderer owns the item geometry;
//! the backend maps pointer positions to items with [`MenuGeometry::hit`].
//!
//! Metrics and colours (bentwire decision, XP "Luna" flat menu look): Tahoma em 11 px
//! (8 pt at 96 dpi), items 17 px high, separators 9 px, 1 px border #ACA899, white
//! background, text from x = 21 with a 21 px right margin, highlight #316AC5 with white
//! text inset 2 px from the border, mnemonics always underlined. A checked choice has a
//! 7 x 7 tick in the left column, a submenu item a 4 x 7 arrow in the right margin; the
//! submenu opens to the right of the menu with its first item level with its parent item.

use bentwire_core::api::{MenuEntry, MenuScene};

use crate::art::fill_rect;
use crate::canvas::{Canvas, Rect, Rgb};
use crate::font::{FontSize, FontSpec, Fonts};

pub const ITEM_H: i32 = 17;
pub const SEPARATOR_H: i32 = 9;
const PAD: i32 = 2;
const TEXT_X: i32 = 21;
const MIN_W: i32 = 100;
const BORDER: Rgb = Rgb(0xAC, 0xA8, 0x99);
const HIGHLIGHT: Rgb = Rgb(0x31, 0x6A, 0xC5);

pub fn menu_font() -> FontSpec {
    FontSpec::tahoma(FontSize::Em(11), false)
}

/// Item rectangles relative to the menu's top-left, and the menu size.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuGeometry {
    pub width: i32,
    pub height: i32,
    pub items: Vec<Rect>,
}

impl MenuGeometry {
    /// The selectable item (not a separator) at menu-local (x, y).
    pub fn hit(&self, entries: &[MenuEntry], x: i32, y: i32) -> Option<usize> {
        self.items
            .iter()
            .position(|r| r.contains(x, y))
            .filter(|&i| !matches!(entries.get(i), Some(MenuEntry::Separator) | None))
    }
}

/// The open submenu of `s` as a menu of its own, at its place right of the menu (B-WL-9).
pub fn submenu_scene(fonts: &Fonts, s: &MenuScene) -> Option<MenuScene> {
    let i = s.open?;
    let Some(MenuEntry::Submenu { entries, .. }) = s.entries.get(i) else {
        return None;
    };
    let g = menu_layout(fonts, &s.entries);
    Some(MenuScene {
        x: s.x + g.width,
        y: s.y + g.items[i].y - 1 - PAD,
        entries: entries.clone(),
        highlighted: s.open_highlighted,
        open: None,
        open_highlighted: None,
    })
}

pub fn menu_layout(fonts: &Fonts, entries: &[MenuEntry]) -> MenuGeometry {
    let font = menu_font();
    let text_w = entries
        .iter()
        .map(|e| match e {
            MenuEntry::Item { label, .. } | MenuEntry::Submenu { label, .. } => {
                fonts.text_width(&font, &crate::font::mnemonic(label).0)
            }
            MenuEntry::Separator => 0,
        })
        .max()
        .unwrap_or(0);
    let width = (TEXT_X + text_w + TEXT_X).max(MIN_W);
    let mut y = 1 + PAD;
    let mut items = Vec::new();
    for e in entries {
        let h = match e {
            MenuEntry::Separator => SEPARATOR_H,
            _ => ITEM_H,
        };
        items.push(Rect::new(1 + PAD, y, width - 2 - 2 * PAD, h));
        y += h;
    }
    MenuGeometry {
        width,
        height: y + PAD + 1,
        items,
    }
}

/// Draws the menu with its top-left at (scene.x, scene.y) in drawing coordinates.
pub fn draw_menu(c: &mut Canvas, k: u32, fonts: &Fonts, s: &MenuScene) -> MenuGeometry {
    let g = menu_layout(fonts, &s.entries);
    let (x0, y0) = (s.x, s.y);
    fill_rect(c, k, x0, y0, g.width, g.height, BORDER);
    fill_rect(c, k, x0 + 1, y0 + 1, g.width - 2, g.height - 2, Rgb::WHITE);
    let font = menu_font();
    let m = fonts.metrics(&font);
    for (i, (e, r)) in s.entries.iter().zip(&g.items).enumerate() {
        let r = Rect::new(r.x + x0, r.y + y0, r.w, r.h);
        match e {
            MenuEntry::Separator => {
                fill_rect(c, k, r.x + 1, r.y + r.h / 2, r.w - 2, 1, BORDER);
            }
            MenuEntry::Item { label, .. } | MenuEntry::Submenu { label, .. } => {
                let hl = s.highlighted == Some(i);
                if hl {
                    fill_rect(c, k, r.x, r.y, r.w, r.h, HIGHLIGHT);
                }
                let col = if hl { Rgb::WHITE } else { Rgb::BLACK };
                let ty = r.y + (r.h - m.height) / 2;
                fonts.draw_caption(c, k, x0 + TEXT_X, ty, &font, label, col, true);
                let cy = r.y + r.h / 2;
                match e {
                    MenuEntry::Item {
                        checked: Some(true),
                        ..
                    } => {
                        // The tick: column x has its lowest pixel at TICK[x], 3 px tall.
                        const TICK: [i32; 7] = [4, 5, 6, 5, 4, 3, 2];
                        for (dx, low) in TICK.into_iter().enumerate() {
                            let x = x0 + 7 + dx as i32;
                            fill_rect(c, k, x, cy - 4 + low - 2, 1, 3, col);
                        }
                    }
                    MenuEntry::Submenu { .. } => {
                        for dx in 0..4 {
                            let x = x0 + g.width - 13 + dx;
                            fill_rect(c, k, x, cy - 3 + dx, 1, 7 - 2 * dx, col);
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    g
}
