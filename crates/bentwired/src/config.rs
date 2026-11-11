//! Config file (B-CFG-1, A-PACK-1 `character_paths`) and state file (B-CFG-2).

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(default)]
pub struct Config {
    pub character: Option<String>,
    pub language: String,
    pub sound_effects: bool,
    pub balloon: bool,
    pub balloon_font: String,
    pub speaking_speed: f64,
    pub idle: bool,
    pub double_click_ms: u32,
    pub character_paths: Vec<PathBuf>,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            character: None,
            language: "auto".into(),
            sound_effects: true,
            balloon: true,
            balloon_font: String::new(),
            speaking_speed: 1.0,
            idle: true,
            double_click_ms: 500,
            character_paths: Vec::new(),
        }
    }
}

/// Written by "Options..." when no config file exists (B-MENU-3).
pub const DEFAULT_CONFIG_TEXT: &str = "\
# bentwire configuration (all keys optional). Send SIGHUP to bentwired to reload.
# character = \"clippit\"
# language = \"auto\"        # auto, de, en
# sound_effects = true
# balloon = true
# balloon_font = \"\"        # empty: each character's own font
# speaking_speed = 1.0      # 0.5 to 1.5
# idle = true
# double_click_ms = 500
# character_paths = []      # extra search roots (A-PACK-1)
";

impl Config {
    /// Missing file gives the defaults; a broken file is an error (the caller logs it
    /// and keeps the previous config).
    pub fn load(path: &Path) -> Result<Config, String> {
        match std::fs::read_to_string(path) {
            Ok(text) => {
                let mut c: Config =
                    toml::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))?;
                c.speaking_speed = c.speaking_speed.clamp(0.5, 1.5);
                Ok(c)
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
            Err(e) => Err(format!("{}: {e}", path.display())),
        }
    }

    pub fn to_core(&self) -> bentwire_core::api::Config {
        use bentwire_core::api::LanguageSetting;
        bentwire_core::api::Config {
            character: self.character.clone(),
            language: match self.language.as_str() {
                "de" => LanguageSetting::De,
                "en" => LanguageSetting::En,
                _ => LanguageSetting::Auto,
            },
            sound_effects: self.sound_effects,
            balloon: self.balloon,
            balloon_font: self.balloon_font.clone(),
            speaking_speed: self.speaking_speed,
            idle: self.idle,
            double_click_ms: self.double_click_ms,
        }
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct State {
    pub character: Option<String>,
    pub left: Option<i32>,
    pub top: Option<i32>,
    pub visible: Option<bool>,
}

impl State {
    pub fn load(path: &Path) -> State {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|t| toml::from_str(&t).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let tmp = path.with_extension("toml.tmp");
        std::fs::write(&tmp, toml::to_string(self).expect("state serializes"))?;
        std::fs::rename(tmp, path)
    }
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var_os(var) {
        Some(v) if Path::new(&v).is_absolute() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

fn home() -> PathBuf {
    std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| "/".into())
}

pub fn config_path() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("bentwire/config.toml")
}

pub fn state_path() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("bentwire/state.toml")
}

pub fn data_home() -> PathBuf {
    xdg("XDG_DATA_HOME", ".local/share")
}

pub fn data_dirs() -> Vec<PathBuf> {
    let v = std::env::var("XDG_DATA_DIRS").unwrap_or_default();
    let v = if v.is_empty() {
        "/usr/local/share:/usr/share".into()
    } else {
        v
    };
    v.split(':')
        .filter(|s| !s.is_empty())
        .map(PathBuf::from)
        .collect()
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    #[test]
    fn b_cfg_1_defaults_and_parse() {
        let c: Config = toml::from_str("").unwrap();
        assert_eq!(c, Config::default());
        let c: Config =
            toml::from_str("character = \"x\"\nspeaking_speed = 9.0\ndouble_click_ms = 300")
                .unwrap();
        assert_eq!(c.character.as_deref(), Some("x"));
        assert_eq!(c.double_click_ms, 300);
        let dir = tempdir();
        let p = dir.join("config.toml");
        std::fs::write(&p, "speaking_speed = 9.0").unwrap();
        assert_eq!(Config::load(&p).unwrap().speaking_speed, 1.5);
        assert_eq!(
            Config::load(&dir.join("missing")).unwrap(),
            Config::default()
        );
        std::fs::write(&p, "nonsense = [").unwrap();
        assert!(Config::load(&p).is_err());
        let d: Config = toml::from_str(DEFAULT_CONFIG_TEXT).unwrap();
        assert_eq!(d, Config::default());
    }

    #[test]
    fn b_cfg_2_state_round_trip() {
        let dir = tempdir();
        let p = dir.join("sub/state.toml");
        let s = State {
            character: Some("a".into()),
            left: Some(-5),
            top: Some(7),
            visible: Some(true),
        };
        s.save(&p).unwrap();
        assert_eq!(State::load(&p), s);
    }

    pub(crate) fn tempdir() -> PathBuf {
        let d = std::env::temp_dir().join(format!(
            "bentwired-test-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&d).unwrap();
        d
    }
}
