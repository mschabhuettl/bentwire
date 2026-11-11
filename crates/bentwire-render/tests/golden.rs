//! Golden images (T-3/T-4 "balloon and menu goldens"): synthetic scenes, bundled
//! Tahoma only. `BENTWIRE_BLESS=1 cargo test -p bentwire-render --test golden`
//! rewrites `tests/golden/*.png`. On a mismatch the actual image is written to the
//! system temp directory, never into the repo.

use std::path::PathBuf;

use bentwire_core::api::{
    AgentBalloonScene, ButtonScene, CharacterScene, CheckboxScene, ControlState, FontSpec,
    IconKind, LabelPrefix, LabelScene, MenuAction, MenuEntry, MenuScene, OfficeBalloonScene,
    OutputInfo, Picture, Rect, Scene, TailSide, TextLine, TextRun, TrayScene,
};
use bentwire_render::balloon::{draw_agent_balloon, draw_office_balloon, window_canvas};
use bentwire_render::canvas::{Canvas, RgbaImage};
use bentwire_render::font::Fonts;
use bentwire_render::menu::{draw_menu, menu_layout};
use bentwire_render::scene::compose_output;
use bentwire_render::theme::load_png;
use bentwire_render::{art, core_font};

fn check(name: &str, img: &RgbaImage) {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/golden/{name}.png"));
    if std::env::var_os("BENTWIRE_BLESS").is_some_and(|v| v == "1") {
        bentwire_render::scene::write_png(&path, img).unwrap();
        return;
    }
    let want = load_png(&path).unwrap_or_else(|e| panic!("{e} (bless with BENTWIRE_BLESS=1)"));
    if &want != img {
        let out = std::env::temp_dir().join(format!("bentwire-golden-{name}.png"));
        bentwire_render::scene::write_png(&out, img).unwrap();
        panic!("golden {name} differs; actual written to {}", out.display());
    }
}

fn tahoma(px: u32, bold: bool) -> FontSpec {
    FontSpec {
        family: "Tahoma".into(),
        px,
        bold,
        italic: false,
    }
}

/// R-BAL-3/R-BAL-4 geometry for a balloon above the character, as the core lays it
/// out: W = 28 x aveCharWidth, H = lines x (TM.height + 2) + 18, tail 16/4.
fn agent_scene(
    fonts: &Fonts,
    origin: (i32, i32),
    lines: &[&str],
    scroll: i32,
) -> AgentBalloonScene {
    let font = tahoma(13, false);
    let m = fonts.metrics(&core_font(&font));
    let (w, h) = (28 * m.ave_char_width, 2 * (m.height + 2) + 18);
    let (x, y) = origin;
    let tx = x + w / 2 + 20;
    AgentBalloonScene {
        window: Rect::new(x, y, w, h + 16),
        body: Rect::new(x, y, w, h),
        tail_side: TailSide::Bottom,
        tail: [(tx, y + h + 16), (tx - 4, y + h), (tx + 4, y + h)],
        font,
        fg: [0, 0, 0],
        bg: [0xFF, 0xFF, 0xE1],
        border: [0, 0, 0],
        text_origin: (x + 10, y + 9),
        line_pitch: m.height + 2,
        lines: lines.iter().map(|s| s.to_string()).collect(),
        scroll_px: scroll,
    }
}

fn agent_image(fonts: &Fonts, s: &AgentBalloonScene, k: u32) -> RgbaImage {
    let mut c = window_canvas(s.window, k);
    draw_agent_balloon(&mut c, k, fonts, s);
    c.to_rgba()
}

#[test]
fn r_bal_4_agent_balloon_golden() {
    let f = Fonts::bundled();
    let s = agent_scene(
        &f,
        (40, 30),
        &["Hello, I am a synthetic", "character. How are you?"],
        0,
    );
    check("agent-balloon", &agent_image(&f, &s, 1));
    check("agent-balloon-2x", &agent_image(&f, &s, 2));
}

#[test]
fn r_bal_9_agent_balloon_scrolling_golden() {
    let f = Fonts::bundled();
    let s = agent_scene(&f, (0, 0), &["first line", "second line", "third"], -7);
    check("agent-balloon-scroll", &agent_image(&f, &s, 1));
}

/// A minimal stand-in for the core's Office layout (R-OB-35 order, G spacing), enough
/// to place every control type.
struct Office<'a> {
    fonts: &'a Fonts,
    s: OfficeBalloonScene,
    y: i32,
    g: i32,
}

