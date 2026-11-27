//! Quack's vector rig: a rubber duck seen from the front, floating on a little water. A round
//! body with a tail behind it, a round head with a feather tuft on top (the pose's curl), an
//! orange beak (the mouth) and two wings (the arms). The whole figure can rock about the
//! ground point (the pose's rock). Every measure is in canvas units of the 128 x 128 canvas,
//! y down, before [`ZOOM`].

use crate::own::parts::{self, Body, EDGE};
use crate::own::pose::{Mouth, Pose, REST, dir};
use crate::own::raster::{P, Role, Shape};

const CX: f64 = 64.0;
const GROUND: f64 = 118.0;
/// Bottom of the body, its half width and half height (times the pose's coil).
const BOTTOM: f64 = 116.0;
const BODY_RX: f64 = 25.0;
const BODY_RY: f64 = 15.0;
const HEAD_R: f64 = 17.0;
/// Wing widths and lengths.
const UPPER_W: f64 = 8.0;
const FORE_W: f64 = 6.0;
const UPPER: f64 = 11.0;
const FORE: f64 = 9.0;
/// Width of the magnifier's handle.
const HANDLE: f64 = 2.4;
/// Beak centre below the head centre (head-local).
const BEAK_Y: f64 = 8.5;
/// Mouth picture cell (mouth sheet) and the beak centre in it.
pub const MOUTH: [u32; 2] = [26, 20];
const MOUTH_C: P = (13.0, 7.0);
/// The whole figure is drawn this much larger than the measures above.
const ZOOM: f64 = 1.3;

/// Quack's colours; the retro palette is this list in order, after the transparent index 0.
pub const ROLES: [Role; 12] = [
    Role::Ink,
    Role::DuckShade,
    Role::Duck,
    Role::DuckLight,
    Role::Beak,
    Role::BeakDark,
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
    ws: f64,
    /// Body centre and half height.
    body: P,
    ry: f64,
    head: P,
    rot: (f64, f64),
    rock: (f64, f64),
}

impl Body for Rig {
    fn pose(&self) -> &Pose {
        &self.p
    }

    fn ws(&self) -> f64 {
        self.ws
    }

    fn t(&self, q: P) -> P {
        let (x, y) = (
            (q.0 - CX) * self.p.sx * ZOOM,
            (q.1 - GROUND) * self.p.sy * ZOOM,
        );
        let (c, s) = self.rock;
        (
            CX + x * c - y * s + self.p.dx,
            GROUND + x * s + y * c + self.p.dy,
        )
    }

    fn h(&self, u: f64, v: f64) -> P {
        let (c, s) = self.rot;
        self.t((self.head.0 + u * c - v * s, self.head.1 + u * s + v * c))
    }

    fn head(&self) -> P {
        self.t(self.head)
    }

    fn left_hand(&self) -> P {
        self.wing(0).2
    }
}

impl Rig {
    fn new(p: &Pose) -> Rig {
        let ry = BODY_RY * p.coil;
        let body = (CX, BOTTOM - ry);
        Rig {
            p: *p,
            ws: (p.sx + p.sy) / 2.0 * ZOOM,
            body,
            ry,
            head: (CX + p.lean, body.1 - ry - HEAD_R * 0.45 + p.nod),
            rot: dir(p.tilt),
            rock: dir(p.rock),
        }
    }

    /// Points of an ellipse (figure coordinates) from angle `a0` to `a1`, as [`arc`].
    fn oval(&self, c: P, rx: f64, ry: f64, a0: f64, a1: f64, n: usize) -> Vec<P> {
        (0..=n)
            .map(|i| {
                let (co, si) = dir(a0 + (a1 - a0) * i as f64 / n as f64);
                self.t((c.0 + rx * co, c.1 + ry * si))
            })
            .collect()
    }

