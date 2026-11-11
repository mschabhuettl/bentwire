//! ACS 2.0 to bentwire (character-format 9.1, 10 "ACS").

use agent_formats::acs::Acs;
use agent_formats::common::{Balloon as SrcBalloon, Voice as SrcVoice};
use bentwire_character::*;

use crate::Error;
use crate::build::*;
use crate::sound::{Decoded, decode_source_sound};

pub(crate) fn balloon(b: Option<&SrcBalloon>, flags: u32) -> Balloon {
    let Some(b) = b else {
        return Balloon {
            enabled: Some(false),
            ..Default::default()
        };
    };
    Balloon {
        enabled: Some(true),
        lines: Some(b.lines as i64),
        chars_per_line: Some(b.chars_per_line as i64),
        fg: Some(colorref(b.foreground)),
        bg: Some(colorref(b.background)),
        border: Some(colorref(b.border)),
        font: Some(b.font.clone()),
        font_height: Some(b.font_height as i64),
        font_weight: Some(b.font_weight as i64),
        italic: Some(b.italic != 0),
        font_flag2: b.flag2.map(|v| v as i64),
        size_to_text: Some(flags & 0x10000 != 0),
        auto_hide: Some(flags & 0x20000 == 0),
        auto_pace: Some(flags & 0x40000 == 0),
    }
}

pub(crate) fn voice(v: Option<&SrcVoice>) -> Option<Voice> {
    let v = v?;
    let mut o = Voice {
        engine: Some(v.engine.to_string()),
        mode: Some(v.mode.to_string()),
        speed: Some(v.speed as i64),
        pitch: Some(v.pitch as i64),
        ..Default::default()
    };
    if let Some(x) = &v.extra {
        o.language = Some(lang_tag(x.language));
        o.dialect = Some(x.dialect.clone());
        o.gender = Some(x.gender as i64);
        o.age = Some(x.age as i64);
        o.style = Some(x.style.clone());
    }
    Some(o)
}

pub(crate) fn opt_str(s: &str) -> Option<String> {
    (!s.is_empty()).then(|| s.to_owned())
}

/// What a source sound became: its sound id in the package, or unplayable.
pub(crate) struct Sounds {
    pub ids: Vec<Option<String>>,
    pub list: Vec<(String, Pcm)>,
    pub unplayable: Vec<String>,
}

/// Decode source sounds, deduplicating by PCM (id = `sNN` of the first source index).
pub(crate) fn sounds(riffs: &[&[u8]], short_ok: bool) -> Result<Sounds, Error> {
    let mut s = Sounds {
        ids: Vec::new(),
        list: Vec::new(),
        unplayable: Vec::new(),
    };
    for (i, r) in riffs.iter().enumerate() {
        let id = format!("s{i:02}");
        match decode_source_sound(r, short_ok)
            .map_err(|e| Error::Source(format!("sound {i}: {e}")))?
        {
            Decoded::Unplayable => {
                s.unplayable.push(id);
                s.ids.push(None);
            }
            Decoded::Pcm(p) => {
                if let Some((k, _)) = s.list.iter().find(|(_, q)| *q == p) {
                    s.ids.push(Some(k.clone()));
                } else {
                    s.list.push((id.clone(), p));
                    s.ids.push(Some(id));
                }
            }
        }
    }
    Ok(s)
}

pub(crate) fn source_table(entries: Vec<(&str, toml::Value)>) -> toml::Table {
    entries
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect()
}