fn run(x: i32, text: &str, color: [u8; 3]) -> TextRun {
    TextRun {
        x,
        text: text.into(),
        color,
        underline: false,
    }
}

const NAVY: [u8; 3] = [0, 0, 0x80];

impl<'a> Office<'a> {
    fn new(fonts: &'a Fonts, w: i32) -> Self {
        let body_font = tahoma(13, false);
        let g = fonts.line_height_px(&body_font) / 2;
        let content = Rect::new(6, 6, w, 0);
        Office {
            fonts,
            s: OfficeBalloonScene {
                window: Rect::new(0, 0, w + 12, 0),
                body: Rect::new(0, 0, w + 12, 0),
                tail_side: TailSide::Bottom,
                tail: [(0, 0); 3],
                content,
                body_font,
                heading_font: tahoma(13, true),
                icon: None,
                heading: vec![],
                text: vec![],
                labels: vec![],
                checkboxes: vec![],
                buttons: vec![],
            },
            y: 6,
            g,
        }
    }

    fn lh(&self) -> i32 {
        self.fonts.line_height_px(&self.s.body_font)
    }

    fn heading(mut self, icon: Option<IconKind>, text: &str) -> Self {
        let x = if icon.is_some() { 6 + 37 } else { 6 };
        let lh = self.lh();
        let block = if icon.is_some() { 32.max(lh) } else { lh };
        self.s.icon = icon.map(|k| (k, Rect::new(6, self.y + block - 32, 32, 32)));
        self.s.heading.push(TextLine {
            y: self.y + block - lh,
            runs: vec![run(x, text, [0, 0, 0])],
        });
        self.y += block;
        self
    }

    fn gap(&mut self) {
        if self.y > 6 {
            self.y += self.g;
        }
    }

    fn text(mut self, runs: Vec<(&str, [u8; 3], bool)>) -> Self {
        self.gap();
        let mut x = 6;
        let mut out = vec![];
        for (t, c, ul) in runs {
            out.push(TextRun {
                x,
                text: t.into(),
                color: c,
                underline: ul,
            });
            x += self.fonts.text_width_px(&self.s.body_font, t);
        }
        self.s.text.push(TextLine {
            y: self.y,
            runs: out,
        });
        self.y += self.lh();
        self
    }

    fn label(
        mut self,
        prefix: LabelPrefix,
        text: &str,
        state: ControlState,
        focused: bool,
    ) -> Self {
        self.gap();
        let lh = self.lh();
        let h = lh.max(15);
        let clickable = prefix == LabelPrefix::Bullet;
        self.s.labels.push(LabelScene {
            rect: Rect::new(6, self.y, self.s.content.w, h),
            prefix,
            prefix_rect: Rect::new(6, self.y + (lh - 15) / 2, 17, 15),
            lines: vec![TextLine {
                y: self.y,
                runs: vec![run(6 + 17 + 5, text, NAVY)],
            }],
            clickable,
            state,
            focused,
        });
        self.y += h;
        self
    }

    fn checkbox(
        mut self,
        text: &str,
        m: Option<char>,
        checked: bool,
        state: ControlState,
        focused: bool,
    ) -> Self {
        self.gap();
        let lh = self.lh();
        self.s.checkboxes.push(CheckboxScene {
            rect: Rect::new(6, self.y, self.s.content.w, lh),
            box_rect: Rect::new(6, self.y + (lh - 13) / 2, 13, 13),
            checked,
            lines: vec![TextLine {
                y: self.y,
                runs: vec![run(6 + 17, text, [0, 0, 0])],
            }],
            mnemonic: m,
            state,
            focused,
        });
        self.y += lh;
        self
    }

    /// R-OB-32 row: uniform width (widest caption rounded up to 5, + 22), 10 px gaps,
    /// centred, 22 px high.
    fn buttons(mut self, caps: &[(&str, ControlState, bool)]) -> Self {
        self.gap();
        self.y += self.g;
        let font = core_font(&self.s.body_font);
        let widest = caps
            .iter()
            .map(|c| {
                self.fonts
                    .text_width(&font, &bentwire_render::font::mnemonic(c.0).0)
                    .max(5)
            })
            .max()
            .unwrap_or(5);
        let bw = (widest + 4) / 5 * 5 + 22;
        let row = caps.len() as i32 * (bw + 10) - 10;
        let mut x = 6 + (self.s.content.w - row) / 2;
        for &(cap, state, focused) in caps {
            let (caption, mnemonic_index) = bentwire_render::font::mnemonic(cap);
            self.s.buttons.push(ButtonScene {
                rect: Rect::new(x, self.y, bw, 22),
                caption,
                mnemonic_index,
                state,
                focused,
            });
            x += bw + 10;
        }
        self.y += 22;
        self
    }

