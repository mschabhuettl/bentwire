//! Localisation and character selection helpers (B-L10N-*, B-MENU-2, B-CHAR-3/4, B-TRAY-1).

use crate::api::{Lang, LanguageSetting, LocaleEnv};
use crate::model::InfoEntry;

/// B-L10N-1.
pub fn resolve_language(setting: LanguageSetting, env: &LocaleEnv) -> Lang {
    match setting {
        LanguageSetting::De => Lang::De,
        LanguageSetting::En => Lang::En,
        LanguageSetting::Auto => {
            let v = [&env.lc_all, &env.lc_messages, &env.lang]
                .into_iter()
                .flatten()
                .find(|s| !s.is_empty());
            match v {
                Some(s) if s.starts_with("de") => Lang::De,
                _ => Lang::En,
            }
        }
    }
}

/// Menu item strings (B-MENU-2).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MenuString {
    Hide,
    Show,
    Options,
    ChooseAssistant,
    Animate,
}

pub fn menu_string(lang: Lang, s: MenuString) -> &'static str {
    use MenuString::*;
    match (lang, s) {
        (Lang::En, Hide) => "&Hide",
        (Lang::De, Hide) => "&Ausblenden",
        (Lang::En, Show) => "&Show",
        (Lang::De, Show) => "&Einblenden",
        (Lang::En, Options) => "&Options...",
        (Lang::De, Options) => "&Optionen...",
        (Lang::En, ChooseAssistant) => "&Choose Assistant...",
        (Lang::De, ChooseAssistant) => "Assistenten &wählen...",
        (Lang::En, Animate) => "&Animate!",
        (Lang::De, Animate) => "A&nimation!",
    }
}

/// B-L10N-2 button captions, by button id 0..14.
pub fn button_caption(lang: Lang, id: usize) -> &'static str {
    const EN: [&str; 15] = [
        "OK",
        "Cancel",
        "&Yes",
        "&No",
        "&Back",
        "&Next",
        "&Retry",
        "&Abort",
        "&Ignore",
        "&Search",
        "&Snooze",
        "Close",
        "&Tips",
        "&Options",
        "Yes To &All",
    ];
    const DE: [&str; 15] = [
        "OK",
        "Abbrechen",
        "&Ja",
        "&Nein",
        "&Zurück",
        "&Weiter",
        "&Wiederholen",
        "&Abbrechen",
        "&Ignorieren",
        "&Suchen",
        "&Erneut erinnern",
        "Schließen",
        "&Tipps",
        "&Optionen",
        "Ja, &alle",
    ];
    match lang {
        Lang::En => EN[id],
        Lang::De => DE[id],
    }
}

/// B-L10N-2 CLI result names, by button id.
pub const RESULT_NAMES: [&str; 15] = [
    "ok",
    "cancel",
    "yes",
    "no",
    "back",
    "next",
    "retry",
    "abort",
    "ignore",
    "search",
    "snooze",
    "close",
    "tips",
    "options",
    "yes-to-all",
];

/// B-GAL-1 gallery button captions: Back, Next, OK, Cancel.
pub fn gallery_captions(lang: Lang) -> [&'static str; 4] {
    match lang {
        Lang::En => ["<&Back", "&Next>", "OK", "Cancel"],
        Lang::De => ["<&Zurück", "&Weiter>", "OK", "Abbrechen"],
    }
}

fn primary(tag: &str) -> &str {
    tag.split(['-', '_']).next().unwrap_or("")
}

/// B-CHAR-3 for one field.
fn pick(info: &[InfoEntry], lang: Lang, field: fn(&InfoEntry) -> &str) -> String {
    let l = match lang {
        Lang::De => "de",
        Lang::En => "en",
    };
    if let Some(e) = info
        .iter()
        .find(|e| e.lang.eq_ignore_ascii_case(l) && !field(e).is_empty())
    {
        return field(e).to_string();
    }
    for p in [l, "en"] {
        if let Some(e) = info
            .iter()
            .rev()
            .find(|e| primary(&e.lang).eq_ignore_ascii_case(p))
        {
            return field(e).to_string();
        }
    }
    String::new()
}

/// B-CHAR-3: (name, description) for the UI language.
pub fn display_names(info: &[InfoEntry], lang: Lang) -> (String, String) {
    (
        pick(info, lang, |e| &e.name),
        pick(info, lang, |e| &e.description),
    )
}

/// B-CHAR-4: candidate ids in order (state file, config, `clippit`, sorted first).
/// The caller tries to load each in turn.
pub fn startup_candidates(
    state: Option<&str>,
    config: Option<&str>,
    sorted_ids: &[String],
) -> Vec<String> {
    let mut v: Vec<String> = Vec::new();
    for c in [state, config, Some("clippit")].into_iter().flatten() {
        let c = c.to_ascii_lowercase();
        if sorted_ids.contains(&c) && !v.contains(&c) {
            v.push(c);
        }
    }
    if let Some(f) = sorted_ids.first()
        && !v.contains(f)
    {
        v.push(f.clone());
    }
    v
}

