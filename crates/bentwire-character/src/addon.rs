//! Add-ons (`docs/addon-format.md`, spec B-ADDON-1 and B-ADDON-3): the `addon.toml`
//! manifest, the installed add-ons of a directory and the list of enabled ones.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// The manifest file at the top of an add-on folder.
pub const MANIFEST: &str = "addon.toml";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Character,
    BalloonTheme,
    Bridge,
}

impl Kind {
    pub fn as_str(self) -> &'static str {
        match self {
            Kind::Character => "character",
            Kind::BalloonTheme => "balloon-theme",
            Kind::Bridge => "bridge",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Addon {
    pub name: String,
    pub kind: Kind,
    pub version: String,
    /// The argv of a bridge; empty for the other kinds.
    pub command: Vec<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Raw {
    name: String,
    kind: String,
    version: String,
    command: Option<Vec<String>>,
}

/// One path component of the form `[a-z0-9][a-z0-9._-]*` without `..`.
pub fn valid_name(name: &str) -> bool {
    let mut b = name.bytes();
    b.next()
        .is_some_and(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        && b.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"._-".contains(&c))
        && !name.contains("..")
}

impl Addon {
    pub fn parse(text: &str) -> Result<Addon, String> {
        let r: Raw = toml::from_str(text).map_err(|e| format!("{MANIFEST}: {e}"))?;
        if !valid_name(&r.name) {
            return Err(format!(
                "{MANIFEST}: name {:?} is not [a-z0-9][a-z0-9._-]* without \"..\"",
                r.name
            ));
        }
        let kind = match r.kind.as_str() {
            "character" => Kind::Character,
            "balloon-theme" => Kind::BalloonTheme,
            "bridge" => Kind::Bridge,
            k => return Err(format!("{MANIFEST}: unknown kind {k:?}")),
        };
        if r.version.trim().is_empty() {
            return Err(format!("{MANIFEST}: version is empty"));
        }
        let command = match (kind, r.command) {
            (Kind::Bridge, Some(c)) if c.first().is_some_and(|p| !p.is_empty()) => c,
            (Kind::Bridge, _) => return Err(format!("{MANIFEST}: a bridge needs a command")),
            (_, None) => Vec::new(),
            (_, Some(_)) => return Err(format!("{MANIFEST}: only a bridge has a command")),
        };
        Ok(Addon {
            name: r.name,
            kind,
            version: r.version,
            command,
        })
    }

    /// The manifest of the add-on folder `dir`, whose name must be the add-on's name.
    pub fn load(dir: &Path) -> Result<Addon, String> {
        let file = dir.join(MANIFEST);
        let text =
            std::fs::read_to_string(&file).map_err(|e| format!("{}: {e}", file.display()))?;
        let a = Addon::parse(&text).map_err(|e| format!("{}: {e}", dir.display()))?;
        if dir.file_name().and_then(|n| n.to_str()) != Some(a.name.as_str()) {
            return Err(format!(
                "{}: the folder is not named after the add-on {:?}",
                dir.display(),
                a.name
            ));
        }
        Ok(a)
    }
}

/// The add-on folders in `dir` (entries starting with `.` are skipped), sorted by name, each
/// with its manifest or why it does not load.
pub fn installed(dir: &Path) -> Vec<(PathBuf, Result<Addon, String>)> {
    let mut v: Vec<PathBuf> = match std::fs::read_dir(dir) {
        Ok(rd) => rd
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| {
                p.is_dir()
                    && !p
                        .file_name()
                        .is_some_and(|n| n.as_encoded_bytes().starts_with(b"."))
            })
            .collect(),
        Err(_) => Vec::new(),
    };
    v.sort();
    v.into_iter()
        .map(|p| {
            let a = Addon::load(&p);
            (p, a)
        })
        .collect()
}

#[derive(Serialize, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct EnabledFile {
    #[serde(default)]
    enabled: Vec<String>,
}

/// The names in the enabled list `file` (`enabled = [...]`); none when it does not exist.
pub fn read_enabled(file: &Path) -> Result<Vec<String>, String> {
    match std::fs::read_to_string(file) {
        Ok(t) => toml::from_str::<EnabledFile>(&t)
            .map(|f| f.enabled)
            .map_err(|e| format!("{}: {e}", file.display())),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Vec::new()),
        Err(e) => Err(format!("{}: {e}", file.display())),
    }
}

