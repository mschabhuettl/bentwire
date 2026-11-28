//! Hermit, bentwire's third own CC0 character (architecture A-CHAR-1): a hermit crab in a
//! spiral shell, described once as a vector rig ([`rig`]) and rendered by the shared code of
//! [`crate::own`] as `hermit` (modern) and `hermit-retro` (16-colour indexed sheets: 15
//! colours and transparent). Its own touches on Wire's animation table: it peeks out of its
//! shell for Show, pulls into it for Hide and when surprised, ducks in and peeks out when
//! idle, sleeps half inside, and its claws snap as they wave and clack on Alert and
//! GetAttention.

pub mod rig;

use bentwire_character::{Info, Pcm};

use crate::own::pose::{Brows, Eyes, Mouth, Pose, Prop, REST};
use crate::own::{self, Anim, Design, Seq, Style, pose, rest};

pub const HERMIT: Design = Design {
    id: "hermit",
    name: "Hermit",
    shapes: rig::shapes,
    mouth_at: rig::mouth_at,
    mouth_shapes: rig::mouth_shapes,
    mouth: rig::MOUTH,
    icon_view: rig::icon_view,
    roles: &rig::ROLES,
    info,
    sounds,
    adapt,
};

fn info(style: Style) -> Vec<Info> {
    let intro = [
        "Hi, I'm Hermit, a hermit crab. I'll come out when your build is green!~~Snip snap! Red builds send me back into my shell.",
        "Hallo, ich bin Hermit, ein Einsiedlerkrebs. Ich komme raus, wenn dein Build grün ist!~~Schnipp schnapp! Bei roten Builds verkrieche ich mich in mein Haus.",
    ];
    match style {
        Style::Modern => own::info(
            [
                "Hermit",
                "A hermit crab in a spiral shell: it peeks out, snaps its claws and hides when things go wrong. Drawn by code (CC0).",
                intro[0],
            ],
            [
                "Hermit",
                "Ein Einsiedlerkrebs im Schneckenhaus: Er lugt heraus, schnappt mit den Scheren und versteckt sich, wenn etwas schiefgeht. Per Code gezeichnet (CC0).",
                intro[1],
            ],
        ),
        Style::Retro => own::info(
            [
                "Hermit (retro)",
                "Hermit as in 1997: 16 colours, hard edges, drawn by code (CC0).",
                intro[0],
            ],
            [
                "Hermit (retro)",
                "Hermit wie 1997: 16 Farben, harte Kanten, per Code gezeichnet (CC0).",
                intro[1],
            ],
        ),
    }
}

/// A pose `tuck` deep in the shell, looking `x` to the side.
fn inside(tuck: f64, x: f64, eyes: Eyes) -> Pose {
    pose(|p| {
        p.tuck = tuck;
        p.look = (x, 0.0);
        p.eyes = eyes;
        p.arms = [(30.0, -100.0), (30.0, -100.0)];
    })
}

