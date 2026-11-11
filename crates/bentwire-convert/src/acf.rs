//! ACF + ACA/AAF (Agent 2.0 web, Agent 1.5 web) and Agent 1.5 OLE `.acs` to bentwire
//! (character-format 9.2, 9.3, 10 "ACF + ACA", "Agent 1.5").

use agent_formats::acf::{AcaAnimation, AcfCharacter, BaseImage};
use bentwire_character::*;

use crate::Error;
use crate::acs::{balloon, opt_str, source_table, voice};
use crate::build::*;
use crate::sound::{Decoded, decode_source_sound};

/// Convert an ACF body plus its animations (index-aligned with `ch.animations`; `None` = not
/// on disk or damaged, listed in `source.missing_animations`). `format` names the source
/// format in `[source]`.
pub fn convert_acf(
    ch: &AcfCharacter,
    anims: &[Option<&AcaAnimation>],
    format: &str,
    file: &str,
    sha: &str,
) -> Result<PackageData, Error> {
    let (w, h) = (ch.width as u32, ch.height as u32);
    let mut m = empty_manifest(Family::Agent, [w, h]);
    m.character.id = Some(ch.guid.to_string());
    m.character.standard_animation_set = ch.flags & 0x100000 != 0;
    m.character.info = ch
        .localized
        .iter()
        .map(|l| Info {
            lang: lang_tag(l.langid),
            name: l.name.clone(),
            description: opt_str(&l.description),
            extra: opt_str(&l.extra),
        })
        .collect();
    m.balloon = Some(balloon(ch.balloon.as_ref(), ch.flags));
    m.voice = voice(ch.voice.as_ref());

    let mut pool = Pool::default();
    let mut sounds: Vec<(String, Pcm)> = Vec::new();
    let mut unplayable = Vec::new();
    let mut missing = Vec::new();
    let pic = |b: &agent_formats::common::Bitmap| Pic {
        w: b.width as u32,
        h: b.height as u32,
        rgba: false,
        px: b.top_down(),
    };
    for (entry, a) in ch.animations.iter().zip(anims) {
        let Some(a) = a else {
            missing.push(entry.name.clone());
            continue;
        };
        let name = &entry.name;
        let img: Vec<Option<u32>> = a
            .images
            .iter()
            .map(|i| {
                i.as_ref()
                    .map(|i| {
                        if (i.bitmap.width as u32, i.bitmap.height as u32) != (w, h) {
                            return Err(Error::Source(format!(
                                "{name}: image not at character size"
                            )));
                        }
                        Ok(pool.add(pic(&i.bitmap)))
                    })
                    .transpose()
            })
            .collect::<Result<_, _>>()?;
        // Sounds of all animations are merged by PCM.
        let mut snd: Vec<Option<String>> = Vec::new();
        for (k, r) in a.sounds.iter().enumerate() {
            match decode_source_sound(r, false)
                .map_err(|e| Error::Source(format!("{name} sound {k}: {e}")))?
            {
                Decoded::Unplayable => {
                    unplayable.push(format!("{name}:{k}"));
                    snd.push(None);
                }
                Decoded::Pcm(p) => match sounds.iter().find(|(_, q)| *q == p) {
                    Some((id, _)) => snd.push(Some(id.clone())),
                    None => {
                        let id = format!("s{:02}", sounds.len());
                        sounds.push((id.clone(), p));
                        snd.push(Some(id));
                    }
                },
            }
        }
        let n = a.frames.len();
        let transition = match a.transition.unwrap_or(0) {
            0 => Transition::Return,
            1 => Transition::Exit,
            2 => Transition::None,
            t => return Err(Error::Source(format!("{name}: transition type {t}"))),
        };
        let mut frames = Vec::with_capacity(n);
        for (fi, fr) in a.frames.iter().enumerate() {
            let wf = || format!("{name}[{fi}]");
            if fr.offset_x != 0 || fr.offset_y != 0 {
                return Err(Error::Source(format!(
                    "{}: nonzero frame offset (not representable)",
                    wf()
                )));
            }
            let layers: Vec<Layer> = if fr.image == 0xFFFF {
                vec![]
            } else {
                match img.get(fr.image as usize) {
                    Some(Some(i)) => vec![pool_layer(*i, 0, 0, None)],
                    Some(None) => vec![],
                    None => return Err(Error::Source(format!("{}: image index invalid", wf()))),
                }
            };
            let sound = if fr.sound == 0xFFFF {
                None
            } else {
                snd.get(fr.sound as usize)
                    .ok_or_else(|| Error::Source(format!("{}: sound index invalid", wf())))?
                    .clone()
            };
            // 1.5 stores no exit: the runtime uses -1.
            let exit = match fr.exit.unwrap_or(-1) {
                -1 => Some(Exit::End),
                -2 => None,
                x if x >= 0 && (x as usize) < n => Some(Exit::Frame(x as u32)),
                x => return Err(Error::Source(format!("{}: exit {x} invalid", wf()))),
            };
            let mut sum = 0u32;
            let mut branch = Vec::new();
            for b in &fr.branches {
                if b.target as usize >= n || b.probability == 0 {
                    return Err(Error::Source(format!("{}: invalid branch", wf())));
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
                    wf()
                )));
            }
            let mut mouths: Vec<Mouth> = Vec::new();
            for o in &fr.overlays {
                let Some(d) = &o.data else {
                    continue; // empty overlay record: no mouth of this type
                };
                let base = if d.replace_base.unwrap_or(0) != 0 {
                    Some(match &fr.base {
                        Some(BaseImage::Image { bitmap, .. }) => {
                            vec![pool_layer(pool.add(pic(bitmap)), 0, 0, None)]
                        }
                        _ => vec![],
                    })
                } else {
                    None
                };
                let mo = Mouth {
                    kind: o.mouth,
                    layer: pool_layer(pool.add(pic(&d.bitmap)), d.x as i32, d.y as i32, None),
                    anchor: None,
                    base,
                };
                match mouths.iter_mut().find(|x| x.kind == o.mouth) {
                    Some(x) => *x = mo,
                    None => mouths.push(mo),
                }
            }
            mouths.sort_by_key(|x| x.kind);
            frames.push(Frame {
                ms: fr.duration as u32 * 10,
                layers,
                sound,
                exit,
                branch,
                mouths,
                ..Frame::default()
            });
        }
        m.animations.push(Animation {
            name: name.clone(),
            transition: Some(transition),
            return_animation: opt_str(&entry.return_animation),
            frames,
        });
    }
    let names: Vec<String> = m.animations.iter().map(|a| a.name.clone()).collect();
    m.states = ch
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
                        .cloned()
                        .unwrap_or_else(|| x.clone())
                })
                .collect();
            (canonical_state(&s.name), members)
        })
        .collect();

    let mut src = vec![
        ("format", toml::Value::from(format)),
        ("file", toml::Value::from(file)),
        ("sha256_16", toml::Value::from(sha)),
        ("version", toml::Value::from(ch.version as i64)),
        ("flags", toml::Value::from(ch.flags as i64)),
    ];
    if let Some((a, b)) = ch.animation_set {
        src.push(("animation_set", toml::Value::from(vec![a as i64, b as i64])));
    }
    if !missing.is_empty() {
        src.push(("missing_animations", toml::Value::from(missing)));
    }
    if !unplayable.is_empty() {
        src.push(("unplayable_sounds", toml::Value::from(unplayable)));
    }
    m.source = Some(source_table(src));
    let icon = match &ch.tray_icon {
        Some(t) => Some(icon_rgba(&t.mask_dib, &t.color_dib)?),
        None => None,
    };
    Model {
        manifest: m,
        pool,
        palette: Some(ch.palette.iter().map(|q| [q.r, q.g, q.b]).collect()),
        transparent: Some(ch.transparent),
        sounds,
        icon,
        drop_unused: false,
    }
    .finish(vec![format!("Converted by bentwire-convert from {file}")])
}
