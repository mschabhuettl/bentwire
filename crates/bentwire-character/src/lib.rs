//! Reader and writer for the bentwire character format (`docs/character-format.md`).
//!
//! ```no_run
//! let c = bentwire_character::Character::load("characters/builtin/bentwire-placeholder").unwrap();
//! let a = c.manifest.animation("Showing").unwrap();
//! let picture = c.compose_frame(&a.frames[0], None, 1); // RGBA at display size
//! ```

pub mod audio;
pub mod dump;
pub mod image;
pub mod model;
pub mod parse;
pub mod validate;
pub mod write;
pub mod zip;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub use audio::Pcm;
pub use image::{Image, Rgba, SheetSet};
pub use model::*;
pub use parse::cheap_check;
pub use validate::validate;
pub use write::PackageData;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("manifest: {0}")]
    Manifest(String),
    #[error("unsupported format version {0}")]
    UnsupportedFormat(i64),
    #[error("unknown family {0:?}")]
    UnknownFamily(String),
    #[error("invalid character: {}", .0.join("; "))]
    Invalid(Vec<String>),
    #[error("ZIP: {0}")]
    Zip(String),
    #[error("missing file {0}")]
    MissingFile(String),
    #[error("image: {0}")]
    Image(String),
    #[error("sound: {0}")]
    Sound(String),
}

/// First 16 hex digits of SHA-256 (the prototype's `sha`).
pub fn sha16(b: &[u8]) -> String {
    use sha2::Digest;
    let d = sha2::Sha256::digest(b);
    d[..8].iter().map(|x| format!("{x:02x}")).collect()
}

/// Where a character's files come from: a folder or a `.bwc` archive.
pub enum Package {
    Folder(PathBuf),
    Zip(zip::Archive),
}

impl Package {
    /// Open a folder (with `character.toml`) or a `.bwc` file.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, Error> {
        let p = path.as_ref();
        if p.is_dir() {
            Ok(Package::Folder(p.to_owned()))
        } else {
            Ok(Package::Zip(zip::Archive::new(std::fs::read(p)?)?))
        }
    }

    /// Read one file by package path.
    pub fn read(&self, name: &str) -> Result<Vec<u8>, Error> {
        parse::check_path(name, "path").map_err(|_| Error::MissingFile(name.to_owned()))?;
        match self {
            Package::Folder(d) => std::fs::read(d.join(name)).map_err(|e| {
                if e.kind() == std::io::ErrorKind::NotFound {
                    Error::MissingFile(name.to_owned())
                } else {
                    Error::Io(e)
                }
            }),
            Package::Zip(z) => z.read(name),
        }
    }

    /// Warnings about ZIP rules a loader tolerates (character-format 3): media should be stored.
    fn zip_warnings(&self) -> Vec<String> {
        let mut w = Vec::new();
        if let Package::Zip(z) = self {
            if z.entries.first().map(|e| e.name.as_str()) != Some("character.toml")
                && z.entry("character.toml").is_some()
            {
                w.push("character.toml is not the first ZIP entry".into());
            }
            for e in &z.entries {
                let media = !e.name.ends_with(".toml");
                if media && e.method != 0 {
                    w.push(format!("{}: media entry is not stored", e.name));
                }
            }
        }
        w
    }
}

/// Cheap check of spec B-CHAR-1 on a folder or `.bwc` path.
pub fn cheap_check_path(path: impl AsRef<Path>) -> Result<Family, Error> {
    let pkg = Package::open(path)?;
    let text = pkg.read("character.toml")?;
    cheap_check(&String::from_utf8(text).map_err(|_| Error::Manifest("not UTF-8".into()))?)
}

/// A loaded character: manifest plus decoded media.
pub struct Character {
    pub manifest: Manifest,
    pub sheets: SheetSet,
    /// Sound id to decoded PCM, or the decode error.
    sounds: HashMap<String, Result<Pcm, String>>,
    pub icon: Option<Rgba>,
    /// Loader warnings: unknown keys, tolerated ZIP deviations, media that failed to decode.
    pub warnings: Vec<String>,
    /// Lowercased names of animations that cannot play because a picture or sound failed to
    /// decode (spec B-CHAR-2).
    broken: HashSet<String>,
}

impl Character {
    /// Load a folder or `.bwc` (spec B-CHAR-2): the manifest must parse and validate; a picture
    /// or sound that fails to decode only makes the animations using it unavailable.
    pub fn load(path: impl AsRef<Path>) -> Result<Self, Error> {
        Self::from_package(&Package::open(path)?)
    }

