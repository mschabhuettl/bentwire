//! T-2, transitively: the daemon's playback path (bentwire-core's player, fed by
//! `model.rs`) against bentwire-character's interpreter B, which is itself checked against
//! the Python oracle. Also the B-FB-2 resolution table on the converted ACS characters.
//! Both run on real packs (`BENTWIRE_PACKS`, A-TEST-1).

use crate::discovery::{self, Entry};
use bentwire_character::Character;
use bentwire_character::dump::{
    Dumper, SEEDS, SplitMix64 as BSplitMix, Step, TRACE_LIMIT, trace_params,
};
use bentwire_core::api::{Draws, Output};
use bentwire_core::fallback::{Resolved, resolve};
use bentwire_core::model::{self as cm, CharacterModel, Family};
use bentwire_core::player::{Env, Player, office_next};
use std::path::PathBuf;

fn packs() -> Vec<Entry> {
    let Some(dir) = std::env::var_os("BENTWIRE_PACKS") else {
        panic!("NOT RUN: set BENTWIRE_PACKS to a characters/ directory with packs");
    };
    discovery::discover(&[PathBuf::from(dir)])
}

/// Core draws over the shared splitmix64, counting the draws taken.
struct Counted {
    g: bentwire_core::draws::SplitMix64,
    n: usize,
}

impl Draws for Counted {
    fn draw(&mut self, k: u32) -> u32 {
        self.n += 1;
        self.g.draw(k)
    }
}

/// One step as compared: (picture hash or office97 pose, ms, sound hash).
type Frame = (String, u32, Option<String>);

enum Outcome {
    Equal,
    /// The core's exit run stopped at a revisited frame (A-CORE-6) where interpreter B,
    /// which has no such rule, goes on: to END or CUT within the trace cap (`false`), or
    /// until the cap, an endless exit loop (`true`).
    Core6(bool),
    /// Agent frames with `ms = 0` cannot be observed on the player (it shows them and
    /// moves on in the same timer callback); exit-mode traces of such animations are
    /// not comparable step by step.
    Skipped,
    Mismatch(String),
}

fn b_frames(steps: &[Step]) -> (Vec<Frame>, bool) {
    let mut v = Vec::new();
    let mut limit = false;
    for s in steps {
        match s {
            Step::Frame(p, ms, snd) => {
                let key = p
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| p.to_string());
                v.push((key, *ms, snd.clone()));
            }
            Step::Limit => limit = true,
            _ => {}
        }
    }
    (v, limit)
}

fn sound_hash(c: &Character, s: &Option<String>) -> Option<String> {
    s.as_ref().and_then(|id| c.sound(id)).map(|p| p.hash())
}

/// The daypart value interpreter B was given, as a local hour for the core.
fn hour_for(daypart: u32) -> u8 {
    [7, 12, 19, 23][daypart as usize % 4]
}

