//! Converter from Microsoft Agent and Office actor files into the bentwire character format.

pub mod acf;
pub mod acs;
pub mod act;
pub mod build;
mod lcid_table;
pub mod placeholder;
pub mod sound;
pub mod wmf;

use std::path::Path;

pub use bentwire_character::PackageData;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O: {0}")]
    Io(#[from] std::io::Error),
    #[error("parse: {0}")]
    Parse(#[from] agent_formats::Error),
    #[error("source: {0}")]
    Source(String),
    #[error("converted character is invalid: {}", .0.join("; "))]
    Invalid(Vec<String>),
    #[error("{0}")]
    Character(#[from] bentwire_character::Error),
    #[error("unsupported source: {0}")]
    Unsupported(String),
}

/// First 16 hex digits of SHA-256 of the source file (`source.sha256_16`).
pub fn sha16(b: &[u8]) -> String {
    bentwire_character::sha16(b)
}

/// Conversion settings that a source file does not carry.
#[derive(Clone, Debug, Default)]
pub struct ConvertOptions {
    /// Where the Office palette for ACT bitmap actors is read from: an Office ACS (its 256-entry
    /// palette) or `MSO97.DLL` (architecture section 4: never embedded).
    pub office_palette: Option<std::path::PathBuf>,
    /// Language tag for ACT actors (`character.info.lang`); default `en`.
    pub lang: Option<String>,
    /// Codepage label (WHATWG, e.g. `shift_jis`) for ACT item names and v1 strings; default
    /// `windows-1252`.
    pub codepage: Option<String>,
}

/// Read the 256-entry Office palette from an Office ACS file or MSO97.DLL.
pub fn read_office_palette(path: &Path) -> Result<[[u8; 3]; 256], Error> {
    let d = std::fs::read(path)?;
    if d.starts_with(b"MZ") {
        return agent_formats::act::office_palette_from_mso97(&d)
            .map_err(|e| Error::Source(format!("{}: {e}", path.display())));
    }
    let acs = agent_formats::acs::Acs::parse(&d)?;
    let p = &acs.character.palette;
    if p.len() != 256 {
        return Err(Error::Source(format!(
            "{}: palette has {} entries, not 256",
            path.display(),
            p.len()
        )));
    }
    let mut out = [[0u8; 3]; 256];
    for (o, q) in out.iter_mut().zip(p) {
        *o = [q.r, q.g, q.b];
    }
    Ok(out)
}

/// Convert a source file into a package in memory with default options.
pub fn convert_to_package(source: &Path) -> Result<PackageData, Error> {
    convert_to_package_with(source, &ConvertOptions::default())
}

/// Convert a source file into a package in memory. The format is detected from the content.
pub fn convert_to_package_with(source: &Path, opts: &ConvertOptions) -> Result<PackageData, Error> {
    let data = std::fs::read(source)?;
    let file = source
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
        .unwrap_or_default();
    let magic = data
        .get(..4)
        .map(|b| u32::from_le_bytes([b[0], b[1], b[2], b[3]]));
    let sha = sha16(&data);
    if magic == Some(agent_formats::acs::MAGIC) {
        let acs = agent_formats::acs::Acs::parse(&data)?;
        return acs::convert_acs(&acs, &file, &sha);
    }
    if magic == Some(agent_formats::acf::MAGIC_20) || magic == Some(agent_formats::acf::MAGIC_15) {
        use agent_formats::acf::AnimationFile;
        let web = agent_formats::acf::read_web_character(source)?;
        let anims: Vec<_> = web
            .animations
            .iter()
            .map(|a| match a {
                AnimationFile::Loaded(a) => Some(&a.animation),
                _ => None,
            })
            .collect();
        let format = if web.acf.magic == agent_formats::acf::MAGIC_20 {
            "acf-aca-2.0"
        } else {
            "acf-aaf-1.5"
        };
        return acf::convert_acf(&web.acf.character, &anims, format, &file, &sha);
    }
    if agent_formats::agent15::is_ole(&data) {
        let ole = agent_formats::agent15::OleAcs::parse(&data)?;
        let anims: Vec<_> = ole
            .animations
            .iter()
            .map(|a| a.as_ref().ok().map(|a| &a.animation))
            .collect();
        return acf::convert_acf(&ole.character, &anims, "acs-1.5", &file, &sha);
    }
    if data.starts_with(b"LP") {
        let act_file = agent_formats::act::parse(&data)
            .map_err(|e| Error::Source(format!("{}: {e}", source.display())))?;
        let mut o = act::ActOptions::default();
        if let Some(l) = &opts.lang {
            o.lang = l.clone();
        }
        if let Some(c) = &opts.codepage {
            o.codepage = encoding_rs::Encoding::for_label(c.as_bytes())
                .ok_or_else(|| Error::Source(format!("unknown codepage {c:?}")))?;
        }
        let vector = act_file
            .payloads
            .iter()
            .any(|p| matches!(p, agent_formats::act::Payload::Wmf(_)));
        if vector {
            return Err(Error::Unsupported(format!(
                "{}: vector (WMF) actor; rasteriser not wired up yet",
                source.display()
            )));
        }
        let pal_path = opts.office_palette.as_deref().ok_or_else(|| {
            Error::Source(
                "ACT bitmap actor: an Office palette source is needed (an Office ACS or MSO97.DLL)"
                    .into(),
            )
        })?;
        let pal = read_office_palette(pal_path)?;
        return act::convert_act(&act_file, &act::Pictures::Bitmaps(&pal), &o, &file, &sha);
    }
    Err(Error::Unsupported(format!(
        "{}: unknown file format",
        source.display()
    )))
}

/// Convert `source` and write it to `dest`: a `.bwc` file when `dest` ends in `.bwc`,
/// otherwise a folder.
pub fn convert(source: &Path, dest: &Path) -> Result<(), Error> {
    convert_with(source, dest, &ConvertOptions::default())
}

/// [`convert`] with options.
pub fn convert_with(source: &Path, dest: &Path, opts: &ConvertOptions) -> Result<(), Error> {
    convert_to_package_with(source, opts)?.write(dest)?;
    Ok(())
}
