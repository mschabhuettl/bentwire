//! Hermit's vector rig: a hermit crab in a spiral shell, seen from the front. The shell sits
//! behind, its spiral whorl up and its flared mouth (the lip) down at the front; the crab
//! looks out of that mouth: a round red face, two eyes on stalks (bent by the pose's curl,
//! long or short by its coil), four little legs and two claws (the arms; the right one is
//! the bigger, and a claw opens as its forearm bends up). The pose's tuck pulls the crab
//! back into the dark of the shell. Every measure is in canvas units of the 128 x 128
//! canvas, y down, before [`ZOOM`].

use crate::own::parts::{self, Body, EDGE};
use crate::own::pose::{Mouth, Pose, REST, bezier, dir};
use crate::own::raster::{P, Role, Shape};

const CX: f64 = 64.0;
const GROUND: f64 = 118.0;
/// Shell whorl: centre and radius; the spiral's centre.
const WHORL: P = (66.0, 77.0);
const WHORL_R: f64 = 24.0;
const SPIRAL: P = (71.0, 68.0);
/// The shell's mouth: centre, lip radii and the dark inside.
const APERTURE: P = (CX, 100.0);
const LIP: (f64, f64) = (23.0, 13.0);
const HOLE: (f64, f64) = (18.5, 9.5);
/// Face centre and radii.
const FACE: P = (CX, 101.0);
const FACE_RX: f64 = 14.0;
const FACE_RY: f64 = 9.5;
/// Eye stalk length (times the pose's coil), from the top of the face.
const STALK: f64 = 13.0;
/// Claw arm widths and lengths.
const UPPER_W: f64 = 4.4;
const FORE_W: f64 = 4.0;
const UPPER: f64 = 8.0;
const FORE: f64 = 7.0;
/// Width of the magnifier's handle.
const HANDLE: f64 = 2.4;
/// Mouth centre below the face centre (crab-local).
const MOUTH_DY: f64 = 1.5;
/// Mouth picture cell (mouth sheet).
pub const MOUTH: [u32; 2] = [16, 12];
/// The whole figure is drawn this much larger than the measures above.
const ZOOM: f64 = 1.2;
/// From this tuck on the crab is all inside the shell.
const GONE: f64 = 0.9;

/// Hermit's colours; the retro palette is this list in order, after the transparent index 0.
pub const ROLES: [Role; 15] = [
    Role::Ink,
    Role::ShellDark,
    Role::ShellShade,
    Role::Shell,
    Role::ShellLight,
    Role::Hollow,
    Role::CrabDark,
    Role::Crab,
    Role::CrabLight,
    Role::White,
    Role::MouthDark,
    Role::Tongue,
    Role::Yellow,
    Role::Blue,
    Role::Grey,
];

/// Placement of a pose: figure transform, how far the crab is inside, the head frame.
struct Rig {
    p: Pose,
    /// Width factor of the shell and of the crab (which shrinks as it goes inside).
    shell_ws: f64,
    ws: f64,
    /// Crab scale towards the shell's mouth.
    k: f64,
    face: P,
    /// Eye frame: centre between the eyes (crab-local) and rotation.
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
        self.t(self.c(self.hf(u, v)))
    }

    fn head(&self) -> P {
        self.t(self.c(self.head))
    }

    fn left_hand(&self) -> P {
        self.c(self.arm(0).2)
    }
}

impl Rig {
    fn new(p: &Pose) -> Rig {
        let ws = (p.sx + p.sy) / 2.0 * ZOOM;
        let k = 1.0 - 0.72 * p.tuck;
        let face = (CX + 0.4 * p.lean, FACE.1 + 0.3 * p.nod);
        Rig {
            p: *p,
            shell_ws: ws,
            ws: ws * k,
            k,
            face,
            head: (
                CX + p.lean,
                face.1 - FACE_RY + 2.0 - STALK * p.coil + 3.0 + 0.7 * p.nod,
            ),
            rot: dir(p.tilt),
        }
    }

