//! Types and helpers shared by the ACS, ACF/ACA and Agent 1.5 readers. Owner: formats teammate.

use std::fmt;

use crate::lz::LzError;

/// Largest decompressed body (ACF, ACA/AAF) accepted. Real bodies are below 10 MB; the cap
/// bounds memory for hostile input (bentwire decision, docs/spec.md 3.14 A-FMT-1).
pub const MAX_BODY: usize = 256 << 20;

/// Largest image buffer accepted, as in the Agent 2.0 runtime (acs-format 3.6).
pub const MAX_IMAGE: usize = 10_000_000;

/// Longest string accepted, in UTF-16 code units (acs-format 2, acf-aaf-format 4).
pub const MAX_STRING: u32 = 0x100_0000;

/// Error for every reader in this crate. Readers never panic on malformed input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum Error {
    #[error(
        "unexpected end of data at offset {offset:#x}: need {need} bytes, block ends at {end:#x}"
    )]
    Truncated {
        offset: usize,
        need: usize,
        end: usize,
    },
    #[error("bad magic {0:#010x}")]
    BadMagic(u32),
    #[error("unsupported version {0:#x}")]
    UnsupportedVersion(u32),
    #[error("decompression failed: {0}")]
    Decompress(#[from] LzError),
    #[error("{what} (offset {offset:#x})")]
    Invalid { what: String, offset: usize },
    #[error("{context}: {source}")]
    Context { context: String, source: Box<Error> },
}

impl Error {
    pub(crate) fn invalid(what: impl Into<String>, offset: usize) -> Self {
        Error::Invalid {
            what: what.into(),
            offset,
        }
    }

    /// The innermost error, without the context chain.
    pub fn root(&self) -> &Error {
        match self {
            Error::Context { source, .. } => source.root(),
            e => e,
        }
    }
}

pub type Result<T> = std::result::Result<T, Error>;

pub(crate) trait Context<T> {
    fn context(self, f: impl FnOnce() -> String) -> Result<T>;
}

impl<T, E: Into<Error>> Context<T> for std::result::Result<T, E> {
    fn context(self, f: impl FnOnce() -> String) -> Result<T> {
        self.map_err(|e| Error::Context {
            context: f(),
            source: Box::new(e.into()),
        })
    }
}

/// Bounded little-endian reader over `data[pos..end]`. Offsets in errors are absolute.
#[derive(Clone)]
pub(crate) struct Reader<'a> {
    data: &'a [u8],
    pub pos: usize,
    end: usize,
}

impl<'a> Reader<'a> {
    pub fn new(data: &'a [u8]) -> Self {
        Reader {
            data,
            pos: 0,
            end: data.len(),
        }
    }

    /// A reader for the block `data[off..off+size]`; fails if the block is outside `data`.
    pub fn block(data: &'a [u8], off: u32, size: u32) -> Result<Self> {
        let (off, size) = (off as usize, size as usize);
        match off.checked_add(size) {
            Some(end) if end <= data.len() => Ok(Reader {
                data,
                pos: off,
                end,
            }),
            _ => Err(Error::invalid(
                format!(
                    "block {off:#x}+{size:#x} outside file of {:#x} bytes",
                    data.len()
                ),
                off,
            )),
        }
    }

    pub fn remaining(&self) -> usize {
        self.end - self.pos
    }

