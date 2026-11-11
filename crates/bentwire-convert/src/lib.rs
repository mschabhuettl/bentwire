//! Converter from Microsoft Agent and Office actor files into the bentwire character format.

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
    if data.len() >= 4
        && u32::from_le_bytes([data[0], data[1], data[2], data[3]]) == agent_formats::acs::MAGIC
    {
        let acs = agent_formats::acs::Acs::parse(&data)?;
        return acs::convert_acs(&acs, &file, &sha16(&data));
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
