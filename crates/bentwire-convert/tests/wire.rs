//! Wire, the builtin character (A-CHAR-1): reproducible, complete in both renderings, every
//! animation ends (or is an idle loop), mouths on the frames speech holds, and the committed
//! copy in characters/builtin is what the generator writes.

use bentwire_character::model::{Branch, Exit, Manifest};
use bentwire_character::write::PackageData;
use bentwire_character::{AGENT_STATES, Character};
use bentwire_convert::own::{LICENCE, Style};
use bentwire_convert::wire::{WIRE, wire};
use std::sync::OnceLock;

const STYLES: [Style; 2] = [Style::Modern, Style::Retro];

/// Each rendering built once per test binary (a build takes seconds in debug mode).
fn built(style: Style) -> &'static PackageData {
    static CELLS: [OnceLock<PackageData>; 2] = [OnceLock::new(), OnceLock::new()];
    CELLS[style as usize].get_or_init(|| wire(style).unwrap())
}

fn loaded(style: Style) -> &'static Character {
    static CELLS: [OnceLock<Character>; 2] = [OnceLock::new(), OnceLock::new()];
    CELLS[style as usize].get_or_init(|| built(style).load().unwrap())
}

/// The animations clients and the brief expect: the Agent standard names, plus the Office
/// names of the spec 3.12 fallback table and Animate! (B-MENU-4).
const NAMES: &[&str] = &[
    "Show",
    "Hide",
    "RestPose",
    "Speak",
    "Blink",
    "Idle1_1",
    "Idle1_2",
    "Idle1_3",
    "Idle1_4",
    "Idle2_1",
    "Idle2_2",
    "Idle3_1",
    "Idle3_2",
    "Greet",
    "Wave",
    "Explain",
    "Think",
    "Thinking",
    "Write",
    "Writing",
    "Search",
    "Searching",
    "Process",
    "Processing",
    "GetAttention",
    "Congratulate",
    "Alert",
    "Acknowledge",
    "Decline",
    "Confused",
    "Surprised",
    "Sad",
    "Pleased",
    "GestureLeft",
    "GestureRight",
    "GestureUp",
    "GestureDown",
    "LookLeft",
    "LookRight",
    "LookUp",
    "LookDown",
    "MoveLeft",
    "MoveRight",
    "MoveUp",
    "MoveDown",
    "Hearing_1",
    "StartListening",
    "StopListening",
    "Greeting",
    "Goodbye",
    "Print",
    "SendMail",
    "Save",
    "GetArtsy",
    "GetTechy",
    "GetWizardy",
    "CheckingSomething",
    "EmptyTrash",
    "LookUpLeft",
    "LookUpRight",
    "LookDownLeft",
    "LookDownRight",
];

#[test]
fn a_char_1_wire_is_reproducible_and_complete() {
    for style in STYLES {
        let a = built(style).to_bwc().unwrap();
        let b = wire(style).unwrap().to_bwc().unwrap();
        assert!(a == b, "{style:?}: two runs differ");
        let c = loaded(style);
        assert!(c.warnings.is_empty(), "{:?}", c.warnings);
        for s in AGENT_STATES {
            let members = c
                .manifest
                .state(s)
                .unwrap_or_else(|| panic!("state {s} missing"));
            assert!(
                !members.is_empty() && members.iter().all(|m| c.is_available(m)),
                "{s}"
            );
        }
        for n in NAMES {
            assert!(c.is_available(n), "{style:?}: {n}");
        }
        // Show starts with a sound (scripts/audio-backends.sh records it).
        assert!(
            c.manifest.animation("Show").unwrap().frames[0]
                .sound
                .is_some()
        );
        let rest = &c.manifest.animation("RestPose").unwrap().frames[0];
        let pic = c.compose_frame(rest, None, 1);
        assert_eq!((pic.width, pic.height), (128, 128));
        let partial = pic.data.chunks(4).any(|p| p[3] > 0 && p[3] < 255);
        match style {
            Style::Modern => {
                assert!(partial, "anti-aliased");
                let hi = c.compose_frame(rest, None, 2);
                assert_eq!((hi.width, hi.height), (256, 256));
                assert!(c.manifest.sheets.iter().all(|s| s.hidpi.contains_key(&2)));
            }
            Style::Retro => {
                assert!(!partial, "hard edges");
                assert!(c.manifest.sheets.iter().all(|s| s.hidpi.is_empty()));
                for (p, b) in built(style)
                    .files
                    .iter()
                    .filter(|(p, _)| p.ends_with(".png"))
                {
                    // IHDR colour type 3: indexed.
                    assert_eq!(b[25], 3, "{p} is not indexed");
                }
            }
        }
    }
}

