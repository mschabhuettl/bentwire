//! Parts every own character draws the same way: eyes and brows, and the props of the
//! animation table (question mark, sparkles, gear, magnifier, paper and pencil, ...).

use super::pose::{Brows, Eyes, Pose, Prop, arc, dir};
use super::raster::{P, Role, Shape};

/// The dark edge on each side of an outlined stroke.
pub const EDGE: f64 = 1.0;

/// What the shared parts need of a rig placed in a pose.
pub trait Body {
    fn pose(&self) -> &Pose;
    /// Width factor (strokes scale with the figure).
    fn ws(&self) -> f64;
    /// Figure coordinates to canvas.
    fn t(&self, q: P) -> P;
    /// Head-local coordinates (origin at the head centre, tilted) to canvas.
    fn h(&self, u: f64, v: f64) -> P;
    /// Head centre, canvas.
    fn head(&self) -> P;
    /// The screen-left hand, figure coordinates.
    fn left_hand(&self) -> P;
}

/// An ink-edged stroke along figure points, `w` wide before the figure's scale.
pub fn stick(b: &impl Body, out: &mut Vec<Shape>, pts: &[P], w: f64, core: Role) {
    let pts: Vec<P> = pts.iter().map(|&q| b.t(q)).collect();
    let w = w * b.ws();
    out.push(Shape::Stroke(
        pts.clone(),
        w + 2.0 * EDGE * b.ws(),
        Role::Ink,
    ));
    out.push(Shape::Stroke(pts, w, core));
}

/// Ink-outlined stroke of `fill` along canvas points (props).
fn outlined(out: &mut Vec<Shape>, pts: Vec<P>, w: f64, fill: Role) {
    out.push(Shape::Stroke(pts.clone(), w + 2.0 * EDGE, Role::Ink));
    out.push(Shape::Stroke(pts, w, fill));
}

/// Eyes and brows on the head; `skin` is the colour of a half-closed lid.
pub fn eyes(b: &impl Body, out: &mut Vec<Shape>, retro: bool, skin: Role) {
    let p = b.pose();
    let ws = b.ws();
    let ink = |w: f64| if retro { w.max(1.2) } else { w } * ws;
    for (i, side) in [-1.0, 1.0].into_iter().enumerate() {
        let e = b.h(side * 7.0, -3.0);
        match p.eyes {
            Eyes::Closed => {
                let l = arc((side * 7.0, -4.5), 4.0, 30.0, 150.0, 8);
                out.push(Shape::Stroke(
                    l.into_iter().map(|(u, v)| b.h(u, v)).collect(),
                    ink(1.4),
                    Role::Ink,
                ));
            }
            Eyes::Happy => {
                let l = arc((side * 7.0, -1.5), 4.0, 210.0, 330.0, 8);
                out.push(Shape::Stroke(
                    l.into_iter().map(|(u, v)| b.h(u, v)).collect(),
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
                let pc = b.h(side * 7.0 + p.look.0 * reach, -3.0 + p.look.1 * reach);
                out.push(Shape::Disc(pc, pr * ws, Role::Ink));
                if p.eyes == Eyes::Half {
                    // Upper lid down to the middle.
                    let lid: Vec<P> = arc((side * 7.0, -3.0), rad + EDGE + 0.4, 180.0, 360.0, 10)
                        .into_iter()
                        .chain([
                            (side * 7.0 + rad + 1.0, -2.6),
                            (side * 7.0 - rad - 1.0, -2.6),
                        ])
                        .map(|(u, v)| b.h(u, v))
                        .collect();
                    out.push(Shape::Polygon(lid, skin));
                    out.push(Shape::Stroke(
                        vec![b.h(side * 7.0 - rad, -2.6), b.h(side * 7.0 + rad, -2.6)],
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
            vec![b.h(side * 3.8, inner), b.h(side * 10.0, outer)],
            ink(1.6),
            Role::Ink,
        ));
    }
}

/// The paper of `Prop::Paper` with its top-left corner at figure point (x0, y0).
pub fn paper(b: &impl Body, out: &mut Vec<Shape>, x0: f64, y0: f64) {
    let Prop::Paper(lines) = b.pose().prop else {
        return;
    };
    let tp = |pts: &[P]| -> Vec<P> { pts.iter().map(|&q| b.t(q)).collect() };
    let sheet = vec![
        (x0, y0),
        (x0 + 18.0, y0 - 2.0),
        (x0 + 20.0, y0 + 20.0),
        (x0 + 2.0, y0 + 22.0),
    ];
    let sheet = tp(&sheet);
    out.push(Shape::Polygon(sheet.clone(), Role::White));
    let mut closed = sheet.clone();
    closed.push(sheet[0]);
    out.push(Shape::Stroke(closed, EDGE * 1.2, Role::Ink));
    for i in 0..lines {
        let y = y0 + 4.0 + 4.0 * i as f64;
        let l = tp(&[(x0 + 4.0, y), (x0 + 15.0, y - 1.2)]);
        out.push(Shape::Stroke(l, 1.1, Role::Grey));
    }
}

/// What the screen-right hand holds: `c` is the hand (figure coordinates), `hd` the hand's
/// direction in degrees; `w` the width of the magnifier's handle.
pub fn hand_prop(b: &impl Body, out: &mut Vec<Shape>, c: P, hd: f64, w: f64, retro: bool) {
    let (hc, hs) = dir(hd);
    match b.pose().prop {
        Prop::Lens => {
            let lc = (c.0 + 7.0 * hs, c.1 + 7.0 * hc);
            stick(
                b,
                out,
                &[c, (c.0 + 2.0 * hs, c.1 + 2.0 * hc)],
                w,
                Role::Grey,
            );
            let lc = b.t(lc);
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
            stick(b, out, &[back, tip], 2.6, Role::Yellow);
            out.push(Shape::Disc(b.t(tip), 1.1, Role::Ink));
        }
        _ => {}
    }
}

/// Props around the head (and the sound arcs at the screen-left hand).
pub fn head_prop(b: &impl Body, out: &mut Vec<Shape>) {
    let hd = b.head();
    let ws = b.ws();
    let at = |dx: f64, dy: f64| (hd.0 + dx * ws, hd.1 + dy * ws);
    match b.pose().prop {
        Prop::Question => {
            let mut q = arc(at(26.0, -24.0), 4.6, 180.0, 360.0 + 70.0, 12);
            q.extend([at(27.0, -16.5), at(26.0, -14.0)]);
            outlined(out, q, 2.4, Role::Blue);
            out.push(Shape::Disc(at(26.0, -9.0), 1.6 + EDGE, Role::Ink));
            out.push(Shape::Disc(at(26.0, -9.0), 1.6, Role::Blue));
        }
        Prop::Exclaim => {
            outlined(
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
                outlined(out, z, 1.2, Role::White);
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
            let h = b.t(b.left_hand());
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