    pub fn is_empty(&self) -> bool {
        self.pos == self.end
    }

    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8]> {
        if n > self.remaining() {
            return Err(Error::Truncated {
                offset: self.pos,
                need: n,
                end: self.end,
            });
        }
        let b = &self.data[self.pos..self.pos + n];
        self.pos += n;
        Ok(b)
    }

    pub fn u8(&mut self) -> Result<u8> {
        Ok(self.bytes(1)?[0])
    }

    pub fn u16(&mut self) -> Result<u16> {
        let b = self.bytes(2)?;
        Ok(u16::from_le_bytes([b[0], b[1]]))
    }

    pub fn i16(&mut self) -> Result<i16> {
        Ok(self.u16()? as i16)
    }

    pub fn u32(&mut self) -> Result<u32> {
        let b = self.bytes(4)?;
        Ok(u32::from_le_bytes([b[0], b[1], b[2], b[3]]))
    }

    pub fn i32(&mut self) -> Result<i32> {
        Ok(self.u32()? as i32)
    }

    pub fn guid(&mut self) -> Result<Guid> {
        let mut g = [0; 16];
        g.copy_from_slice(self.bytes(16)?);
        Ok(Guid(g))
    }

    /// Fails unless the whole block was consumed.
    pub fn finish(&self, what: &str) -> Result<()> {
        if self.is_empty() {
            Ok(())
        } else {
            Err(Error::invalid(
                format!("{} unread bytes at end of {what}", self.remaining()),
                self.pos,
            ))
        }
    }

    /// A count of items, each at least `min_size` bytes, checked against the remaining bytes
    /// so that a corrupt count cannot cause a huge allocation.
    pub fn count(&self, n: usize, min_size: usize) -> Result<usize> {
        if n.saturating_mul(min_size) > self.remaining() {
            return Err(Error::invalid(
                format!(
                    "count {n} does not fit in the remaining {} bytes",
                    self.remaining()
                ),
                self.pos,
            ));
        }
        Ok(n)
    }

    fn utf16(&mut self, n: u32) -> Result<String> {
        let at = self.pos;
        if n > MAX_STRING {
            return Err(Error::invalid(format!("string length {n} too large"), at));
        }
        let b = self.bytes(n as usize * 2)?;
        let units: Vec<u16> = b
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes(*c))
            .collect();
        String::from_utf16(&units).map_err(|_| Error::invalid("string is not valid UTF-16", at))
    }

    /// ACS string: u32 count, then count+1 UTF-16LE units ending in 0 (nothing when count is 0).
    pub fn acs_string(&mut self) -> Result<String> {
        let n = self.u32()?;
        if n == 0 {
            return Ok(String::new());
        }
        let s = self.utf16(n)?;
        let at = self.pos;
        if self.u16()? != 0 {
            return Err(Error::invalid("string not terminated by 0", at));
        }
        Ok(s)
    }

    /// ACF string: u32 count, then count UTF-16LE units, no terminator.
    #[allow(dead_code)] // used by the ACF reader (next step)
    pub fn acf_string(&mut self) -> Result<String> {
        let n = self.u32()?;
        self.utf16(n)
    }
}

/// A GUID in Win32 byte layout (u32, u16, u16, u8[8], little-endian).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct Guid(pub [u8; 16]);

impl fmt::Display for Guid {
    /// Registry form, e.g. `{4E574F44-B521-11D0-9E9A-00C04FD7081F}`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let g = &self.0;
        write!(
            f,
            "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-",
            u32::from_le_bytes([g[0], g[1], g[2], g[3]]),
            u16::from_le_bytes([g[4], g[5]]),
            u16::from_le_bytes([g[6], g[7]]),
            g[8],
            g[9]
        )?;
        for b in &g[10..] {
            write!(f, "{b:02X}")?;
        }
        write!(f, "}}")
    }
}

impl fmt::Debug for Guid {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        fmt::Display::fmt(self, f)
    }
}

/// Palette entry as stored: bytes B, G, R, reserved.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rgbquad {
    pub b: u8,
    pub g: u8,
    pub r: u8,
    pub reserved: u8,
}

/// Text-to-speech settings (flags bit 0x20).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Voice {
    pub engine: Guid,
    pub mode: Guid,
    pub speed: u32,
    pub pitch: u16,
    /// The "extra" block. `None` when its presence byte is 0, or when the format version has
    /// no presence byte (ACF below 0x20001).
    pub extra: Option<VoiceExtra>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceExtra {
    pub language: u16,
    pub dialect: String,
    pub gender: u16,
    pub age: u16,
    pub style: String,
}

/// Word balloon settings (present unless flags bit 0x100). Raw values; no runtime defaults.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Balloon {
    pub lines: u8,
    pub chars_per_line: u8,
    /// COLORREF 0x00BBGGRR.
    pub foreground: u32,
    pub background: u32,
    pub border: u32,
    pub font: String,
    pub font_height: i32,
    pub font_weight: u32,
    pub italic: u8,
    /// The trailing byte (underline or strikeout, unproven). `None` for ACF 0x1001E and older.
    pub flag2: Option<u8>,
}

