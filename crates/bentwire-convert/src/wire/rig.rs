//! Wire's vector rig: one piece of copper wire coiled into a spring (its body), bent up into
//! a neck and round into a loop that holds its face, ending in a curl on top; a second, short
//! wire twisted round the neck makes the two arms. A [`Pose`] sets the joints; [`shapes`]
//! turns it into strokes and fills for the rasteriser. Every measure is in canvas units of
//! the 128 x 128 canvas, y down.

use crate::own::parts::{self, Body, EDGE};
use crate::own::pose::{Mouth, Pose, REST, arc, bezier, dir};
use crate::own::raster::{P, Role, Shape};
use std::f64::consts::PI;

const CX: f64 = 64.0;
const GROUND: f64 = 118.0;
const COIL_BOTTOM: f64 = 114.0;
const COIL_H: f64 = 20.0;
const COIL_RX: f64 = 13.0;
const COIL_RY: f64 = 3.4;
const TURNS: f64 = 2.75;
const NECK: f64 = 8.0;
pub const HEAD_R: f64 = 18.0;
/// Copper core width (the dark edge is [`EDGE`] on each side).
const WIRE: f64 = 3.4;
const ARM: f64 = 2.4;
const UPPER: f64 = 20.0;
const FORE: f64 = 16.0;
/// Mouth centre below the head centre (head-local).
const MOUTH_Y: f64 = 8.0;
/// Mouth picture cell (mouth sheet).
pub const MOUTH: [u32; 2] = [16, 12];
/// The whole figure is drawn this much larger than the measures above.
const ZOOM: f64 = 1.2;

/// Wire's colours; the retro palette is this list in order, after the transparent index 0.
pub const ROLES: [Role; 11] = [
    Role::Ink,
    Role::CopperDark,
    Role::Copper,
    Role::CopperLight,
    Role::Face,
    Role::White,
    Role::MouthDark,
    Role::Tongue,
    Role::Yellow,
    Role::Blue,
    Role::Grey,
];

/// Placement of a pose: figure transform and the head frame.
struct Rig {
    p: Pose,
    /// Width factor (strokes scale with the figure).
    ws: f64,
    head: P,
    rot: (f64, f64),
}

impl Body for Rig {
    fn pose(&self) -> &Pose {
        &self.p
    }

    fn ws(&self) -> f64 {
        self.ws
    }

    fn t(&self, q: P) -> P {
        (
            CX + (q.0 - CX) * self.p.sx * ZOOM + self.p.dx,
            GROUND + (q.1 - GROUND) * self.p.sy * ZOOM + self.p.dy,
        )
    }

    fn h(&self, u: f64, v: f64) -> P {
        self.t(self.hf(u, v))
    }

    fn head(&self) -> P {
        self.t(self.head)
    }

    fn left_hand(&self) -> P {
        self.arm(0).2
    }
}

impl Rig {
    fn new(p: &Pose) -> Rig {
        let top = COIL_BOTTOM - COIL_H * p.coil;
        Rig {
            p: *p,
            ws: (p.sx + p.sy) / 2.0 * ZOOM,
            head: (CX + p.lean, top - COIL_RY - NECK - HEAD_R + p.nod),
            rot: dir(p.tilt),
        }
    }

    /// Head-local coordinates (origin at the head centre, tilted) to figure coordinates.
    fn hf(&self, u: f64, v: f64) -> P {
        let (c, s) = self.rot;
        (self.head.0 + u * c - v * s, self.head.1 + u * s + v * c)
    }

    fn tp(&self, pts: &[P]) -> Vec<P> {
        pts.iter().map(|&q| self.t(q)).collect()
    }

    /// A wire along figure points: dark edge, copper core, highlight.
    fn wire(&self, out: &mut Vec<Shape>, pts: &[P], w: f64, core: Role, retro: bool) {
        let pts = self.tp(pts);
        let w = w * self.ws;
        out.push(Shape::Stroke(
            pts.clone(),
            w + 2.0 * EDGE * self.ws,
            Role::Ink,
        ));
        out.push(Shape::Stroke(pts.clone(), w, core));
        if core == Role::Copper {
            let o = if retro { 0.9 } else { 0.7 } * self.ws;
            let hi = pts.iter().map(|&(x, y)| (x - o, y - o)).collect();
            let hw = if retro { 1.0 } else { 0.9 * self.ws };
            out.push(Shape::Stroke(hi, hw, Role::CopperLight));
        }
    }

    fn shoulder(&self, side: usize) -> P {
        let n = self.neck();
        let m = n[n.len() / 2];
        (m.0 + if side == 0 { -1.5 } else { 1.5 }, m.1)
    }

    /// Elbow, hand and hand direction (degrees) of an arm, figure coordinates.
    fn arm(&self, side: usize) -> (P, P, P, f64) {
        let s = if side == 0 { -1.0 } else { 1.0 };
        let (a, b) = self.p.arms[side];
        let sh = self.shoulder(side);
        let (ca, sa) = dir(a);
        let e = (sh.0 + s * UPPER * sa, sh.1 + UPPER * ca);
        let (cb, sb) = dir(a + b);
        let hand = (e.0 + s * FORE * sb, e.1 + FORE * cb);
        (sh, e, hand, if side == 0 { -(a + b) } else { a + b })
    }

    fn neck(&self) -> Vec<P> {
        let top = COIL_BOTTOM - COIL_H * self.p.coil;
        let p0 = (CX, top - COIL_RY);
        let p3 = self.hf(0.0, HEAD_R);
        let p1 = (p0.0, p0.1 - NECK * 0.5);
        let p2 = self.hf(0.0, HEAD_R + NECK * 0.5);
        bezier(p0, p1, p2, p3, 10)
    }
}

