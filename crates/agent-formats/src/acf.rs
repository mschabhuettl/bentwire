//! ACF character files and ACA/AAF animation files (docs/research/acf-aaf-format.md, and
//! agent15-format 4 for versions 0x1001C to 0x1001F). Owner: formats teammate.
//!
//! One reader serves Agent 2.0 web characters (ACF magic 0xABCDABC4 + `.aca`), Agent 1.5 web
//! characters (magic 0xABCDABC2 + `.aaf`) and the streams inside Agent 1.5 OLE files
//! ([`crate::agent15`]). Fields that a version does not store are `None`; the runtime's
//! substitutes (exit -1, transition 0, animation set 1.50, LANGID 9) are not applied.

use std::path::Path;

pub use crate::acs::Branch;
use crate::common::{
    Balloon, Bitmap, Context, Error, Guid, LocalizedInfo, MAX_BODY, MAX_IMAGE, Reader, Region,
    Result, Rgbquad, State, TrayIcon, Voice, VoiceExtra, names_equal, palette, states, stride,
    tray_icon,
};
use crate::lz;

/// Agent 2.0 web ACF.
pub const MAGIC_20: u32 = 0xABCD_ABC4;
/// Agent 1.5 web ACF.
pub const MAGIC_15: u32 = 0xABCD_ABC2;
/// ACF and ACA/AAF versions the runtime accepts.
pub const VERSIONS: [u32; 4] = [0x1_001C, 0x1_001E, 0x1_001F, 0x2_0001];

/// A flat ACF file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acf {
    /// [`MAGIC_20`] or [`MAGIC_15`] (handled identically).
    pub magic: u32,
    pub character: AcfCharacter,
}

/// The decompressed ACF body (acf-aaf-format 6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcfCharacter {
    pub version: u32,
    pub animations: Vec<AcfEntry>,
    pub guid: Guid,
    /// For versions below 0x20001 the file holds exactly one entry without LANGID; it is
    /// returned here with `langid` 9, the value the runtime files it under.
    pub localized: Vec<LocalizedInfo>,
    pub width: u16,
    pub height: u16,
    pub transparent: u8,
    pub flags: u32,
    /// `None` below 0x20001 (the runtime reports 1.50).
    pub animation_set: Option<(u16, u16)>,
    pub voice: Option<Voice>,
    pub balloon: Option<Balloon>,
    pub palette: Vec<Rgbquad>,
    /// Never present below 0x20001.
    pub tray_icon: Option<TrayIcon>,
    pub states: Vec<State>,
}

/// Animation table entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcfEntry {
    pub name: String,
    /// ACA/AAF file name (web) or stream name (OLE).
    pub file_name: String,
    /// Empty means none.
    pub return_animation: String,
    /// `None` for versions up to 0x1001D.
    pub checksum: Option<u32>,
}

/// ACA/AAF file header (acf-aaf-format 5.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AcaHeader {
    /// The ACF version is below 0x1001D: no header.
    Absent,
    Present {
        version: u32,
        checksum: u32,
    },
    /// A header was expected but its version is not recognized; the runtime then reads the
    /// file from the start as headerless (and re-downloads it).
    Unrecognized {
        version: u32,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Aca {
    pub header: AcaHeader,
    /// Compression flag 1.
    pub compressed: bool,
    pub animation: AcaAnimation,
}

/// The decompressed ACA/AAF body (acf-aaf-format 7, agent15-format 4.6).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcaAnimation {
    /// Complete RIFF files, verbatim.
    pub sounds: Vec<Vec<u8>>,
    /// `None` for an empty image (size 0).
    pub images: Vec<Option<AcaImage>>,
    /// `None` when the version stores none (below 0x10020).
    pub transition: Option<u8>,
    /// Never empty.
    pub frames: Vec<AcaFrame>,
}

