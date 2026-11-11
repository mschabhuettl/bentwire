//! A loaded bentwire character to the core's playback model (bentwire_core::model).
//! Facts that need pixels or audio are computed here: sound durations, mouth anchors
//! (B-BAL-4) and unavailable animations (B-CHAR-2).

use bentwire_character::{self as bc, Character};
use bentwire_core::model::{self as cm, CharacterModel};
use std::collections::HashMap;

fn color(s: &Option<String>, default: [u8; 3]) -> [u8; 3] {
    let Some(h) = s.as_deref().and_then(|s| s.strip_prefix('#')) else {
        return default;
    };
    let v = u32::from_str_radix(h, 16).ok().filter(|_| h.len() == 6);
    v.map(|v| [(v >> 16) as u8, (v >> 8) as u8, v as u8])
        .unwrap_or(default)
}

fn balloon(b: Option<&bc::Balloon>) -> cm::BalloonStyle {
    let d = cm::BalloonStyle::default();
    let Some(b) = b else { return d };
    cm::BalloonStyle {
        enabled: b.enabled.unwrap_or(d.enabled),
        lines: b.lines.map(|v| v.max(0) as u32).unwrap_or(d.lines),
        chars_per_line: b
            .chars_per_line
            .map(|v| v.max(0) as u32)
            .unwrap_or(d.chars_per_line),
        fg: color(&b.fg, d.fg),
        bg: color(&b.bg, d.bg),
        border: color(&b.border, d.border),
        font: b.font.clone().unwrap_or(d.font),
        font_height: b.font_height.map(|v| v as i32).unwrap_or(d.font_height),
        font_weight: b
            .font_weight
            .map(|v| v.max(0) as u32)
            .unwrap_or(d.font_weight),
        italic: b.italic.unwrap_or(d.italic),
        size_to_text: b.size_to_text.unwrap_or(d.size_to_text),
        auto_hide: b.auto_hide.unwrap_or(d.auto_hide),
        auto_pace: b.auto_pace.unwrap_or(d.auto_pace),
    }
}

/// B-BAL-4: centre of the opaque bounding box of the mouth layer, canvas coordinates.
fn mouth_anchor(
    c: &Character,
    layer: &bc::Layer,
    cache: &mut HashMap<bc::Layer, (i32, i32)>,
) -> (i32, i32) {
    if let Some(p) = cache.get(layer) {
        return *p;
    }
    let canvas = c.manifest.canvas();
    let img = c.compose_at(std::slice::from_ref(layer), canvas);
    let (mut x0, mut y0, mut x1, mut y1) = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
    for y in 0..img.height {
        for x in 0..img.width {
            if img.data[((y * img.width + x) * 4 + 3) as usize] > 0 {
                x0 = x0.min(x as i32);
                y0 = y0.min(y as i32);
                x1 = x1.max(x as i32 + 1);
                y1 = y1.max(y as i32 + 1);
            }
        }
    }
    let p = if x0 > x1 {
        (layer.x, layer.y)
    } else {
        cm::mouth_point_from_bbox(x0, y0, x1, y1)
    };
    cache.insert(layer.clone(), p);
    p
}

pub fn model_from(id: &str, c: &Character) -> CharacterModel {
    let m = &c.manifest;
    let family = match m.family {
        bc::Family::Agent => cm::Family::Agent,
        bc::Family::Office97 => cm::Family::Office97,
    };
    let canvas = m.canvas();
    let display = m.display();
    let mut anchors = HashMap::new();
    let animations = m
        .animations
        .iter()
        .filter(|a| !m.is_missing(&a.name))
        .map(|a| cm::Animation {
            name: a.name.clone(),
            frames: a
                .frames
                .iter()
                .map(|f| cm::Frame {
                    ms: f.ms,
                    has_picture: !f.layers.is_empty() || f.pose.is_some(),
                    pose: f.pose,
                    sound: f.sound.clone(),
                    branches: f
                        .branch
                        .iter()
                        .map(|b| match *b {
                            bc::Branch::Random { target, p } => cm::Branch::Prob {
                                target: target as usize,
                                p,
                            },
                            bc::Branch::Daypart { target, value } => cm::Branch::Daypart {
                                target: target as usize,
                                period: value.min(255) as u8,
                            },
                            bc::Branch::Host { target, value } => cm::Branch::Host {
                                target: target as usize,
                                value: value as i32,
                            },
                        })
                        .collect(),
                    exit: match f.exit {
                        None => cm::Exit::None,
                        Some(bc::Exit::End) => cm::Exit::End,
                        Some(bc::Exit::Frame(n)) => cm::Exit::To(n as usize),
                    },
                    mouths: f
                        .mouths
                        .iter()
                        .map(|mo| cm::Mouth {
                            kind: mo.kind,
                            anchor: match mo.anchor {
                                Some([x, y]) => (x, y),
                                None => mouth_anchor(c, &mo.layer, &mut anchors),
                            },
                        })
                        .collect(),
                })
                .collect(),
            transition: match a.transition {
                Some(bc::Transition::Return) => cm::Transition::Return,
                Some(bc::Transition::Exit) => cm::Transition::Exit,
                _ => cm::Transition::None,
            },
            return_animation: a.return_animation.clone(),
            available: c.is_available(&a.name),
        })
        .collect();
    CharacterModel {
        id: id.to_string(),
        family,
        canvas: (canvas[0], canvas[1]),
        display: (display[0], display[1]),
        info: m
            .character
            .info
            .iter()
            .map(|i| cm::InfoEntry {
                lang: i.lang.clone(),
                name: i.name.clone(),
                description: i.description.clone().unwrap_or_default(),
            })
            .collect(),
        balloon: balloon(m.balloon.as_ref()),
        states: m.states.clone(),
        animations,
        sounds: m
            .sounds
            .iter()
            .map(|(sid, _)| cm::Sound {
                id: sid.clone(),
                duration_ms: c.sound(sid).map(|p| p.duration_ms()),
            })
            .collect(),
        office97: m.office97.as_ref().map(|o| cm::Office97 {
            rest_pose: o.rest_pose,
            idle: o.idle.clone(),
            transitions: o.transitions.clone(),
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn r_bal_1_balloon_defaults_and_colours() {
        let d = balloon(None);
        assert_eq!(d, cm::BalloonStyle::default());
        let b = bc::Balloon {
            fg: Some("#102030".into()),
            bg: Some("bogus".into()),
            lines: Some(3),
            ..Default::default()
        };
        let s = balloon(Some(&b));
        assert_eq!(s.fg, [0x10, 0x20, 0x30]);
        assert_eq!(s.bg, d.bg);
        assert_eq!(s.lines, 3);
    }
}
