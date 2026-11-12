//! Core scene to surfaces: pixel buffers, input regions and the compositor requests a
//! Wayland backend makes for them (B-WL-2, B-WL-6, B-WL-8, B-WL-9). Shared by the
//! Wayland backend and the headless trace.

use crate::library::{self, Library};
use bentwire_core::api::{MenuEntry, OutputInfo, Scene, Surface as CoreSurface};
use bentwire_render::balloon::{self, agent_balloon_region, office_balloon_region, window_canvas};
use bentwire_render::canvas::{Rect, Region, RgbaImage};
use bentwire_render::font::Fonts;
use bentwire_render::menu::{MenuGeometry, draw_menu, menu_layout};
use bentwire_render::picture::picture_buffer;
use bentwire_render::theme::Theme;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// One surface's contents. `argb` is wl_shm ARGB8888 premultiplied (little-endian
/// B, G, R, A) at `scale` times the logical size.
#[derive(Clone, Debug, PartialEq)]
pub struct Buf {
    pub w: u32,
    pub h: u32,
    pub scale: u32,
    pub argb: Vec<u8>,
    /// Logical, surface-relative.
    pub input: Vec<Rect>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CharSurface {
    pub x: i32,
    pub y: i32,
    /// Fractional output scale: shown through a viewport (B-WL-6).
    pub viewport: bool,
    /// Origin of the output the surface is on; margins are relative to it (B-WL-2).
    pub origin: (i32, i32),
    pub output: Option<u32>,
    pub dragging: bool,
    pub buf: Buf,
}

#[derive(Clone, Debug, PartialEq)]
pub struct BalloonSurface {
    pub office: bool,
    /// Global logical window position.
    pub x: i32,
    pub y: i32,
    pub buf: Buf,
    pub region: Region,
}

#[derive(Clone, Debug, PartialEq)]
pub struct MenuSurface {
    pub x: i32,
    pub y: i32,
    pub buf: Buf,
    pub geometry: MenuGeometry,
    pub entries: Vec<MenuEntry>,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Rendered {
    pub character: Option<CharSurface>,
    /// Agent balloon (index 0) and Office balloon (index 1, stacked above).
    pub balloons: [Option<BalloonSurface>; 2],
    pub menu: Option<MenuSurface>,
    pub keyboard: bool,
}

pub fn scale_of(outputs: &[OutputInfo], id: Option<u32>) -> (u32, bool) {
    let s = outputs
        .iter()
        .find(|o| Some(o.id) == id)
        .or(outputs.first())
        .map(|o| o.scale)
        .unwrap_or(1.0);
    (bentwire_render::scene::buffer_scale(s), s.fract() != 0.0)
}

/// The character picture at buffer scale `k` and the scene's size factor (B-SCALE-1):
/// at `k` or at `k` times the factor, see [`library::density`]. The buffer maps it to the
/// surface (logical size times `k`) by nearest neighbour.
pub fn character_picture(scene: &Scene, lib: &Library, k: u32) -> Option<RgbaImage> {
    let ch = scene.character.as_ref()?;
    let c = lib.get(&ch.id)?;
    Some(library::picture(
        c,
        &ch.picture,
        library::density(c, k, ch.scale),
    ))
}

pub fn render(scene: &Scene, lib: &Library, fonts: &Fonts, theme: Option<&Theme>) -> Rendered {
    let mut r = Rendered::default();
    let Some(ch) = scene.character.as_ref().filter(|c| c.mapped) else {
        return r;
    };
    let (k, viewport) = scale_of(&scene.outputs, ch.output);
    let pic = character_picture(scene, lib, k)
        .unwrap_or_else(|| RgbaImage::new(ch.width * k, ch.height * k));
    let sb = picture_buffer(&pic, ch.width, ch.height, k);
    r.keyboard = ch.keyboard_on_demand;
    r.character = Some(CharSurface {
        x: ch.x,
        y: ch.y,
        viewport,
        origin: scene
            .outputs
            .iter()
            .find(|o| Some(o.id) == ch.output)
            .map(|o| (o.rect.x, o.rect.y))
            .unwrap_or((0, 0)),
        output: ch.output,
        dragging: ch.dragging,
        buf: Buf {
            w: ch.width,
            h: ch.height,
            scale: k,
            argb: sb.data,
            input: sb.input_region,
        },
    });
    let mk_balloon = |office: bool,
                      window: bentwire_core::api::Rect,
                      region: Region,
                      c: bentwire_render::canvas::Canvas| BalloonSurface {
        office,
        x: window.x,
        y: window.y,
        buf: Buf {
            w: window.w as u32,
            h: window.h as u32,
            scale: k,
            argb: c.to_argb8888(),
            input: region.offset(-window.x, -window.y).rects(),
        },
        region,
    };
    if let Some(b) = &scene.office_balloon {
        let mut c = window_canvas(b.window, k);
        balloon::draw_office_balloon(&mut c, k, fonts, theme, b);
        r.balloons[1] = Some(mk_balloon(true, b.window, office_balloon_region(b), c));
    }
    if let Some(b) = &scene.agent_balloon {
        let mut c = window_canvas(b.window, k);
        balloon::draw_agent_balloon(&mut c, k, fonts, b);
        r.balloons[0] = Some(mk_balloon(false, b.window, agent_balloon_region(b), c));
    }
    if let Some(m) = &scene.menu {
        let g = menu_layout(fonts, &m.entries);
        let mut c = bentwire_render::canvas::Canvas::new(g.width as u32 * k, g.height as u32 * k);
        c.origin = (m.x * k as i32, m.y * k as i32);
        draw_menu(&mut c, k, fonts, m);
        r.menu = Some(MenuSurface {
            x: m.x,
            y: m.y,
            buf: Buf {
                w: g.width as u32,
                h: g.height as u32,
                scale: k,
                argb: c.to_argb8888(),
                input: vec![Rect::new(0, 0, g.width, g.height)],
            },
            geometry: g,
            entries: m.entries.clone(),
        });
    }
    r
}

/// Where a pointer event at global (x, y) lands, top surface first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    Menu(Option<usize>),
    Surface(CoreSurface),
}

pub fn hit(r: &Rendered, x: i32, y: i32) -> Option<Hit> {
    if let Some(m) = &r.menu
        && Rect::new(m.x, m.y, m.buf.w as i32, m.buf.h as i32).contains(x, y)
    {
        return Some(Hit::Menu(m.geometry.hit(&m.entries, x - m.x, y - m.y)));
    }
    if let Some(b) = r
        .balloons
        .iter()
        .rev()
        .flatten()
        .find(|b| b.region.contains(x, y))
    {
        return Some(Hit::Surface(if b.office {
            CoreSurface::OfficeBalloon
        } else {
            CoreSurface::AgentBalloon
        }));
    }
    if let Some(c) = &r.character
        && c.buf.input.iter().any(|q| q.contains(x - c.x, y - c.y))
    {
        return Some(Hit::Surface(CoreSurface::Character));
    }
    None
}

fn hash(parts: &[&[u8]]) -> String {
    let mut h = Sha256::new();
    for p in parts {
        h.update(p);
    }
    h.finalize()[..8]
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn region_hash(r: &[Rect]) -> String {
    let s: String = r
        .iter()
        .map(|q| format!("{},{},{},{};", q.x, q.y, q.w, q.h))
        .collect();
    hash(&[s.as_bytes()])
}

fn buffer_req(surface: &str, b: &Buf, viewport: bool) -> Value {
    json!({
        "req": "buffer",
        "surface": surface,
        "format": "argb8888",
        "buffer_scale": b.scale,
        "viewport": viewport,
        "logical_size": [b.w, b.h],
        "pixels": hash(&[&b.argb]),
        "input_region": region_hash(&b.input),
    })
}

/// The requests a Wayland backend makes to go from `prev` to `next` (headless trace).
pub fn requests(
    prev: &Rendered,
    next: &Rendered,
    outputs: &[OutputInfo],
    menu_serial: u32,
) -> Vec<Value> {
    let mut out = Vec::new();
    match (&prev.character, &next.character) {
        (Some(_), None) => out.push(json!({"req": "unmap", "surface": "character"})),
        (p, Some(c)) => {
            let o = outputs.iter().find(|o| Some(o.id) == c.output);
            let layer = |c: &CharSurface| {
                json!({
                    "req": "layer_surface",
                    "layer": "top",
                    "namespace": "bentwire",
                    "anchor": "top|left",
                    "exclusive_zone": -1,
                    "output": c.output,
                    "margin": o.map(|o| json!({"top": c.y - o.rect.y, "left": c.x - o.rect.x})),
                    "size": [c.buf.w, c.buf.h],
                })
            };
            let recreate = p.as_ref().is_some_and(|p| p.output != c.output);
            if recreate {
                out.push(
                    json!({"req": "destroy", "surface": "character", "reason": "output_change"}),
                );
            }
            let l = layer(c);
            if recreate || p.as_ref().map(layer) != Some(l.clone()) {
                out.push(l);
            }
            if p.is_none() || recreate || prev.keyboard != next.keyboard {
                let kb = if next.keyboard { "on_demand" } else { "none" };
                out.push(json!({"req": "keyboard_interactivity", "value": kb}));
            }
            if recreate || p.as_ref().map(|p| (&p.buf, p.viewport)) != Some((&c.buf, c.viewport)) {
                out.push(buffer_req("character", &c.buf, c.viewport));
            }
        }
        (None, None) => {}
    }
    let (ax, ay) = next
        .character
        .as_ref()
        .map(|c| (c.x, c.y))
        .unwrap_or((0, 0));
    for (i, name) in ["agent_balloon", "office_balloon"].into_iter().enumerate() {
        match (&prev.balloons[i], &next.balloons[i]) {
            (Some(_), None) => out.push(json!({"req": "destroy", "surface": name})),
            (p, Some(b)) if p.as_ref() != Some(b) => {
                out.push(json!({
                    "req": "subsurface",
                    "surface": name,
                    "position": [b.x - ax, b.y - ay],
                    "place_above": if i == 0 { "character" } else { "agent_balloon" },
                }));
                out.push(buffer_req(name, &b.buf, false));
            }
            _ => {}
        }
    }
    match (&prev.menu, &next.menu) {
        (Some(_), None) => out.push(json!({"req": "destroy", "surface": "menu"})),
        (p, Some(m)) if p.as_ref() != Some(m) => {
            if p.is_none() {
                out.push(json!({
                    "req": "popup",
                    "surface": "menu",
                    "parent": "layer_surface",
                    "anchor_rect": [m.x - ax, m.y - ay, 1, 1],
                    "size": [m.buf.w, m.buf.h],
                    "constraint_adjustment": "flip_x|flip_y|slide_x|slide_y",
                    "grab_serial": menu_serial,
                }));
            }
            out.push(buffer_req("menu", &m.buf, false));
        }
        _ => {}
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use bentwire_core::api::{
        CharacterLoader, CharacterScene, Picture, Rect as CoreRect, TrayScene,
    };
    use std::path::Path;

    fn lib(dir: &str, id: &str) -> Library {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join(dir);
        let shared = std::rc::Rc::new(std::cell::RefCell::new(Library::new(vec![root])));
        library::Loader(shared.clone()).load(id).unwrap();
        shared.take()
    }

    /// The rest frame of `id` (display `size`) at `scale` on an output of `out_scale`.
    fn rendered(l: &Library, id: &str, size: u32, scale: u32, out_scale: f64) -> Buf {
        let scene = Scene {
            character: Some(CharacterScene {
                id: id.into(),
                mapped: true,
                x: 10,
                y: 20,
                width: size * scale,
                height: size * scale,
                scale,
                picture: Picture::Frame {
                    animation: "RestPose".into(),
                    frame: 0,
                    mouth: None,
                },
                output: Some(0),
                dragging: false,
                keyboard_on_demand: false,
            }),
            agent_balloon: None,
            office_balloon: None,
            menu: None,
            tray: TrayScene {
                icon_character: None,
                title: String::new(),
                tooltip: String::new(),
                menu: Vec::new(),
            },
            outputs: vec![OutputInfo {
                id: 0,
                rect: CoreRect::new(0, 0, 800, 600),
                scale: out_scale,
            }],
        };
        let fonts = Fonts::bundled();
        render(&scene, l, &fonts, None).character.unwrap().buf
    }

    fn px(b: &Buf, x: u32, y: u32) -> &[u8] {
        let o = ((y * b.w * b.scale + x) * 4) as usize;
        &b.argb[o..o + 4]
    }

    fn covered(b: &Buf, x: i32, y: i32) -> bool {
        b.input.iter().any(|q| q.contains(x, y))
    }

    #[test]
    fn b_scale_1_bitmap_character_pixels_replicated_and_region_doubled() {
        // pebble: a 32 x 32 bitmap character without hidpi sheets.
        let l = lib("tests/fixtures/characters", "pebble");
        let one = rendered(&l, "pebble", 32, 1, 1.0);
        assert!(one.argb.chunks(4).any(|p| p[3] == 255) && !one.input.is_empty());
        for (scale, out_scale) in [(2, 1.0), (3, 1.0), (2, 2.0)] {
            let b = rendered(&l, "pebble", 32, scale, out_scale);
            assert_eq!((b.w, b.h), (32 * scale, 32 * scale));
            // Device pixels per source pixel: factor times buffer scale (4 on a 2x output).
            let n = scale * b.scale;
            for y in 0..32 * n {
                for x in 0..32 * n {
                    assert_eq!(px(&b, x, y), px(&one, x / n, y / n), "{scale} {x},{y}");
                }
            }
            for y in 0..(32 * scale) as i32 {
                for x in 0..(32 * scale) as i32 {
                    let s = scale as i32;
                    assert_eq!(covered(&b, x, y), covered(&one, x / s, y / s));
                }
            }
        }
    }

    #[test]
    fn b_scale_1_vector_character_drawn_at_the_target_density() {
        // The placeholder has 2x sheets: a 2x character on a 1x output is the picture of a
        // 1x character on a 2x output (its 2x sheet), not the 1x picture doubled.
        let l = lib("../../characters/builtin", "bentwire-placeholder");
        let two = rendered(&l, "bentwire-placeholder", 96, 2, 1.0);
        let hidpi = rendered(&l, "bentwire-placeholder", 96, 1, 2.0);
        assert_eq!((two.w, two.scale, hidpi.w, hidpi.scale), (192, 1, 96, 2));
        assert_eq!(two.argb, hidpi.argb);
        let one = rendered(&l, "bentwire-placeholder", 96, 1, 1.0);
        let doubled = (0..192)
            .flat_map(|y| (0..192).map(move |x| (x, y)))
            .all(|(x, y)| px(&two, x, y) == px(&one, x / 2, y / 2));
        assert!(!doubled);
    }
}