/// Both renderings share one animation table: same names, timing, branches and states.
#[test]
fn a_char_1_retro_and_modern_share_the_animations() {
    let m = &built(Style::Modern).manifest;
    let r = &built(Style::Retro).manifest;
    assert_eq!(m.states, r.states);
    assert_eq!(m.animations.len(), r.animations.len());
    for (a, b) in m.animations.iter().zip(&r.animations) {
        assert_eq!(a.name, b.name);
        assert_eq!(a.transition, b.transition);
        assert_eq!(a.return_animation, b.return_animation);
        let shape = |f: &bentwire_character::model::Frame| {
            (
                f.ms,
                f.branch.clone(),
                f.exit,
                f.sound.clone(),
                f.mouths.len(),
            )
        };
        let (sa, sb): (Vec<_>, Vec<_>) = (
            a.frames.iter().map(shape).collect(),
            b.frames.iter().map(shape).collect(),
        );
        assert_eq!(sa, sb, "{}", a.name);
    }
}

/// No picture reaches the canvas edge (nothing of a pose is cut off).
#[test]
fn a_char_1_no_picture_touches_the_border() {
    let mut cut = Vec::new();
    for style in STYLES {
        let c = loaded(style);
        for a in &c.manifest.animations {
            for (i, f) in a.frames.iter().enumerate() {
                let p = c.compose_frame(f, None, 1);
                let edge = (0..128).any(|t| {
                    p.alpha(t, 0) > 0
                        || p.alpha(t, 127) > 0
                        || p.alpha(0, t) > 0
                        || p.alpha(127, t) > 0
                });
                if edge {
                    cut.push(format!("{style:?} {} frame {i}", a.name));
                }
            }
        }
    }
    assert!(cut.is_empty(), "{cut:#?}");
}

/// R-LIP-1, B-PICK-A, B-PIX-2: the frame that speech holds (the last frame of every Speaking
/// member, and the held ends of Explain, the gestures and RestPose) has all seven mouth
/// types, each drawn differently, inside the frame.
#[test]
fn r_lip_1_wire_speaks_with_every_mouth_type() {
    for style in STYLES {
        let c = loaded(style);
        let m = &c.manifest;
        let mut held: Vec<&str> = m
            .state("Speaking")
            .unwrap()
            .iter()
            .map(String::as_str)
            .collect();
        held.extend([
            "RestPose",
            "Explain",
            "GestureLeft",
            "GestureRight",
            "GestureUp",
            "GestureDown",
        ]);
        for name in held {
            let a = m.animation(name).unwrap();
            let f = a.frames.last().unwrap();
            let kinds: Vec<u8> = f.mouths.iter().map(|m| m.kind).collect();
            assert_eq!(kinds, (0..7).collect::<Vec<u8>>(), "{style:?} {name}");
            let plain = c.compose_frame(f, None, 1);
            let mut seen = vec![plain.data.clone()];
            for k in 0..7 {
                let pic = c.compose_frame(f, Some(k), 1);
                assert!(
                    !seen.contains(&pic.data),
                    "{style:?} {name}: mouth {k} not distinct"
                );
                // The overlay lies on the face: nothing changes where the frame is transparent.
                for i in 0..pic.data.len() / 4 {
                    if plain.data[i * 4 + 3] == 0 {
                        assert_eq!(pic.data[i * 4 + 3], 0, "{style:?} {name}: mouth {k}");
                    }
                }
                seen.push(pic.data);
            }
        }
        // Every Speak frame can take a mouth, so speech never shows a frame without one.
        let speak = m.animation("Speak").unwrap();
        assert!(speak.frames.iter().all(|f| f.mouths.len() == 7));
    }
}