    /// Closes the body and adds a bottom tail (R-OB-25 variant 6) at 70 %.
    fn finish(mut self) -> OfficeBalloonScene {
        let h = self.y + 6;
        self.s.body.h = h;
        self.s.content.h = h - 12;
        self.s.window.h = h + 16;
        let t = (self.s.body.w * 7 / 10).clamp(8, self.s.body.w - 18);
        let b = h - 1;
        self.s.tail = [(t, b), (t, b + 16), (t + 10, b)];
        self.s
    }
}

trait PxMetrics {
    fn line_height_px(&self, f: &FontSpec) -> i32;
    fn text_width_px(&self, f: &FontSpec, t: &str) -> i32;
}

impl PxMetrics for Fonts {
    fn line_height_px(&self, f: &FontSpec) -> i32 {
        self.metrics(&core_font(f)).height
    }
    fn text_width_px(&self, f: &FontSpec, t: &str) -> i32 {
        self.text_width(&core_font(f), t)
    }
}

fn office_image(f: &Fonts, s: &OfficeBalloonScene, k: u32) -> RgbaImage {
    let mut c = window_canvas(s.window, k);
    draw_office_balloon(&mut c, k, f, None, s);
    c.to_rgba()
}

#[test]
fn b_ob_1_office_heading_text_icon_golden() {
    let f = Fonts::bundled();
    let s = Office::new(&f, 220)
        .heading(Some(IconKind::Tip), "A synthetic tip")
        .text(vec![
            ("Plain, ", [0, 0, 0], false),
            ("red", [0xFF, 0, 0], false),
            (" and ", [0, 0, 0], false),
            ("underlined", [0, 0x80, 0], true),
        ])
        .buttons(&[("OK", ControlState::Normal, true)])
        .finish();
    check("office-heading-text", &office_image(&f, &s, 1));
    check("office-heading-text-2x", &office_image(&f, &s, 2));
}

#[test]
fn b_ob_1_office_labels_golden() {
    let f = Fonts::bundled();
    let s = Office::new(&f, 200)
        .heading(None, "Pick one")
        .label(
            LabelPrefix::Bullet,
            "Normal label",
            ControlState::Normal,
            false,
        )
        .label(LabelPrefix::Bullet, "Hot label", ControlState::Hot, false)
        .label(
            LabelPrefix::Bullet,
            "Pressed label",
            ControlState::Pressed,
            false,
        )
        .label(
            LabelPrefix::Bullet,
            "Focused label",
            ControlState::Normal,
            true,
        )
        .label(
            LabelPrefix::Bullet,
            "Disabled label",
            ControlState::Disabled,
            false,
        )
        .finish();
    check("office-labels", &office_image(&f, &s, 1));
}

#[test]
fn b_ob_1_office_bullets_numbers_golden() {
    let f = Fonts::bundled();
    let s = Office::new(&f, 180)
        .label(
            LabelPrefix::Char("\u{2022}".into()),
            "Bullet item",
            ControlState::Normal,
            false,
        )
        .label(
            LabelPrefix::Char("\u{2022}".into()),
            "Another",
            ControlState::Normal,
            false,
        )
        .label(
            LabelPrefix::Number("1.".into()),
            "Number one",
            ControlState::Normal,
            false,
        )
        .label(
            LabelPrefix::Number("2.".into()),
            "Number two",
            ControlState::Normal,
            false,
        )
        .buttons(&[("OK", ControlState::Normal, false)])
        .finish();
    check("office-bullets-numbers", &office_image(&f, &s, 1));
}

#[test]
fn b_ob_1_office_checkboxes_golden() {
    let f = Fonts::bundled();
    let s = Office::new(&f, 200)
        .checkbox("Unchecked", Some('u'), false, ControlState::Normal, false)
        .checkbox("Checked", None, true, ControlState::Normal, false)
        .checkbox("Hot", None, false, ControlState::Hot, false)
        .checkbox("Pressed", None, true, ControlState::Pressed, false)
        .checkbox("Focused", None, false, ControlState::Normal, true)
        .checkbox("Disabled", None, true, ControlState::Disabled, false)
        .buttons(&[
            ("OK", ControlState::Normal, false),
            ("Cancel", ControlState::Normal, false),
        ])
        .finish();
    check("office-checkboxes", &office_image(&f, &s, 1));
}