    /// Crab-local figure coordinates to figure coordinates: towards the shell's mouth by
    /// the tuck.
    fn c(&self, q: P) -> P {
        (
            APERTURE.0 + (q.0 - APERTURE.0) * self.k,
            APERTURE.1 + 2.5 * self.p.tuck + (q.1 - APERTURE.1) * self.k,
        )
    }

    /// Crab-local to canvas.
    fn tc(&self, q: P) -> P {
        self.t(self.c(q))
    }

    /// Eye frame (origin between the eyes, tilted) to crab-local coordinates.
    fn hf(&self, u: f64, v: f64) -> P {
        let (c, s) = self.rot;
        (self.head.0 + u * c - v * s, self.head.1 + u * s + v * c)
    }

    /// Shoulder, elbow, hand and hand direction (degrees) of a claw arm, crab-local.
    fn arm(&self, side: usize) -> (P, P, P, f64) {
        let s = if side == 0 { -1.0 } else { 1.0 };
        let (a, b) = self.p.arms[side];
        let sh = (CX + s * (FACE_RX - 3.0), self.face.1 - 2.0);
        let (ca, sa) = dir(a);
        let e = (sh.0 + s * UPPER * sa, sh.1 + UPPER * ca);
        let (cb, sb) = dir(a + b);
        let hand = (e.0 + s * FORE * sb, e.1 + FORE * cb);
        (sh, e, hand, if side == 0 { -(a + b) } else { a + b })
    }

    /// An ink-edged stroke along crab-local points.
    fn limb(&self, out: &mut Vec<Shape>, pts: &[P], w: f64, core: Role) {
        let pts: Vec<P> = pts.iter().map(|&q| self.tc(q)).collect();
        out.push(Shape::Stroke(
            pts.clone(),
            (w + 2.0 * EDGE) * self.ws,
            Role::Ink,
        ));
        out.push(Shape::Stroke(pts, w * self.ws, core));
    }
}

/// An ellipse of canvas points.
fn oval(c: P, rx: f64, ry: f64, n: usize) -> Vec<P> {
    (0..n)
        .map(|i| {
            let (co, si) = dir(360.0 * i as f64 / n as f64);
            (c.0 + rx * co, c.1 + ry * si)
        })
        .collect()
}