/// A full-frame picture at the character size.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcaImage {
    /// The byte after the size (0 in all samples, never read).
    pub unknown: u8,
    pub bitmap: Bitmap,
    /// `None` when the region size is 0.
    pub region: Option<Region>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcaFrame {
    /// Into [`AcaAnimation::images`]; 0xFFFF = none. Raw.
    pub image: u16,
    /// Into [`AcaAnimation::sounds`]; 0xFFFF = none. Raw.
    pub sound: u16,
    pub duration: u16,
    pub offset_x: i16,
    pub offset_y: i16,
    /// `None` when the version stores none (below 0x10020).
    pub exit: Option<i16>,
    pub branches: Vec<Branch>,
    /// Base image block, stored only above 0x1001F when the frame has overlays.
    pub base: Option<BaseImage>,
    pub overlays: Vec<AcaOverlay>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BaseImage {
    /// Presence byte 0.
    Absent,
    /// Present with size 0.
    Empty,
    Image {
        bitmap: Bitmap,
        region: Option<Region>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcaOverlay {
    /// 0..=6 (checked).
    pub mouth: u8,
    /// `None` for an empty overlay (size 0).
    pub data: Option<OverlayData>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OverlayData {
    pub reserved: u8,
    /// "Replace base frame" byte; `None` when the version stores none (0x1001F and older).
    pub replace_base: Option<u8>,
    /// Top-left of the rectangle in frame coordinates (top-down); the bitmap is its size.
    pub x: i16,
    pub y: i16,
    pub bitmap: Bitmap,
    /// Stored only when `replace_base` is nonzero; `None` also when its size is 0.
    pub region: Option<Region>,
}

/// Reads "u32 uncompressed size, u32 compressed size, compressed bytes" at `r`.
pub(crate) fn compressed_block(r: &mut Reader) -> Result<Vec<u8>> {
    let usize_ = r.u32()? as usize;
    let csize = r.u32()? as usize;
    if usize_ > MAX_BODY {
        return Err(Error::invalid(
            format!("body size {usize_} too large"),
            r.pos - 8,
        ));
    }
    Ok(lz::decompress_exact(r.bytes(csize)?, usize_)?)
}

impl Acf {
    /// Parses a flat ACF file (magic 0xABCDABC4 or 0xABCDABC2).
    pub fn parse(data: &[u8]) -> Result<Acf> {
        let mut r = Reader::new(data);
        let magic = r.u32()?;
        if magic != MAGIC_20 && magic != MAGIC_15 {
            return Err(Error::BadMagic(magic));
        }
        let body = compressed_block(&mut r).context(|| "ACF body".into())?;
        let character = AcfCharacter::parse_body(&body).context(|| "ACF body".into())?;
        Ok(Acf { magic, character })
    }
}

impl AcfCharacter {
    /// Parses a decompressed ACF body; the whole body must be consumed.
    pub fn parse_body(body: &[u8]) -> Result<AcfCharacter> {
        let mut r = Reader::new(body);
        let version = r.u32()?;
        if !VERSIONS.contains(&version) {
            return Err(Error::UnsupportedVersion(version));
        }
        let n = r.u16()? as usize;
        let n = r.count(n, 12)?;
        let animations = (0..n)
            .map(|_| {
                Ok(AcfEntry {
                    name: r.acf_string()?,
                    file_name: r.acf_string()?,
                    return_animation: r.acf_string()?,
                    checksum: if version > 0x1_001D {
                        Some(r.u32()?)
                    } else {
                        None
                    },
                })
            })
            .collect::<Result<_>>()?;
        let guid = r.guid()?;
        let entry = |r: &mut Reader, langid| -> Result<LocalizedInfo> {
            Ok(LocalizedInfo {
                langid,
                name: r.acf_string()?,
                description: r.acf_string()?,
                extra: r.acf_string()?,
            })
        };
        let localized = if version >= 0x2_0001 {
            let n = r.u16()? as usize;
            let n = r.count(n, 14)?;
            (0..n)
                .map(|_| {
                    let l = r.u16()?;
                    entry(&mut r, l)
                })
                .collect::<Result<_>>()?
        } else {
            vec![entry(&mut r, 9)?]
        };
        let width = r.u16()?;
        let height = r.u16()?;
        let transparent = r.u8()?;
        let flags = r.u32()?;
        let animation_set = if version >= 0x2_0001 {
            Some((r.u16()?, r.u16()?))
        } else {
            None
        };
        let voice = if flags & 0x20 != 0 {
            let engine = r.guid()?;
            let mode = r.guid()?;
            let speed = r.u32()?;
            let pitch = r.u16()?;
            let extra = if version > 0x2_0000 && r.u8()? != 0 {
                Some(VoiceExtra {
                    language: r.u16()?,
                    dialect: r.acf_string()?,
                    gender: r.u16()?,
                    age: r.u16()?,
                    style: r.acf_string()?,
                })
            } else {
                None
            };
            Some(Voice {
                engine,
                mode,
                speed,
                pitch,
                extra,
            })
        } else {
            None
        };
        let balloon = if flags & 0x100 == 0 {
            Some(Balloon {
                lines: r.u8()?,
                chars_per_line: r.u8()?,
                foreground: r.u32()?,
                background: r.u32()?,
                border: r.u32()?,
                font: r.acf_string()?,
                font_height: r.i32()?,
                font_weight: r.u32()?,
                italic: r.u8()?,
                flag2: if version > 0x1_001E {
                    Some(r.u8()?)
                } else {
                    None
                },
            })
        } else {
            None
        };
        let palette = palette(&mut r)?;
        let tray_icon = if version > 0x2_0000 && r.u8()? != 0 {
            Some(tray_icon(&mut r)?)
        } else {
            None
        };
        let states = states(&mut r, Reader::acf_string)?;
        r.finish("ACF body")?;
        Ok(AcfCharacter {
            version,
            animations,
            guid,
            localized,
            width,
            height,
            transparent,
            flags,
            animation_set,
            voice,
            balloon,
            palette,
            tray_icon,
            states,
        })
    }

    /// Case-insensitive lookup in the animation table.
    pub fn entry(&self, name: &str) -> Option<&AcfEntry> {
        self.animations.iter().find(|a| names_equal(&a.name, name))
    }
}

impl Aca {
    /// Parses an ACA/AAF file or OLE animation stream of `character`. The checksum is not
    /// compared here (the OLE path never compares it); see [`Aca::check_checksum`].
    pub fn parse(data: &[u8], character: &AcfCharacter) -> Result<Aca> {
        let v = character.version;
        let mut r = Reader::new(data);
        let header = if v >= 0x1_001D {
            let version = r.u32()?;
            if VERSIONS.contains(&version) {
                AcaHeader::Present {
                    version,
                    checksum: r.u32()?,
                }
            } else {
                r.pos = 0;
                AcaHeader::Unrecognized { version }
            }
        } else {
            AcaHeader::Absent
        };
        let at = r.pos;
        let (compressed, body) = match r.u8()? {
            1 => (
                true,
                compressed_block(&mut r).context(|| "animation body".into())?,
            ),
            0 => (false, r.bytes(r.remaining())?.to_vec()),
            f => return Err(Error::invalid(format!("compression flag {f}"), at)),
        };
        let animation =
            AcaAnimation::parse_body(&body, character).context(|| "animation body".into())?;
        Ok(Aca {
            header,
            compressed,
            animation,
        })
    }

    /// Web loading rule: a header checksum must equal the ACF entry's checksum.
    pub fn check_checksum(&self, entry: &AcfEntry) -> Result<()> {
        match (self.header, entry.checksum) {
            (AcaHeader::Present { checksum, .. }, Some(expected)) if checksum != expected => {
                Err(Error::ChecksumMismatch {
                    expected,
                    found: checksum,
                })
            }
            _ => Ok(()),
        }
    }
}

/// A full-frame or overlay picture of `size` bytes, which must be `stride(w) * h`.
fn bitmap(r: &mut Reader, size: usize, width: u16, height: u16) -> Result<Bitmap> {
    let at = r.pos;
    let need = stride(width) * height as usize;
    if size != need || size > MAX_IMAGE {
        return Err(Error::invalid(
            format!("picture of {size} bytes, {width}x{height} needs {need}"),
            at,
        ));
    }
    Ok(Bitmap {
        width,
        height,
        pixels: r.bytes(size)?.to_vec(),
    })
}

fn region(r: &mut Reader) -> Result<Option<Region>> {
    let n = r.u32()? as usize;
    let at = r.pos;
    let b = r.bytes(n)?;
    if n == 0 {
        Ok(None)
    } else {
        Ok(Some(Region::parse(b, at)?))
    }
}

impl AcaAnimation {
    /// Parses a decompressed body with the version gates of `character.version`.
    pub fn parse_body(body: &[u8], character: &AcfCharacter) -> Result<AcaAnimation> {
        let v = character.version;
        let (w, h) = (character.width, character.height);
        let mut r = Reader::new(body);
        let n = r.u16()? as usize;
        let n = r.count(n, 4)?;
        let sounds = (0..n)
            .map(|_| {
                let k = r.u32()? as usize;
                Ok(r.bytes(k)?.to_vec())
            })
            .collect::<Result<_>>()?;
        let n = r.u16()? as usize;
        let n = r.count(n, 4)?;
        let images = (0..n)
            .map(|i| {
                let go = |r: &mut Reader| -> Result<Option<AcaImage>> {
                    let size = r.u32()? as usize;
                    if size == 0 {
                        return Ok(None);
                    }
                    let unknown = r.u8()?;
                    let bitmap = bitmap(r, size, w, h)?;
                    Ok(Some(AcaImage {
                        unknown,
                        bitmap,
                        region: region(r)?,
                    }))
                };
                go(&mut r).context(|| format!("image {i}"))
            })
            .collect::<Result<_>>()?;
        let transition = if v >= 0x1_0020 { Some(r.u8()?) } else { None };
        let at = r.pos;
        let n = r.u16()? as usize;
        if n == 0 {
            return Err(Error::invalid("animation has no frames", at));
        }
        let n = r.count(n, 12)?;
        let frames = (0..n)
            .map(|i| parse_frame(&mut r, v, w, h).context(|| format!("frame {i}")))
            .collect::<Result<_>>()?;
        r.finish("animation body")?;
        Ok(AcaAnimation {
            sounds,
            images,
            transition,
            frames,
        })
    }
}

fn parse_frame(r: &mut Reader, v: u32, w: u16, h: u16) -> Result<AcaFrame> {
    let image = r.u16()?;
    let sound = r.u16()?;
    let duration = r.u16()?;
    let offset_x = r.i16()?;
    let offset_y = r.i16()?;
    let exit = if v >= 0x1_0020 { Some(r.i16()?) } else { None };
    let n = r.u8()? as usize;
    let branches = (0..n)
        .map(|_| {
            Ok(Branch {
                target: r.u16()?,
                probability: r.u16()?,
            })
        })
        .collect::<Result<_>>()?;
    let n = r.u8()? as usize;
    let base = if v > 0x1_001F && n != 0 {
        Some(if r.u8()? == 0 {
            BaseImage::Absent
        } else {
            match r.u32()? as usize {
                0 => BaseImage::Empty,
                size => BaseImage::Image {
                    bitmap: bitmap(r, size, w, h)?,
                    region: region(r)?,
                },
            }
        })
    } else {
        None
    };
    let overlays = (0..n)
        .map(|_| {
            let at = r.pos;
            let mouth = r.u8()?;
            if mouth > 6 {
                return Err(Error::invalid(format!("mouth type {mouth}"), at));
            }
            let size = r.u32()? as usize;
            if size == 0 {
                return Ok(AcaOverlay { mouth, data: None });
            }
            let reserved = r.u8()?;
            let replace_base = if v > 0x1_001F { Some(r.u8()?) } else { None };
            let x = r.i16()?;
            let y = r.i16()?;
            let (ow, oh) = (r.u16()?, r.u16()?);
            let bitmap = bitmap(r, size, ow, oh)?;
            let region = if replace_base.unwrap_or(0) != 0 {
                region(r)?
            } else {
                None
            };
            Ok(AcaOverlay {
                mouth,
                data: Some(OverlayData {
                    reserved,
                    replace_base,
                    x,
                    y,
                    bitmap,
                    region,
                }),
            })
        })
        .collect::<Result<_>>()?;
    Ok(AcaFrame {
        image,
        sound,
        duration,
        offset_x,
        offset_y,
        exit,
        branches,
        base,
        overlays,
    })
}

/// One animation of a web character as found on disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AnimationFile {
    /// No file with the entry's file name next to the ACF.
    Missing,
    Loaded(Aca),
    /// The file exists but does not parse, or its checksum is stale.
    Failed(Error),
}

/// A web character: the ACF plus each table entry's animation file, index-aligned with
/// `acf.character.animations`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WebCharacter {
    pub acf: Acf,
    pub animations: Vec<AnimationFile>,
}

/// Reads an ACF file and the animation files next to it. File names are matched
/// case-insensitively (the original server was case-insensitive).
pub fn read_web_character(acf_path: &Path) -> Result<WebCharacter> {
    let io = |p: &Path, e: std::io::Error| Error::Io {
        path: p.display().to_string(),
        message: e.to_string(),
    };
    let data = std::fs::read(acf_path).map_err(|e| io(acf_path, e))?;
    let acf = Acf::parse(&data)?;
    let dir = acf_path.parent().unwrap_or(Path::new("."));
    let mut files = Vec::new();
    for e in std::fs::read_dir(dir).map_err(|e| io(dir, e))? {
        let e = e.map_err(|e| io(dir, e))?;
        files.push(e.path());
    }
    let animations = acf
        .character
        .animations
        .iter()
        .map(|entry| {
            let Some(p) = files.iter().find(|p| {
                p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| names_equal(n, &entry.file_name))
            }) else {
                return AnimationFile::Missing;
            };
            let res = std::fs::read(p)
                .map_err(|e| io(p, e))
                .and_then(|d| Aca::parse(&d, &acf.character))
                .and_then(|a| a.check_checksum(entry).map(|_| a))
                .context(|| format!("animation {:?}", entry.name));
            match res {
                Ok(a) => AnimationFile::Loaded(a),
                Err(e) => AnimationFile::Failed(e),
            }
        })
        .collect();
    Ok(WebCharacter { acf, animations })
}
