//! Software rendering of characters, balloons and menus for bentwired.
//!
//! - [`font`]: bundled Tahoma (A-FONT-1), fontconfig fallback (B-BAL-2), metrics;
//!   [`font::Fonts`] implements the core's [`TextMetrics`].
//! - [`balloon`]: Agent balloon (R-BAL-4) and Office balloon (B-OB-1, B-OB-2).
//! - [`menu`]: XP-style character menu (B-WL-9).
//! - [`picture`]: picture to wl_shm buffer, input region, mouth clipping.
//! - [`scene`]: headless whole-output compositor and PNG output (T-4).
//! - [`theme`]: balloon themes from packs (A-THEME-1).

pub mod art;
pub mod balloon;
pub mod canvas;
pub mod font;
pub mod menu;
pub mod picture;
pub mod scene;
pub mod theme;

use bentwire_core::api::TextMetrics;

/// Maps the core's font request to a renderer font (em height, B-BAL-2).
pub fn core_font(f: &bentwire_core::api::FontSpec) -> font::FontSpec {
    font::FontSpec {
        family: f.family.clone(),
        size: font::FontSize::Em(f.px),
        bold: f.bold,
        italic: f.italic,
    }
}

impl TextMetrics for font::Fonts {
    fn line_height(&self, f: &bentwire_core::api::FontSpec) -> i32 {
        self.metrics(&core_font(f)).height
    }
    fn ave_char_width(&self, f: &bentwire_core::api::FontSpec) -> i32 {
        self.metrics(&core_font(f)).ave_char_width
    }
    fn text_width(&self, text: &str, f: &bentwire_core::api::FontSpec) -> i32 {
        font::Fonts::text_width(self, &core_font(f), text)
    }
}
