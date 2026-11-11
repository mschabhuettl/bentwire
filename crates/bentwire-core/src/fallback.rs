//! Name resolution for Play (B-FB-1), derived office97 states (B-O97-5).

use crate::model::{CharacterModel, Family};

/// What a name resolves to.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Resolved {
    /// An animation, by its manifest spelling.
    Animation(String),
    /// A state or office97 group, by its manifest spelling (a member is drawn when the
    /// request runs).
    State(String),
}

/// B-FB-1 step 3 alias pairs (Office 97 type-library name / Office XP name).
pub const ALIASES: [(&str, &str); 13] = [
    ("Idle", "Idle1_1"),
    ("BeginSpeaking", "Explain"),
    ("CharacterSuccessMajor", "Congratulate"),
    ("GetAttentionMajor", "GetAttention"),
    ("GetAttentionMinor", "Wave"),
    ("Printing", "Print"),
    ("WritingNotingSomething", "Writing"),
    ("WorkingAtSomething", "Processing"),
    ("SendingMail", "SendMail"),
    ("ListensToComputer", "Hearing_1"),
    ("Disappear", "Hide"),
    ("Appear", "Show"),
    ("Saving", "Save"),
];

/// B-O97-5 derived states: Agent state to office97 group names (Office 97, Office XP).
const DERIVED: [(&str, &[&str]); 7] = [
    ("Showing", &["Appear", "Show"]),
    ("Hiding", &["Disappear", "Hide"]),
    ("GesturingLeft", &["GestureLeft"]),
    ("GesturingRight", &["GestureRight"]),
    ("GesturingUp", &["GestureUp"]),
    ("GesturingDown", &["GestureDown"]),
    ("Hearing", &["ListensToComputer", "Hearing_1"]),
];

/// Members of a state, with B-O97-5 for office97 manifests that do not define it.
/// `None` if the state does not exist at all.
pub fn state_members(model: &CharacterModel, name: &str) -> Option<Vec<String>> {
    if let Some(m) = model.state(name) {
        return Some(m.to_vec());
    }
    if model.family == Family::Office97 {
        let (_, groups) = DERIVED.iter().find(|(s, _)| s.eq_ignore_ascii_case(name))?;
        for g in *groups {
            if let Some(m) = model.state(g) {
                return Some(m.to_vec());
            }
        }
    }
    None
}

/// The canonical (manifest) spelling of a state name.
fn state_spelling(model: &CharacterModel, name: &str) -> Option<String> {
    model
        .states
        .iter()
        .find(|(n, m)| n.eq_ignore_ascii_case(name) && !m.is_empty())
        .map(|(n, _)| n.clone())
}

/// Steps 1 and 2.
fn steps_1_2(model: &CharacterModel, name: &str) -> Option<Resolved> {
    if let Some(i) = model.animation_index(name) {
        return Some(Resolved::Animation(model.animations[i].name.clone()));
    }
    if model.family == Family::Office97 {
        return state_spelling(model, name).map(Resolved::State);
    }
    None
}

/// Steps 1 to 3 (also the Animate! filter, B-MENU-4).
pub fn resolve_direct(model: &CharacterModel, name: &str) -> Option<Resolved> {
    if let Some(r) = steps_1_2(model, name) {
        return Some(r);
    }
    for (a, b) in ALIASES {
        let other = if a.eq_ignore_ascii_case(name) {
            b
        } else if b.eq_ignore_ascii_case(name) {
            a
        } else {
            continue;
        };
        if let Some(r) = steps_1_2(model, other) {
            return Some(r);
        }
    }
    None
}

