//! The builtin placeholder (A-CHAR-1): reproducible, complete, and the committed copy in
//! characters/builtin is what the generator writes.

use bentwire_character::{AGENT_STATES, Character};
use bentwire_convert::placeholder::placeholder;

#[test]
fn placeholder_is_reproducible_and_complete() {
    let a = placeholder().unwrap().to_bwc().unwrap();
    let b = placeholder().unwrap().to_bwc().unwrap();
    assert_eq!(a, b);
    let c = placeholder().unwrap().load().unwrap();
    assert!(c.warnings.is_empty(), "{:?}", c.warnings);
    for s in AGENT_STATES {
        let members = c
            .manifest
            .state(s)
            .unwrap_or_else(|| panic!("state {s} missing"));
        assert!(members.iter().all(|m| c.is_available(m)), "{s}");
    }
    // Office names of the spec 3.12 fallback table that the placeholder covers directly.
    for n in [
        "Idle1_1",
        "Greeting",
        "Goodbye",
        "Explain",
        "Congratulate",
        "GetAttention",
        "Wave",
        "Searching",
        "Print",
        "Writing",
        "Processing",
        "Thinking",
        "SendMail",
        "Hearing_1",
        "Show",
        "Hide",
        "GetArtsy",
        "GetTechy",
        "GetWizardy",
        "CheckingSomething",
        "LookDown",
        "LookUp",
        "LookLeft",
        "LookRight",
        "LookDownLeft",
        "LookDownRight",
        "LookUpLeft",
        "LookUpRight",
        "Save",
        "GestureRight",
        "GestureLeft",
        "GestureUp",
        "GestureDown",
        "EmptyTrash",
        "RestPose",
    ] {
        assert!(c.is_available(n), "{n}");
    }
    let speak = c.manifest.animation("Speak").unwrap();
    assert_eq!(speak.frames[0].mouths.len(), 7);
    // Real alpha and a 2x sheet.
    let f = c.compose_frame(&speak.frames[0], None, 1);
    assert!(f.data.chunks(4).any(|p| p[3] > 0 && p[3] < 255));
    let hi = c.compose_frame(&speak.frames[0], Some(3), 2);
    assert_eq!((hi.width, hi.height), (192, 192));
}

#[test]
fn committed_builtin_matches_generator() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../characters/builtin/bentwire-placeholder");
    let pkg = placeholder().unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("character.toml")).unwrap(),
        pkg.manifest_text(),
        "run `cargo run -p bentwire-convert --bin gen-builtin`"
    );
    for (p, b) in &pkg.files {
        assert_eq!(
            &std::fs::read(dir.join(p)).unwrap(),
            b,
            "{p} is stale: run gen-builtin"
        );
    }
    let c = Character::load(&dir).unwrap();
    assert!(c.warnings.is_empty());
}

/// Every animation that is not only an IdlingLevel member reaches END in normal mode under
/// any draw sequence: no reachable cycle (e.g. a 100% back-branch), so request phases such
/// as Speak (R-SPEAK-1) end.
#[test]
fn non_idle_animations_reach_end_in_normal_mode() {
    use bentwire_character::model::Branch;
    let c = placeholder().unwrap().load().unwrap();
    let m = &c.manifest;
    let idle: Vec<&String> = m
        .states
        .iter()
        .filter(|(s, _)| s.starts_with("IdlingLevel"))
        .flat_map(|(_, l)| l)
        .collect();
    let used_elsewhere = |n: &String| {
        m.states
            .iter()
            .any(|(s, l)| !s.starts_with("IdlingLevel") && l.contains(n))
    };
    let mut checked = 0;
    for a in &m.animations {
        if idle.contains(&&a.name) && !used_elsewhere(&a.name) {
            continue;
        }
        let n = a.frames.len();
        // Successors of frame i in normal mode; `n` stands for END.
        let next = |i: usize| -> Vec<usize> {
            let mut v = Vec::new();
            let mut sum = 0;
            for b in &a.frames[i].branch {
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
        let mut stack = vec![(0usize, 0usize)];
        while let Some((i, depth)) = stack.pop() {
            if i >= n {
                continue;
            }
            assert!(depth < n, "{}: a draw sequence never reaches END", a.name);
            stack.extend(next(i).into_iter().map(|j| (j, depth + 1)));
        }
        checked += 1;
    }
    assert!(checked > 30, "only {checked} animations checked");
}
