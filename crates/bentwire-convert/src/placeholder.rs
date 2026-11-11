//! The CC0 placeholder character `bentwire-placeholder` (architecture A-CHAR-1), drawn by code:
//! a stylised paper clip with a small face. Every Agent state, the Office names of the spec
//! 3.12 fallback table, mouth overlays, real alpha and a 2x hidpi sheet. Deterministic: only
//! exact arithmetic and `sqrt` (correctly rounded), no trigonometry.

use bentwire_character::write::{encode_flac, encode_png_rgba};
use bentwire_character::*;

use crate::Error;

/// Character canvas (and display) size.
pub const SIZE: u32 = 96;
const MOUTH: [u32; 2] = [14, 10];

/// Rotations as exact (cos, sin) pairs.
const ROT: [(f64, f64); 8] = [
    (1.0, 0.0),
    (0.8, 0.6),
    (0.0, 1.0),
    (-0.8, 0.6),
    (-1.0, 0.0),
    (-0.8, -0.6),
    (0.0, -1.0),
    (0.8, -0.6),
];
const TILT_L: (f64, f64) = (0.96, -0.28);
const TILT_R: (f64, f64) = (0.96, 0.28);

#[derive(Clone, Copy, PartialEq)]
enum Eyes {
    Open,
    Closed,
    Wide,
}

#[derive(Clone, Copy, PartialEq)]
enum Extra {
    None,
    Zzz,
    Question,
    Sparkle,
    Pointer(i32, i32),
}

/// One drawn pose.
#[derive(Clone, Copy)]
struct Pose {
    rot: (f64, f64),
    /// Offset in canvas px.
    dx: f64,
    dy: f64,
    /// Overall scale (Show/Hide).
    scale: f64,
    /// Vertical squash (1 = none).
    squash: f64,
    /// Pupil direction, -1..1.
    look: (f64, f64),
    eyes: Eyes,
    extra: Extra,
}

const REST: Pose = Pose {
    rot: (1.0, 0.0),
    dx: 0.0,
    dy: 0.0,
    scale: 1.0,
    squash: 1.0,
    look: (0.0, 0.0),
    eyes: Eyes::Open,
    extra: Extra::None,
};

/// Anti-aliased RGBA canvas at `k` times the canvas size.
struct Canvas {
    w: usize,
    h: usize,
    k: f64,
    px: Vec<[f64; 4]>,
}

impl Canvas {
    fn new(w: u32, h: u32, k: u32) -> Self {
        Canvas {
            w: (w * k) as usize,
            h: (h * k) as usize,
            k: k as f64,
            px: vec![[0.0; 4]; (w * k * h * k) as usize],
        }
    }

    /// Source-over a colour with coverage `a` (0..1) at pixel (x, y).
    fn put(&mut self, x: usize, y: usize, c: [f64; 3], a: f64) {
        if a <= 0.0 {
            return;
        }
        let d = &mut self.px[y * self.w + x];
        let oa = a + d[3] * (1.0 - a);
        for i in 0..3 {
            d[i] = (c[i] * a + d[i] * d[3] * (1.0 - a)) / oa;
        }
        d[3] = oa;
    }

    /// Fill by coverage function f(x, y) -> signed distance (px, negative inside), in canvas
    /// units; 1 output px of antialiasing.
    fn fill(
        &mut self,
        bbox: (f64, f64, f64, f64),
        c: [f64; 3],
        alpha: f64,
        f: impl Fn(f64, f64) -> f64,
    ) {
        let k = self.k;
        let x0 = ((bbox.0 * k).floor().max(0.0)) as usize;
        let y0 = ((bbox.1 * k).floor().max(0.0)) as usize;
        let x1 = ((bbox.2 * k).ceil().min(self.w as f64)) as usize;
        let y1 = ((bbox.3 * k).ceil().min(self.h as f64)) as usize;
        for y in y0..y1 {
            for x in x0..x1 {
                let (cx, cy) = ((x as f64 + 0.5) / k, (y as f64 + 0.5) / k);
                let d = f(cx, cy) * k;
                let cov = (0.5 - d).clamp(0.0, 1.0);
                self.put(x, y, c, cov * alpha);
            }
        }
    }