/// Show peeks out of the shell, Hide pulls into it, Idle1_3 ducks in and peeks out,
/// Surprised ducks in, the crab sleeps half inside, and Alert and GetAttention clack.
fn adapt(anims: &mut [Anim]) {
    let shell = |sx: f64, sy: f64| {
        pose(|p| {
            *p = inside(1.0, 0.0, Eyes::Open);
            p.sx = sx;
            p.sy = sy;
        })
    };
    let peek = |x: f64| inside(0.5, x, Eyes::Open);
    let show = Seq::at(shell(0.5, 0.12), 70)
        .to(shell(0.8, 0.6), 1, 70)
        .to(shell(1.04, 1.06), 1, 80)
        .to(shell(1.0, 1.0), 1, 160)
        .to(peek(0.0), 1, 90)
        .to(peek(-1.0), 1, 90)
        .hold(250)
        .to(peek(1.0), 2, 80)
        .hold(250)
        .to(REST, 2, 80);
    let hide = rest(60)
        .to(pose(|p| p.eyes = Eyes::Wide), 1, 120)
        .to(inside(0.5, 0.0, Eyes::Wide), 1, 60)
        .to(inside(1.0, 0.0, Eyes::Closed), 1, 60)
        .hold(160)
        .to(shell(1.04, 0.9), 1, 70)
        .to(shell(0.8, 0.5), 1, 60)
        .to(shell(0.5, 0.1), 1, 60);
    let duck = rest(80)
        .to(inside(1.0, 0.0, Eyes::Closed), 2, 60)
        .hold(700)
        .to(peek(0.0), 1, 120)
        .to(peek(-1.0), 1, 110)
        .hold(300)
        .to(peek(1.0), 2, 90)
        .hold(300)
        .to(REST, 2, 90);
    let fright = pose(|p| {
        p.eyes = Eyes::Wide;
        p.brows = Brows::Up;
        p.mouth = Mouth::O;
        p.arms = [(115.0, 25.0), (115.0, 25.0)];
    });
    let surprised = rest(60)
        .to(inside(1.0, 0.0, Eyes::Wide), 1, 60)
        .hold(500)
        .to(inside(0.5, 0.0, Eyes::Wide), 1, 120)
        .hold(200)
        .to(fright, 2, 80)
        .hold(600)
        .to(REST, 2, 100);
    for a in anims.iter_mut() {
        let sound = a.sound;
        match a.name.as_str() {
            "Show" => *a = Anim::plain("Show", show.0.clone()),
            "Hide" => *a = Anim::plain("Hide", hide.0.clone()),
            "Idle1_3" => *a = Anim::plain("Idle1_3", duck.0.clone()),
            "Surprised" => *a = Anim::plain("Surprised", surprised.0.clone()),
            "Idle3_1" => {
                for (p, _) in &mut a.frames {
                    if let Prop::Zzz(_) = p.prop {
                        p.tuck = 0.55;
                        p.arms = [(30.0, -100.0), (30.0, -100.0)];
                    }
                }
            }
            _ => {}
        }
        a.sound = match a.name.as_str() {
            "Alert" | "GetAttention" => Some("clack"),
            _ => sound,
        };
    }
}

fn sounds() -> Vec<(&'static str, Pcm)> {
    let pcm = |data: Vec<i16>| Pcm {
        channels: 1,
        rate: RATE,
        bits: 16,
        data: data.iter().flat_map(|v| v.to_le_bytes()).collect(),
    };
    vec![
        (
            "show",
            pcm(clicks(
                &[(0, 520, 7000), (330, 1500, 5000), (470, 1900, 5500)],
                620,
            )),
        ),
        (
            "hide",
            pcm(clicks(
                &[(0, 1700, 5000), (120, 1200, 5000), (300, 480, 8000)],
                450,
            )),
        ),
        (
            "clack",
            pcm(clicks(
                &[(0, 2100, 6500), (90, 1900, 6000), (180, 2300, 7000)],
                300,
            )),
        ),
    ]
}

const RATE: u32 = 22050;

/// Hard little clicks, as of a claw or a shell knocking: each (start ms, pitch Hz, peak) a
/// triangle tone with a breath of noise at its start, dying away fast; `ms` long in all.
/// Integer arithmetic only.
fn clicks(hits: &[(u64, u64, i64)], ms: u64) -> Vec<i16> {
    let n = (ms * RATE as u64 / 1000) as usize;
    let mut out = vec![0i64; n];
    let mut noise: u32 = 0x1b87_3593;
    for &(start, f, peak) in hits {
        let s0 = (start * RATE as u64 / 1000) as usize;
        let mut env = peak << 10;
        let mut phase: u64 = 0;
        for (i, v) in out.iter_mut().enumerate().skip(s0) {
            phase += f * 65536 / RATE as u64;
            let tri = ((phase & 0xFFFF) as i64 - 32768).abs() - 16384;
            noise = noise.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let hiss = ((noise >> 16) & 0x7FFF) as i64 - 16384;
            let wave = if i - s0 < 60 { (tri + hiss) / 2 } else { tri };
            *v += wave * (env >> 10) / 16384;
            // About 8 ms to a third.
            env = env * 1012 / 1024;
        }
    }
    out.into_iter()
        .map(|v| v.clamp(i16::MIN as i64, i16::MAX as i64) as i16)
        .collect()
}
