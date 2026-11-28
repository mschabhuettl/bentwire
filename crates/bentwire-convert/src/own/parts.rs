//! Parts every own character draws the same way: eyes and brows, a drawn mouth (Wire,
//! Hermit), and the props of the animation table (question mark, sparkles, gear, magnifier, paper and pencil, ...).

use super::pose::{Brows, Eyes, Mouth, Pose, Prop, arc, dir};
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

/// A drawn mouth of expression `mouth` around mouth-local (0, 0), mapped by `m`; widths times
/// `ws`. The closed smile is here for poses that are not upright (upright ones take the
/// mouth layer of [`mouth_picture`]).
pub fn mouth(out: &mut Vec<Shape>, m: &dyn Fn(f64, f64) -> P, mouth: Mouth, ws: f64, retro: bool) {
    let ink = |w: f64| if retro { w.max(1.2) } else { w } * ws;
    match mouth {
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
            let (rx, ry) = if mouth == Mouth::O {
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

/// The drawn mouth pictures in a `cell` (mouth sheet) at `zoom`: Agent mouth types 0..6
/// (acs-format 5.4: closed, f/n, t/d/s, e, a, sh, o/w), then 7 = the neutral smile. Each
/// sits on a `skin`-coloured patch that covers the smile below it.
pub fn mouth_picture(t: u8, retro: bool, skin: Role, zoom: f64, cell: [u32; 2]) -> Vec<Shape> {
    let centre = (cell[0] as f64 / 2.0, cell[1] as f64 / 2.0);
    let c = (0.0, 0.0);
    let mut out = vec![Shape::Ellipse(c, 6.6, 4.6, skin)];
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
    out.iter().map(|s| s.scaled(c, zoom, centre)).collect()
}
