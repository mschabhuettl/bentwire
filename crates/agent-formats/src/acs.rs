//! ACS 2.0 single-file characters (magic 0xABCDABC3, docs/research/acs-format.md).
//! Owner: formats teammate.
//!
//! [`Acs::parse`] reads the whole file eagerly: character info, animations, every image
//! (decompressed, with its region) and every sound. Values are kept as stored; indices are
//! not resolved and runtime defaults are not applied.

use crate::common::{
    Balloon, Bitmap, Context, Error, Guid, LocalizedInfo, MAX_IMAGE, Region, Result, Rgbquad,
    State, TrayIcon, Voice, VoiceExtra, palette, states, stride, tray_icon,
};
use crate::common::{Reader, names_equal};
use crate::lz;

pub const MAGIC: u32 = 0xABCD_ABC3;

/// File offset and size of a block.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Locator {
    pub offset: u32,
    pub size: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Acs {
    pub character: Character,
    /// Animations in animation-list order.
    pub animations: Vec<Animation>,
    pub images: Vec<ImageEntry>,
    pub sounds: Vec<SoundEntry>,
}

/// Character info block (acs-format 3.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Character {
    /// 0x20001 or 0x1001F (the only accepted values).
    pub version: u32,
    /// Localized-info locator, kept verbatim; the runtime does not use it.
    pub localized_locator: Locator,
    pub guid: Guid,
    pub width: u16,
    pub height: u16,
    pub transparent: u8,
    pub flags: u32,
    /// Animation set version (major, minor).
    pub animation_set: (u16, u16),
    pub voice: Option<Voice>,
    pub balloon: Option<Balloon>,
    pub palette: Vec<Rgbquad>,
    pub tray_icon: Option<TrayIcon>,
    pub states: Vec<State>,
    pub localized: Vec<LocalizedInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Animation {
    /// Name from the animation list (author's case; the lookup key).
    pub name: String,
    /// Upper-case copy stored inside the record.
    pub record_name: String,
    /// 0 = use return animation, 1 = use exit branches, 2 = none. Raw value.
    pub transition: u8,
    /// Empty means none.
    pub return_animation: String,
    /// Never empty (the runtime rejects 0 frames).
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Frame {
    /// Listed top-most first; the runtime draws them last to first.
    pub images: Vec<FrameImage>,
    /// Index into [`Acs::sounds`]; any value out of range (0xFFFF in practice) means none.
    pub sound: u16,
    /// 1/100 s.
    pub duration: u16,
    /// -1 end, -2 continue normally, otherwise a frame index (raw).
    pub exit: i16,
    pub branches: Vec<Branch>,
    pub overlays: Vec<Overlay>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FrameImage {
    /// Index into [`Acs::images`]; out-of-range values are skipped by the runtime.
    pub image: u32,
    pub x: i16,
    pub y: i16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Branch {
    pub target: u16,
    /// Percent.
    pub probability: u16,
}

/// Mouth overlay (acs-format 3.4, 14 bytes).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Overlay {
    /// 0..=6 (checked).
    pub mouth: u8,
    /// Nonzero: frame image #0 is not drawn while this overlay is shown.
    pub replace_top: u8,
    pub image: u32,
    pub x: i16,
    pub y: i16,
    /// Mouth point relative to (x, y); not a size.
    pub point_x: u16,
    pub point_y: u16,
}

/// Image list entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageEntry {
    /// The unknown "checksum" dword, verbatim.
    pub checksum: u32,
    /// `None` when the record's kind byte is 0 ("no image").
    pub image: Option<Image>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Image {
    /// Decoded pixels, `stride(width) * height` bytes, bottom-up.
    pub bitmap: Bitmap,
    /// Whether the pixels were stored compressed.
    pub compressed: bool,
    /// Whether the region was stored compressed.
    pub region_compressed: bool,
    /// `None` when the stored region is empty (uncompressed size 0).
    pub region: Option<Region>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SoundEntry {
    pub checksum: u32,
    /// The complete RIFF/WAVE file, verbatim.
    pub data: Vec<u8>,
}

impl Acs {
    /// Parses a whole ACS 2.0 file.
    pub fn parse(data: &[u8]) -> Result<Acs> {
        let mut r = Reader::new(data);
        let magic = r.u32().context(|| "header".into())?;
        if magic != MAGIC {
            return Err(Error::BadMagic(magic));
        }
        let mut loc = || -> Result<Locator> {
            Ok(Locator {
                offset: r.u32()?,
                size: r.u32()?,
            })
        };
        let (ci, al, il, sl) = (loc(), loc(), loc(), loc());
        let (ci, al, il, sl) = (ci?, al?, il?, sl?);

        let character = parse_character(data, ci).context(|| "character info".into())?;
        let images = parse_list(data, il, "image list")?
            .into_iter()
            .enumerate()
            .map(|(i, (l, checksum))| {
                let image = parse_image(data, l).context(|| format!("image {i}"))?;
                Ok(ImageEntry { checksum, image })
            })
            .collect::<Result<_>>()?;
        let sounds = parse_list(data, sl, "audio list")?
            .into_iter()
            .enumerate()
            .map(|(i, (l, checksum))| {
                let mut r =
                    Reader::block(data, l.offset, l.size).context(|| format!("sound {i}"))?;
                Ok(SoundEntry {
                    checksum,
                    data: r.bytes(l.size as usize)?.to_vec(),
                })
            })
            .collect::<Result<_>>()?;
        let animations = parse_animation_list(data, al)?;
        Ok(Acs {
            character,
            animations,
            images,
            sounds,
        })
    }

    /// Case-insensitive animation lookup, as the runtime does it.
    pub fn animation(&self, name: &str) -> Option<&Animation> {
        self.animations.iter().find(|a| names_equal(&a.name, name))
    }
}

fn parse_character(data: &[u8], l: Locator) -> Result<Character> {
    let mut r = Reader::block(data, l.offset, l.size)?;
    let version = r.u32()?;
    if version != 0x2_0001 && version != 0x1_001F {
        return Err(Error::UnsupportedVersion(version));
    }
    let localized_locator = Locator {
        offset: r.u32()?,
        size: r.u32()?,
    };
    let guid = r.guid()?;
    let width = r.u16()?;
    let height = r.u16()?;
    let transparent = r.u8()?;
    let flags = r.u32()?;
    let animation_set = (r.u16()?, r.u16()?);
    let voice = if flags & 0x20 != 0 {
        let engine = r.guid()?;
        let mode = r.guid()?;
        let speed = r.u32()?;
        let pitch = r.u16()?;
        let extra = if r.u8()? != 0 {
            Some(VoiceExtra {
                language: r.u16()?,
                dialect: r.acs_string()?,
                gender: r.u16()?,
                age: r.u16()?,
                style: r.acs_string()?,
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
            font: r.acs_string()?,
            font_height: r.i32()?,
            font_weight: r.u32()?,
            italic: r.u8()?,
            flag2: Some(r.u8()?),
        })
    } else {
        None
    };
    let palette = palette(&mut r)?;
    let tray_icon = if r.u8()? != 0 {
        Some(tray_icon(&mut r)?)
    } else {
        None
    };
    let states = states(&mut r, Reader::acs_string)?;
    let n = r.u16()? as usize;
    let n = r.count(n, 14)?;
    let localized = (0..n)
        .map(|_| {
            Ok(LocalizedInfo {
                langid: r.u16()?,
                name: r.acs_string()?,
                description: r.acs_string()?,
                extra: r.acs_string()?,
            })
        })
        .collect::<Result<_>>()?;
    r.finish("character info")?;
    Ok(Character {
        version,
        localized_locator,
        guid,
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
        localized,
    })
}

/// Image or audio list: u32 count, then (locator, checksum) entries.
fn parse_list(data: &[u8], l: Locator, what: &str) -> Result<Vec<(Locator, u32)>> {
    let go = || -> Result<_> {
        let mut r = Reader::block(data, l.offset, l.size)?;
        let n = r.u32()? as usize;
        let n = r.count(n, 12)?;
        let v = (0..n)
            .map(|_| {
                Ok((
                    Locator {
                        offset: r.u32()?,
                        size: r.u32()?,
                    },
                    r.u32()?,
                ))
            })
            .collect::<Result<_>>()?;
        r.finish(what)?;
        Ok(v)
    };
    go().context(|| what.into())
}

fn parse_image(data: &[u8], l: Locator) -> Result<Option<Image>> {
    let mut r = Reader::block(data, l.offset, l.size)?;
    let kind = r.u8()?;
    match kind {
        0 => return Ok(None),
        1 => {}
        k => return Err(Error::invalid(format!("image kind {k}"), l.offset as usize)),
    }
    let width = r.u16()?;
    let height = r.u16()?;
    let need = stride(width) * height as usize;
    if need > MAX_IMAGE {
        return Err(Error::invalid(
            format!("image {width}x{height} too large"),
            l.offset as usize,
        ));
    }
    let comp = r.u8()?;
    let size = r.u32()? as usize;
    let at = r.pos;
    let pixels = match comp {
        1 => lz::decompress_exact(r.bytes(size)?, need).context(|| "pixels".into())?,
        // The runtime ignores the data size of raw images and reads stride * height bytes;
        // the region follows those bytes.
        0 => r.bytes(need)?.to_vec(),
        c => {
            return Err(Error::invalid(
                format!("image compression flag {c}"),
                at - 5,
            ));
        }
    };
    let csize = r.u32()? as usize;
    let usize_ = r.u32()? as usize;
    let at = r.pos;
    let raw;
    let region_bytes = if csize == 0 {
        r.bytes(usize_)?
    } else {
        if usize_ > MAX_IMAGE {
            return Err(Error::invalid(
                format!("region size {usize_} too large"),
                at,
            ));
        }
        raw = lz::decompress_exact(r.bytes(csize)?, usize_).context(|| "region".into())?;
        &raw[..]
    };
    let region = if usize_ == 0 {
        None
    } else {
        Some(Region::parse(region_bytes, at)?)
    };
    r.finish("image record")?;
    Ok(Some(Image {
        bitmap: Bitmap {
            width,
            height,
            pixels,
        },
        compressed: comp == 1,
        region_compressed: csize != 0,
        region,
    }))
}

fn parse_animation_list(data: &[u8], l: Locator) -> Result<Vec<Animation>> {
    let entries = (|| -> Result<Vec<(String, Locator)>> {
        let mut r = Reader::block(data, l.offset, l.size)?;
        let n = r.u32()? as usize;
        let n = r.count(n, 12)?;
        let v = (0..n)
            .map(|_| {
                Ok((
                    r.acs_string()?,
                    Locator {
                        offset: r.u32()?,
                        size: r.u32()?,
                    },
                ))
            })
            .collect::<Result<_>>()?;
        r.finish("animation list")?;
        Ok(v)
    })()
    .context(|| "animation list".into())?;
    entries
        .into_iter()
        .map(|(name, l)| {
            parse_animation(data, name.clone(), l).context(|| format!("animation {name:?}"))
        })
        .collect()
}

fn parse_animation(data: &[u8], name: String, l: Locator) -> Result<Animation> {
    let mut r = Reader::block(data, l.offset, l.size)?;
    let record_name = r.acs_string()?;
    let transition = r.u8()?;
    let return_animation = r.acs_string()?;
    let at = r.pos;
    let n = r.u16()? as usize;
    if n == 0 {
        return Err(Error::invalid("animation has no frames", at));
    }
    let n = r.count(n, 11)?;
    let frames = (0..n)
        .map(|i| parse_frame(&mut r).context(|| format!("frame {i}")))
        .collect::<Result<_>>()?;
    r.finish("animation record")?;
    Ok(Animation {
        name,
        record_name,
        transition,
        return_animation,
        frames,
    })
}

fn parse_frame(r: &mut Reader) -> Result<Frame> {
    let n = r.u16()? as usize;
    let n = r.count(n, 8)?;
    let images = (0..n)
        .map(|_| {
            Ok(FrameImage {
                image: r.u32()?,
                x: r.i16()?,
                y: r.i16()?,
            })
        })
        .collect::<Result<_>>()?;
    let sound = r.u16()?;
    let duration = r.u16()?;
    let exit = r.i16()?;
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
    let overlays = (0..n)
        .map(|_| {
            let at = r.pos;
            let o = Overlay {
                mouth: r.u8()?,
                replace_top: r.u8()?,
                image: r.u32()?,
                x: r.i16()?,
                y: r.i16()?,
                point_x: r.u16()?,
                point_y: r.u16()?,
            };
            if o.mouth > 6 {
                return Err(Error::invalid(format!("mouth type {}", o.mouth), at));
            }
            Ok(o)
        })
        .collect::<Result<_>>()?;
    Ok(Frame {
        images,
        sound,
        duration,
        exit,
        branches,
        overlays,
    })
}