    fn disc(&mut self, cx: f64, cy: f64, r: f64, c: [f64; 3], alpha: f64) {
        self.fill(
            (cx - r - 1.0, cy - r - 1.0, cx + r + 1.0, cy + r + 1.0),
            c,
            alpha,
            |x, y| ((x - cx) * (x - cx) + (y - cy) * (y - cy)).sqrt() - r,
        );
    }

    fn ellipse(&mut self, cx: f64, cy: f64, rx: f64, ry: f64, c: [f64; 3], alpha: f64) {
        self.fill(
            (cx - rx - 1.0, cy - ry - 1.0, cx + rx + 1.0, cy + ry + 1.0),
            c,
            alpha,
            |x, y| {
                let (u, v) = ((x - cx) / rx, (y - cy) / ry);
                ((u * u + v * v).sqrt() - 1.0) * rx.min(ry)
            },
        );
    }

    fn stroke(&mut self, pts: &[(f64, f64)], width: f64, c: [f64; 3]) {
        let r = width / 2.0;
        let (mut bx0, mut by0, mut bx1, mut by1) = (f64::MAX, f64::MAX, f64::MIN, f64::MIN);
        for &(x, y) in pts {
            bx0 = bx0.min(x);
            by0 = by0.min(y);
            bx1 = bx1.max(x);
            by1 = by1.max(y);
        }
        self.fill(
            (bx0 - r - 1.0, by0 - r - 1.0, bx1 + r + 1.0, by1 + r + 1.0),
            c,
            1.0,
            |x, y| {
                let mut best = f64::MAX;
                for s in pts.windows(2) {
                    best = best.min(seg_dist((x, y), s[0], s[1]));
                }
                best - r
            },
        );
    }

    fn rgba(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for p in &self.px {
            let a = (p[3] * 255.0).round() as u8;
            if a == 0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                for v in &p[..3] {
                    out.push((v * 255.0).round().clamp(0.0, 255.0) as u8);
                }
                out.push(a);
            }
        }
        out
    }
}

fn seg_dist(p: (f64, f64), a: (f64, f64), b: (f64, f64)) -> f64 {
    let (vx, vy) = (b.0 - a.0, b.1 - a.1);
    let l2 = vx * vx + vy * vy;
    let t = if l2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * vx + (p.1 - a.1) * vy) / l2).clamp(0.0, 1.0)
    };
    let (dx, dy) = (p.0 - a.0 - t * vx, p.1 - a.1 - t * vy);
    (dx * dx + dy * dy).sqrt()
}

/// Half circle through `steps` points, from angle 180 to 0 degrees (top half when `up`).
fn arc(cx: f64, cy: f64, r: f64, up: bool, steps: usize) -> Vec<(f64, f64)> {
    (0..=steps)
        .map(|i| {
            let t = -1.0 + 2.0 * i as f64 / steps as f64;
            let h = (1.0 - t * t).max(0.0).sqrt() * r;
            (cx + t * r, if up { cy - h } else { cy + h })
        })
        .collect()
}

const WIRE: [f64; 3] = [0.55, 0.60, 0.68];
const WIRE_HI: [f64; 3] = [0.85, 0.88, 0.93];
const INK: [f64; 3] = [0.08, 0.08, 0.10];
const WHITE: [f64; 3] = [1.0, 1.0, 1.0];
const FACE: [f64; 3] = [0.97, 0.94, 0.80];
const LIPS: [f64; 3] = [0.55, 0.10, 0.12];

/// The paper clip outline in local coordinates (origin at the clip centre, y down).
fn clip_path() -> Vec<(f64, f64)> {
    // Outer loop: left leg up, top arc, right leg down, bottom arc, inner left leg up, inner
    // top arc, inner right leg down to the end.
    let mut p = vec![(-9.0, 18.0), (-9.0, -16.0)];
    p.extend(arc(0.0, -16.0, 9.0, true, 12).into_iter().skip(1));
    p.push((9.0, 22.0));
    p.extend(arc(3.0, 22.0, 6.0, false, 8).into_iter().rev().skip(1));
    p.push((-3.0, -10.0));
    p.extend(arc(1.0, -10.0, 4.0, true, 8).into_iter().skip(1));
    p.push((5.0, 14.0));
    p
}

