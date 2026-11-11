//! Office balloon themes from packs: `balloon-themes/<id>/theme.toml` (A-THEME-1).
//!
//! ```toml
//! format = 1
//! name = "..."
//! [buttons.small]          # height 22, output width < 1024 (R-OB-11)
//! left = "b-left.png"      # strips: equal-width frames side by side
//! middle = "b-mid.png"
//! right = "b-right.png"
//! frames = ["normal", "default", "hot", "pressed", "disabled"]
//! [buttons.large]          # height 24
//! [bullets]
//! label = "bullet.png"
//! label_frames = ["normal", "hot", "pressed", "disabled"]
//! [icons]                  # 32 x 32, each optional
//! tip = "tip.png"
//! ```
//!
//! Masks are already folded into alpha. Anything missing falls back to the
//! look-alike (B-OB-1).

use std::collections::HashMap;
use std::path::Path;

use bentwire_core::api::{ControlState, IconKind};
use serde::Deserialize;

use crate::art;
use crate::canvas::{Canvas, Rect, Rgb, RgbaImage};

#[derive(Deserialize)]
struct ThemeFile {
    format: u32,
    #[serde(default)]
    buttons: HashMap<String, ButtonFiles>,
    bullets: Option<BulletFiles>,
    #[serde(default)]
    icons: HashMap<String, String>,
}

#[derive(Deserialize)]
struct ButtonFiles {
    left: String,
    middle: String,
    right: String,
    frames: Vec<String>,
}

#[derive(Deserialize)]
struct BulletFiles {
    label: Option<String>,
    #[serde(default)]
    label_frames: Vec<String>,
}

struct Strip {
    img: RgbaImage,
    frames: Vec<String>,
}

impl Strip {
    fn frame_width(&self) -> u32 {
        self.img.width / self.frames.len().max(1) as u32
    }
    fn index(&self, name: &str) -> Option<u32> {
        self.frames.iter().position(|f| f == name).map(|i| i as u32)
    }
}

struct ButtonArt {
    left: Strip,
    middle: Strip,
    right: Strip,
}

/// A loaded balloon theme.
pub struct Theme {
    buttons: Vec<ButtonArt>,
    bullet: Option<Strip>,
    icons: HashMap<String, RgbaImage>,
}

#[derive(Debug)]
pub struct ThemeError(pub String);

impl std::fmt::Display for ThemeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for ThemeError {}

/// Decodes a PNG to straight RGBA8.
pub fn load_png(path: &Path) -> Result<RgbaImage, ThemeError> {
    let err = |e: &dyn std::fmt::Display| ThemeError(format!("{}: {e}", path.display()));
    let file = std::fs::File::open(path).map_err(|e| err(&e))?;
    let mut dec = png::Decoder::new(std::io::BufReader::new(file));
    dec.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = dec.read_info().map_err(|e| err(&e))?;
    let mut buf = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut buf).map_err(|e| err(&e))?;
    let px = (info.width * info.height) as usize;
    let data = match info.color_type {
        png::ColorType::Rgba => buf[..px * 4].to_vec(),
        png::ColorType::Rgb => buf[..px * 3]
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::GrayscaleAlpha => buf[..px * 2]
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Grayscale => buf[..px].iter().flat_map(|&g| [g, g, g, 255]).collect(),
        png::ColorType::Indexed => return Err(err(&"indexed PNG not expanded")),
    };
    Ok(RgbaImage {
        width: info.width,
        height: info.height,
        data,
    })
}

fn icon_key(kind: IconKind) -> &'static str {
    match kind {
        IconKind::Alert => "alert",
        IconKind::Tip => "tip",
        IconKind::Info => "info",
        IconKind::Warning => "warning",
        IconKind::Query => "query",
        IconKind::Critical => "critical",
    }
}

