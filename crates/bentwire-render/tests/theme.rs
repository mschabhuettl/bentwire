//! A-THEME-1: a synthetic balloon theme replaces the look-alike where it has art.

use bentwire_core::api::{
    ButtonScene, ControlState, FontSpec, IconKind, OfficeBalloonScene, Rect, TailSide,
};
use bentwire_render::balloon::{draw_office_balloon, window_canvas};
use bentwire_render::canvas::RgbaImage;
use bentwire_render::font::Fonts;
use bentwire_render::scene::write_png;
use bentwire_render::theme::Theme;

fn solid(w: u32, h: u32, frames: &[[u8; 4]]) -> RgbaImage {
    let mut img = RgbaImage::new(w * frames.len() as u32, h);
    for y in 0..h {
        for (i, c) in frames.iter().enumerate() {
            for x in 0..w {
                let o = ((y * img.width + i as u32 * w + x) * 4) as usize;
                img.data[o..o + 4].copy_from_slice(c);
            }
        }
    }
    img
}

const NORMAL: [u8; 4] = [255, 0, 255, 255];
const HOT: [u8; 4] = [0, 255, 255, 255];
const ICON: [u8; 4] = [10, 20, 30, 255];

fn theme_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bentwire-theme-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("theme.toml"),
        r#"format = 1
name = "Synthetic"
[buttons.small]
left = "l.png"
middle = "m.png"
right = "r.png"
frames = ["normal", "hot"]
[icons]
info = "info.png"
"#,
    )
    .unwrap();
    for (f, w) in [("l.png", 11), ("m.png", 5), ("r.png", 11)] {
        write_png(&dir.join(f), &solid(w, 22, &[NORMAL, HOT])).unwrap();
    }
    write_png(&dir.join("info.png"), &solid(32, 32, &[ICON])).unwrap();
    dir
}

fn font(bold: bool) -> FontSpec {
    FontSpec {
        family: "Tahoma".into(),
        px: 13,
        bold,
        italic: false,
        cell: true,
    }
}

fn px(img: &RgbaImage, x: u32, y: u32) -> [u8; 4] {
    let o = ((y * img.width + x) * 4) as usize;
    img.data[o..o + 4].try_into().unwrap()
}