#[test]
fn r_ob_11_office_button_states_golden() {
    let f = Fonts::bundled();
    let s = Office::new(&f, 450)
        .buttons(&[
            ("&Normal", ControlState::Normal, false),
            ("&Hot", ControlState::Hot, false),
            ("&Pressed", ControlState::Pressed, false),
            ("&Focus", ControlState::Normal, true),
            ("&Disabled", ControlState::Disabled, false),
        ])
        .finish();
    check("office-button-states", &office_image(&f, &s, 1));
}

#[test]
fn r_ob_32_office_button_sets_golden() {
    // Captions as listed in R-OB-32 / B-ASK-2 (English).
    let f = Fonts::bundled();
    let n = |c: &'static str| (c, ControlState::Normal, false);
    let mut b = Office::new(&f, 330);
    for set in [
        vec![n("OK"), n("Cancel")],
        vec![n("&Yes"), n("&No"), n("Cancel")],
        vec![n("< &Back"), n("&Next >"), n("Close")],
        vec![n("&Abort"), n("&Retry"), n("&Ignore")],
        vec![n("&Yes"), n("Yes to &All"), n("&No"), n("Cancel")],
    ] {
        b = b.buttons(&set);
    }
    check("office-button-sets", &office_image(&f, &b.finish(), 1));
}

#[test]
fn r_ob_5_icons_golden() {
    let f = Fonts::bundled();
    let mut c = Canvas::new(6 * 36 + 4, 40);
    c.fill_rect(
        0,
        0,
        c.width as i32,
        40,
        bentwire_render::balloon::OFFICE_BG,
    );
    for (i, kind) in [
        IconKind::Alert,
        IconKind::Tip,
        IconKind::Info,
        IconKind::Warning,
        IconKind::Query,
        IconKind::Critical,
    ]
    .into_iter()
    .enumerate()
    {
        art::icon(&mut c, 1, &f, kind, 4 + 36 * i as i32, 4);
    }
    check("office-icons", &c.to_rgba());
}

fn menu_entries(labels: [&str; 4]) -> Vec<MenuEntry> {
    let item = |l: &str, action| MenuEntry::Item {
        label: l.into(),
        action,
    };
    vec![
        item(labels[0], MenuAction::Hide),
        MenuEntry::Separator,
        item(labels[1], MenuAction::Options),
        item(labels[2], MenuAction::ChooseAssistant),
        item(labels[3], MenuAction::Animate),
    ]
}

fn menu_image(f: &Fonts, entries: Vec<MenuEntry>, highlighted: Option<usize>, k: u32) -> RgbaImage {
    let g = menu_layout(f, &entries);
    let mut c = Canvas::new(g.width as u32 * k, g.height as u32 * k);
    let s = MenuScene {
        x: 0,
        y: 0,
        entries,
        highlighted,
    };
    draw_menu(&mut c, k, f, &s);
    c.to_rgba()
}

#[test]
fn b_menu_2_menu_en_de_golden() {
    let f = Fonts::bundled();
    let en = menu_entries(["&Hide", "&Options...", "&Choose Assistant...", "&Animate!"]);
    let de = menu_entries([
        "&Ausblenden",
        "&Optionen...",
        "Assistenten &wählen...",
        "A&nimation!",
    ]);
    check("menu-en", &menu_image(&f, en.clone(), Some(3), 1));
    check("menu-de", &menu_image(&f, de, Some(0), 1));
    check("menu-en-2x", &menu_image(&f, en, Some(2), 2));
}

#[test]
fn b_wl_9_menu_hit_skips_separator() {
    let f = Fonts::bundled();
    let entries = menu_entries(["&Hide", "&Options...", "&Choose Assistant...", "&Animate!"]);
    let g = menu_layout(&f, &entries);
    let r1 = g.items[1];
    assert_eq!(g.hit(&entries, r1.x + 1, r1.y + 1), None);
    let r2 = g.items[2];
    assert_eq!(g.hit(&entries, r2.x + 5, r2.y + 5), Some(2));
    assert_eq!(g.hit(&entries, -1, -1), None);
}

fn synthetic_picture(w: u32, h: u32) -> RgbaImage {
    let mut img = RgbaImage::new(w, h);
    let (cx, cy, r) = (w as f32 / 2.0, h as f32 / 2.0, w.min(h) as f32 / 2.0 - 1.0);
    for y in 0..h {
        for x in 0..w {
            let d = ((x as f32 + 0.5 - cx).powi(2) + (y as f32 + 0.5 - cy).powi(2)).sqrt();
            if d <= r {
                let o = ((y * w + x) * 4) as usize;
                img.data[o..o + 4].copy_from_slice(&[0x40, 0x90, 0x40, 255]);
            }
        }
    }
    img
}