/// Convert a parsed ACS 2.0 file. `file` and `sha` go into `[source]`.
pub fn convert_acs(acs: &Acs, file: &str, sha: &str) -> Result<PackageData, Error> {
    let ci = &acs.character;
    let mut m = empty_manifest(Family::Agent, [ci.width as u32, ci.height as u32]);
    m.character.id = Some(ci.guid.to_string());
    m.character.standard_animation_set = ci.flags & 0x100000 != 0;
    m.character.info = ci
        .localized
        .iter()
        .map(|l| Info {
            lang: lang_tag(l.langid),
            name: l.name.clone(),
            description: opt_str(&l.description),
            extra: opt_str(&l.extra),
        })
        .collect();
    m.balloon = Some(balloon(ci.balloon.as_ref(), ci.flags));
    m.voice = voice(ci.voice.as_ref());

    let mut pool = Pool::default();
    let remap: Vec<Option<u32>> = acs
        .images
        .iter()
        .map(|e| {
            e.image.as_ref().map(|im| {
                pool.add(Pic {
                    w: im.bitmap.width as u32,
                    h: im.bitmap.height as u32,
                    rgba: false,
                    px: im.bitmap.top_down(),
                })
            })
        })
        .collect();
    let snd = sounds(
        &acs.sounds
            .iter()
            .map(|s| s.data.as_slice())
            .collect::<Vec<_>>(),
        false,
    )?;

    let names: Vec<&str> = acs.animations.iter().map(|a| a.name.as_str()).collect();
    for a in &acs.animations {
        let n = a.frames.len();
        let transition = match a.transition {
            0 => Transition::Return,
            1 => Transition::Exit,
            2 => Transition::None,
            t => return Err(Error::Source(format!("{}: transition type {t}", a.name))),
        };
        let mut frames = Vec::with_capacity(n);
        for (fi, fr) in a.frames.iter().enumerate() {
            let w = || format!("{}[{fi}]", a.name);
            // ACS lists the topmost image first; layers are back to front.
            let layers: Vec<Layer> = fr
                .images
                .iter()
                .rev()
                .filter_map(|im| {
                    remap
                        .get(im.image as usize)
                        .copied()
                        .flatten()
                        .map(|i| pool_layer(i, im.x as i32, im.y as i32, None))
                })
                .collect();
            let exit = match fr.exit {
                -1 => Some(Exit::End),
                -2 => None,
                x if x >= 0 && (x as usize) < n => Some(Exit::Frame(x as u32)),
                x => return Err(Error::Source(format!("{}: exit {x} invalid", w()))),
            };
            let mut sum = 0u32;
            let mut branch = Vec::new();
            for b in &fr.branches {
                if b.target as usize >= n || b.probability == 0 {
                    return Err(Error::Source(format!("{}: invalid branch", w())));
                }
                sum += b.probability as u32;
                branch.push(Branch::Random {
                    target: b.target as u32,
                    p: b.probability as u32,
                });
            }
            if sum > 100 {
                return Err(Error::Source(format!(
                    "{}: branch sum {sum} above 100",
                    w()
                )));
            }
            let mut mouths: Vec<Mouth> = Vec::new();
            for o in &fr.overlays {
                let img = remap
                    .get(o.image as usize)
                    .copied()
                    .flatten()
                    .ok_or_else(|| Error::Source(format!("{}: overlay image missing", w())))?;
                let mo = Mouth {
                    kind: o.mouth,
                    layer: pool_layer(img, o.x as i32, o.y as i32, None),
                    anchor: Some([o.x as i32 + o.point_x as i32, o.y as i32 + o.point_y as i32]),
                    base: (o.replace_top != 0)
                        .then(|| layers[..layers.len().saturating_sub(1)].to_vec()),
                };
                // A later overlay of the same mouth type replaces an earlier one.
                match mouths.iter_mut().find(|x| x.kind == o.mouth) {
                    Some(x) => *x = mo,
                    None => mouths.push(mo),
                }
            }
            mouths.sort_by_key(|x| x.kind);
            frames.push(Frame {
                ms: fr.duration as u32 * 10,
                layers,
                sound: snd.ids.get(fr.sound as usize).cloned().flatten(),
                exit,
                branch,
                mouths,
                ..Frame::default()
            });
        }
        m.animations.push(Animation {
            name: a.name.clone(),
            transition: Some(transition),
            return_animation: opt_str(&a.return_animation),
            frames,
        });
    }
    m.states = ci
        .states
        .iter()
        .map(|s| {
            let members = s
                .animations
                .iter()
                .map(|x| {
                    names
                        .iter()
                        .find(|n| n.eq_ignore_ascii_case(x))
                        .map_or_else(|| x.clone(), |n| (*n).to_owned())
                })
                .collect();
            (canonical_state(&s.name), members)
        })
        .collect();

    let mut src = vec![
        ("format", toml::Value::from("acs-2.0")),
        ("file", toml::Value::from(file)),
        ("sha256_16", toml::Value::from(sha)),
        ("version", toml::Value::from(ci.version as i64)),
        ("flags", toml::Value::from(ci.flags as i64)),
        (
            "animation_set",
            toml::Value::from(vec![ci.animation_set.0 as i64, ci.animation_set.1 as i64]),
        ),
    ];
    if !snd.unplayable.is_empty() {
        src.push((
            "unplayable_sounds",
            toml::Value::from(snd.unplayable.clone()),
        ));
    }
    m.source = Some(source_table(src));

    let icon = match &ci.tray_icon {
        Some(t) => Some(icon_rgba(&t.mask_dib, &t.color_dib)?),
        None => None,
    };
    let model = Model {
        manifest: m,
        pool,
        palette: Some(ci.palette.iter().map(|q| [q.r, q.g, q.b]).collect()),
        transparent: Some(ci.transparent),
        sounds: snd.list,
        icon,
        drop_unused: false,
        hidpi2: Default::default(),
    };
    model.finish(vec![format!("Converted by bentwire-convert from {file}")])
}