impl Theme {
    /// Loads `dir/theme.toml` and the PNGs it names.
    pub fn load(dir: &Path) -> Result<Theme, ThemeError> {
        let path = dir.join("theme.toml");
        let text = std::fs::read_to_string(&path)
            .map_err(|e| ThemeError(format!("{}: {e}", path.display())))?;
        let f: ThemeFile =
            toml::from_str(&text).map_err(|e| ThemeError(format!("{}: {e}", path.display())))?;
        if f.format != 1 {
            return Err(ThemeError(format!(
                "{}: unsupported format {}",
                path.display(),
                f.format
            )));
        }
        let strip = |file: &str, frames: &[String]| -> Result<Strip, ThemeError> {
            Ok(Strip {
                img: load_png(&dir.join(file))?,
                frames: frames.to_vec(),
            })
        };
        let mut buttons = Vec::new();
        for b in f.buttons.values() {
            buttons.push(ButtonArt {
                left: strip(&b.left, &b.frames)?,
                middle: strip(&b.middle, &b.frames)?,
                right: strip(&b.right, &b.frames)?,
            });
        }
        let bullet = match f.bullets {
            Some(BulletFiles {
                label: Some(l),
                label_frames,
            }) => Some(strip(&l, &label_frames)?),
            _ => None,
        };
        let mut icons = HashMap::new();
        for (k, file) in &f.icons {
            icons.insert(k.clone(), load_png(&dir.join(file))?);
        }
        Ok(Theme {
            buttons,
            bullet,
            icons,
        })
    }

    pub(crate) fn draw_icon(&self, c: &mut Canvas, k: u32, kind: IconKind, x: i32, y: i32) -> bool {
        let Some(img) = self.icons.get(icon_key(kind)) else {
            return false;
        };
        let ki = k as i32;
        c.draw_image_part(
            img,
            0,
            0,
            img.width,
            img.height,
            x * ki,
            y * ki,
            32 * k,
            32 * k,
        );
        true
    }

    pub(crate) fn draw_bullet(
        &self,
        c: &mut Canvas,
        k: u32,
        state: ControlState,
        x: i32,
        y: i32,
    ) -> bool {
        let Some(s) = &self.bullet else { return false };
        let Some(i) = s.index(state_name(state)).or(s.index("normal")) else {
            return false;
        };
        let fw = s.frame_width();
        let ki = k as i32;
        c.draw_image_part(
            &s.img,
            i * fw,
            0,
            fw,
            s.img.height,
            x * ki,
            y * ki,
            fw * k,
            s.img.height * k,
        );
        true
    }

    /// Draws a themed button whose strip height equals the rect height. Returns the
    /// caption colour and shift as the look-alike does.
    pub(crate) fn draw_button(
        &self,
        c: &mut Canvas,
        k: u32,
        r: Rect,
        state: ControlState,
        focused: bool,
    ) -> Option<(Rgb, i32)> {
        let b = self
            .buttons
            .iter()
            .find(|b| b.left.img.height as i32 == r.h)?;
        let name = match state {
            ControlState::Normal if focused => "default",
            s => state_name(s),
        };
        let i = b.left.index(name).or(b.left.index("normal"))?;
        let ki = k as i32;
        let h = r.h as u32;
        let (lw, rw) = (b.left.frame_width(), b.right.frame_width());
        let mw = b.middle.frame_width();
        let mid = (r.w as u32).saturating_sub(lw + rw);
        c.draw_image_part(
            &b.left.img,
            i * lw,
            0,
            lw,
            h,
            r.x * ki,
            r.y * ki,
            lw * k,
            h * k,
        );
        c.draw_image_part(
            &b.middle.img,
            i * mw,
            0,
            mw,
            h,
            (r.x + lw as i32) * ki,
            r.y * ki,
            mid * k,
            h * k,
        );
        c.draw_image_part(
            &b.right.img,
            i * rw,
            0,
            rw,
            h,
            (r.right() - rw as i32) * ki,
            r.y * ki,
            rw * k,
            h * k,
        );
        if focused && state != ControlState::Disabled {
            art::dotted_rect(
                c,
                k,
                Rect::new(r.x + 3, r.y + 3, r.w - 6, r.h - 6),
                Rgb::BLACK,
            );
        }
        Some(match state {
            ControlState::Disabled => (art::GREY, 0),
            ControlState::Pressed => (Rgb::BLACK, 1),
            _ => (Rgb::BLACK, 0),
        })
    }
}

fn state_name(s: ControlState) -> &'static str {
    match s {
        ControlState::Normal => "normal",
        ControlState::Hot => "hot",
        ControlState::Pressed => "pressed",
        ControlState::Disabled => "disabled",
    }
}