/// The B-FB-1 fallback chain for a requested name (XP spelling, case-insensitive).
pub fn chain(name: &str) -> Vec<String> {
    let s = |v: &[&str]| v.iter().map(|x| x.to_string()).collect::<Vec<_>>();
    let n = name.to_ascii_lowercase();
    match n.as_str() {
        "idle1_1" => s(&["Idle", "Idle (1)", "state:IdlingLevel1"]),
        "greeting" => s(&["Greet", "Wave", "GetAttentionMinor", "Welcome"]),
        "goodbye" => s(&["Wave", "GetAttentionMinor"]),
        "explain" => s(&["Announce", "Acknowledge", "Suggest"]),
        "congratulate" => s(&["CharacterSucceeds", "Pleased"]),
        "getattention" => s(&["GetAttentionMinor", "Wave", "Alert"]),
        "wave" => s(&["GetAttentionMinor", "Greet"]),
        "searching" => s(&["Search", "ImageSearching"]),
        "print" => s(&["Printing", "Processing", "Writing"]),
        "writing" => s(&["Write"]),
        "processing" => s(&["Process", "Thinking", "Searching"]),
        "thinking" => s(&["Think"]),
        "sendmail" => s(&["Writing", "Write"]),
        "hearing_1" => s(&["ListensToComputer", "state:Hearing"]),
        "show" => s(&["state:Showing"]),
        "hide" => s(&["Disappear", "state:Hiding"]),
        "getartsy" => s(&["Writing", "Write"]),
        "gettechy" => s(&["Processing", "Process", "Thinking"]),
        "getwizardy" => s(&["DoMagic1", "Thinking", "Think"]),
        "checkingsomething" => s(&["Searching", "Search", "Thinking"]),
        "lookdown" | "lookup" | "lookleft" | "lookright" => vec![format!("{name}Blink")],
        "lookdownleft" | "lookdownright" | "lookupleft" | "lookupright" => {
            let (v, h) = if n.starts_with("lookdown") {
                ("LookDown", &name[8..])
            } else {
                ("LookUp", &name[6..])
            };
            vec![
                v.to_string(),
                format!("{v}Blink"),
                format!("Look{h}"),
                format!("Look{h}Blink"),
            ]
        }
        "save" => s(&["Writing", "Processing"]),
        "gestureright" | "gestureleft" | "gestureup" | "gesturedown" => {
            let d = &name[7..];
            vec![format!("state:Gesturing{d}"), format!("Point{d}")]
        }
        "emptytrash" => s(&["Processing", "Thinking"]),
        _ => Vec::new(),
    }
}

