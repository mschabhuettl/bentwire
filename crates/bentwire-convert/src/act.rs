//! ACT (Office 97 and Microsoft Bob actors) to bentwire (character-format 7.3, 9.4, 10 "ACT",
//! decisions D2 and D6).

use std::collections::BTreeMap;

use agent_formats::act::{
    ActFile, CMD_DAYPART, CMD_FRAME, CMD_HOST, CMD_RANDOM, CMD_SOUND, Command, NONE, Payload,
};
use bentwire_character::*;

use crate::Error;
use crate::acs::{opt_str, sounds, source_table};
use crate::build::*;

/// Office XP's Agent animation name for an Office 97 `MsoAnimationType` id (act-format 6).
pub fn office_xp_name(id: u16) -> Option<&'static str> {
    Some(match id {
        1 => "Idle1_1",
        2 => "Greeting",
        3 => "Goodbye",
        4 => "Explain",
        6 => "Congratulate",
        11 => "GetAttention",
        12 => "Wave",
        13 => "Searching",
        18 => "Print",
        19 => "GestureRight",
        22 => "Writing",
        23 => "Processing",
        24 => "Thinking",
        25 => "SendMail",
        26 => "Hearing_1",
        31 => "Hide",
        32 => "Show",
        100 => "GetArtsy",
        101 => "GetTechy",
        102 => "GetWizardy",
        103 => "CheckingSomething",
        104 => "LookDown",
        105 => "LookDownLeft",
        106 => "LookDownRight",
        107 => "LookLeft",
        108 => "LookRight",
        109 => "LookUp",
        110 => "LookUpLeft",
        111 => "LookUpRight",
        112 => "Save",
        113 => "GestureDown",
        114 => "GestureLeft",
        115 => "GestureUp",
        116 => "EmptyTrash",
        _ => return None,
    })
}

/// Agent state aliases added for Office actors (D6, character-format 10).
pub const STATE_ALIASES: [(&str, u16); 7] = [
    ("Showing", 32),
    ("Hiding", 31),
    ("GesturingLeft", 114),
    ("GesturingRight", 19),
    ("GesturingUp", 115),
    ("GesturingDown", 113),
    ("Hearing", 26),
];

/// Picture source for poses.
pub enum Pictures<'a> {
    /// Bitmap actor: the Office palette (256 entries, index 10 transparent).
    Bitmaps(&'a [[u8; 3]; 256]),
    /// Vector actor: each pose already rasterised at the display size (1x) and, optionally, at
    /// 2x, as indexed pictures with one palette.
    Raster {
        palette: Vec<[u8; 3]>,
        transparent: u8,
        /// Pose id to (1x pixels, 2x pixels).
        poses: BTreeMap<u16, (Vec<u8>, Option<Vec<u8>>)>,
    },
}

/// Options that cannot be read from an ACT file.
pub struct ActOptions {
    /// Language tag for `character.info` (ACT has none).
    pub lang: String,
    /// Decoder for the item name and the v1 (ANSI) strings.
    pub codepage: &'static encoding_rs::Encoding,
}

impl Default for ActOptions {
    fn default() -> Self {
        ActOptions {
            lang: "en".into(),
            codepage: encoding_rs::WINDOWS_1252,
        }
    }
}

struct Unit {
    sound: Option<(u16, u16)>,
    pose: u16,
    ms: u16,
    branches: Vec<Command>,
}

/// Split a script into units `[sound] frame branch*` (character-format 7.3).
fn units(cmds: &[Command], what: &str) -> Result<(Vec<Unit>, BTreeMap<usize, u32>), Error> {
    let mut out = Vec::new();
    let mut start = BTreeMap::new();
    let mut i = 0;
    while i < cmds.len() {
        let first = i;
        let mut sound = None;
        if cmds[i].kind == CMD_SOUND {
            sound = Some((cmds[i].a, cmds[i].b));
            i += 1;
        }
        if i >= cmds.len() || cmds[i].kind != CMD_FRAME {
            return Err(Error::Source(format!(
                "{what}: script does not fit `[sound] frame branch*` at command {i}"
            )));
        }
        let (pose, ms) = (cmds[i].a, cmds[i].b);
        i += 1;
        let mut branches = Vec::new();
        while i < cmds.len() && [CMD_RANDOM, CMD_HOST, CMD_DAYPART].contains(&cmds[i].kind) {
            branches.push(cmds[i]);
            i += 1;
        }
        start.insert(first, out.len() as u32);
        out.push(Unit {
            sound,
            pose,
            ms,
            branches,
        });
    }
    for u in &out {
        for b in &u.branches {
            if !start.contains_key(&(b.a as usize)) {
                return Err(Error::Source(format!(
                    "{what}: branch target {} is not the start of a unit",
                    b.a
                )));
            }
        }
    }
    Ok((out, start))
}

