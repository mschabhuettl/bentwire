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