fn draw(p: &Pose, k: u32) -> Canvas {
    let mut cv = Canvas::new(SIZE, SIZE, k);
    let (c, s) = p.rot;
    let (ox, oy) = (48.0 + p.dx, 52.0 + p.dy);
    let tf = |x: f64, y: f64| -> (f64, f64) {
        let (x, y) = (x * p.scale, y * p.scale * p.squash);
        (ox + x * c - y * s, oy + x * s + y * c)
    };
    // Shadow (real partial alpha).
    let sh = 14.0 * p.scale;
    cv.ellipse(48.0 + p.dx, 86.0, sh, 3.0 * p.scale.max(0.3), INK, 0.25);
    let path: Vec<(f64, f64)> = clip_path().iter().map(|&(x, y)| tf(x, y)).collect();
    cv.stroke(&path, 4.0 * p.scale, WIRE);
    cv.stroke(&path, 1.5 * p.scale, WIRE_HI);
    if let Extra::Pointer(px, py) = p.extra {
        let a = tf(9.0, 0.0);
        let b = (a.0 + px as f64 * 16.0, a.1 + py as f64 * 16.0);
        cv.stroke(&[a, b], 3.0, WIRE);
        cv.disc(b.0, b.1, 2.5, WIRE_HI, 1.0);
    }
    if p.scale >= 0.75 {
        // Face plate with eyes; the mouth sits at MOUTH_AT and is a separate layer.
        let f = tf(0.0, -6.0);
        cv.ellipse(f.0, f.1, 13.0 * p.scale, 10.0 * p.scale, FACE, 1.0);
        for side in [-1.0, 1.0] {
            let e = tf(side * 5.5, -9.0);
            match p.eyes {
                Eyes::Closed => cv.stroke(&[(e.0 - 3.0, e.1), (e.0 + 3.0, e.1)], 1.5, INK),
                Eyes::Open | Eyes::Wide => {
                    let r = if p.eyes == Eyes::Wide { 4.2 } else { 3.4 };
                    cv.disc(e.0, e.1, r, WHITE, 1.0);
                    cv.disc(e.0 + p.look.0 * 1.6, e.1 + p.look.1 * 1.6, 1.6, INK, 1.0);
                }
            }
        }
        if p.extra == Extra::None || matches!(p.extra, Extra::Pointer(..)) {
            // Closed mouth line, drawn into the base picture.
            let m = tf(0.0, -1.0);
            cv.stroke(&[(m.0 - 3.5, m.1), (m.0 + 3.5, m.1)], 1.2, LIPS);
        }
    }
    match p.extra {
        Extra::Zzz => {
            for (i, (x, y)) in [(70.0, 22.0), (77.0, 14.0), (84.0, 7.0)]
                .into_iter()
                .enumerate()
            {
                let r = 2.0 + i as f64;
                cv.stroke(
                    &[
                        (x - r, y - r),
                        (x + r, y - r),
                        (x - r, y + r),
                        (x + r, y + r),
                    ],
                    1.2,
                    INK,
                );
            }
        }
        Extra::Question => {
            let mut q = arc(76.0, 14.0, 5.0, true, 8);
            q.extend([(81.0, 14.0), (76.0, 20.0), (76.0, 24.0)]);
            cv.stroke(&q, 2.0, INK);
            cv.disc(76.0, 29.0, 1.4, INK, 1.0);
        }
        Extra::Sparkle => {
            for (x, y) in [(18.0, 16.0), (78.0, 12.0), (82.0, 40.0)] {
                cv.stroke(&[(x - 4.0, y), (x + 4.0, y)], 1.3, [0.95, 0.75, 0.10]);
                cv.stroke(&[(x, y - 4.0), (x, y + 4.0)], 1.3, [0.95, 0.75, 0.10]);
            }
        }
        _ => {}
    }
    cv
}