/// B-FB-1: resolve a Play name. `Err` carries the UnknownAnimation message.
pub fn resolve(model: &CharacterModel, name: &str) -> Result<Resolved, String> {
    if let Some(r) = resolve_direct(model, name) {
        return Ok(r);
    }
    let c = chain(name);
    for cand in &c {
        if let Some(st) = cand.strip_prefix("state:") {
            if state_members(model, st).is_some_and(|m| !m.is_empty()) {
                let spelled = model
                    .states
                    .iter()
                    .find(|(n, _)| n.eq_ignore_ascii_case(st))
                    .map(|(n, _)| n.clone())
                    .unwrap_or_else(|| st.to_string());
                return Ok(Resolved::State(spelled));
            }
        } else if let Some(r) = resolve_direct(model, cand) {
            return Ok(r);
        }
    }
    let tried = if c.is_empty() {
        String::from("no fallback chain")
    } else {
        format!("tried {}", c.join(", "))
    };
    Err(format!("Play(\"{name}\"): unknown animation ({tried})"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::Animation;

    fn ch(anims: &[&str], states: &[(&str, &[&str])]) -> CharacterModel {
        let mut m = CharacterModel::new("t", Family::Agent, (80, 80));
        for a in anims {
            m.animations.push(Animation {
                name: a.to_string(),
                available: true,
                ..Default::default()
            });
        }
        for (s, mem) in states {
            m.states
                .push((s.to_string(), mem.iter().map(|x| x.to_string()).collect()));
        }
        m
    }

    fn anim(s: &str) -> Result<Resolved, String> {
        Ok(Resolved::Animation(s.into()))
    }

    /// B-FB-2 vectors, with synthetic characters that have the relevant name sets.
    #[test]
    fn b_fb_2_rover_like() {
        let rover = ch(
            &[
                "Greet",
                "Acknowledge",
                "GetAttentionMinor",
                "Writing",
                "Idle1",
                "PointUp",
                "GestureUpX",
            ],
            &[
                ("IdlingLevel1", &["Idle1"]),
                ("GesturingUp", &["GestureUpX"]),
            ],
        );
        assert_eq!(resolve(&rover, "Greeting"), anim("Greet"));
        assert_eq!(resolve(&rover, "Explain"), anim("Acknowledge"));
        assert_eq!(resolve(&rover, "Goodbye"), anim("GetAttentionMinor"));
        assert_eq!(resolve(&rover, "Print"), anim("Writing"));
        assert!(resolve(&rover, "Hearing_1").is_err());
        assert_eq!(
            resolve(&rover, "GestureUp"),
            Ok(Resolved::State("GesturingUp".into()))
        );
    }

    #[test]
    fn b_fb_2_courtney_earl_merlin_qmark_like() {
        let courtney = ch(&["Idle (1)", "Thinking", "LookUp", "LookRight"], &[]);
        assert_eq!(resolve(&courtney, "Idle1_1"), anim("Idle (1)"));
        assert_eq!(resolve(&courtney, "Processing"), anim("Thinking"));
        assert_eq!(resolve(&courtney, "LookUpRight"), anim("LookUp"));

        let earl = ch(&["Printing", "ListensToComputer", "GetAttentionMinor"], &[]);
        assert_eq!(resolve(&earl, "Print"), anim("Printing"));
        assert_eq!(resolve(&earl, "Hearing_1"), anim("ListensToComputer"));
        assert_eq!(resolve(&earl, "GetAttention"), anim("GetAttentionMinor"));

        let merlin = ch(
            &["Wave", "DoMagic1", "Processing", "LookDown", "LookLeft"],
            &[],
        );
        assert_eq!(resolve(&merlin, "Goodbye"), anim("Wave"));
        assert_eq!(resolve(&merlin, "GetWizardy"), anim("DoMagic1"));
        assert_eq!(resolve(&merlin, "EmptyTrash"), anim("Processing"));
        assert_eq!(resolve(&merlin, "LookDownLeft"), anim("LookDown"));

        let qmark = ch(
            &["Welcome", "PointLeft", "Bye", "GL"],
            &[("Hiding", &["Bye"]), ("GesturingLeft", &["GL"])],
        );
        assert_eq!(resolve(&qmark, "Greeting"), anim("Welcome"));
        assert_eq!(
            resolve(&qmark, "GestureLeft"),
            Ok(Resolved::State("GesturingLeft".into()))
        );
        assert_eq!(
            resolve(&qmark, "Hide"),
            Ok(Resolved::State("Hiding".into()))
        );
        assert!(resolve(&qmark, "Wave").is_err());
        assert!(resolve(&qmark, "Show").is_err());
    }

    #[test]
    fn b_fb_2_office_names_resolve_to_themselves() {
        let names = [
            "Greeting",
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
            "Save",
            "GestureDown",
            "GestureLeft",
            "GestureUp",
            "GestureRight",
            "EmptyTrash",
            "Idle1_1",
            "Goodbye",
            "LookDown",
            "LookUp",
            "LookLeft",
            "LookRight",
            "LookDownLeft",
            "LookDownRight",
            "LookUpLeft",
            "LookUpRight",
            "RestPose",
        ];
        assert_eq!(names.len(), 35);
        let c = ch(&names, &[]);
        for n in names {
            assert_eq!(resolve(&c, n), anim(n));
            // Case-insensitive.
            assert_eq!(resolve(&c, &n.to_ascii_uppercase()), anim(n));
        }
    }

    #[test]
    fn b_fb_1_b_o97_5_alias_and_office97_groups() {
        let mut m = ch(
            &[],
            &[("Appear", &["Appear.1"]), ("Greeting", &["Greeting.1"])],
        );
        m.family = Family::Office97;
        // Step 2: group.
        assert_eq!(
            resolve(&m, "greeting"),
            Ok(Resolved::State("Greeting".into()))
        );
        // Step 3: alias Show -> Appear.
        assert_eq!(resolve(&m, "Show"), Ok(Resolved::State("Appear".into())));
        // B-O97-5: derived state Showing.
        assert_eq!(state_members(&m, "Showing"), Some(vec!["Appear.1".into()]));
        assert_eq!(state_members(&m, "Speaking"), None);
        // Steps 1 and 2 only for agent groups: a state name is not an animation.
        let a = ch(&[], &[("Greeting", &["X"])]);
        assert!(resolve(&a, "Greeting").is_err());
    }

    #[test]
    fn b_fb_1_error_lists_chain() {
        let c = ch(&[], &[]);
        let e = resolve(&c, "Goodbye").unwrap_err();
        assert!(
            e.contains("Goodbye") && e.contains("Wave, GetAttentionMinor"),
            "{e}"
        );
        assert!(resolve(&c, "RestPose").is_err());
        // Goodbye never falls back to Hide.
        let h = ch(&["Hide"], &[]);
        assert!(resolve(&h, "Goodbye").is_err());
    }
}
