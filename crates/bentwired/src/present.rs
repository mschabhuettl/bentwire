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

/// The character picture at buffer scale `k`.
pub fn character_picture(scene: &Scene, lib: &Library, k: u32) -> Option<RgbaImage> {
    let ch = scene.character.as_ref()?;
    let c = lib.get(&ch.id)?;
    Some(library::picture(c, &ch.picture, k))
}

pub fn render(scene: &Scene, lib: &Library, fonts: &Fonts, theme: Option<&Theme>) -> Rendered {
    let mut r = Rendered::default();
    let Some(ch) = scene.character.as_ref().filter(|c| c.mapped) else {
        return r;
    };
    let (k, _) = scale_of(&scene.outputs, ch.output);
    let pic = character_picture(scene, lib, k)
        .unwrap_or_else(|| RgbaImage::new(ch.width * k, ch.height * k));
    let sb = picture_buffer(&pic, ch.width, ch.height, k);
    r.keyboard = ch.keyboard_on_demand;
    r.character = Some(CharSurface {
        x: ch.x,
        y: ch.y,
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
            if recreate || p.as_ref().map(|p| &p.buf) != Some(&c.buf) {
                let (_, vp) = scale_of(outputs, c.output);
                out.push(buffer_req("character", &c.buf, vp));
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