/// Mouth position (top-left of the mouth picture) for an upright rest-like pose.
fn mouth_at(p: &Pose) -> (i32, i32) {
    let (c, s) = p.rot;
    let (x, y) = (0.0, -1.0);
    let (mx, my) = (48.0 + p.dx + x * c - y * s, 52.0 + p.dy + x * s + y * c);
    (
        (mx - MOUTH[0] as f64 / 2.0).round() as i32,
        (my - MOUTH[1] as f64 / 2.0).round() as i32,
    )
}

/// Mouth picture for Agent mouth type `t` (0..6, acs-format 5.4) at density `k`.
fn mouth(t: u8, k: u32) -> Canvas {
    let mut cv = Canvas::new(MOUTH[0], MOUTH[1], k);
    let open = [0.8, 2.0, 3.0, 4.2, 3.2, 2.4, 1.6][t as usize];
    let wide = [4.5, 5.0, 5.5, 5.0, 4.0, 3.2, 6.0][t as usize];
    let (cx, cy) = (MOUTH[0] as f64 / 2.0, MOUTH[1] as f64 / 2.0);
    cv.ellipse(cx, cy, wide + 1.0, open + 1.0, FACE, 1.0);
    cv.ellipse(cx, cy, wide, open, LIPS, 1.0);
    if open > 2.0 {
        cv.ellipse(
            cx,
            cy + open * 0.35,
            wide * 0.55,
            open * 0.4,
            [0.85, 0.35, 0.35],
            1.0,
        );
    }
    cv
}

struct Anim {
    name: &'static str,
    transition: Transition,
    frames: Vec<(Pose, u32)>,
    /// Frame indices that loop back: (frame, target, p).
    branches: Vec<(usize, u32, u32)>,
    /// Exit target for every frame except the last, which gets `end`; None = no exits.
    exit: Option<u32>,
    mouths: bool,
    sound_on_first: bool,
}

fn anim(name: &'static str, frames: Vec<(Pose, u32)>) -> Anim {
    Anim {
        name,
        transition: Transition::None,
        frames,
        branches: vec![],
        exit: None,
        mouths: false,
        sound_on_first: false,
    }
}

fn pose(f: impl FnOnce(&mut Pose)) -> Pose {
    let mut p = REST;
    f(&mut p);
    p
}

/// A loop of `frames` that repeats until exit, then returns to rest (transition "exit").
/// Only for idle animations: a request phase waits for its animation to reach END.
fn looping(name: &'static str, frames: Vec<(Pose, u32)>) -> Anim {
    let n = frames.len() as u32;
    let mut f = frames;
    f.push((REST, 100));
    Anim {
        name,
        transition: Transition::Exit,
        branches: vec![(n as usize - 1, 0, 100)],
        exit: Some(n),
        ..anim(name, f)
    }
}

fn look(dx: f64, dy: f64) -> Pose {
    pose(|p| p.look = (dx, dy))
}