    pub fn from_package(pkg: &Package) -> Result<Self, Error> {
        let text = String::from_utf8(pkg.read("character.toml")?)
            .map_err(|_| Error::Manifest("character.toml is not UTF-8".into()))?;
        let manifest = parse::parse_manifest(&text)?;
        let mut warnings: Vec<String> = manifest
            .unknown_keys
            .iter()
            .map(|k| format!("unknown key {k}"))
            .collect();
        let errs: Vec<String> = validate(&manifest)
            .into_iter()
            .filter(|e| !e.starts_with("unknown key "))
            .collect();
        if !errs.is_empty() {
            return Err(Error::Invalid(errs));
        }
        warnings.extend(pkg.zip_warnings());

        let mut sheets = SheetSet::default();
        for s in &manifest.sheets {
            let files = std::iter::once((1, &s.file)).chain(s.hidpi.iter().map(|(k, f)| (*k, f)));
            for (key, file) in files {
                let r = pkg
                    .read(file)
                    .and_then(|b| image::decode_png(&b))
                    .map_err(|e| e.to_string())
                    .and_then(|img| image::check_grid(s, key, &img).map(|_| img));
                if let Err(e) = &r {
                    warnings.push(format!("sheet {} ({file}): {e}", s.id));
                }
                sheets.files.insert((s.id.clone(), key), r);
            }
        }
        let mut sounds = HashMap::new();
        for (id, file) in &manifest.sounds {
            let r = pkg
                .read(file)
                .and_then(|b| audio::decode_sound(&b))
                .map_err(|e| e.to_string());
            if let Err(e) = &r {
                warnings.push(format!("sound {id} ({file}): {e}"));
            }
            sounds.insert(id.clone(), r);
        }
        let icon = match &manifest.icon {
            None => None,
            Some(i) => match pkg.read(&i.file).and_then(|b| image::decode_png(&b)) {
                Ok(img) => Some(img.to_rgba()),
                Err(e) => {
                    warnings.push(format!("icon ({}): {e}", i.file));
                    None
                }
            },
        };

        let mut c = Character {
            manifest,
            sheets,
            sounds,
            icon,
            warnings,
            broken: HashSet::new(),
        };
        c.broken = c
            .manifest
            .animations
            .iter()
            .filter(|a| !c.frames_ok(a))
            .map(|a| a.name.to_ascii_lowercase())
            .collect();
        Ok(c)
    }

    fn frames_ok(&self, a: &Animation) -> bool {
        let layer_ok = |l: &Layer| self.sheets.ok(&l.image.sheet);
        a.frames.iter().all(|f| {
            self.manifest.frame_layers(f).iter().all(layer_ok)
                && f.mouths
                    .iter()
                    .all(|m| layer_ok(&m.layer) && m.base.iter().flatten().all(layer_ok))
                && f.sound
                    .as_ref()
                    .is_none_or(|s| matches!(self.sounds.get(s), Some(Ok(_))))
        })
    }

    pub fn family(&self) -> Family {
        self.manifest.family
    }

    /// True when the animation exists, is not listed as missing and all its media decoded.
    pub fn is_available(&self, name: &str) -> bool {
        self.manifest.animation(name).is_some()
            && !self.manifest.is_missing(name)
            && !self.broken.contains(&name.to_ascii_lowercase())
    }

    /// Decoded sound by id (`None` if unknown or it failed to decode).
    pub fn sound(&self, id: &str) -> Option<&Pcm> {
        self.sounds.get(id).and_then(|r| r.as_ref().ok())
    }

    /// Composite layers at `display * density` (character-format 5.3).
    pub fn compose(&self, layers: &[Layer], density: u32) -> Rgba {
        let [w, h] = self.manifest.display();
        let k = density.max(1);
        image::compose(&self.manifest, &self.sheets, layers, [w * k, h * k])
    }

    /// Composite layers at an explicit output size.
    pub fn compose_at(&self, layers: &[Layer], out: [u32; 2]) -> Rgba {
        image::compose(&self.manifest, &self.sheets, layers, out)
    }

    /// The layers of a frame, or of its mouth variant `mouth` (index into `frame.mouths`):
    /// the frame's layers (or the mouth's `base`) followed by the mouth layer. Unclipped.
    pub fn frame_layers(&self, frame: &Frame, mouth: Option<usize>) -> Vec<Layer> {
        let base = self.manifest.frame_layers(frame);
        match mouth.and_then(|i| frame.mouths.get(i)) {
            None => base.to_vec(),
            Some(m) => {
                let mut v = m.base.clone().unwrap_or_else(|| base.to_vec());
                v.push(m.layer.clone());
                v
            }
        }
    }

    /// Picture of a frame (or mouth variant) at `display * density`, unclipped (the round-trip
    /// form). The mouth clipping of spec B-PIX-2 is a presentation step left to the renderer.
    pub fn compose_frame(&self, frame: &Frame, mouth: Option<usize>, density: u32) -> Rgba {
        self.compose(&self.frame_layers(frame, mouth), density)
    }

    /// Name and description for UI language `lang` (spec B-CHAR-3).
    pub fn display_name(&self, lang: &str) -> (String, String) {
        let info = &self.manifest.character.info;
        let primary = |t: &str| t.split('-').next().unwrap_or("").to_ascii_lowercase();
        let pick = |field: fn(&Info) -> &str| -> String {
            let lp = primary(lang);
            info.iter()
                .find(|i| i.lang.eq_ignore_ascii_case(lang) && !field(i).is_empty())
                .or_else(|| info.iter().rev().find(|i| primary(&i.lang) == lp))
                .or_else(|| info.iter().rev().find(|i| primary(&i.lang) == "en"))
                .map(|i| field(i).to_owned())
                .unwrap_or_default()
        };
        (
            pick(|i| &i.name),
            pick(|i| i.description.as_deref().unwrap_or("")),
        )
    }
}
