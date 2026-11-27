//! Wire, bentwire's first own CC0 character (architecture A-CHAR-1): a piece of bent wire
//! with a face, described once as a vector rig ([`rig`]) and rendered by the shared code of
//! [`crate::own`] as `wire` (modern) and `wire-retro` (12-colour indexed sheets: 11 colours
//! and transparent).

pub mod rig;

use bentwire_character::{Info, PackageData, Pcm};

use crate::Error;
use crate::own::{self, Design, Style};

pub const WIRE: Design = Design {
    id: "wire",
    name: "Wire",
    shapes: rig::shapes,
    mouth_at: rig::mouth_at,
    mouth_shapes: rig::mouth_shapes,
    mouth: rig::MOUTH,
    icon_view: rig::icon_view,
    roles: &rig::ROLES,
    info,
    sounds,
    adapt: |_| {},
};

/// Build Wire in `style`.
pub fn wire(style: Style) -> Result<PackageData, Error> {
    WIRE.build(style)
}

fn info(style: Style) -> Vec<Info> {
    let intro = [
        "Hi, I'm Wire, a piece of bent wire with a face. Bent, but never broken!~~Hello! I'm made of code, from my coil to my curl.",
        "Hallo, ich bin Wire, ein Stück gebogener Draht mit Gesicht. Verbogen, aber nie gebrochen!~~Hallo! Ich bin aus Code gemacht, von der Feder bis zur Locke.",
    ];
    match style {
        Style::Modern => own::info(
            [
                "Wire",
                "bentwire's own character: a piece of bent wire with a face, drawn by code (CC0).",
                intro[0],
            ],
            [
                "Wire",
                "bentwires eigener Assistent: ein Stück gebogener Draht mit Gesicht, per Code gezeichnet (CC0).",
                intro[1],
            ],
        ),
        Style::Retro => own::info(
            [
                "Wire (retro)",
                "Wire as in 1997: 12 colours, hard edges, drawn by code (CC0).",
                intro[0],
            ],
            [
                "Wire (retro)",
                "Wire wie 1997: 12 Farben, harte Kanten, per Code gezeichnet (CC0).",
                intro[1],
            ],
        ),
    }
}

fn sounds() -> Vec<(&'static str, Pcm)> {
    vec![
        ("show", boing(260, 900, 4400)),
        ("hide", boing(800, 200, 3300)),
    ]
}

/// A short synthetic spring sound: a triangle wave whose pitch slides from `f0` to `f1` Hz
/// with a wobble, fading out. Integer arithmetic only.
fn boing(f0: u64, f1: u64, n: usize) -> Pcm {
    let rate = 22050u32;
    let mut data = Vec::with_capacity(n * 2);
    let mut phase: u64 = 0;
    for i in 0..n {
        let i = i as u64;
        let n = n as u64;
        // Wobble: a slow triangle of +-12 % of the pitch.
        let w = (i * 24 * 65536 / rate as u64) & 0xFFFF;
        let tri_w = (w as i64 - 32768).abs() - 16384;
        let f = (f0 * (n - i) + f1 * i) / n;
        let f = (f as i64 + f as i64 * tri_w / 16384 * 12 / 100) as u64;
        phase += f * 65536 / rate as u64;
        let env = (n - i) as i64 * 9000 / n as i64;
        let tri = ((phase & 0xFFFF) as i64 - 32768).abs() - 16384;
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
