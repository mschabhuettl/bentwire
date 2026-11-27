//! Quack, bentwire's second own CC0 character (architecture A-CHAR-1): a rubber debugging
//! duck, described once as a vector rig ([`rig`]) and rendered by the shared code of
//! [`crate::own`] as `quack` (modern) and `quack-retro` (13-colour indexed sheets: 12
//! colours and transparent). Its own touches on Wire's animation table: it wobbles on the
//! water when idle and asleep, and squeaks on Alert and GetAttention.

pub mod rig;

use bentwire_character::{Info, Pcm};

use crate::own::pose::{Eyes, Prop, REST};
use crate::own::{self, Anim, Design, Style, pose, rest};

pub const QUACK: Design = Design {
    id: "quack",
    name: "Quack",
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
        "Hi, I'm Quack, a rubber debugging duck. Tell me about your bug, line by line!~~Squeak! Explain it to me and it is half fixed.",
        "Hallo, ich bin Quack, eine Quietscheente zum Debuggen. Erzähl mir von deinem Fehler, Zeile für Zeile!~~Quietsch! Erklär ihn mir, dann ist er schon halb behoben.",
    ];
    match style {
        Style::Modern => own::info(
            [
                "Quack",
                "A rubber debugging duck: explain your bug to it, it listens. Drawn by code (CC0).",
                intro[0],
            ],
            [
                "Quack",
                "Eine Quietscheente zum Debuggen: Erklär ihr deinen Fehler, sie hört zu. Per Code gezeichnet (CC0).",
                intro[1],
            ],
        ),
        Style::Retro => own::info(
            [
                "Quack (retro)",
                "Quack as in 1997: 13 colours, hard edges, drawn by code (CC0).",
                intro[0],
            ],
            [
                "Quack (retro)",
                "Quack wie 1997: 13 Farben, harte Kanten, per Code gezeichnet (CC0).",
                intro[1],
            ],
        ),
    }
}

/// Idle1_3 becomes a wobble on the water, the duck rocks gently while asleep, and Alert and
/// GetAttention squeak.
fn adapt(anims: &mut [Anim]) {
    let rock = |a: f64, e: Eyes| {
        pose(|p| {
            p.rock = a;
            p.eyes = e;
            p.look = (-a / 8.0, 0.0);
        })
    };
    let wobble = rest(80)
        .to(rock(-6.0, Eyes::Open), 1, 110)
        .to(rock(5.0, Eyes::Happy), 1, 120)
        .to(rock(-4.0, Eyes::Happy), 1, 120)
        .to(rock(2.5, Eyes::Open), 1, 120)
        .to(rock(-1.2, Eyes::Open), 1, 120)
        .to(REST, 1, 200);
    for a in anims.iter_mut() {
        match a.name.as_str() {
            "Idle1_3" => *a = Anim::plain("Idle1_3", wobble.0.clone()),
            "Idle3_1" => {
                for (p, _) in &mut a.frames {
                    if let Prop::Zzz(n) = p.prop {
                        p.rock = [0.0, 2.0, 0.0, -2.0][n as usize];
                    }
                }
            }
            "Alert" | "GetAttention" => a.sound = Some("squeak"),
            _ => {}
        }
    }
}

fn sounds() -> Vec<(&'static str, Pcm)> {
    let pcm = |data: Vec<i16>| Pcm {
        channels: 1,
        rate: RATE,
        bits: 16,
        data: data.iter().flat_map(|v| v.to_le_bytes()).collect(),
    };
    let mut twice = squeak(1200, 2000, 1600, 3000);
    twice.extend(vec![0; 900]);
    twice.extend(squeak(1250, 2150, 1700, 3400));
    vec![
        ("show", pcm(squeak(900, 1800, 1400, 5000))),
        ("hide", pcm(squeak(1500, 1100, 650, 4200))),
        ("squeak", pcm(twice)),
    ]
}

const RATE: u32 = 22050;

/// A rubber duck's squeak: a buzzy tone (triangle plus a little square) whose pitch slides
/// from `f0` to `f1` Hz in the first half and on to `f2` in the second, with a fast vibrato,
/// a breath of noise, a quick attack and a fade. Integer arithmetic only.
fn squeak(f0: u64, f1: u64, f2: u64, n: usize) -> Vec<i16> {
    let n = n as u64;
    let half = n / 2;
    let mut phase: u64 = 0;
    let mut noise: u32 = 0x2545_f491;
    (0..n)
        .map(|i| {
            let f = if i < half {
                (f0 * (half - i) + f1 * i) / half
            } else {
                (f1 * (n - i) + f2 * (i - half)) / (n - half)
            };
            // Vibrato: a 30 Hz triangle of +-4 % of the pitch.
            let w = (i * 30 * 65536 / RATE as u64) & 0xFFFF;
            let tri_w = (w as i64 - 32768).abs() - 16384;
            let f = (f as i64 + f as i64 * tri_w / 16384 * 4 / 100) as u64;
            phase += f * 65536 / RATE as u64;
            let p = (phase & 0xFFFF) as i64;
            let tri = (p - 32768).abs() - 16384;
            let sq = if p < 32768 { 16384 } else { -16384 };
            noise = noise.wrapping_mul(1_103_515_245).wrapping_add(12345);
            let hiss = ((noise >> 16) & 0x7FFF) as i64 - 16384;
            let wave = (tri * 6 + sq * 2 + hiss) / 9;
            let attack = n / 12;
            let env = if i < attack {
                i as i64 * 8000 / attack as i64
            } else {
                (n - i) as i64 * 8000 / (n - attack) as i64
            };
            (wave * env / 16384) as i16
        })
        .collect()
}