/// Convert a parsed ACT file.
pub fn convert_act(
    act: &ActFile,
    pictures: &Pictures,
    opts: &ActOptions,
    file: &str,
    sha: &str,
) -> Result<PackageData, Error> {
    let h = &act.header;
    if h.logical_w <= 0 || h.logical_h <= 0 || h.default_px_w <= 0 || h.default_px_h <= 0 {
        return Err(Error::Source("non-positive logical or pixel size".into()));
    }
    let (lw, lh) = (h.logical_w as u32, h.logical_h as u32);
    let (dw, dh) = (h.default_px_w as u32, h.default_px_h as u32);
    let mut m = empty_manifest(Family::Office97, [lw, lh]);
    m.character.display = Some([dw, dh]);
    let office = h.kind == 7;
    let gname = |id: u16| -> String {
        office
            .then(|| office_xp_name(id))
            .flatten()
            .map_or_else(|| format!("Anim{id}"), str::to_owned)
    };
    let default_arg = if act.major == 1 { 0x0100 } else { 0x0080 };
    let snd = sounds(
        &act.sounds
            .iter()
            .map(|s| s.bytes.as_slice())
            .collect::<Vec<_>>(),
        true,
    )?;

    let mut used: Vec<u16> = Vec::new();
    let mut use_pose = |p: u16| {
        if !used.contains(&p) {
            used.push(p);
        }
    };
    let mut ids = Vec::new();
    let mut group_members: Vec<(u16, String, Vec<String>)> = Vec::new();
    for g in &act.animations {
        let name = gname(g.id);
        let mut members = Vec::new();
        for (vi, script) in g.variants.iter().enumerate() {
            let an = if g.variants.len() == 1 {
                name.clone()
            } else {
                format!("{name}.{}", vi + 1)
            };
            let (us, start) = units(&script.commands, &an)?;
            let mut frames = Vec::new();
            for u in &us {
                let mut f = Frame {
                    ms: u.ms as u32,
                    ..Frame::default()
                };
                if u.pose != NONE {
                    use_pose(u.pose);
                    f.pose = Some(u.pose as u32);
                }
                if let Some((s, arg)) = u.sound {
                    if s != NONE {
                        f.sound = snd.ids.get(s as usize).cloned().flatten();
                        if f.sound.is_none() {
                            return Err(Error::Source(format!("{an}: sound {s} missing")));
                        }
                    }
                    if arg != default_arg || s == NONE {
                        f.sound_arg = Some(arg as u32);
                    }
                }
                f.branch = u
                    .branches
                    .iter()
                    .map(|b| {
                        let target = start[&(b.a as usize)];
                        match b.kind {
                            CMD_RANDOM => Branch::Random {
                                target,
                                p: b.b as u32,
                            },
                            CMD_HOST => Branch::Host {
                                target,
                                value: b.b as u32,
                            },
                            _ => Branch::Daypart {
                                target,
                                value: b.b as u32,
                            },
                        }
                    })
                    .collect();
                frames.push(f);
            }
            m.animations.push(Animation {
                name: an.clone(),
                transition: None,
                return_animation: None,
                frames,
            });
            members.push(an);
        }
        ids.push((name.clone(), g.id as u32));
        group_members.push((g.id, name, members));
    }
    m.states = group_members
        .iter()
        .map(|(_, n, l)| (n.clone(), l.clone()))
        .collect();
    if office {
        for (alias, id) in STATE_ALIASES {
            if let Some((_, _, l)) = group_members.iter().find(|(g, _, _)| *g == id) {
                m.states.push((alias.to_owned(), l.clone()));
            }
        }
    }
    if h.rest_pose != NONE && (h.rest_pose as usize) < act.payloads.len() {
        use_pose(h.rest_pose);
    }
    for t in &act.transitions {
        use_pose(t.from_pose);
        use_pose(t.to_pose);
    }

    // Strings (act-format 4.12): v2 UTF-16LE, v1 ANSI in the build codepage.
    let text = |raw: &[u8]| -> String {
        act.string_text(raw).unwrap_or_else(|| {
            opts.codepage
                .decode_without_bom_handling(raw)
                .0
                .into_owned()
        })
    };
    let strings: BTreeMap<u32, Vec<String>> = act
        .strings
        .iter()
        .map(|g| (g.id as u32, g.variants.iter().map(|v| text(v)).collect()))
        .collect();
    let item_name = opts
        .codepage
        .decode_without_bom_handling(&act.item_name)
        .0
        .into_owned();
    let first = |id: u32| strings.get(&id).and_then(|v| v.first()).cloned();
    m.character.info = vec![Info {
        lang: opts.lang.clone(),
        name: first(1).unwrap_or_else(|| item_name.clone()),
        description: first(8).and_then(|d| opt_str(&d)),
        extra: None,
    }];
    m.office97 = Some(Office97 {
        rest_pose: Some(h.rest_pose as u32),
        idle: gname(h.idle_anim),
        transitions: act
            .transitions
            .iter()
            .map(|t| (t.from_pose as u32, t.to_pose as u32, gname(t.anim_id)))
            .collect(),
        ids,
        strings: strings
            .iter()
            .filter(|(k, _)| **k != 1 && **k != 8)
            .map(|(k, v)| (*k, v.clone()))
            .collect(),
    });

    // Poses.
    let mut pool = Pool::default();
    let mut img_ix: BTreeMap<u16, u32> = BTreeMap::new();
    let mut hidpi2 = std::collections::HashMap::new();
    let (palette, transparent) = match pictures {
        Pictures::Bitmaps(pal) => (pal.to_vec(), 10u8),
        Pictures::Raster {
            palette,
            transparent,
            ..
        } => (palette.clone(), *transparent),
    };
    used.sort_unstable();
    for &pid in &used {
        let layers = match pictures {
            Pictures::Raster { poses, .. } => {
                let (one, two) = poses
                    .get(&pid)
                    .ok_or_else(|| Error::Source(format!("pose {pid} not rasterised")))?;
                let i = pool.add(Pic {
                    w: dw,
                    h: dh,
                    rgba: false,
                    px: one.clone(),
                });
                if let Some(two) = two {
                    hidpi2.insert(
                        i,
                        Pic {
                            w: dw * 2,
                            h: dh * 2,
                            rgba: false,
                            px: two.clone(),
                        },
                    );
                }
                vec![pool_layer(i, 0, 0, Some([lw, lh]))]
            }
            Pictures::Bitmaps(_) => {
                let mut image = |id: u16| -> Result<u32, Error> {
                    if let Some(&i) = img_ix.get(&id) {
                        return Ok(i);
                    }
                    let Some(Payload::Bitmap(b)) = act.payloads.get(id as usize) else {
                        return Err(Error::Source(format!(
                            "pose {pid}: payload {id} is not a bitmap"
                        )));
                    };
                    let i = pool.add(Pic {
                        w: b.width,
                        h: b.height,
                        rgba: false,
                        px: b.pixels.clone(),
                    });
                    img_ix.insert(id, i);
                    Ok(i)
                };
                match act.payloads.get(pid as usize) {
                    Some(Payload::Composite(children)) => {
                        let mut v = Vec::new();
                        for c in children {
                            if matches!(
                                act.payloads.get(c.id as usize),
                                Some(Payload::Composite(_))
                            ) {
                                return Err(Error::Source(format!("pose {pid}: nested composite")));
                            }
                            let (w, hh) = (
                                c.right as i32 - c.left as i32,
                                c.bottom as i32 - c.top as i32,
                            );
                            if w <= 0 || hh <= 0 {
                                return Err(Error::Source(format!(
                                    "pose {pid}: empty child rectangle"
                                )));
                            }
                            v.push(pool_layer(
                                image(c.id)?,
                                c.left as i32,
                                c.top as i32,
                                Some([w as u32, hh as u32]),
                            ));
                        }
                        v
                    }
                    Some(Payload::Bitmap(_)) => vec![pool_layer(image(pid)?, 0, 0, Some([lw, lh]))],
                    _ => {
                        return Err(Error::Source(format!(
                            "pose {pid}: unknown or vector payload"
                        )));
                    }
                }
            }
        };
        m.poses.insert(pid as u32, layers);
    }

    let hdr: toml::Table = [
        ("kind", h.kind as i64),
        ("unk10", h.unk10 as i64),
        ("unk12", h.unk12 as i64),
        ("unk14", h.unk14 as i64),
        ("unk16", h.unk16 as i64),
        ("rect_18", h.rect[0] as i64),
        ("rect_1a", h.rect[1] as i64),
        ("rect_1c", h.rect[2] as i64),
        ("rect_1e", h.rect[3] as i64),
        ("nohit_image", h.nohit_image as i64),
        ("stretch_x0", h.stretch_x0 as i64),
        ("stretch_x1", h.stretch_x1 as i64),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), toml::Value::from(v)))
    .collect();
    let mut src = vec![
        (
            "format",
            toml::Value::from(format!("act-{}.{}", act.major, act.minor)),
        ),
        ("file", toml::Value::from(file)),
        ("sha256_16", toml::Value::from(sha)),
        ("item_name", toml::Value::from(item_name)),
        ("header", toml::Value::Table(hdr)),
    ];
    if matches!(pictures, Pictures::Raster { .. }) {
        src.push(("wmf_raster", toml::Value::from("bentwire, aliased")));
    }
    m.source = Some(source_table(src));
    Model {
        manifest: m,
        pool,
        palette: Some(palette),
        transparent: Some(transparent),
        sounds: snd.list,
        icon: None,
        drop_unused: true,
        hidpi2,
    }
    .finish(vec![format!("Converted by bentwire-convert from {file}")])
}
