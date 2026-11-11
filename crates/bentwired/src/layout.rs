//! Compositor-request decisions shared by the Wayland backend and the headless trace
//! (B-WL-2, B-WL-4, B-WL-6, B-WL-8).

use crate::engine::{Output, Scene, Surface, SurfaceKind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

/// B-WL-2: the output containing the frame centre, else the nearest one.
pub fn output_for(outputs: &[Output], x: i32, y: i32, w: u32, h: u32) -> Option<&Output> {
    let cx = x as i64 + w as i64 / 2;
    let cy = y as i64 + h as i64 / 2;
    let dist = |o: &Output| {
        let dx = (o.x as i64 - cx)
            .max(0)
            .max(cx - (o.x as i64 + o.w as i64 - 1));
        let dy = (o.y as i64 - cy)
            .max(0)
            .max(cy - (o.y as i64 + o.h as i64 - 1));
        dx * dx + dy * dy
    };
    outputs.iter().min_by_key(|o| dist(o))
}

/// B-WL-6: buffer scale = ceil(output scale).
pub fn buffer_scale(o: &Output) -> u32 {
    (o.scale.ceil() as u32).max(1)
}

pub fn region_hash(r: &[(i32, i32, u32, u32)]) -> String {
    let mut h = Sha256::new();
    for (x, y, w, hh) in r {
        h.update(format!("{x},{y},{w},{hh};"));
    }
    hex(&h.finalize()[..8])
}

pub fn pixels_hash(p: &[u8]) -> String {
    hex(&Sha256::digest(p)[..8])
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn kind_name(k: SurfaceKind) -> &'static str {
    match k {
        SurfaceKind::Character => "character",
        SurfaceKind::Balloon => "balloon",
        SurfaceKind::Menu => "menu",
    }
}

/// The requests a Wayland backend makes to go from `prev` to `next`, as JSON values
/// for the headless trace. `menu_serial` is the grab serial of the opening press.
pub fn requests(prev: &Scene, next: &Scene, outputs: &[Output], menu_serial: u32) -> Vec<Value> {
    let mut out = Vec::new();
    let pc = prev
        .surfaces
        .iter()
        .find(|s| s.kind == SurfaceKind::Character);
    let nc = next
        .surfaces
        .iter()
        .find(|s| s.kind == SurfaceKind::Character);
    match (pc, nc) {
        (Some(_), None) => out.push(json!({"req": "unmap", "surface": "character"})),
        (_, Some(c)) => {
            let o = output_for(outputs, c.x, c.y, c.w, c.h);
            let kb = if next.keyboard { "on_demand" } else { "none" };
            let layer = |c: &Surface| {
                let o = output_for(outputs, c.x, c.y, c.w, c.h);
                json!({
                    "req": "layer_surface",
                    "layer": "top",
                    "namespace": "bentwire",
                    "anchor": "top|left",
                    "exclusive_zone": -1,
                    "output": o.map(|o| o.id),
                    "margin": o.map(|o| json!({"top": c.y - o.y, "left": c.x - o.x})),
                    "size": [c.w, c.h],
                })
            };
            let new_layer = layer(c);
            if pc.map(layer) != Some(new_layer.clone()) {
                out.push(new_layer);
            }
            if pc.is_none() || prev.keyboard != next.keyboard {
                out.push(json!({"req": "keyboard_interactivity", "value": kb}));
            }
            let scale = o.map(buffer_scale).unwrap_or(1);
            if pc.map(|p| (&p.rgba, &p.input, p.scale)) != Some((&c.rgba, &c.input, c.scale)) {
                out.push(json!({
                    "req": "buffer",
                    "surface": "character",
                    "buffer_scale": scale,
                    "viewport": o.is_some_and(|o| o.scale.fract() != 0.0),
                    "pixels": pixels_hash(&c.rgba),
                    "input_region": region_hash(&c.input),
                }));
            }
        }
        (None, None) => {}
    }
    for kind in [SurfaceKind::Balloon, SurfaceKind::Menu] {
        let p: Vec<&Surface> = prev.surfaces.iter().filter(|s| s.kind == kind).collect();
        let n: Vec<&Surface> = next.surfaces.iter().filter(|s| s.kind == kind).collect();
        if p == n {
            continue;
        }
        if n.is_empty() {
            out.push(json!({"req": "destroy", "surface": kind_name(kind)}));
            continue;
        }
        for s in n {
            let (ax, ay) = nc.map(|c| (c.x, c.y)).unwrap_or((0, 0));
            let mut v = json!({
                "req": if kind == SurfaceKind::Menu { "popup" } else { "subsurface" },
                "surface": kind_name(kind),
                "position": [s.x - ax, s.y - ay],
                "size": [s.w, s.h],
                "pixels": pixels_hash(&s.rgba),
                "input_region": region_hash(&s.input),
            });
            if kind == SurfaceKind::Menu {
                v["grab_serial"] = json!(menu_serial);
                v["constraint_adjustment"] = json!("flip|slide");
            }
            out.push(v);
        }
    }
    out
}

/// Composite the scene onto one output (straight RGBA, logical pixels at scale 1).
pub fn compose(scene: &Scene, o: &Output) -> (u32, u32, Vec<u8>) {
    let (w, h) = (o.w, o.h);
    let mut buf = vec![0u8; (w * h * 4) as usize];
    for s in &scene.surfaces {
        let sc = s.scale.max(1);
        for sy in 0..s.h {
            let gy = s.y + sy as i32 - o.y;
            if gy < 0 || gy >= h as i32 {
                continue;
            }
            for sx in 0..s.w {
                let gx = s.x + sx as i32 - o.x;
                if gx < 0 || gx >= w as i32 {
                    continue;
                }
                let si = (((sy * sc) * s.w * sc + sx * sc) * 4) as usize;
                let Some(src) = s.rgba.get(si..si + 4) else {
                    continue;
                };
                let di = ((gy as u32 * w + gx as u32) * 4) as usize;
                let a = src[3] as u32;
                for c in 0..3 {
                    buf[di + c] =
                        ((src[c] as u32 * a + buf[di + c] as u32 * (255 - a)) / 255) as u8;
                }
                buf[di + 3] = (a + buf[di + 3] as u32 * (255 - a) / 255) as u8;
            }
        }
    }
    (w, h, buf)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn out(id: u32, x: i32, w: u32) -> Output {
        Output {
            id,
            x,
            y: 0,
            w,
            h: 100,
            scale: 1.0,
        }
    }

    #[test]
    fn b_wl_2_output_choice() {
        let o = [out(0, 0, 100), out(1, 100, 100)];
        assert_eq!(output_for(&o, 90, 10, 20, 20).unwrap().id, 1);
        assert_eq!(output_for(&o, 80, 10, 20, 20).unwrap().id, 0);
        assert_eq!(output_for(&o, 500, 10, 20, 20).unwrap().id, 1);
        assert_eq!(output_for(&o, -500, 10, 20, 20).unwrap().id, 0);
    }

    #[test]
    fn b_wl_2_b_wl_8_requests() {
        let o = [
            out(0, 0, 100),
            Output {
                scale: 1.5,
                ..out(1, 100, 100)
            },
        ];
        let c = Surface {
            kind: SurfaceKind::Character,
            x: 150,
            y: -5,
            w: 10,
            h: 10,
            scale: 2,
            rgba: vec![0; 1600],
            input: vec![(0, 0, 10, 10)],
        };
        let next = Scene {
            surfaces: vec![c],
            keyboard: true,
        };
        let r = requests(&Scene::default(), &next, &o, 0);
        assert_eq!(r[0]["req"], "layer_surface");
        assert_eq!(r[0]["output"], 1);
        assert_eq!(r[0]["margin"], json!({"top": -5, "left": 50}));
        assert_eq!(r[0]["exclusive_zone"], -1);
        assert_eq!(
            r[1],
            json!({"req": "keyboard_interactivity", "value": "on_demand"})
        );
        assert_eq!(r[2]["buffer_scale"], 2);
        assert_eq!(r[2]["viewport"], true);
        assert!(requests(&next, &next, &o, 0).is_empty());
        let r = requests(&next, &Scene::default(), &o, 0);
        assert_eq!(r, vec![json!({"req": "unmap", "surface": "character"})]);
    }
}