/// Agent family: the real player. Exit mode is entered the way the daemon does it, by
/// requesting another animation (B-SW-A); the target is a probe animation that draws
/// nothing, so the draw counts stay comparable.
fn compare_agent(
    c: &Character,
    d: &Dumper,
    m: &CharacterModel,
    a: usize,
    seed: u64,
    mode: u64,
) -> Outcome {
    let anim = &m.animations[a];
    let (exit_at, dp, _host, s) = trace_params(seed, mode, anim.frames.len());
    let mut bg = BSplitMix(s);
    let mut bn = 0usize;
    let steps = d.trace(
        &anim.name,
        &mut |k| {
            bn += 1;
            bg.draw(k)
        },
        exit_at,
        dp,
        u32::MAX,
        TRACE_LIMIT,
    );
    let (bf, limited) = b_frames(&steps);
    let has_zero = bf.iter().any(|f| f.1 == 0);
    if has_zero && exit_at.is_some() {
        return Outcome::Skipped;
    }
    let want: Vec<Frame> = bf.iter().filter(|f| f.1 > 0).cloned().collect();

    let mut model = m.clone();
    let probe = model.animations.len();
    model.animations.push(cm::Animation {
        name: "__probe".into(),
        frames: vec![cm::Frame {
            ms: 1,
            has_picture: true,
            ..Default::default()
        }],
        transition: cm::Transition::None,
        return_animation: None,
        available: true,
    });
    let mf = &c
        .manifest
        .animation(&anim.name)
        .expect("in manifest")
        .frames;
    let key = |i: usize| -> Frame {
        let f = &mf[i];
        (
            d.picture_hash(&c.frame_layers(f, None)),
            f.ms,
            sound_hash(c, &f.sound),
        )
    };
    let mut draws = Counted {
        g: bentwire_core::draws::SplitMix64::new(s),
        n: 0,
    };
    let mut out: Vec<Output> = Vec::new();
    let mut p = Player::new();
    let mut now = 0;
    macro_rules! env {
        () => {
            &mut Env {
                model: &model,
                draws: &mut draws,
                visible: true,
                sounds_on: false,
                hour: hour_for(dp),
                out: &mut out,
            }
        };
    }
    let mut got: Vec<Frame> = Vec::new();
    let mut shown = 0usize; // frames shown, including the 0 ms ones (B counts those)
    let record = |p: &Player, got: &mut Vec<Frame>, shown: &mut usize| {
        if p.busy()
            && let Some((x, i)) = p.current()
            && x == a
        {
            *shown += 1;
            if mf[i].ms > 0 {
                got.push(key(i));
            }
        }
    };
    p.play(now, env!(), a, true);
    record(&p, &mut got, &mut shown);
    let mut exited = false;
    loop {
        if let Some(e) = exit_at
            && !exited
            && shown >= e
        {
            exited = true;
            p.play(now, env!(), probe, false);
            record(&p, &mut got, &mut shown);
            continue;
        }
        if !p.busy() || p.current().is_some_and(|(x, _)| x == probe) || got.len() >= TRACE_LIMIT {
            break;
        }
        let Some(dl) = p.deadline() else { break };
        now = dl;
        p.tick(now, env!());
        record(&p, &mut got, &mut shown);
    }
    let n = want.len().min(got.len());
    if limited || got.len() >= TRACE_LIMIT {
        return if want[..n] != got[..n] {
            Outcome::Mismatch(first_diff(&want, &got))
        } else if exited && got.len() < want.len() && got.len() < TRACE_LIMIT {
            Outcome::Core6(true)
        } else {
            Outcome::Equal
        };
    }
    if want == got {
        if bn == draws.n {
            Outcome::Equal
        } else {
            Outcome::Mismatch(format!("draws: interpreter B {bn}, core {}", draws.n))
        }
    } else if exited && got.len() < want.len() && want[..got.len()] == got[..] {
        Outcome::Core6(false)
    } else {
        Outcome::Mismatch(first_diff(&want, &got))
    }
}

/// office97: the logical frame steps (before B-O97-3 coalescing), with the core's branch
/// walk `office_next` over the adapted model.
fn compare_office(
    c: &Character,
    d: &Dumper,
    m: &CharacterModel,
    a: usize,
    seed: u64,
    mode: u64,
) -> Outcome {
    let anim = &m.animations[a];
    let (exit_at, dp, _host, s) = trace_params(seed, mode, anim.frames.len());
    let mut bg = BSplitMix(s);
    let mut bn = 0usize;
    // Host branches are never taken in v1 (B-O97-1): give interpreter B an impossible
    // host value.
    let steps = d.trace(
        &anim.name,
        &mut |k| {
            bn += 1;
            bg.draw(k)
        },
        exit_at,
        dp,
        u32::MAX,
        TRACE_LIMIT,
    );
    let (want, limited) = b_frames(&steps);
    let mut draws = Counted {
        g: bentwire_core::draws::SplitMix64::new(s),
        n: 0,
    };
    let fr = &anim.frames;
    let mut got = Vec::new();
    let mut i = 0usize;
    let mut exiting = false;
    while i < fr.len() && got.len() < TRACE_LIMIT {
        let f = &fr[i];
        let ms = if exiting && f.ms > 99 { 100 } else { f.ms };
        let pose = f
            .pose
            .map(|p| p.to_string())
            .unwrap_or_else(|| "null".into());
        got.push((pose, ms, sound_hash(c, &f.sound)));
        if exit_at.is_some_and(|e| got.len() >= e) {
            exiting = true;
        }
        i = office_next(f, i, exiting, hour_for(dp), &mut draws);
    }
    if limited || got.len() >= TRACE_LIMIT {
        let n = want.len().min(got.len());
        return if want[..n] == got[..n] {
            Outcome::Equal
        } else {
            Outcome::Mismatch(first_diff(&want, &got))
        };
    }
    if want != got {
        Outcome::Mismatch(first_diff(&want, &got))
    } else if bn != draws.n {
        Outcome::Mismatch(format!("draws: interpreter B {bn}, core {}", draws.n))
    } else {
        Outcome::Equal
    }
}