fn animations() -> Vec<Anim> {
    let blink = pose(|p| p.eyes = Eyes::Closed);
    let jump = |h: f64| pose(|p| p.dy = -h);
    let squat = pose(|p| p.squash = 0.85);
    let wiggle = vec![
        (pose(|p| p.rot = TILT_L), 150),
        (REST, 100),
        (pose(|p| p.rot = TILT_R), 150),
        (REST, 100),
    ];
    let spin: Vec<(Pose, u32)> = ROT
        .iter()
        .map(|&r| (pose(|p| p.rot = r), 80))
        .chain([(REST, 100)])
        .collect();
    let bounce = [
        (squat, 80),
        (jump(6.0), 100),
        (jump(10.0), 120),
        (jump(6.0), 100),
        (squat, 80),
        (REST, 100),
    ];
    let think = |e: Extra| {
        vec![
            (look(0.6, -1.0), 300),
            (
                pose(|p| {
                    p.look = (0.6, -1.0);
                    p.extra = e;
                }),
                900,
            ),
            (REST, 200),
        ]
    };
    let busy = |dy: f64| {
        vec![
            (
                pose(|p| {
                    p.dy = dy;
                    p.look = (0.0, 1.0);
                }),
                200,
            ),
            (look(0.0, 1.0), 200),
            (
                pose(|p| {
                    p.dy = dy;
                    p.look = (-0.5, 1.0);
                }),
                200,
            ),
            (look(0.5, 1.0), 200),
            (REST, 100),
        ]
    };
    let search = vec![
        (
            pose(|p| {
                p.look = (-1.0, 0.0);
                p.extra = Extra::Question;
            }),
            400,
        ),
        (
            pose(|p| {
                p.look = (1.0, 0.0);
                p.extra = Extra::Question;
            }),
            400,
        ),
        (REST, 150),
    ];
    let gesture = |x: i32, y: i32, rot: (f64, f64)| {
        let g = pose(|p| {
            p.extra = Extra::Pointer(x, y);
            p.look = (x as f64, y as f64);
            p.rot = rot;
        });
        vec![(pose(|p| p.rot = rot), 100), (g, 900), (REST, 100)]
    };
    // Ends on the moving pose, which stays shown during the slide (R-MOVE-1).
    let mv = |rot: (f64, f64), dx: f64, dy: f64| {
        anim(
            "",
            vec![
                (
                    pose(|p| {
                        p.rot = rot;
                        p.dx = dx;
                        p.dy = dy;
                        p.look = (dx.signum(), dy.signum());
                    }),
                    120,
                ),
                (
                    pose(|p| {
                        p.rot = rot;
                        p.dx = dx;
                        p.dy = dy - 2.0;
                        p.look = (dx.signum(), dy.signum());
                    }),
                    120,
                ),
            ],
        )
    };
    let scaled = |s: f64| {
        pose(|p| {
            p.scale = s;
            p.eyes = Eyes::Open;
        })
    };
    let speak_frames: Vec<(Pose, u32)> = vec![
        (REST, 120),
        (look(0.2, 0.0), 120),
        (REST, 120),
        (look(-0.2, 0.0), 120),
    ];
    let named = |name: &'static str, a: Anim| Anim { name, ..a };

    let mut v = vec![
        anim("RestPose", vec![(REST, 100)]),
        Anim {
            sound_on_first: true,
            ..anim(
                "Show",
                vec![
                    (scaled(0.25), 80),
                    (scaled(0.5), 80),
                    (scaled(0.75), 80),
                    (jump(4.0), 80),
                    (REST, 100),
                ],
            )
        },
        anim(
            "Hide",
            vec![
                (jump(4.0), 80),
                (scaled(0.75), 80),
                (scaled(0.5), 80),
                (scaled(0.25), 80),
            ],
        ),
        anim("Idle1_1", vec![(REST, 1500), (blink, 120), (REST, 1500)]),
        anim(
            "IdleBlink",
            vec![(blink, 120), (REST, 200), (blink, 120), (REST, 800)],
        ),
        anim(
            "IdleLookAround",
            vec![
                (look(-1.0, 0.0), 700),
                (REST, 300),
                (look(1.0, 0.0), 700),
                (look(0.0, -1.0), 500),
                (REST, 300),
            ],
        ),
        named(
            "IdleSnooze",
            looping(
                "",
                vec![
                    (
                        pose(|p| {
                            p.eyes = Eyes::Closed;
                            p.squash = 0.95;
                        }),
                        600,
                    ),
                    (
                        pose(|p| {
                            p.eyes = Eyes::Closed;
                            p.extra = Extra::Zzz;
                        }),
                        900,
                    ),
                ],
            ),
        ),
        anim("IdleSpin", spin.clone()),
        Anim {
            mouths: true,
            ..anim(
                "Speak",
                speak_frames.iter().copied().chain([(REST, 100)]).collect(),
            )
        },
        Anim {
            mouths: true,
            ..anim(
                "Explain",
                speak_frames.iter().copied().chain([(REST, 100)]).collect(),
            )
        },
        named("MoveLeft", mv(TILT_L, -3.0, 0.0)),
        named("MoveRight", mv(TILT_R, 3.0, 0.0)),
        named("MoveUp", mv(REST.rot, 0.0, -4.0)),
        named("MoveDown", mv(REST.rot, 0.0, 3.0)),
        anim("GestureLeft", gesture(-1, 0, TILT_L)),
        anim("GestureRight", gesture(1, 0, TILT_R)),
        anim("GestureUp", gesture(0, -1, REST.rot)),
        anim("GestureDown", gesture(0, 1, REST.rot)),
        anim(
            "Hearing_1",
            vec![
                (
                    pose(|p| {
                        p.rot = TILT_R;
                        p.eyes = Eyes::Wide;
                        p.look = (1.0, 0.0);
                    }),
                    400,
                ),
                (
                    pose(|p| {
                        p.rot = TILT_R;
                        p.look = (1.0, 0.0);
                    }),
                    300,
                ),
                (REST, 100),
            ],
        ),
        anim(
            "Alert",
            vec![
                (
                    pose(|p| {
                        p.eyes = Eyes::Wide;
                        p.dy = -3.0;
                    }),
                    150,
                ),
                (pose(|p| p.eyes = Eyes::Wide), 600),
                (REST, 100),
            ],
        ),
        anim(
            "Greeting",
            wiggle
                .iter()
                .copied()
                .chain(wiggle.iter().copied())
                .collect(),
        ),
        anim("Wave", wiggle.clone()),
        anim(
            "Goodbye",
            wiggle
                .iter()
                .copied()
                .chain([(scaled(0.75), 100), (scaled(0.5), 100)])
                .collect(),
        ),
        anim(
            "Congratulate",
            bounce
                .iter()
                .copied()
                .map(|(mut p, ms)| {
                    if p.dy < -8.0 {
                        p.extra = Extra::Sparkle;
                    }
                    (p, ms)
                })
                .collect(),
        ),
        anim(
            "GetAttention",
            bounce
                .iter()
                .copied()
                .chain(bounce.iter().copied())
                .collect(),
        ),
        anim("Searching", search.clone()),
        anim("CheckingSomething", search),
        anim("Print", busy(2.0)),
        anim("Writing", busy(1.0)),
        anim("SendMail", busy(-2.0)),
        anim("Save", busy(2.0)),
        anim("GetArtsy", busy(-1.0)),
        anim("Processing", spin.clone()),
        anim("GetTechy", spin.clone()),
        anim("EmptyTrash", spin),
        anim("Thinking", think(Extra::Question)),
        anim("GetWizardy", think(Extra::Sparkle)),
    ];
    for (name, x, y) in [
        ("LookLeft", -1.0, 0.0),
        ("LookRight", 1.0, 0.0),
        ("LookUp", 0.0, -1.0),
        ("LookDown", 0.0, 1.0),
        ("LookUpLeft", -0.7, -0.7),
        ("LookUpRight", 0.7, -0.7),
        ("LookDownLeft", -0.7, 0.7),
        ("LookDownRight", 0.7, 0.7),
    ] {
        v.push(anim(name, vec![(look(x, y), 800), (REST, 100)]));
    }
    v
}

