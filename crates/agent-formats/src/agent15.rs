//! Agent 1.5 characters (docs/research/agent15-format.md). Owner: formats teammate.
//!
//! The OLE `.acs` container is read here: stream `char.acf` (magic 0xABCDABC1, compressed
//! ACF body) plus one stream per animation, named by the ACF table. The bodies are parsed by
//! [`crate::acf`] with its version gates (0x1001F, 0x1001E, 0x1001C). Agent 1.5 web files
//! (flat ACF magic 0xABCDABC2 + `.aaf`) are plain [`crate::acf`] input.

use crate::acf::{Aca, AcfCharacter, compressed_block};
use crate::common::{Budget, Context, Error, Guid, Reader, Result};
use crate::ole::{CompoundFile, SIGNATURE};

/// Magic of the `char.acf` stream.
pub const MAGIC_OLE: u32 = 0xABCD_ABC1;

/// An Agent 1.5 OLE `.acs` file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OleAcs {
    /// CLSID of the root storage (not checked; 1.5 files carry the DocFile provider's).
    pub root_clsid: Guid,
    pub character: AcfCharacter,
    /// Index-aligned with `character.animations`. Each animation is read on its own, as the
    /// runtime loads them lazily: a missing or damaged stream is an `Err` here and does not
    /// fail the file.
    pub animations: Vec<Result<Aca>>,
}

/// Whether `data` starts with the compound file signature.
pub fn is_ole(data: &[u8]) -> bool {
    data.starts_with(&SIGNATURE)
}

impl OleAcs {
    pub fn parse(data: &[u8]) -> Result<OleAcs> {
        let cf = CompoundFile::parse(data)?;
        let streams: Vec<_> = cf
            .root_children()?
            .into_iter()
            .filter(|e| e.kind == 2)
            .collect();
        let find = |name: &str| streams.iter().find(|e| e.name.eq_ignore_ascii_case(name));
        let s = find("char.acf").ok_or_else(|| Error::invalid("no char.acf stream", 0))?;
        let s = cf.stream(s).context(|| "char.acf".into())?;
        let mut r = Reader::new(&s);
        let magic = r.u32()?;
        if magic != MAGIC_OLE {
            return Err(Error::BadMagic(magic));
        }
        let mut budget = Budget::new();
        let body = compressed_block(&mut r, &mut budget).context(|| "char.acf".into())?;
        let character = AcfCharacter::parse_body(&body).context(|| "char.acf".into())?;
        let animations = character
            .animations
            .iter()
            .map(|entry| {
                let e = find(&entry.file_name)
                    .ok_or_else(|| Error::invalid(format!("no stream {:?}", entry.file_name), 0))?;
                budget.take(e.size.try_into().unwrap_or(usize::MAX))?;
                let d = cf.stream(e)?;
                Aca::parse_with(&d, &character, true, &mut budget)
            })
            .zip(&character.animations)
            .map(|(r, entry)| {
                r.context(|| format!("animation {:?} ({})", entry.name, entry.file_name))
            })
            .collect();
        Ok(OleAcs {
            root_clsid: cf.entries[0].clsid,
            character,
            animations,
        })
    }
}