fn first_diff(want: &[Frame], got: &[Frame]) -> String {
    let i = want.iter().zip(got).take_while(|(a, b)| a == b).count();
    format!(
        "step {i}: interpreter B {:?}, core {:?} (lengths {} and {})",
        want.get(i),
        got.get(i),
        want.len(),
        got.len()
    )
}

#[test]
#[ignore = "needs BENTWIRE_PACKS=<characters dir>"]
fn t_2_core_player_matches_interpreter_b() {
    let entries = packs();
    assert!(!entries.is_empty(), "no characters found");
    let (mut chars, mut anims, mut traces, mut equal, mut skipped) = (0, 0, 0, 0, 0);
    // A-CORE-6 cases: interpreter B ends within the cap / loops until the cap.
    let (mut core6_end, mut core6_loop) = (Vec::new(), Vec::new());
    let mut failures = Vec::new();
    for e in &entries {
        let c = Character::load(&e.path).unwrap_or_else(|err| panic!("{}: {err}", e.id));
        let m = crate::model::model_from(&e.id, &c);
        let d = Dumper::new(&c);
        chars += 1;
        for a in 0..m.animations.len() {
            if !m.animations[a].available || m.animations[a].frames.is_empty() {
                continue;
            }
            anims += 1;
            for seed in 0..SEEDS {
                for mode in 0..3 {
                    traces += 1;
                    let o = match m.family {
                        Family::Agent => compare_agent(&c, &d, &m, a, seed, mode),
                        Family::Office97 => compare_office(&c, &d, &m, a, seed, mode),
                    };
                    match o {
                        Outcome::Equal => equal += 1,
                        Outcome::Core6(endless) => {
                            let name = format!("{}/{}", e.id, m.animations[a].name);
                            if endless {
                                core6_loop.push(name)
                            } else {
                                // A-CORE-6 may only end an exit run whose END is
                                // unreachable; interpreter B reaching END is a finding.
                                failures.push(format!(
                                    "{name} seed {seed} mode {mode}: exit run ended by \
                                     A-CORE-6 although interpreter B reaches END"
                                ));
                                core6_end.push(name)
                            }
                        }
                        Outcome::Skipped => skipped += 1,
                        Outcome::Mismatch(why) => failures.push(format!(
                            "{} {} seed {seed} mode {mode}: {why}",
                            e.id, m.animations[a].name
                        )),
                    }
                }
            }
        }
    }
    eprintln!(
        "T-2 via the daemon path: {chars} characters, {anims} animations, {traces} traces: \
         {equal} equal, {} ended by A-CORE-6 ({} where interpreter B reaches END/CUT within \
         the cap, {} where it loops to the cap), {skipped} skipped (0 ms frames in exit \
         mode), {} mismatches",
        core6_end.len() + core6_loop.len(),
        core6_end.len(),
        core6_loop.len(),
        failures.len()
    );
    for (what, v) in [("B ends", &mut core6_end), ("B loops", &mut core6_loop)] {
        v.sort();
        v.dedup();
        eprintln!(
            "A-CORE-6, {what}: {} animations: {:?}",
            v.len(),
            &v[..v.len().min(12)]
        );
    }
    assert!(
        failures.is_empty(),
        "{:#?}",
        &failures[..failures.len().min(40)]
    );
}