    /// Shoulder, elbow, hand and hand direction (degrees) of a wing, figure coordinates.
    fn wing(&self, side: usize) -> (P, P, P, f64) {
        let s = if side == 0 { -1.0 } else { 1.0 };
        let (a, b) = self.p.arms[side];
        let sh = (CX + s * (BODY_RX - 13.0), self.body.1 - self.ry * 0.25);
        let (ca, sa) = dir(a);
        let e = (sh.0 + s * UPPER * sa, sh.1 + UPPER * ca);
        let (cb, sb) = dir(a + b);
        let hand = (e.0 + s * FORE * sb, e.1 + FORE * cb);
        (sh, e, hand, if side == 0 { -(a + b) } else { a + b })
    }
}

/// A filled outline: `pts` filled with `fill`, edged in ink.
fn filled(out: &mut Vec<Shape>, pts: Vec<P>, fill: Role, w: f64) {
    let mut closed = pts.clone();
    closed.push(pts[0]);
    out.push(Shape::Polygon(pts, fill));
    out.push(Shape::Stroke(closed, w, Role::Ink));
}

/// The shapes of `p`, back to front. `retro` adapts line widths to hard pixels and leaves out
/// the translucent shadow.
pub fn shapes(p: &Pose, retro: bool) -> Vec<Shape> {
    let r = Rig::new(p);
    let ws = r.ws;
    let edge = 2.0 * EDGE * ws;
    let mut out = Vec::new();
    // Shadow and water stay level when the duck rocks.
    let level = |q: P| {
        (
            CX + (q.0 - CX) * p.sx * ZOOM + p.dx,
            GROUND + (q.1 - GROUND) * p.sy * ZOOM + p.dy,
        )
    };
    out.push(Shape::Shadow(
        level((CX, GROUND - 1.0)),
        24.0 * ZOOM * p.sx,
        3.0 * ZOOM * p.sx,
        0.22,
    ));
    // Tail, behind the body on the screen right.
    let (bx, by) = r.body;
    let tail: Vec<P> = [
        (bx + 12.0, by - r.ry * 0.5),
        (bx + 20.0, by - r.ry * 0.9),
        (bx + 26.0, by - r.ry - 2.0),
        (bx + 30.0, by - r.ry - 3.0),
        (bx + 29.0, by - r.ry * 0.6),
        (bx + 20.0, by - 2.0),
    ]
    .into_iter()
    .map(|q| r.t(q))
    .collect();
    filled(&mut out, tail, Role::Duck, edge);
    // Body: shade, the lit part up and left, a highlight, the edge.
    let body = r.oval(r.body, BODY_RX, r.ry, 0.0, 360.0, 40);
    out.push(Shape::Polygon(body.clone(), Role::DuckShade));
    out.push(Shape::Polygon(
        r.oval(
            (bx - 2.0, by - 2.0),
            BODY_RX - 2.5,
            r.ry - 2.0,
            0.0,
            360.0,
            40,
        ),
        Role::Duck,
    ));
    out.push(Shape::Polygon(
        r.oval((bx - 13.0, by - r.ry * 0.35), 4.5, 2.0, 0.0, 360.0, 16),
        Role::DuckLight,
    ));
    out.push(Shape::Stroke(body, edge, Role::Ink));
    // Feather tuft on top, its base under the head.
    let (cc, cs) = dir(p.curl);
    for tuft in [
        &[(0.0, 0.0), (0.4, -3.0), (1.8, -5.6), (4.4, -6.8)][..],
        &[(-1.0, 0.0), (-1.6, -3.2), (-3.6, -4.8)][..],
    ] {
        let pts: Vec<P> = tuft
            .iter()
            .map(|&(u, v)| r.h(u * cc - v * cs, u * cs + v * cc - HEAD_R + 2.0))
            .collect();
        out.push(Shape::Stroke(
            pts.clone(),
            (3.0 + 2.0 * EDGE) * ws,
            Role::Ink,
        ));
        out.push(Shape::Stroke(pts, 3.0 * ws, Role::Duck));
    }
    // Head: shade at the lower right, the lit disc, the edge.
    let hc = r.h(0.0, 0.0);
    out.push(Shape::Disc(hc, (HEAD_R + EDGE) * ws, Role::Ink));
    out.push(Shape::Disc(hc, HEAD_R * ws, Role::DuckShade));
    out.push(Shape::Disc(
        r.h(-1.6, -1.6),
        (HEAD_R - 1.4) * ws,
        Role::Duck,
    ));
    parts::eyes(&r, &mut out, retro, Role::Duck);
    match p.mouth {
        Mouth::Smile if p.upright() => {}
        m => beak(
            &mut out,
            &|u, v| r.h(u, BEAK_Y + v),
            ws,
            retro,
            expression(m),
        ),
    }
    parts::paper(&r, &mut out, CX + 20.0, 94.0);
    // Wings: edges first, so that the feathers of a wing join without lines between them.
    for side in 0..2 {
        let (s, e, hand, hd) = r.wing(side);
        let feathers: Vec<[P; 2]> = [-32.0, 0.0, 32.0]
            .into_iter()
            .map(|a| {
                let (fc, fs) = dir(hd + a);
                [hand, (hand.0 + 3.4 * fs, hand.1 + 3.4 * fc)]
            })
            .collect();
        for (fill, extra) in [(Role::Ink, 2.0 * EDGE), (Role::Duck, 0.0)] {
            out.push(Shape::Stroke(
                vec![r.t(s), r.t(e)],
                (UPPER_W + extra) * ws,
                fill,
            ));
            out.push(Shape::Stroke(
                vec![r.t(e), r.t(hand)],
                (FORE_W + extra) * ws,
                fill,
            ));
            for f in &feathers {
                out.push(Shape::Stroke(
                    vec![r.t(f[0]), r.t(f[1])],
                    (3.0 + extra) * ws,
                    fill,
                ));
            }
        }
        if side == 1 {
            let (hc, hs) = dir(hd);
            let c = (hand.0 + 4.0 * hs, hand.1 + 4.0 * hc);
            parts::hand_prop(&r, &mut out, c, hd, HANDLE, retro);
        }
    }
    // Water: a wavy line in front of the bottom of the body.
    let water: Vec<P> = (0..=24)
        .map(|i| {
            let x = -32.0 + 64.0 * i as f64 / 24.0;
            let (_, s) = dir(x * 22.5);
            level((CX + x, GROUND - 2.0 + 0.9 * s))
        })
        .collect();
    out.push(Shape::Stroke(water.clone(), 3.4 * ws, Role::Ink));
    out.push(Shape::Stroke(water, 1.4 * ws, Role::Blue));
    parts::head_prop(&r, &mut out);
    out
}

