//! The pose of an own character's rig, shared by all of them so that one animation table
//! drives every character, and the arithmetic their rigs share. Measures are canvas units of
//! the 128 x 128 canvas, y down.
//!
//! Left and right in poses are screen sides. Agent names mean the character's own side
//! (GestureRight points to screen left, R-GEST-1); the animation table maps them.

use super::raster::P;
use std::f64::consts::PI;

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

pub fn bezier(p0: P, p1: P, p2: P, p3: P, n: usize) -> Vec<P> {
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
pub fn arc(c: P, r: f64, a0: f64, a1: f64, n: usize) -> Vec<P> {
    (0..=n)
        .map(|i| {
            let (co, si) = dir(a0 + (a1 - a0) * i as f64 / n as f64);
            (c.0 + r * co, c.1 + r * si)
        })
        .collect()
}