/// Every animation that is not only an IdlingLevel member reaches END in normal mode under
/// any draw sequence (no reachable cycle, so request phases such as Speak end, R-SPEAK-1, and
/// `play Searching -- say` is not blocked, R-Q-6), and every animation leaves exit mode
/// (B-SW-A) from any frame. Return animations exist and have `exit = "end"` on every frame.
#[test]
fn b_sw_a_animations_end_and_exit() {
    for style in STYLES {
        let m = &built(style).manifest;
        let idle_only = |n: &String| {
            m.states
                .iter()
                .any(|(s, l)| s.starts_with("IdlingLevel") && l.contains(n))
                && !m
                    .states
                    .iter()
                    .any(|(s, l)| !s.starts_with("IdlingLevel") && l.contains(n))
        };
        let mut checked = 0;
        for a in &m.animations {
            let n = a.frames.len();
            let next = |i: usize, exiting: bool| -> Vec<usize> {
                let f = &a.frames[i];
                if exiting {
                    match f.exit {
                        Some(Exit::End) => return vec![n],
                        Some(Exit::Frame(t)) => return vec![t as usize],
                        None => {}
                    }
                }
                let mut v = Vec::new();
                let mut sum = 0;
                for b in &f.branch {
                    if let Branch::Random { target, p } = *b {
                        if p > 0 {
                            v.push(target as usize);
                        }
                        sum += p;
                    }
                }
                if sum < 100 {
                    v.push(i + 1);
                }
                v
            };
            // Depth-first over all paths; a path longer than n frames has a cycle.
            let ends = |start: usize, exiting: bool| {
                let mut stack = vec![(start, 0usize)];
                while let Some((i, depth)) = stack.pop() {
                    if i >= n {
                        continue;
                    }
                    if depth > n {
                        return false;
                    }
                    stack.extend(next(i, exiting).into_iter().map(|j| (j, depth + 1)));
                }
                true
            };
            if !idle_only(&a.name) {
                assert!(ends(0, false), "{style:?} {}: may never end", a.name);
                checked += 1;
            }
            for i in 0..n {
                assert!(
                    ends(i, true),
                    "{style:?} {} frame {i}: exit mode loops",
                    a.name
                );
            }
            if let Some(r) = &a.return_animation {
                assert!(m.animation(r).is_some(), "{}: return {r} missing", a.name);
                assert!(
                    a.frames.iter().all(|f| f.exit == Some(Exit::End)),
                    "{}",
                    a.name
                );
            }
        }
        assert!(checked > 60, "only {checked} animations checked");
        let returns = m
            .animations
            .iter()
            .filter(|a| a.return_animation.is_some())
            .count();
        assert!(returns >= 19, "{returns} return animations");
    }
}

#[test]
fn a_char_1_committed_builtin_matches_generator() {
    let pack = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../characters/builtin");
    assert_eq!(
        std::fs::read_to_string(pack.join("pack.toml")).unwrap(),
        bentwire_convert::own::BUILTIN_PACK_TOML
    );
    assert_eq!(
        std::fs::read_to_string(pack.join("LICENSE")).unwrap(),
        LICENCE
    );
    let mut ids: Vec<String> = std::fs::read_dir(&pack)
        .unwrap()
        .map(|e| e.unwrap())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    ids.sort();
    assert_eq!(
        ids,
        ["wire", "wire-retro"],
        "stale folders in characters/builtin"
    );
    for style in STYLES {
        let dir = pack.join(WIRE.id(style));
        let pkg = built(style);
        assert_eq!(
            std::fs::read_to_string(dir.join("character.toml")).unwrap(),
            pkg.manifest_text(),
            "run `cargo run -p bentwire-convert --bin gen-builtin`"
        );
        for (p, b) in &pkg.files {
            assert!(
                std::fs::read(dir.join(p)).unwrap() == *b,
                "{p} is stale: run gen-builtin"
            );
        }
        let c = Character::load(&dir).unwrap();
        assert!(c.warnings.is_empty());
        assert_manifest_ids(&c.manifest);
    }
}

fn assert_manifest_ids(m: &Manifest) {
    let en = m.character.info.iter().find(|i| i.lang == "en").unwrap();
    assert!(en.name.starts_with("Wire"));
    assert!(m.character.info.iter().any(|i| i.lang == "de"));
}