/// The shapes of `p`, back to front. `retro` adapts line widths to hard pixels and leaves out
/// the translucent shadow.
pub fn shapes(p: &Pose, retro: bool) -> Vec<Shape> {
    let r = Rig::new(p);
    let (ws, sx, sy) = (r.shell_ws, p.sx * ZOOM, p.sy * ZOOM);
    let ink = |w: f64| if retro { w.max(1.2) } else { w };
    let mut out = Vec::new();
    out.push(Shape::Shadow(
        r.t((CX, GROUND - 1.0)),
        26.0 * sx,
        3.0 * sx,
        0.22,
    ));
    // Shell whorl: shade, the lit part up and left, a highlight, the spiral, the edge.
    let wc = r.t(WHORL);
    let whorl = oval(wc, WHORL_R * sx, WHORL_R * sy, 48);
    out.push(Shape::Polygon(whorl.clone(), Role::ShellShade));
    out.push(Shape::Polygon(
        oval(
            r.t((WHORL.0 - 2.5, WHORL.1 - 2.5)),
            (WHORL_R - 3.0) * sx,
            (WHORL_R - 3.0) * sy,
            48,
        ),
        Role::Shell,
    ));
    out.push(Shape::Polygon(
        oval(
            r.t((WHORL.0 - 12.0, WHORL.1 - 11.0)),
            4.0 * sx,
            2.6 * sy,
            16,
        ),
        Role::ShellLight,
    ));
    // The spiral: a little over two turns, its radius growing by the same factor each step
    // (no transcendental functions, see `dir`).
    let steps = 90;
    let mut rad = 1.6;
    let grow = 1.0 + 2.55 / steps as f64;
    let spiral: Vec<P> = (0..=steps)
        .map(|i| {
            let (co, si) = dir(-100.0 + 820.0 * i as f64 / steps as f64);
            let q = r.t((SPIRAL.0 + rad * co, SPIRAL.1 + rad * si));
            rad *= grow * (1.0 - 0.0105 * (i as f64 / steps as f64));
            q
        })
        .collect();
    out.push(Shape::Stroke(spiral, ink(2.0) * ws, Role::ShellDark));
    let mut edge = whorl;
    edge.push(edge[0]);
    out.push(Shape::Stroke(edge, 2.0 * EDGE * ws, Role::Ink));
    // The shell's mouth: the flared lip and the dark inside.
    let ac = r.t(APERTURE);
    let lip = oval(ac, LIP.0 * sx, LIP.1 * sy, 40);
    out.push(Shape::Polygon(lip.clone(), Role::Shell));
    out.push(Shape::Polygon(
        oval(
            (ac.0, ac.1 + 1.5 * sy),
            (LIP.0 - 1.5) * sx,
            (LIP.1 - 1.0) * sy,
            40,
        ),
        Role::ShellShade,
    ));
    let mut edge = lip;
    edge.push(edge[0]);
    out.push(Shape::Stroke(edge, 2.0 * EDGE * ws, Role::Ink));
    let hole = oval((ac.0, ac.1 - 1.0 * sy), HOLE.0 * sx, HOLE.1 * sy, 40);
    out.push(Shape::Polygon(hole.clone(), Role::Hollow));
    let mut edge = hole;
    edge.push(edge[0]);
    out.push(Shape::Stroke(edge, ink(1.2) * ws, Role::Ink));
    if p.tuck < GONE {
        crab(&r, &mut out, retro);
    }
    parts::head_prop(&r, &mut out);
    out
}