/// Tray icon as two packed DIBs (BITMAPINFOHEADER + colour table + bits), stored verbatim.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayIcon {
    pub mask_dib: Vec<u8>,
    pub color_dib: Vec<u8>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct State {
    pub name: String,
    pub animations: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalizedInfo {
    pub langid: u16,
    pub name: String,
    pub description: String,
    pub extra: String,
}

/// Win32 RECT, top-down coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

/// A Win32 RGNDATA with RDH_RECTANGLES (`dwSize` 32, `iType` 1). The parser requires
/// `32 + 16 * nCount == size`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Region {
    /// `nRgnSize` header field, verbatim.
    pub rgn_size: u32,
    pub bound: Rect,
    pub rects: Vec<Rect>,
}

impl Region {
    pub(crate) fn parse(b: &[u8], offset: usize) -> Result<Region> {
        let mut r = Reader::new(b);
        let bad = |what: &str| Error::invalid(format!("region: {what}"), offset);
        let (dw, ty, n, rgn_size) = (r.u32()?, r.u32()?, r.u32()?, r.u32()?);
        if dw != 32 || ty != 1 {
            return Err(bad("header is not dwSize 32, iType 1"));
        }
        if (n as u64) * 16 + 32 != b.len() as u64 {
            return Err(bad("rectangle count does not match the size"));
        }
        let rect = |r: &mut Reader| -> Result<Rect> {
            Ok(Rect {
                left: r.i32()?,
                top: r.i32()?,
                right: r.i32()?,
                bottom: r.i32()?,
            })
        };
        let bound = rect(&mut r)?;
        let rects = (0..n).map(|_| rect(&mut r)).collect::<Result<_>>()?;
        Ok(Region {
            rgn_size,
            bound,
            rects,
        })
    }
}

/// Row stride of an 8 bpp DIB: width rounded up to 4.
pub fn stride(width: u16) -> usize {
    (width as usize + 3) & !3
}

/// An 8 bpp palette-indexed picture as stored: bottom-up rows, each `stride(width)` bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u16,
    pub height: u16,
    pub pixels: Vec<u8>,
}

impl Bitmap {
    /// Palette index at (x, y) in top-down coordinates. Panics if out of range.
    pub fn pixel(&self, x: usize, y: usize) -> u8 {
        let row = self.height as usize - 1 - y;
        self.pixels[row * stride(self.width) + x]
    }

    /// The pixels top-down, `width * height` bytes, padding removed.
    pub fn top_down(&self) -> Vec<u8> {
        let (w, h, s) = (
            self.width as usize,
            self.height as usize,
            stride(self.width),
        );
        let mut out = Vec::with_capacity(w * h);
        for y in 0..h {
            let row = h - 1 - y;
            out.extend_from_slice(&self.pixels[row * s..row * s + w]);
        }
        out
    }
}

pub(crate) fn palette(r: &mut Reader) -> Result<Vec<Rgbquad>> {
    let n = r.u32()? as usize;
    let n = r.count(n, 4)?;
    (0..n)
        .map(|_| {
            let b = r.bytes(4)?;
            Ok(Rgbquad {
                b: b[0],
                g: b[1],
                r: b[2],
                reserved: b[3],
            })
        })
        .collect()
}

pub(crate) fn tray_icon(r: &mut Reader) -> Result<TrayIcon> {
    let n = r.u32()? as usize;
    let mask_dib = r.bytes(n)?.to_vec();
    let n = r.u32()? as usize;
    let color_dib = r.bytes(n)?.to_vec();
    Ok(TrayIcon {
        mask_dib,
        color_dib,
    })
}

pub(crate) fn states<'a>(
    r: &mut Reader<'a>,
    string: fn(&mut Reader<'a>) -> Result<String>,
) -> Result<Vec<State>> {
    let n = r.u16()? as usize;
    let n = r.count(n, 6)?;
    (0..n)
        .map(|_| {
            let name = string(r)?;
            let k = r.u16()? as usize;
            let k = r.count(k, 4)?;
            let animations = (0..k).map(|_| string(r)).collect::<Result<_>>()?;
            Ok(State { name, animations })
        })
        .collect()
}

/// Case-insensitive name comparison as the runtime does it (`_wcsicmp`, ASCII folding).
pub fn names_equal(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
}