#[test]
fn a_theme_1_theme_art_replaces_look_alike() {
    let dir = theme_dir("ok");
    let theme = Theme::load(&dir).unwrap();
    let button = |x, state| ButtonScene {
        rect: Rect::new(x, 50, 60, 22),
        caption: String::new(),
        mnemonic_index: None,
        state,
        focused: false,
    };
    let s = OfficeBalloonScene {
        window: Rect::new(0, 0, 200, 100),
        body: Rect::new(0, 0, 200, 84),
        tail_side: TailSide::Bottom,
        tail: [(140, 83), (140, 99), (150, 83)],
        content: Rect::new(6, 6, 188, 72),
        body_font: font(false),
        heading_font: font(true),
        icon: Some((IconKind::Info, Rect::new(6, 6, 32, 32))),
        heading: vec![],
        text: vec![],
        labels: vec![],
        checkboxes: vec![],
        buttons: vec![
            button(10, ControlState::Normal),
            button(100, ControlState::Hot),
        ],
    };
    let f = Fonts::bundled();
    let mut c = window_canvas(s.window, 1);
    draw_office_balloon(&mut c, 1, &f, Some(&theme), &s);
    let img = c.to_rgba();
    assert_eq!(px(&img, 10, 50), NORMAL);
    assert_eq!(px(&img, 40, 60), NORMAL, "stretched middle");
    assert_eq!(px(&img, 69, 71), NORMAL);
    assert_eq!(px(&img, 100, 50), HOT);
    assert_eq!(px(&img, 20, 20), ICON);
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_theme_1_rejects_unknown_format() {
    let dir = std::env::temp_dir().join(format!("bentwire-theme-bad-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("theme.toml"), "format = 2\n").unwrap();
    assert!(Theme::load(&dir).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

#[test]
fn a_theme_1_rejects_mismatched_button_strips() {
    let dir = theme_dir("mismatch");
    // Middle strip one row shorter than the caps.
    write_png(&dir.join("m.png"), &solid(5, 21, &[NORMAL, HOT])).unwrap();
    assert!(Theme::load(&dir).is_err());
    std::fs::remove_dir_all(&dir).unwrap();
}

const DEFAULT: [u8; 4] = [255, 128, 0, 255];
const DISABLED: [u8; 4] = [250, 250, 250, 255];

/// A theme written by an older converter: a "default" frame and the old state order.
fn old_theme_dir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("bentwire-theme-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("theme.toml"),
        r#"format = 1
name = "Old"
[buttons.small]
left = "l.png"
middle = "m.png"
right = "r.png"
frames = ["normal", "default", "hot", "pressed", "disabled"]
[bullets]
label = "b.png"
label_frames = ["normal", "hot", "pressed", "disabled"]
"#,
    )
    .unwrap();
    let frames = [NORMAL, DEFAULT, HOT, [0, 0, 255, 255], DISABLED];
    for (f, w) in [("l.png", 11), ("m.png", 5), ("r.png", 11)] {
        write_png(&dir.join(f), &solid(w, 22, &frames)).unwrap();
    }
    write_png(&dir.join("b.png"), &solid(15, 15, &frames[..4])).unwrap();
    dir
}

// R-OB-11, R-OB-10, B-REN-7: a focused themed button keeps its state frame (no "default"
// frame) and gets the dotted rectangle around the caption, widened by 2 px left and right
// and 1 px at the bottom; a disabled one keeps its frame with a #B2B2B2 caption. An old
// theme with a "default" frame still loads.
#[test]
fn r_ob_11_r_ob_10_themed_focus_and_disabled_caption() {
    let dir = old_theme_dir("focus");
    let theme = Theme::load(&dir).unwrap();
    let button = |x, caption: &str, state, focused| ButtonScene {
        rect: Rect::new(x, 50, 60, 22),
        caption: caption.into(),
        mnemonic_index: None,
        state,
        focused,
    };
    let s = OfficeBalloonScene {
        window: Rect::new(0, 0, 200, 100),
        body: Rect::new(0, 0, 200, 84),
        tail_side: TailSide::Bottom,
        tail: [(140, 83), (140, 99), (150, 83)],
        content: Rect::new(6, 6, 188, 72),
        body_font: font(false),
        heading_font: font(true),
        icon: None,
        heading: vec![],
        text: vec![],
        labels: vec![],
        checkboxes: vec![],
        buttons: vec![
            button(10, "OK", ControlState::Normal, true),
            button(100, "lll", ControlState::Disabled, false),
        ],
    };
    let f = Fonts::bundled();
    let mut c = window_canvas(s.window, 1);
    draw_office_balloon(&mut c, 1, &f, Some(&theme), &s);
    let img = c.to_rgba();
    // State frame, not the old "default" frame.
    assert_eq!(px(&img, 11, 51), NORMAL);
    let body = bentwire_render::core_font(&font(false));
    let w = f.text_width(&body, "OK");
    let th = f.metrics(&body).height;
    let (x, y) = (10 + (60 - w) / 2, 50 + (22 - th) / 2);
    let (fx, fy, fw, fh) = (x - 2, y, w + 4, th + 1);
    let black = [0, 0, 0, 255];
    // The dotted pattern of art::dotted_rect: pixels with an even x + y.
    let on = |x: i32, y: i32| px(&img, x as u32, y as u32);
    // A dotted pixel next to (x, y), along a vertical (v) or horizontal edge.
    let dot = |x: i32, y: i32, v: bool| match ((x + y) % 2 == 0, v) {
        (true, _) => (x, y),
        (false, true) => (x, y + 1),
        (false, false) => (x + 1, y),
    };
    for (x, y) in [
        dot(fx, fy + 2, true),
        dot(fx + fw - 1, fy + 2, true),
        dot(fx + 3, fy, false),
        dot(fx + 3, fy + fh - 1, false),
    ] {
        assert_eq!(on(x, y), black, "focus rectangle at ({x}, {y})");
    }
    // Nothing of the old inset-3 rectangle.
    for y in 53..69 {
        assert_eq!(on(13, y), NORMAL, "no inset focus rectangle at (13, {y})");
    }
    // Disabled: its own frame, caption #B2B2B2. The antialiased caption blends between the
    // frame (250) and #B2B2B2 (178): every pixel is grey and no darker than 178.
    assert_eq!(px(&img, 101, 51), DISABLED);
    let caption: Vec<[u8; 4]> = (100..160)
        .flat_map(|x| (50..72).map(move |y| (x, y)))
        .map(|(x, y)| px(&img, x, y))
        .filter(|p| *p != DISABLED)
        .collect();
    assert!(caption.iter().any(|p| p[0] < 0xD0), "caption drawn");
    for p in caption {
        assert!(p[0] == p[1] && p[1] == p[2] && p[0] >= 0xB2, "{p:?}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}
