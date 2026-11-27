//! Wire's vector rig: one piece of copper wire coiled into a spring (its body), bent up into
//! a neck and round into a loop that holds its face, ending in a curl on top; a second, short
//! wire twisted round the neck makes the two arms. A [`Pose`] sets the joints; [`shapes`]
//! turns it into strokes and fills for the rasteriser. Every measure is in canvas units of
//! the 128 x 128 canvas, y down.
//!
//! Left and right in poses are screen sides. Agent names mean the character's own side
//! (GestureRight points to screen left, R-GEST-1); the animation table maps them.

use super::raster::{P, Role, Shape};
use std::f64::consts::PI;

pub const SIZE: u32 = 128;
const CX: f64 = 64.0;
const GROUND: f64 = 118.0;
const COIL_BOTTOM: f64 = 114.0;
const COIL_H: f64 = 20.0;
const COIL_RX: f64 = 13.0;
const COIL_RY: f64 = 3.4;
const TURNS: f64 = 2.75;
const NECK: f64 = 8.0;
pub const HEAD_R: f64 = 18.0;
/// Copper core width and the dark edge on each side.
const WIRE: f64 = 3.4;
const EDGE: f64 = 1.0;
const ARM: f64 = 2.4;
const UPPER: f64 = 20.0;
const FORE: f64 = 16.0;
/// Mouth centre below the head centre (head-local).
const MOUTH_Y: f64 = 8.0;
/// Mouth picture cell (mouth sheet).
pub const MOUTH: [u32; 2] = [16, 12];
/// The whole figure is drawn this much larger than the measures above.
const ZOOM: f64 = 1.2;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Eyes {
    Open,
    Closed,
    Wide,
    Happy,
    Half,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Brows {
    Neutral,
    Up,
    Sad,
    Cross,
    Quizzical,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Mouth {
    /// The closed smile; a separate layer on upright frames, so mouth overlays replace it.
    Smile,
    Grin,
    Frown,
    O,
    Flat,
    Wavy,
    Yawn,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Prop {
    None,
    Question,
    Exclaim,
    /// Sparkles, phase 0..2.
    Sparkle(u8),
    /// 1..3 z's.
    Zzz(u8),
    /// Gear beside the head, turned by degrees.
    Gear(f64),
    /// Magnifier in the right (screen) hand.
    Lens,
    /// Paper on the right with 0..4 written lines; pencil in the right hand.
    Paper(u8),
    /// Sound arcs at the left hand (listening).
    Waves,
}

/// One pose of the rig.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Pose {
    /// Figure offset.
    pub dx: f64,
    pub dy: f64,
    /// Figure scale about the ground point (squash, stretch, Show and Hide).
    pub sx: f64,
    pub sy: f64,
    /// Coil height factor.
    pub coil: f64,
    /// Head offset: sideways (the neck bends) and down.
    pub lean: f64,
    pub nod: f64,
    /// Head tilt in degrees, clockwise.
    pub tilt: f64,
    /// Pupil direction, -1..1.
    pub look: (f64, f64),
    pub eyes: Eyes,
    pub brows: Brows,
    pub mouth: Mouth,
    /// Per arm (screen left, screen right): upper arm angle from straight down, outward
    /// positive, and the elbow bend added for the forearm; degrees.
    pub arms: [(f64, f64); 2],
    /// Curl on top, degrees.
    pub curl: f64,
    pub prop: Prop,
}

pub const REST: Pose = Pose {
    dx: 0.0,
    dy: 0.0,
    sx: 1.0,
    sy: 1.0,
    coil: 1.0,
    lean: 0.0,
    nod: 0.0,
    tilt: 0.0,
    look: (0.0, 0.0),
    eyes: Eyes::Open,
    brows: Brows::Neutral,
    mouth: Mouth::Smile,
    arms: [(55.0, -100.0), (55.0, -100.0)],
    curl: 0.0,
    prop: Prop::None,
};

impl Pose {
    /// Upright and unscaled: the mouth is the separate smile layer and can take overlays.
    pub fn upright(&self) -> bool {
        self.sx == 1.0 && self.sy == 1.0 && self.tilt.abs() <= 6.0 && self.mouth == Mouth::Smile
    }

    /// Linear blend towards `b` at `t` (0..1); discrete parts switch at the half.
    pub fn tween(&self, b: &Pose, t: f64) -> Pose {
        let l = |x: f64, y: f64| x + (y - x) * t;
        let h = t >= 0.5;
        Pose {
            dx: l(self.dx, b.dx),
            dy: l(self.dy, b.dy),
            sx: l(self.sx, b.sx),
            sy: l(self.sy, b.sy),
            coil: l(self.coil, b.coil),
            lean: l(self.lean, b.lean),
            nod: l(self.nod, b.nod),
            tilt: l(self.tilt, b.tilt),
            look: (l(self.look.0, b.look.0), l(self.look.1, b.look.1)),
            eyes: if h { b.eyes } else { self.eyes },
            brows: if h { b.brows } else { self.brows },
            mouth: if h { b.mouth } else { self.mouth },
            arms: [0, 1].map(|i| {
                (
                    l(self.arms[i].0, b.arms[i].0),
                    l(self.arms[i].1, b.arms[i].1),
                )
            }),
            curl: l(self.curl, b.curl),
            prop: if h { b.prop } else { self.prop },
        }
    }
}

/// (cos, sin) of `deg` degrees by a Taylor series: IEEE arithmetic only, so every machine
/// computes the same bits (`f64::sin` comes from the platform's libm).
pub fn dir(deg: f64) -> (f64, f64) {
    let mut d = deg;
    while d > 180.0 {
        d -= 360.0;
    }
    while d < -180.0 {
        d += 360.0;
    }
    let x = d * PI / 180.0;
    let x2 = x * x;
    let (mut s, mut c, mut ts, mut tc) = (0.0, 0.0, x, 1.0);
    for n in 0..16 {
        s += ts;
        c += tc;
        let n = n as f64;
        ts *= -x2 / ((2.0 * n + 2.0) * (2.0 * n + 3.0));
        tc *= -x2 / ((2.0 * n + 1.0) * (2.0 * n + 2.0));
    }
    (c, s)
}

/// Placement of a pose: figure transform and the head frame.
struct Rig {
    p: Pose,
    /// Width factor (strokes scale with the figure).
    ws: f64,
    head: P,
    rot: (f64, f64),
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

    /// Figure coordinates to canvas.
    fn t(&self, q: P) -> P {
        (
            CX + (q.0 - CX) * self.p.sx * ZOOM + self.p.dx,
            GROUND + (q.1 - GROUND) * self.p.sy * ZOOM + self.p.dy,
        )
    }

    /// Head-local coordinates (origin at the head centre, tilted) to figure coordinates.
    fn hf(&self, u: f64, v: f64) -> P {
        let (c, s) = self.rot;
        (self.head.0 + u * c - v * s, self.head.1 + u * s + v * c)
    }

    fn h(&self, u: f64, v: f64) -> P {
        self.t(self.hf(u, v))
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

    /// Ink-outlined stroke of `fill` (props).
    fn outlined(&self, out: &mut Vec<Shape>, pts: Vec<P>, w: f64, fill: Role) {
        out.push(Shape::Stroke(pts.clone(), w + 2.0 * EDGE, Role::Ink));
        out.push(Shape::Stroke(pts, w, fill));
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

fn bezier(p0: P, p1: P, p2: P, p3: P, n: usize) -> Vec<P> {
    (0..=n)
        .map(|i| {
            let t = i as f64 / n as f64;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            )
        })
        .collect()
}

/// Points on a circle from angle `a0` to `a1` (degrees, 0 = +x, clockwise on screen).
fn arc(c: P, r: f64, a0: f64, a1: f64, n: usize) -> Vec<P> {
    (0..=n)
        .map(|i| {
            let (co, si) = dir(a0 + (a1 - a0) * i as f64 / n as f64);
            (c.0 + r * co, c.1 + r * si)
        })
        .collect()
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
    if let Prop::Paper(lines) = p.prop {
        let (x0, y0) = (CX + 18.0, 96.0);
        let sheet = vec![
            (x0, y0),
            (x0 + 18.0, y0 - 2.0),
            (x0 + 20.0, y0 + 20.0),
            (x0 + 2.0, y0 + 22.0),
        ];
        let sheet = r.tp(&sheet);
        out.push(Shape::Polygon(sheet.clone(), Role::White));
        let mut closed = sheet.clone();
        closed.push(sheet[0]);
        out.push(Shape::Stroke(closed, EDGE * 1.2, Role::Ink));
        for i in 0..lines {
            let y = y0 + 4.0 + 4.0 * i as f64;
            let l = r.tp(&[(x0 + 4.0, y), (x0 + 15.0, y - 1.2)]);
            out.push(Shape::Stroke(l, 1.1, Role::Grey));
        }
    }
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
            hand_prop(&r, &mut out, loop_c, hd, retro);
        }
    }
    head_prop(&r, &mut out);
    out
}

fn face(r: &Rig, out: &mut Vec<Shape>, retro: bool) {
    let p = &r.p;
    let ws = r.ws;
    let ink = |w: f64| if retro { w.max(1.2) } else { w } * ws;
    for (i, side) in [-1.0, 1.0].into_iter().enumerate() {
        let e = r.h(side * 7.0, -3.0);
        match p.eyes {
            Eyes::Closed => {
                let l = arc((side * 7.0, -4.5), 4.0, 30.0, 150.0, 8);
                out.push(Shape::Stroke(
                    l.into_iter().map(|(u, v)| r.h(u, v)).collect(),
                    ink(1.4),
                    Role::Ink,
                ));
            }
            Eyes::Happy => {
                let l = arc((side * 7.0, -1.5), 4.0, 210.0, 330.0, 8);
                out.push(Shape::Stroke(
                    l.into_iter().map(|(u, v)| r.h(u, v)).collect(),
                    ink(1.6),
                    Role::Ink,
                ));
            }
            Eyes::Open | Eyes::Wide | Eyes::Half => {
                let rad = if p.eyes == Eyes::Wide { 5.8 } else { 5.0 };
                out.push(Shape::Disc(e, (rad + EDGE) * ws, Role::Ink));
                out.push(Shape::Disc(e, rad * ws, Role::White));
                let pr = if p.eyes == Eyes::Wide { 1.9 } else { 2.3 };
                let reach = rad - pr - 0.6;
                let pc = r.h(side * 7.0 + p.look.0 * reach, -3.0 + p.look.1 * reach);
                out.push(Shape::Disc(pc, pr * ws, Role::Ink));
                if p.eyes == Eyes::Half {
                    // Upper lid down to the middle.
                    let lid: Vec<P> = arc((side * 7.0, -3.0), rad + EDGE + 0.4, 180.0, 360.0, 10)
                        .into_iter()
                        .chain([
                            (side * 7.0 + rad + 1.0, -2.6),
                            (side * 7.0 - rad - 1.0, -2.6),
                        ])
                        .map(|(u, v)| r.h(u, v))
                        .collect();
                    out.push(Shape::Polygon(lid, Role::Face));
                    out.push(Shape::Stroke(
                        vec![r.h(side * 7.0 - rad, -2.6), r.h(side * 7.0 + rad, -2.6)],
                        ink(1.4),
                        Role::Ink,
                    ));
                }
            }
        }
        // Brows: inner end at side * 3.5, outer at side * 10.
        let (inner, outer) = match (p.brows, i) {
            (Brows::Neutral, _) => (-11.0, -11.0),
            (Brows::Up, _) | (Brows::Quizzical, 1) => (-13.5, -13.0),
            (Brows::Sad, _) => (-13.0, -10.5),
            (Brows::Cross, _) => (-9.6, -12.0),
            (Brows::Quizzical, _) => (-10.4, -10.0),
        };
        out.push(Shape::Stroke(
            vec![r.h(side * 3.8, inner), r.h(side * 10.0, outer)],
            ink(1.6),
            Role::Ink,
        ));
    }
    let m = |u: f64, v: f64| r.h(u, MOUTH_Y + v);
    match p.mouth {
        Mouth::Smile if p.upright() => {}
        Mouth::Smile => out.push(Shape::Stroke(
            smile().into_iter().map(|(u, v)| m(u, v)).collect(),
            ink(1.3),
            Role::Ink,
        )),
        Mouth::Grin => {
            let g: Vec<P> = [(-5.0, -1.5)]
                .into_iter()
                .chain(arc((0.0, -1.5), 5.0, 0.0, 180.0, 12).into_iter().rev())
                .map(|(u, v)| m(u, v))
                .collect();
            let mut closed = g.clone();
            closed.push(g[0]);
            out.push(Shape::Stroke(closed, ink(1.2), Role::Ink));
            out.push(Shape::Polygon(g, Role::MouthDark));
            out.push(Shape::Ellipse(
                m(0.0, 2.0),
                2.6 * ws,
                1.2 * ws,
                Role::Tongue,
            ));
        }
        Mouth::Frown => out.push(Shape::Stroke(
            smile().into_iter().map(|(u, v)| m(u, 1.0 - v)).collect(),
            ink(1.3),
            Role::Ink,
        )),
        Mouth::O | Mouth::Yawn => {
            let (rx, ry) = if p.mouth == Mouth::O {
                (2.4, 2.8)
            } else {
                (3.6, 4.4)
            };
            let c = m(0.0, 0.5);
            out.push(Shape::Ellipse(
                c,
                (rx + EDGE) * ws,
                (ry + EDGE) * ws,
                Role::Ink,
            ));
            out.push(Shape::Ellipse(c, rx * ws, ry * ws, Role::MouthDark));
        }
        Mouth::Flat => out.push(Shape::Stroke(
            vec![m(-3.5, 0.5), m(3.5, 0.5)],
            ink(1.3),
            Role::Ink,
        )),
        Mouth::Wavy => out.push(Shape::Stroke(
            [
                (-4.5, 0.5),
                (-2.2, -0.6),
                (0.0, 0.6),
                (2.2, -0.6),
                (4.5, 0.5),
            ]
            .into_iter()
            .map(|(u, v)| m(u, v))
            .collect(),
            ink(1.3),
            Role::Ink,
        )),
    }
}

/// The neutral closed smile around (0, 0).
fn smile() -> Vec<P> {
    (0..=8)
        .map(|i| {
            let t = -1.0 + i as f64 / 4.0;
            (4.0 * t, -0.6 + 1.4 * (1.0 - t * t))
        })
        .collect()
}

fn hand_prop(r: &Rig, out: &mut Vec<Shape>, c: P, hd: f64, retro: bool) {
    let (hc, hs) = dir(hd);
    match r.p.prop {
        Prop::Lens => {
            let lc = (c.0 + 7.0 * hs, c.1 + 7.0 * hc);
            r.wire(
                out,
                &[c, (c.0 + 2.0 * hs, c.1 + 2.0 * hc)],
                ARM,
                Role::Grey,
                retro,
            );
            let lc = r.t(lc);
            out.push(Shape::Disc(lc, 6.4 + EDGE + 1.6, Role::Ink));
            out.push(Shape::Disc(lc, 6.4 + 1.6, Role::Grey));
            out.push(Shape::Disc(lc, 6.4, Role::Blue));
            out.push(Shape::Stroke(
                arc(lc, 4.2, 200.0, 250.0, 4),
                if retro { 1.2 } else { 1.0 },
                Role::White,
            ));
        }
        Prop::Paper(_) => {
            // Pencil from the hand down to the paper.
            let tip = (c.0 - 1.0, c.1 + 6.0);
            let back = (c.0 + 5.0, c.1 - 6.0);
            r.wire(out, &[back, tip], 2.6, Role::Yellow, retro);
            out.push(Shape::Disc(r.t(tip), 1.1, Role::Ink));
        }
        _ => {}
    }
}

fn head_prop(r: &Rig, out: &mut Vec<Shape>) {
    let hd = r.t(r.head);
    let at = |dx: f64, dy: f64| (hd.0 + dx * r.ws, hd.1 + dy * r.ws);
    match r.p.prop {
        Prop::Question => {
            let mut q = arc(at(26.0, -24.0), 4.6, 180.0, 360.0 + 70.0, 12);
            q.extend([at(27.0, -16.5), at(26.0, -14.0)]);
            r.outlined(out, q, 2.4, Role::Blue);
            out.push(Shape::Disc(at(26.0, -9.0), 1.6 + EDGE, Role::Ink));
            out.push(Shape::Disc(at(26.0, -9.0), 1.6, Role::Blue));
        }
        Prop::Exclaim => {
            r.outlined(
                out,
                vec![at(25.0, -30.0), at(24.0, -17.0)],
                3.0,
                Role::Yellow,
            );
            out.push(Shape::Disc(at(23.8, -11.5), 1.8 + EDGE, Role::Ink));
            out.push(Shape::Disc(at(23.8, -11.5), 1.8, Role::Yellow));
        }
        Prop::Sparkle(ph) => {
            let spots: [(f64, f64, f64); 4] = [
                (-30.0, -14.0, 4.0),
                (28.0, -22.0, 5.0),
                (32.0, 8.0, 3.5),
                (-26.0, 14.0, 3.0),
            ];
            for (i, &(x, y, s)) in spots.iter().enumerate() {
                let s = s * if (i as u8 + ph).is_multiple_of(2) {
                    1.0
                } else {
                    0.6
                };
                let c = at(x, y);
                let star: Vec<P> = (0..8)
                    .map(|k| {
                        let rr = if k % 2 == 0 { s } else { s * 0.32 };
                        let (co, si) = dir(k as f64 * 45.0 - 90.0);
                        (c.0 + rr * co, c.1 + rr * si)
                    })
                    .collect();
                let mut closed = star.clone();
                closed.push(star[0]);
                out.push(Shape::Stroke(closed, 1.8, Role::Ink));
                out.push(Shape::Polygon(star, Role::Yellow));
            }
        }
        Prop::Zzz(n) => {
            for i in 0..n {
                let (x, y, s) = (
                    20.0 + 6.0 * i as f64,
                    -20.0 - 7.0 * i as f64,
                    2.2 + 0.8 * i as f64,
                );
                let z = vec![
                    at(x - s, y - s),
                    at(x + s, y - s),
                    at(x - s, y + s),
                    at(x + s, y + s),
                ];
                r.outlined(out, z, 1.2, Role::White);
            }
        }
        Prop::Gear(a) => {
            let c = at(27.0, -14.0);
            let teeth: Vec<P> = (0..32)
                .map(|k| {
                    let rr = if (k / 2) % 2 == 0 { 7.6 } else { 5.6 };
                    let (co, si) = dir(a + k as f64 * 11.25 - 5.6);
                    (c.0 + rr * co, c.1 + rr * si)
                })
                .collect();
            let mut closed = teeth.clone();
            closed.push(teeth[0]);
            out.push(Shape::Stroke(closed, 2.0, Role::Ink));
            out.push(Shape::Polygon(teeth, Role::Grey));
            out.push(Shape::Disc(c, 2.0, Role::Ink));
        }
        Prop::Waves => {
            let (_, _, hand, _) = r.arm(0);
            let h = r.t(hand);
            for i in 0..2 {
                let rr = 4.0 + 3.5 * i as f64;
                out.push(Shape::Stroke(
                    arc((h.0 + 1.0, h.1), rr, 140.0, 220.0, 6),
                    1.3,
                    Role::Ink,
                ));
            }
        }
        _ => {}
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

/// The mouth pictures in the cell: Agent mouth types 0..6 (acs-format 5.4: closed, f/n,
/// t/d/s, e, a, sh, o/w), then 7 = the neutral smile. Each sits on a face-coloured patch
/// that covers the smile below it.
pub fn mouth_shapes(t: u8, retro: bool) -> Vec<Shape> {
    let centre = (MOUTH[0] as f64 / 2.0, MOUTH[1] as f64 / 2.0);
    let c = (0.0, 0.0);
    let mut out = vec![Shape::Ellipse(c, 6.6, 4.6, Role::Face)];
    let ink = if retro { 1.3 } else { 1.2 };
    match t {
        7 => out.push(Shape::Stroke(
            smile()
                .into_iter()
                .map(|(u, v)| (c.0 + u, c.1 + v))
                .collect(),
            1.3,
            Role::Ink,
        )),
        0 => {
            out.push(Shape::Stroke(
                vec![(c.0 - 4.0, c.1 + 0.4), (c.0 + 4.0, c.1 + 0.4)],
                2.2,
                Role::Ink,
            ));
            out.push(Shape::Stroke(
                vec![(c.0 - 3.0, c.1 + 0.4), (c.0 + 3.0, c.1 + 0.4)],
                0.9,
                Role::MouthDark,
            ));
        }
        _ => {
            let (rx, ry) = [
                (0.0, 0.0),
                (3.8, 1.3),
                (4.2, 2.2),
                (4.4, 2.9),
                (4.6, 3.6),
                (3.2, 2.8),
                (2.2, 2.7),
            ][t as usize];
            out.push(Shape::Ellipse(c, rx + ink, ry + ink, Role::Ink));
            out.push(Shape::Ellipse(c, rx, ry, Role::MouthDark));
            if matches!(t, 2 | 5) {
                // Upper teeth.
                out.push(Shape::Ellipse(
                    (c.0, c.1 - ry * 0.6),
                    rx * 0.7,
                    ry * 0.4,
                    Role::White,
                ));
            }
            if matches!(t, 3 | 4) {
                out.push(Shape::Ellipse(
                    (c.0, c.1 + ry * 0.5),
                    rx * 0.6,
                    ry * 0.4,
                    Role::Tongue,
                ));
            }
        }
    }
    out.iter().map(|s| s.scaled(c, ZOOM, centre)).collect()
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