/// The crab, crab-local and scaled by the tuck.
fn crab(r: &Rig, out: &mut Vec<Shape>, retro: bool) {
    let p = &r.p;
    let (fx, fy) = r.face;
    // Legs behind the face: two a side, a knee up and a foot down to the ground.
    for s in [-1.0, 1.0] {
        for (top, knee, foot) in [
            ((8.0, 4.0), (16.5, 2.0), (20.0, 15.0)),
            ((4.0, 6.0), (10.0, 9.0), (11.0, 16.0)),
        ] {
            r.limb(
                out,
                &[
                    (fx + s * top.0, fy + top.1),
                    (fx + s * knee.0, fy + knee.1),
                    (fx + s * foot.0, fy + foot.1),
                ],
                2.6,
                Role::Crab,
            );
        }
    }
    // Eye stalks from the top of the face up to the eyes, bent by the curl.
    let bend = p.curl / 6.0;
    for side in [-1.0, 1.0] {
        let base = (fx + side * 4.0, fy - FACE_RY + 3.0);
        let eye = r.hf(side * 7.0, -3.0);
        let mid = (
            (base.0 + eye.0) / 2.0 + bend,
            (base.1 + eye.1) / 2.0 + bend.abs() * 0.3,
        );
        r.limb(out, &bezier(base, mid, mid, eye, 8), 3.0, Role::Crab);
    }
    // Face: shade below, the lit part, a highlight, the edge.
    let fc = r.tc(r.face);
    let (ex, ey) = (p.sx * ZOOM * r.k, p.sy * ZOOM * r.k);
    let face = oval(fc, FACE_RX * ex, FACE_RY * ey, 40);
    out.push(Shape::Polygon(face.clone(), Role::CrabDark));
    out.push(Shape::Polygon(
        oval(
            (fc.0, fc.1 - 1.6 * ey),
            (FACE_RX - 1.2) * ex,
            (FACE_RY - 1.6) * ey,
            40,
        ),
        Role::Crab,
    ));
    out.push(Shape::Polygon(
        oval((fc.0 - 8.5 * ex, fc.1 - 4.5 * ey), 2.6 * ex, 1.5 * ey, 12),
        Role::CrabLight,
    ));
    let mut edge = face;
    edge.push(edge[0]);
    out.push(Shape::Stroke(edge, 2.0 * EDGE * r.ws, Role::Ink));
    parts::eyes(r, out, retro, Role::Crab);
    match p.mouth {
        Mouth::Smile if p.upright() => {}
        m => parts::mouth(
            out,
            &|u, v| r.tc((fx + u, fy + MOUTH_DY + v)),
            m,
            r.ws,
            retro,
        ),
    }
    parts::paper(r, out, CX + 20.0, 94.0);
    // Claws: edges first, so that arm, palm and fingers join without lines between them.
    for side in 0..2 {
        let (s, e, hand, hd) = r.arm(side);
        let big = if side == 1 { 1.2 } else { 1.0 };
        // The wrist turns the claw a little up from the forearm.
        let hd = hd + if side == 0 { 30.0 } else { -30.0 };
        let (hc, hs) = dir(hd);
        let palm = (hand.0 + 2.5 * big * hs, hand.1 + 2.5 * big * hc);
        // The claw opens as the forearm bends up (waving snaps it).
        let gape = 10.0 + 34.0 * ((p.arms[side].1 - 30.0) / 50.0).clamp(0.0, 1.0);
        let fingers: Vec<[P; 2]> = [-gape, gape * 0.6]
            .into_iter()
            .map(|a| {
                let (fc, fs) = dir(hd + a);
                let l = 6.5 * big;
                [palm, (palm.0 + l * fs, palm.1 + l * fc)]
            })
            .collect();
        for (fill, extra) in [(Role::Ink, 2.0 * EDGE), (Role::Crab, 0.0)] {
            let w = |x: f64| (x + extra) * r.ws;
            out.push(Shape::Stroke(vec![r.tc(s), r.tc(e)], w(UPPER_W), fill));
            out.push(Shape::Stroke(vec![r.tc(e), r.tc(hand)], w(FORE_W), fill));
            out.push(Shape::Disc(
                r.tc(palm),
                (4.8 * big + extra / 2.0) * r.ws,
                fill,
            ));
            for f in &fingers {
                out.push(Shape::Stroke(
                    vec![r.tc(f[0]), r.tc(f[1])],
                    w(2.8 * big),
                    fill,
                ));
            }
        }
        out.push(Shape::Disc(
            r.tc((palm.0 - 1.2, palm.1 - 1.2)),
            1.1 * big * r.ws,
            Role::CrabLight,
        ));
        if side == 1 {
            let c = r.c((palm.0 + 5.0 * hs, palm.1 + 5.0 * hc));
            parts::hand_prop(r, out, c, hd, HANDLE, retro);
        }
    }
}

/// Top-left of the mouth picture on an upright pose (canvas units, whole pixels).
pub fn mouth_at(p: &Pose) -> (i32, i32) {
    let r = Rig::new(p);
    let c = r.tc((r.face.0, r.face.1 + MOUTH_DY));
    (
        (c.0 - MOUTH[0] as f64 / 2.0).round() as i32,
        (c.1 - MOUTH[1] as f64 / 2.0).round() as i32,
    )
}

/// The mouth pictures in the cell: [`parts::mouth_picture`] on the crab's red.
pub fn mouth_shapes(t: u8, retro: bool) -> Vec<Shape> {
    parts::mouth_picture(t, retro, Role::Crab, ZOOM, MOUTH)
}

/// The tray icon: the face and eyes in the shell's mouth, from `origin` at `k` pixels per
/// unit.
pub fn icon_view() -> (P, f64) {
    let r = Rig::new(&REST);
    let c = r.t((CX + 1.0, 81.0));
    let side = 60.0 * ZOOM;
    ((c.0 - side / 2.0, c.1 - side / 2.0), 32.0 / side)
}