const STATES: &[(&str, &[&str])] = &[
    ("Showing", &["Show"]),
    ("Hiding", &["Hide"]),
    ("Speaking", &["Speak"]),
    ("Listening", &["Alert"]),
    ("Hearing", &["Hearing_1"]),
    ("MovingUp", &["MoveUp"]),
    ("MovingDown", &["MoveDown"]),
    ("MovingLeft", &["MoveLeft"]),
    ("MovingRight", &["MoveRight"]),
    ("GesturingUp", &["GestureUp"]),
    ("GesturingDown", &["GestureDown"]),
    ("GesturingLeft", &["GestureLeft"]),
    ("GesturingRight", &["GestureRight"]),
    ("IdlingLevel1", &["Idle1_1", "IdleBlink"]),
    ("IdlingLevel2", &["IdleLookAround", "IdleBlink", "Idle1_1"]),
    ("IdlingLevel3", &["IdleSnooze", "IdleSpin"]),
];

/// A short synthetic "pop" (rising chirp, 8-bit-free 16-bit mono), deterministic.
fn pop() -> Pcm {
    let rate = 22050u32;
    let n = 2200usize;
    let mut data = Vec::with_capacity(n * 2);
    // Square-ish chirp from integer phase arithmetic.
    let mut phase: u64 = 0;
    for i in 0..n {
        let f = 300 + (i as u64 * 900) / n as u64;
        phase += f * 65536 / rate as u64;
        let env = (n - i) as i64 * 9000 / n as i64;
        let tri = ((phase & 0xFFFF) as i64 - 32768).abs() - 16384; // triangle, -16384..16384
        let v = (tri * env / 16384) as i16;
        data.extend_from_slice(&v.to_le_bytes());
    }
    Pcm {
        channels: 1,
        rate,
        bits: 16,
        data,
    }
}