/// Write the enabled list (temp file, then rename), sorted and without duplicates.
pub fn write_enabled(file: &Path, names: &[String]) -> std::io::Result<()> {
    let mut enabled = names.to_vec();
    enabled.sort();
    enabled.dedup();
    if let Some(d) = file.parent() {
        std::fs::create_dir_all(d)?;
    }
    let text = format!(
        "# Enabled add-ons, written by `bentwire addons enable` and `disable`.\n{}",
        toml::to_string(&EnabledFile { enabled }).expect("list serializes")
    );
    let tmp = file.with_extension("toml.tmp");
    std::fs::write(&tmp, text)?;
    std::fs::rename(tmp, file)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(t: &str) -> Result<Addon, String> {
        Addon::parse(t)
    }

    #[test]
    fn b_addon_1_manifest() {
        let a = parse("name = \"wire\"\nkind = \"character\"\nversion = \"1.0\"\n").unwrap();
        assert_eq!(
            a,
            Addon {
                name: "wire".into(),
                kind: Kind::Character,
                version: "1.0".into(),
                command: vec![]
            }
        );
        let t = parse("name = \"x\"\nkind = \"balloon-theme\"\nversion = \"2\"").unwrap();
        assert_eq!(t.kind.as_str(), "balloon-theme");
        let b = parse(
            "name = \"notify-1.2_x\"\nkind = \"bridge\"\nversion = \"0.1\"\n\
             command = [\"./run\", \"--flag\"]",
        )
        .unwrap();
        assert_eq!((b.kind, b.command.len()), (Kind::Bridge, 2));
    }

    #[test]
    fn b_addon_1_manifest_rejected() {
        let ok_rest = "kind = \"character\"\nversion = \"1\"";
        for name in [
            "", "Wire", "..", "a..b", ".x", "-x", "_x", "a/b", "a b", "ä", "a\\b",
        ] {
            let e = parse(&format!("name = {name:?}\n{ok_rest}")).unwrap_err();
            assert!(e.contains("is not"), "{name}: {e}");
        }
        let bad = [
            (
                "name = \"a\"\nkind = \"plugin\"\nversion = \"1\"",
                "unknown kind",
            ),
            (
                "name = \"a\"\nkind = \"character\"\nversion = \" \"",
                "version is empty",
            ),
            ("name = \"a\"\nkind = \"character\"", "version"),
            (
                "name = \"a\"\nkind = \"bridge\"\nversion = \"1\"",
                "needs a command",
            ),
            (
                "name = \"a\"\nkind = \"bridge\"\nversion = \"1\"\ncommand = []",
                "needs a command",
            ),
            (
                "name = \"a\"\nkind = \"bridge\"\nversion = \"1\"\ncommand = [\"\"]",
                "needs a command",
            ),
            (
                "name = \"a\"\nkind = \"bridge\"\nversion = \"1\"\ncommand = \"x\"",
                "invalid type",
            ),
            (
                "name = \"a\"\nkind = \"character\"\nversion = \"1\"\ncommand = [\"x\"]",
                "only a bridge",
            ),
            (
                "name = \"a\"\nkind = \"character\"\nversion = \"1\"\ncomand = [\"x\"]",
                "unknown field",
            ),
            ("name = [", "addon.toml"),
        ];
        for (t, want) in bad {
            let e = parse(t).unwrap_err();
            assert!(e.contains(want), "{t}: {e}");
        }
    }

    #[test]
    fn b_addon_3_installed_and_enabled_list() {
        let d = std::env::temp_dir().join(format!("bw-addon-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        let put = |dir: &str, text: &str| {
            std::fs::create_dir_all(d.join(dir)).unwrap();
            std::fs::write(d.join(dir).join(MANIFEST), text).unwrap();
        };
        put("b", "name = \"b\"\nkind = \"character\"\nversion = \"1\"");
        put(
            "a",
            "name = \"other\"\nkind = \"character\"\nversion = \"1\"",
        );
        put(
            ".install-1",
            "name = \"x\"\nkind = \"character\"\nversion = \"1\"",
        );
        std::fs::write(d.join("file"), "").unwrap();
        let v = installed(&d);
        assert_eq!(v.len(), 2);
        assert!(v[0].1.as_ref().unwrap_err().contains("not named after"));
        assert_eq!(v[1].1.as_ref().unwrap().name, "b");
        let f = d.join("cfg/addons.toml");
        assert_eq!(read_enabled(&f).unwrap(), Vec::<String>::new());
        write_enabled(&f, &["z".into(), "b".into(), "z".into()]).unwrap();
        assert_eq!(read_enabled(&f).unwrap(), ["b", "z"]);
        std::fs::write(&f, "enabled = 1").unwrap();
        assert!(read_enabled(&f).is_err());
        std::fs::remove_dir_all(&d).unwrap();
    }
}