/// B-TRAY-1: names of 64 or more characters are cut to 60 + "...".
pub fn tray_title(name: &str) -> String {
    if name.chars().count() >= 64 {
        let mut s: String = name.chars().take(60).collect();
        s.push_str("...");
        s
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn env(all: Option<&str>, msg: Option<&str>, lang: Option<&str>) -> LocaleEnv {
        LocaleEnv {
            lc_all: all.map(Into::into),
            lc_messages: msg.map(Into::into),
            lang: lang.map(Into::into),
        }
    }

    #[test]
    fn b_l10n_1_language() {
        use LanguageSetting::*;
        assert_eq!(resolve_language(De, &env(None, None, None)), Lang::De);
        assert_eq!(
            resolve_language(En, &env(Some("de_AT"), None, None)),
            Lang::En
        );
        assert_eq!(
            resolve_language(Auto, &env(None, None, Some("de_AT.UTF-8"))),
            Lang::De
        );
        assert_eq!(
            resolve_language(Auto, &env(Some("en_US"), None, Some("de_DE"))),
            Lang::En
        );
        assert_eq!(
            resolve_language(Auto, &env(None, Some("de_DE"), Some("en_US"))),
            Lang::De
        );
        assert_eq!(resolve_language(Auto, &env(None, None, None)), Lang::En);
        assert_eq!(
            resolve_language(Auto, &env(None, None, Some("C"))),
            Lang::En
        );
    }

    #[test]
    fn b_menu_2_strings() {
        assert_eq!(
            menu_string(Lang::De, MenuString::ChooseAssistant),
            "Assistenten &wählen..."
        );
        assert_eq!(menu_string(Lang::En, MenuString::Animate), "&Animate!");
        assert_eq!(menu_string(Lang::De, MenuString::Animate), "A&nimation!");
        assert_eq!(menu_string(Lang::De, MenuString::Show), "&Einblenden");
    }

    #[test]
    fn b_l10n_2_captions() {
        assert_eq!(button_caption(Lang::De, 10), "&Erneut erinnern");
        assert_eq!(button_caption(Lang::En, 14), "Yes To &All");
        assert_eq!(button_caption(Lang::De, 11), "Schließen");
        assert_eq!(RESULT_NAMES[14], "yes-to-all");
        assert_eq!(gallery_captions(Lang::De)[0], "<&Zurück");
    }

    fn ie(lang: &str, name: &str, desc: &str) -> InfoEntry {
        InfoEntry {
            lang: lang.into(),
            name: name.into(),
            description: desc.into(),
        }
    }

    #[test]
    fn b_char_3_display_names() {
        let info = vec![
            ie("en-US", "Pebble", "A stone"),
            ie("de-AT", "Kiesel AT", ""),
            ie("de-DE", "Kiesel", "Ein Stein"),
            ie("en-GB", "Pebble GB", "A British stone"),
        ];
        // Step 2: last entry with primary language de.
        assert_eq!(
            display_names(&info, Lang::De),
            ("Kiesel".into(), "Ein Stein".into())
        );
        // Step 2 for en (no exact "en"): last en entry.
        assert_eq!(
            display_names(&info, Lang::En),
            ("Pebble GB".into(), "A British stone".into())
        );
        // Step 1: exact tag with non-empty field wins.
        let info2 = vec![ie("de", "Exakt", ""), ie("de-DE", "Kiesel", "Ein Stein")];
        assert_eq!(
            display_names(&info2, Lang::De),
            ("Exakt".into(), "Ein Stein".into())
        );
        // Step 3 and 4.
        let info3 = vec![ie("fr", "Caillou", "x"), ie("en", "Pebble", "y")];
        assert_eq!(display_names(&info3, Lang::De).0, "Pebble");
        assert_eq!(display_names(&[ie("fr", "Caillou", "")], Lang::De).0, "");
    }

    #[test]
    fn b_char_4_startup_order() {
        let ids: Vec<String> = ["a", "b", "clippit", "z"].map(String::from).to_vec();
        assert_eq!(
            startup_candidates(Some("z"), Some("b"), &ids),
            vec!["z", "b", "clippit", "a"]
        );
        assert_eq!(
            startup_candidates(Some("gone"), None, &ids),
            vec!["clippit", "a"]
        );
        let ids2: Vec<String> = ["bentwire-placeholder"].map(String::from).to_vec();
        // A-CHAR-1: with only builtin installed, step 4 picks the placeholder.
        assert_eq!(
            startup_candidates(None, None, &ids2),
            vec!["bentwire-placeholder"]
        );
        assert!(startup_candidates(None, None, &[]).is_empty());
    }

    #[test]
    fn b_tray_1_title_cut() {
        let n63 = "x".repeat(63);
        assert_eq!(tray_title(&n63), n63);
        let n64 = "y".repeat(64);
        assert_eq!(tray_title(&n64), format!("{}...", "y".repeat(60)));
    }
}