/// Build the placeholder package.
pub fn placeholder() -> Result<PackageData, Error> {
    let anims = animations();
    // Distinct frame pictures (by pose identity through their pixels).
    let mut pics: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let mut index_of = |p: &Pose| -> u32 {
        let one = draw(p, 1).rgba();
        if let Some(i) = pics.iter().position(|(a, _)| *a == one) {
            return i as u32;
        }
        pics.push((one, draw(p, 2).rgba()));
        (pics.len() - 1) as u32
    };
    let lay = |sheet: &str, i: u32, x: i32, y: i32| Layer {
        image: ImageRef {
            sheet: sheet.into(),
            index: i,
        },
        x,
        y,
        size: None,
    };
    let mut m = crate::build::empty_manifest(Family::Agent, [SIZE, SIZE]);
    m.character.info = vec![
        Info {
            lang: "en".into(),
            name: "Placeholder".into(),
            description: Some("bentwire's own stand-in: a wire clip drawn by code (CC0).".into()),
            extra: None,
        },
        Info {
            lang: "de".into(),
            name: "Platzhalter".into(),
            description: Some(
                "bentwires eigener Ersatz: eine per Code gezeichnete Drahtklammer (CC0).".into(),
            ),
            extra: None,
        },
    ];
    m.balloon = Some(Balloon {
        enabled: Some(true),
        lines: Some(2),
        chars_per_line: Some(32),
        fg: Some("#000000".into()),
        bg: Some("#FFFFE1".into()),
        border: Some("#000000".into()),
        font: Some("Tahoma".into()),
        font_height: Some(-11),
        font_weight: Some(400),
        italic: Some(false),
        size_to_text: Some(true),
        auto_hide: Some(true),
        auto_pace: Some(true),
        ..Default::default()
    });
    for a in &anims {
        let n = a.frames.len();
        let mut frames = Vec::with_capacity(n);
        for (i, (p, ms)) in a.frames.iter().enumerate() {
            let mut f = Frame {
                ms: *ms,
                layers: vec![lay("a", index_of(p), 0, 0)],
                ..Frame::default()
            };
            if a.sound_on_first && i == 0 {
                f.sound = Some("pop".into());
            }
            if let Some(x) = a.exit {
                f.exit = Some(if i + 1 == n {
                    Exit::End
                } else {
                    Exit::Frame(x)
                });
            }
            f.branch = a
                .branches
                .iter()
                .filter(|b| b.0 == i)
                .map(|b| Branch::Random {
                    target: b.1,
                    p: b.2,
                })
                .collect();
            if a.mouths && i + 1 < n {
                let (x, y) = mouth_at(p);
                f.mouths = (0..7)
                    .map(|t| Mouth {
                        kind: t,
                        layer: lay("m", t as u32, x, y),
                        anchor: Some([x + MOUTH[0] as i32 / 2, y + MOUTH[1] as i32 / 2]),
                        base: None,
                    })
                    .collect();
            }
            frames.push(f);
        }
        m.animations.push(Animation {
            name: a.name.into(),
            transition: Some(a.transition),
            return_animation: None,
            frames,
        });
    }
    m.states = STATES
        .iter()
        .map(|(s, l)| (s.to_string(), l.iter().map(|x| x.to_string()).collect()))
        .collect();

    // Sheets: frames in `a` (as many columns as fit 4096 px), mouths in `m`; both with 2x.
    let grid = |pics: &[&Vec<u8>], cell: [u32; 2], k: u32| -> Result<(u32, Vec<u8>), Error> {
        let (cw, ch) = (cell[0] * k, cell[1] * k);
        let cols = (4096 / cell[0]).min(pics.len() as u32).max(1);
        let rows = (pics.len() as u32).div_ceil(cols);
        let (w, h) = (cw * cols, ch * rows);
        let mut buf = vec![0u8; (w * h * 4) as usize];
        for (n, p) in pics.iter().enumerate() {
            let (ox, oy) = ((n as u32 % cols) * cw, (n as u32 / cols) * ch);
            for y in 0..ch {
                let d = (((oy + y) * w + ox) * 4) as usize;
                let s = (y * cw * 4) as usize;
                buf[d..d + (cw * 4) as usize].copy_from_slice(&p[s..s + (cw * 4) as usize]);
            }
        }
        Ok((cols, encode_png_rgba(w, h, &buf)?))
    };
    let ones: Vec<&Vec<u8>> = pics.iter().map(|p| &p.0).collect();
    let twos: Vec<&Vec<u8>> = pics.iter().map(|p| &p.1).collect();
    let mouths1: Vec<Vec<u8>> = (0..7).map(|t| mouth(t, 1).rgba()).collect();
    let mouths2: Vec<Vec<u8>> = (0..7).map(|t| mouth(t, 2).rgba()).collect();
    let mut files = Vec::new();
    for (id, cell, a, b) in [
        ("a", [SIZE, SIZE], ones, twos),
        (
            "m",
            MOUTH,
            mouths1.iter().collect(),
            mouths2.iter().collect(),
        ),
    ] {
        let (_, png1) = grid(&a, cell, 1)?;
        let (_, png2) = grid(&b, cell, 2)?;
        let f1 = format!("sheets/{id}.png");
        let f2 = format!("sheets/{id}@2x.png");
        files.push((f1.clone(), png1));
        files.push((f2.clone(), png2));
        m.sheets.push(Sheet {
            id: id.into(),
            file: f1,
            cell,
            count: a.len() as u32,
            sizes: None,
            scale: None,
            hidpi: [(2, f2)].into_iter().collect(),
        });
    }
    files.push(("sounds/pop.flac".into(), encode_flac(&pop())?));
    m.sounds = vec![("pop".into(), "sounds/pop.flac".into())];
    // Tray icon: the rest picture at 32x32 (nearest of the 96x96, every third pixel).
    let rest = draw(&REST, 1).rgba();
    let mut icon = vec![0u8; 32 * 32 * 4];
    for y in 0..32 {
        for x in 0..32 {
            let s = ((y * 3 + 1) * SIZE as usize + 24 + x * 3 / 2) * 4;
            let s = s.min(rest.len() - 4);
            icon[(y * 32 + x) * 4..(y * 32 + x) * 4 + 4].copy_from_slice(&rest[s..s + 4]);
        }
    }
    files.push(("icon.png".into(), encode_png_rgba(32, 32, &icon)?));
    m.icon = Some(IconRef {
        file: "icon.png".into(),
    });
    m.source = Some(
        [
            ("format", toml::Value::from("generated")),
            (
                "generator",
                toml::Value::from("cargo run -p bentwire-convert --bin gen-builtin"),
            ),
            ("licence", toml::Value::from("CC0-1.0")),
        ]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect(),
    );
    let errs = validate(&m);
    if !errs.is_empty() {
        return Err(Error::Invalid(errs));
    }
    Ok(PackageData {
        manifest: m,
        header: vec![
            "bentwire placeholder character, generated by `cargo run -p bentwire-convert --bin gen-builtin`.".into(),
            "CC0-1.0. Template for hand-made characters: edit this file and the PNG sheets.".into(),
        ],
        files,
    })
}

/// `pack.toml` of `characters/builtin` (A-PACK-1).
pub const BUILTIN_PACK_TOML: &str = r#"name = "bentwire builtin"
licence = "CC0-1.0"
provenance = "Generated by bentwire's own code (cargo run -p bentwire-convert --bin gen-builtin); no third-party data."
description = "The placeholder character bentwire-placeholder, usable without any Microsoft file."
"#;