/// How a beak is opened and shaped.
#[derive(Clone, Copy)]
struct Beak {
    /// Lower half this far down.
    open: f64,
    /// Width factor.
    wide: f64,
    /// The split line's sag in the middle: positive smiles, negative frowns.
    smile: f64,
    wavy: bool,
    tongue: bool,
}

const CLOSED: Beak = Beak {
    open: 0.0,
    wide: 1.0,
    smile: 1.2,
    wavy: false,
    tongue: false,
};

fn expression(m: Mouth) -> Beak {
    match m {
        Mouth::Smile => CLOSED,
        Mouth::Grin => Beak {
            open: 3.0,
            wide: 1.06,
            smile: 1.8,
            tongue: true,
            ..CLOSED
        },
        Mouth::Frown => Beak {
            smile: -1.2,
            ..CLOSED
        },
        Mouth::O => Beak {
            open: 3.4,
            wide: 0.78,
            smile: 0.3,
            ..CLOSED
        },
        Mouth::Flat => Beak {
            smile: 0.0,
            ..CLOSED
        },
        Mouth::Wavy => Beak {
            smile: 0.0,
            wavy: true,
            ..CLOSED
        },
        Mouth::Yawn => Beak {
            open: 6.0,
            wide: 0.9,
            smile: 0.4,
            tongue: true,
            ..CLOSED
        },
    }
}