/// The shapes of `p`, back to front. `retro` adapts line widths to hard pixels and leaves out
/// the translucent shadow.
pub fn shapes(p: &Pose, retro: bool) -> Vec<Shape> {
    let r = Rig::new(p);
    let mut out = Vec::new();
    let ground = r.t((CX, GROUND - 1.0));
    out.push(Shape::Shadow(
        ground,
        16.0 * ZOOM * p.sx * (2.0 - p.coil.min(1.5)),
        3.0 * ZOOM * p.sx,
        0.22,
    ));
    // Coil: back half-turns first (darker), then the front ones.
    let top = COIL_BOTTOM - COIL_H * p.coil;
    let total = TURNS * 2.0 * PI;
    let pt = |t: f64| {
        let (c, s) = dir(t * 180.0 / PI);
        (
            CX + COIL_RX * c,
            COIL_BOTTOM - (COIL_BOTTOM - top) * t / total + COIL_RY * s,
        )
    };
    let halves = (TURNS * 2.0).ceil() as usize;
    for front in [false, true] {
        for h in 0..halves {
            // Half-turn h runs from h*pi to (h+1)*pi; sin > 0 (lower) is the front.
            let is_front = h % 2 == 0;
            if is_front != front {
                continue;
            }
            let t0 = h as f64 * PI;
            let t1 = ((h + 1) as f64 * PI).min(total);
            let pts: Vec<P> = (0..=12)
                .map(|i| pt(t0 + (t1 - t0) * i as f64 / 12.0))
                .collect();
            let core = if front {
                Role::Copper
            } else {
                Role::CopperDark
            };
            r.wire(&mut out, &pts, WIRE, core, retro);
        }
    }
    r.wire(&mut out, &r.neck(), WIRE, Role::Copper, retro);
    // Face and the loop round it (one turn and a bit, from the neck).
    out.push(Shape::Disc(r.h(0.0, 0.0), HEAD_R * r.ws, Role::Face));
    let ring: Vec<P> = arc((0.0, 0.0), HEAD_R, 90.0, 90.0 + 380.0, 48)
        .into_iter()
        .map(|(u, v)| r.hf(u, v))
        .collect();
    r.wire(&mut out, &ring, WIRE, Role::Copper, retro);
    // Curl on top.
    let (cc, cs) = dir(p.curl);
    let curl: Vec<P> = [(0.0, 0.0), (0.6, -3.0), (1.6, -5.4)]
        .into_iter()
        .chain(arc((4.2, -6.4), 2.6, 160.0, 160.0 + 300.0, 10))
        .map(|(u, v)| {
            let (u, v) = (u * cc - v * cs, u * cs + v * cc);
            r.hf(u, v - HEAD_R - 1.0)
        })
        .collect();
    r.wire(&mut out, &curl, WIRE * 0.8, Role::Copper, retro);
    face(&r, &mut out, retro);
    // Paper behind the right arm.
    parts::paper(&r, &mut out, CX + 18.0, 96.0);
    // Arms: the twist round the neck, then each arm with a loop for a hand.
    let sh = r.shoulder(0);
    for dy in [-1.4, 1.4] {
        r.wire(
            &mut out,
            &[(sh.0 - 1.5, sh.1 + dy + 1.4), (sh.0 + 4.5, sh.1 + dy - 1.4)],
            ARM,
            Role::Copper,
            retro,
        );
    }
    for side in 0..2 {
        let (s, e, hand, hd) = r.arm(side);
        let (hc, hs) = dir(hd);
        let loop_c = (hand.0 + 2.2 * hs, hand.1 + 2.2 * hc);
        let mut pts = vec![s, e, hand];
        pts.extend(arc(loop_c, 2.2, 0.0, 360.0, 12));
        r.wire(&mut out, &pts, ARM, Role::Copper, retro);
        if side == 1 {
            parts::hand_prop(&r, &mut out, loop_c, hd, ARM, retro);
        }
    }
    parts::head_prop(&r, &mut out);
    out
}

fn face(r: &Rig, out: &mut Vec<Shape>, retro: bool) {
    let p = &r.p;
    let ws = r.ws;
    parts::eyes(r, out, retro, Role::Face);
    match p.mouth {
        Mouth::Smile if p.upright() => {}
        m => parts::mouth(out, &|u, v| r.h(u, MOUTH_Y + v), m, ws, retro),
    }
}

/// Top-left of the mouth picture on an upright pose (canvas units, whole pixels).
pub fn mouth_at(p: &Pose) -> (i32, i32) {
    let r = Rig::new(p);
    let c = r.h(0.0, MOUTH_Y);
    (
        (c.0 - MOUTH[0] as f64 / 2.0).round() as i32,
        (c.1 - MOUTH[1] as f64 / 2.0).round() as i32,
    )
}

/// The mouth pictures in the cell: [`parts::mouth_picture`] on the face colour.
pub fn mouth_shapes(t: u8, retro: bool) -> Vec<Shape> {
    parts::mouth_picture(t, retro, Role::Face, ZOOM, MOUTH)
}

/// The tray icon: the head, from `origin` at `k` pixels per unit.
pub fn icon_view() -> (P, f64) {
    let r = Rig::new(&REST);
    let head = r.t(r.head);
    let side = 2.0 * (HEAD_R + 6.0) * ZOOM;
    (
        (head.0 - side / 2.0, head.1 - side / 2.0 - 3.0),
        32.0 / side,
    )
}
