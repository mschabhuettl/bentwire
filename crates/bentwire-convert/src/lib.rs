//! Converter from Microsoft Agent and Office actor files into the bentwire character format.

pub mod acf;
pub mod acs;
pub mod build;
mod lcid_table;
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

/// Convert a source file into a package in memory. The format is detected from the content.
pub fn convert_to_package(source: &Path) -> Result<PackageData, Error> {
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
    Err(Error::Unsupported(format!(
        "{}: unknown file format",
        source.display()
    )))
}

/// Convert `source` and write it to `dest`: a `.bwc` file when `dest` ends in `.bwc`,
/// otherwise a folder.
pub fn convert(source: &Path, dest: &Path) -> Result<(), Error> {
    convert_to_package(source)?.write(dest)?;
    Ok(())
}