#[test]
fn t_4_headless_scene_golden() {
    let f = Fonts::bundled();
    let output = |scale| OutputInfo {
        id: 7,
        rect: Rect::new(100, 0, 360, 240),
        scale,
    };
    let character = CharacterScene {
        id: "synthetic".into(),
        mapped: true,
        x: 220,
        y: 140,
        width: 64,
        height: 64,
        picture: Picture::Empty,
        output: Some(7),
        dragging: false,
        keyboard_on_demand: false,
    };
    let entries = menu_entries(["&Hide", "&Options...", "&Choose Assistant...", "&Animate!"]);
    let scene = Scene {
        character: Some(character),
        agent_balloon: Some(agent_scene(&f, (150, 60), &["Hi there!"], 0)),
        office_balloon: None,
        menu: Some(MenuScene {
            x: 260,
            y: 170,
            entries: entries.clone(),
            highlighted: Some(0),
        }),
        tray: TrayScene {
            icon_character: None,
            title: String::new(),
            tooltip: String::new(),
            menu: entries,
        },
        outputs: vec![output(1.0)],
    };
    let pic = synthetic_picture(64, 64);
    check(
        "scene",
        &compose_output(&scene, &output(1.0), Some(&pic), &f, None),
    );
    check(
        "scene-1.5x",
        &compose_output(&scene, &output(1.5), Some(&pic), &f, None),
    );
}

/// Office balloons laid out and placed by the core (B-OB-1..3), drawn by the renderer.
fn core_office(
    spec: bentwire_core::api::AskSpec,
    it: impl FnOnce(&mut bentwire_core::office::Interaction),
) -> RgbaImage {
    use bentwire_core::office;
    let f = Fonts::bundled();
    let model = office::validate(&spec, bentwire_core::api::Lang::En).expect("valid ask");
    let mut inter = office::Interaction {
        checked: vec![false; spec.checkboxes.as_ref().map_or(0, |c| c.len())],
        focus: model.initial_focus(),
        ..Default::default()
    };
    it(&mut inter);
    let cx = office::Ctx::new(&f, 800);
    let placed = office::place(
        &model,
        &inter,
        Rect::new(500, 350, 96, 96),
        Rect::new(0, 0, 800, 600),
        &cx,
    )
    .expect("fits");
    office_image(&f, &placed.scene, 1)
}

fn strings(v: &[&str]) -> Option<Vec<String>> {
    Some(v.iter().map(|s| s.to_string()).collect())
}

#[test]
fn b_ob_1_core_layout_heading_markup_golden() {
    use bentwire_core::api::{AskButtons, AskSpec};
    let img = core_office(
        AskSpec {
            heading: Some("Synthetic heading".into()),
            text: Some("Body with {cf 249}red{cf 0} and {ul 1}underlined{ul 0} words that wrap over more than one line.".into()),
            icon: Some("info".into()),
            buttons: Some(AskButtons::Set("yes-no-cancel".into())),
            ..Default::default()
        },
        |_| {},
    );
    check("core-office-heading", &img);
}

#[test]
fn b_ob_1_core_layout_labels_checkboxes_golden() {
    use bentwire_core::api::{AskButtons, AskSpec};
    use bentwire_core::office::Control;
    let img = core_office(
        AskSpec {
            heading: Some("Choose".into()),
            labels: strings(&["First label", "Second label"]),
            checkboxes: strings(&["&Remember this", "Other box"]),
            buttons: Some(AskButtons::Set("ok-cancel".into())),
            ..Default::default()
        },
        |it| {
            it.checked[0] = true;
            it.hot = Some(Control::Label(1));
            it.pressed = Some(Control::Button(1));
        },
    );
    check("core-office-controls", &img);
}

#[test]
fn b_ob_1_core_layout_numbers_custom_buttons_golden() {
    use bentwire_core::api::{AskButtons, AskSpec};
    let img = core_office(
        AskSpec {
            text: Some("Steps:".into()),
            icon: Some("warning".into()),
            labels: strings(&["Open the lid", "Insert the disk"]),
            label_type: Some("numbers".into()),
            buttons: Some(AskButtons::Custom(vec!["&Go".into(), "&Later".into()])),
            ..Default::default()
        },
        |_| {},
    );
    check("core-office-numbers", &img);
}