/// A beak around beak-local (0, 0), mapped by `m`; widths times `ws`.
fn beak(out: &mut Vec<Shape>, m: &dyn Fn(f64, f64) -> P, ws: f64, retro: bool, b: Beak) {
    let ink = if retro { 1.3 } else { 1.1 } * ws;
    let ell = |c: P, rx: f64, ry: f64, a0: f64, a1: f64, n: usize| -> Vec<P> {
        (0..=n)
            .map(|i| {
                let (co, si) = dir(a0 + (a1 - a0) * i as f64 / n as f64);
                m(c.0 + rx * co, c.1 + ry * si)
            })
            .collect()
    };
    let rx = 8.5 * b.wide;
    // The split line from the left corner to the right one.
    let split: Vec<(f64, f64)> = (0..=12)
        .map(|i| {
            let t = -1.0 + i as f64 / 6.0;
            let wave = if b.wavy { 0.7 * dir(t * 540.0).1 } else { 0.0 };
            (rx * t, b.smile * (1.0 - t * t) + wave)
        })
        .collect();
    let jaw = 1.4 + b.open;
    let jaw_rx = 6.2 * b.wide;
    if b.open > 0.0 {
        // The opening, down to the top of the lower half.
        let mut inside: Vec<P> = split.iter().map(|&(u, v)| m(u * 0.9, v)).collect();
        inside.extend(ell((0.0, jaw), jaw_rx, 2.4, 0.0, -180.0, 12));
        out.push(Shape::Polygon(inside, Role::MouthDark));
        if b.tongue {
            out.push(Shape::Polygon(
                ell((0.0, jaw - 1.2), 3.2 * b.wide, 1.3, 0.0, 360.0, 12),
                Role::Tongue,
            ));
        }
    }
    filled(
        out,
        ell((0.0, jaw), jaw_rx, 2.4, 0.0, 360.0, 20),
        Role::BeakDark,
        ink,
    );
    let mut upper = ell((0.0, 0.0), rx, 3.4, 180.0, 360.0, 12);
    upper.extend(split.iter().rev().map(|&(u, v)| m(u, v)));
    filled(out, upper, Role::Beak, ink);
}

/// Top-left of the mouth picture on an upright pose (canvas units, whole pixels).
pub fn mouth_at(p: &Pose) -> (i32, i32) {
    let c = Rig::new(p).h(0.0, BEAK_Y);
    (
        (c.0 - MOUTH_C.0).round() as i32,
        (c.1 - MOUTH_C.1).round() as i32,
    )
}

/// The mouth pictures in the cell: Agent mouth types 0..6 (acs-format 5.4: closed, f/n,
/// t/d/s, e, a, sh, o/w), then 7 = the closed beak. Each sits on a duck-coloured patch that
/// covers the closed beak below it.
pub fn mouth_shapes(t: u8, retro: bool) -> Vec<Shape> {
    let mut out = vec![Shape::Ellipse((0.0, 0.2), 9.9, 5.2, Role::Duck)];
    let (open, wide, smile) = [
        (0.0, 1.0, 0.3),
        (1.0, 1.0, 0.6),
        (2.0, 1.0, 0.8),
        (3.0, 1.0, 1.0),
        (4.4, 0.98, 1.0),
        (2.6, 0.84, 0.5),
        (3.4, 0.7, 0.3),
        (0.0, 1.0, 1.2),
    ][t as usize];
    let b = Beak {
        open,
        wide,
        smile,
        tongue: matches!(t, 3 | 4),
        ..CLOSED
    };
    beak(&mut out, &|u, v| (u, v), 1.0, retro, b);
    out.iter()
        .map(|s| s.scaled((0.0, 0.0), ZOOM, MOUTH_C))
        .collect()
}

/// The tray icon: the head, from `origin` at `k` pixels per unit.
pub fn icon_view() -> (P, f64) {
    let r = Rig::new(&REST);
    let head = r.t(r.head);
    let side = 2.0 * (HEAD_R + 6.0) * ZOOM;
    (
        (head.0 - side / 2.0, head.1 - side / 2.0 - 2.0),
        32.0 / side,
    )
}