/// B-FB-2 (spec 3.12) on the converted ACS characters, through the core's resolution
/// (B-FB-1) over the daemon's adapted model.
#[test]
#[ignore = "needs BENTWIRE_PACKS=<characters dir>"]
fn b_fb_2_table_on_converted_acs() {
    let entries = packs();
    let model = |id: &str| -> CharacterModel {
        let e = entries
            .iter()
            .find(|e| e.id == id)
            .unwrap_or_else(|| panic!("character {id} not in BENTWIRE_PACKS"));
        let c = Character::load(&e.path).unwrap();
        crate::model::model_from(id, &c)
    };
    let check = |m: &CharacterModel, name: &str, want: &str| -> Option<String> {
        let got = match resolve(m, name) {
            Ok(Resolved::Animation(a)) => a,
            Ok(Resolved::State(s)) => format!("state:{s}"),
            Err(_) => "error".into(),
        };
        (!got.eq_ignore_ascii_case(want)).then(|| format!("{} {name}: {got}, want {want}", m.id))
    };
    let table = [
        ("rover", "Greeting", "Greet"),
        ("rover", "Explain", "Acknowledge"),
        ("rover", "Goodbye", "GetAttentionMinor"),
        ("rover", "Print", "Writing"),
        ("rover", "Hearing_1", "error"),
        ("rover", "GestureUp", "state:GesturingUp"),
        ("courtney", "Idle1_1", "Idle (1)"),
        ("courtney", "Processing", "Thinking"),
        ("courtney", "LookUpRight", "LookUp"),
        ("earl", "Print", "Printing"),
        ("earl", "Hearing_1", "ListensToComputer"),
        ("earl", "GetAttention", "GetAttentionMinor"),
        ("merlin", "Goodbye", "Wave"),
        ("merlin", "GetWizardy", "DoMagic1"),
        ("merlin", "EmptyTrash", "Processing"),
        ("merlin", "LookDownLeft", "LookDown"),
        ("qmark", "Greeting", "Welcome"),
        ("qmark", "GestureLeft", "state:GesturingLeft"),
        ("qmark", "Hide", "state:Hiding"),
        ("qmark", "Wave", "error"),
        ("qmark", "Show", "error"),
    ];
    let mut failures = Vec::new();
    let mut n = 0;
    for (id, name, want) in table {
        n += 1;
        failures.extend(check(&model(id), name, want));
    }
    // "any Office ACS, any of the 35 names: itself" (office-assistant 1).
    const OFFICE_35: [&str; 35] = [
        "Idle1_1",
        "Greeting",
        "Explain",
        "Congratulate",
        "GetAttention",
        "Wave",
        "Searching",
        "Print",
        "GestureRight",
        "Writing",
        "Processing",
        "Thinking",
        "SendMail",
        "Hearing_1",
        "Show",
        "GetArtsy",
        "GetTechy",
        "GetWizardy",
        "CheckingSomething",
        "LookDown",
        "LookDownLeft",
        "LookDownRight",
        "LookLeft",
        "LookRight",
        "LookUp",
        "LookUpLeft",
        "LookUpRight",
        "Save",
        "GestureDown",
        "GestureLeft",
        "GestureUp",
        "EmptyTrash",
        "RestPose",
        "Goodbye",
        "Hide",
    ];
    for id in ["clippit", "dot", "f1", "logo", "mnature", "offcat", "rocky"] {
        let m = model(id);
        for name in OFFICE_35 {
            n += 1;
            failures.extend(check(&m, name, name));
        }
    }
    eprintln!("B-FB-2: {n} resolutions, {} failures", failures.len());
    assert!(failures.is_empty(), "{failures:#?}");
}
