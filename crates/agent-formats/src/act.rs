//! ACT (Office 97 and Microsoft Bob actors), act-format 4. Owner: act teammate.
//!
//! [`parse`] reads a whole `.ACT` / `.ACP` file into faithful source structures:
//! header, colour and transition tables, decoded payloads (composite poses, bitmaps,
//! WMF images with their record list), sounds as stored RIFF bytes, animation scripts
//! as raw command lists and string groups as raw bytes. Malformed input gives an
//! [`ActError`], never a panic.

use std::collections::HashMap;

pub mod lz;
pub mod palette;

pub use palette::office_palette_from_mso97;

/// Errors from [`parse`].
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ActError {
    #[error("truncated: {what} needs bytes {start}..{end}, have {len}")]
    Truncated {
        what: &'static str,
        start: usize,
        end: usize,
        len: usize,
    },
    #[error("bad pack magic (expected \"LP\")")]
    BadMagic,
    #[error("unsupported pack version {major}.{minor}")]
    Version { major: u16, minor: u16 },
    #[error("invalid {what}: {detail}")]
    Invalid { what: &'static str, detail: String },
    #[error("payload {index}: {detail}")]
    Payload { index: usize, detail: String },
    #[error("sound {index}: {detail}")]
    Sound { index: usize, detail: String },
    #[error("animation group {id}: {detail}")]
    Animation { id: u16, detail: String },
}

fn invalid(what: &'static str, detail: impl Into<String>) -> ActError {
    ActError::Invalid {
        what,
        detail: detail.into(),
    }
}

/// Bounds-checked slice.
fn slice<'a>(
    d: &'a [u8],
    start: usize,
    len: usize,
    what: &'static str,
) -> Result<&'a [u8], ActError> {
    let end = start.checked_add(len).ok_or(ActError::Truncated {
        what,
        start,
        end: usize::MAX,
        len: d.len(),
    })?;
    d.get(start..end).ok_or(ActError::Truncated {
        what,
        start,
        end,
        len: d.len(),
    })
}

fn u16_at(d: &[u8], o: usize, what: &'static str) -> Result<u16, ActError> {
    let s = slice(d, o, 2, what)?;
    Ok(u16::from_le_bytes([s[0], s[1]]))
}

fn i16_at(d: &[u8], o: usize, what: &'static str) -> Result<i16, ActError> {
    Ok(u16_at(d, o, what)? as i16)
}

fn u32_at(d: &[u8], o: usize, what: &'static str) -> Result<u32, ActError> {
    let s = slice(d, o, 4, what)?;
    Ok(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// A parsed `.ACT` / `.ACP` file. Only the first pack item is read (every sample has
/// one; the runtime also uses the first).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActFile {
    /// Pack version: 1.0 Microsoft Bob, 2.2 Office 97.
    pub major: u16,
    pub minor: u16,
    /// Raw item name bytes without the NUL (build codepage, e.g. cp932).
    pub item_name: Vec<u8>,
    pub header: Header,
    /// Colour table RGB triples (v2.2 only, empty otherwise). The slot array is dropped.
    pub color_table: Vec<[u8; 3]>,
    /// Pose transition table (v2.2 only).
    pub transitions: Vec<Transition>,
    /// Copy of the rest pose stored in the transition block (v2.2 only).
    pub transition_rest_pose: Option<u16>,
    /// Payloads by payload id.
    pub payloads: Vec<Payload>,
    pub sounds: Vec<Sound>,
    /// Animation groups sorted by id, variants in group order.
    pub animations: Vec<Group<Script>>,
    /// String groups sorted by id, raw bytes (UTF-16LE for v2, ANSI for v1).
    pub strings: Vec<Group<Vec<u8>>>,
    /// Animation index entries referenced by no group.
    pub unused_animation_entries: usize,
    /// String index entries referenced by no group.
    pub unused_string_entries: usize,
}

/// The 0x46-byte resource header (act-format 4.2), section offsets omitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Header {
    pub kind: u16,
    pub rest_pose: u16,
    pub idle_anim: u16,
    pub logical_w: i16,
    pub logical_h: i16,
    pub default_px_w: i16,
    pub default_px_h: i16,
    pub unk10: u16,
    pub unk12: u16,
    pub unk14: u16,
    pub unk16: u16,
    /// Raw words at 0x18, 0x1A, 0x1C, 0x1E (meaning unknown, act-format 4.2).
    pub rect: [i16; 4],
    pub nohit_image: u16,
    pub stretch_x0: u16,
    pub stretch_x1: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Transition {
    pub from_pose: u16,
    pub to_pose: u16,
    pub anim_id: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Payload {
    Composite(Vec<Child>),
    Bitmap(Bitmap),
    Wmf(Wmf),
}

/// One placed child of a composite pose, rectangle in logical units.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Child {
    pub id: u16,
    pub left: i16,
    pub top: i16,
    pub right: i16,
    pub bottom: i16,
}

/// A decoded 8-bit bitmap. `pixels` are Office palette indices, top-down, `width *
/// height` bytes without row padding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Bitmap {
    pub width: u32,
    pub height: u32,
    pub flags: u32,
    pub pixels: Vec<u8>,
}

/// A placeable WMF payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Wmf {
    /// The whole payload, 22-byte placeable header included.
    pub bytes: Vec<u8>,
    /// Placeable bounding box: left, top, right, bottom.
    pub bbox: [i16; 4],
    pub inch: u16,
    /// Records up to and including EOF.
    pub records: Vec<WmfRecord>,
}

/// One metafile record: function number and parameter bytes (size words minus 3).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WmfRecord {
    pub function: u16,
    pub params: Vec<u8>,
}

impl WmfRecord {
    /// Parameter word `i` (u16), if present.
    pub fn word(&self, i: usize) -> Option<u16> {
        let b = self.params.get(2 * i..2 * i + 2)?;
        Some(u16::from_le_bytes([b[0], b[1]]))
    }

    /// Parameter word `i` as i16.
    pub fn int(&self, i: usize) -> Option<i16> {
        self.word(i).map(|v| v as i16)
    }

    /// Parameter dword starting at word `i`.
    pub fn dword(&self, i: usize) -> Option<u32> {
        Some(u32::from(self.word(i)?) | (u32::from(self.word(i + 1)?) << 16))
    }
}

/// An embedded sound: the complete RIFF WAVE file as stored plus its `fmt ` fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Sound {
    pub bytes: Vec<u8>,
    pub format_tag: u16,
    pub channels: u16,
    pub sample_rate: u32,
    pub bits_per_sample: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Group<T> {
    pub id: u16,
    pub variants: Vec<T>,
}

/// An animation script (act-format 4.11).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Script {
    /// Header word (0x0100 in every sample, not read by the runtime).
    pub header: u16,
    pub commands: Vec<Command>,
}

/// One 6-byte script command, raw. Kinds: 0 frame, 1 random branch, 2 sound,
/// 3 host-condition branch, 4 time-of-day branch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Command {
    pub kind: u16,
    pub a: u16,
    pub b: u16,
}

pub const CMD_FRAME: u16 = 0;
pub const CMD_RANDOM: u16 = 1;
pub const CMD_SOUND: u16 = 2;
pub const CMD_HOST: u16 = 3;
pub const CMD_DAYPART: u16 = 4;

/// Payload id / sound index meaning "none".
pub const NONE: u16 = 0xFFFF;

pub const WMF_PLACEABLE_KEY: [u8; 4] = [0xD7, 0xCD, 0xC6, 0x9A];
pub const COMPOSITE_TAG: u16 = 0x0014;

impl ActFile {
    /// Decode a string group entry: UTF-16LE for v2 files, `None` for v1 (ANSI,
    /// codepage chosen by the caller) or invalid UTF-16.
    pub fn string_text(&self, raw: &[u8]) -> Option<String> {
        if self.major < 2 || !raw.len().is_multiple_of(2) {
            return None;
        }
        let units: Vec<u16> = raw
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect();
        String::from_utf16(&units).ok()
    }

    pub fn animation(&self, id: u16) -> Option<&Group<Script>> {
        self.animations.iter().find(|g| g.id == id)
    }

    pub fn string_group(&self, id: u16) -> Option<&Group<Vec<u8>>> {
        self.strings.iter().find(|g| g.id == id)
    }
}

/// Parse a complete `.ACT` / `.ACP` file.
pub fn parse(d: &[u8]) -> Result<ActFile, ActError> {
    if slice(d, 0, 2, "magic")? != b"LP" {
        return Err(ActError::BadMagic);
    }
    let major = u16_at(d, 2, "pack header")?;
    let minor = u16_at(d, 4, "pack header")?;
    if major != 1 && major != 2 {
        return Err(ActError::Version { major, minor });
    }
    let count = usize::from(u16_at(d, 6, "pack header")?);
    let ntsz = usize::from(u16_at(d, 8, "pack header")?);
    if count == 0 {
        return Err(invalid("pack", "no items"));
    }
    let mut offs = Vec::with_capacity(count + 1);
    for i in 0..=count {
        offs.push(u32_at(d, 10 + 4 * i, "item offsets")? as usize);
    }
    if offs[count] != d.len() {
        return Err(invalid("pack", "last item offset != file size"));
    }
    let nt_start = 10 + 4 * (count + 1);
    let nt = slice(d, nt_start, ntsz, "name table")?;
    if nt.last() != Some(&0) {
        return Err(invalid("pack", "name table not NUL terminated"));
    }
    let names: Vec<&[u8]> = nt[..nt.len() - 1].split(|&b| b == 0).collect();
    if names.len() != count {
        return Err(invalid("pack", "name count != item count"));
    }
    if offs[0] != nt_start + ntsz {
        return Err(invalid("pack", "first item does not follow the name table"));
    }
    if offs.windows(2).any(|w| w[0] > w[1]) {
        return Err(invalid("pack", "item offsets not ascending"));
    }
    let res = &d[offs[0]..offs[1]];
    parse_resource(res, major, minor, names[0].to_vec())
}

struct Sections {
    pay_data: usize,
    pay_index: usize,
    snd_data: usize,
    snd_index: usize,
    anim_data: usize,
    anim_index: usize,
    str_data: usize,
    str_index: usize,
}

fn parse_resource(
    r: &[u8],
    major: u16,
    minor: u16,
    item_name: Vec<u8>,
) -> Result<ActFile, ActError> {
    slice(r, 0, 0x46, "resource header")?;
    if u16_at(r, 0, "header")? != 0x46 {
        return Err(invalid("header", "header size != 0x46"));
    }
    let w = |o| u16_at(r, o, "header");
    let i = |o| i16_at(r, o, "header");
    let header = Header {
        kind: w(0x02)?,
        rest_pose: w(0x04)?,
        idle_anim: w(0x06)?,
        logical_w: i(0x08)?,
        logical_h: i(0x0A)?,
        default_px_w: i(0x0C)?,
        default_px_h: i(0x0E)?,
        unk10: w(0x10)?,
        unk12: w(0x12)?,
        unk14: w(0x14)?,
        unk16: w(0x16)?,
        rect: [i(0x18)?, i(0x1A)?, i(0x1C)?, i(0x1E)?],
        nohit_image: w(0x20)?,
        stretch_x0: w(0x22)?,
        stretch_x1: w(0x24)?,
    };
    let mut sec = [0usize; 8];
    for (k, s) in sec.iter_mut().enumerate() {
        *s = u32_at(r, 0x26 + 4 * k, "header")? as usize;
    }
    if sec.windows(2).any(|p| p[0] > p[1]) {
        return Err(invalid("header", "section offsets not ascending"));
    }
    if sec[7] > r.len() {
        return Err(invalid("header", "string index beyond resource"));
    }
    let s = Sections {
        pay_data: sec[0],
        pay_index: sec[1],
        snd_data: sec[2],
        snd_index: sec[3],
        anim_data: sec[4],
        anim_index: sec[5],
        str_data: sec[6],
        str_index: sec[7],
    };

    // Optional block (act-format 4.3).
    let mut p = 0x46usize;
    let mut color_table = Vec::new();
    let mut transitions = Vec::new();
    let mut transition_rest_pose = None;
    if major == 2 && minor != 0 {
        let n = u32_at(r, p, "colour table")? as usize;
        p += 4;
        let tab = slice(r, p, n.saturating_mul(6), "colour table")?;
        color_table = tab[..4 * n]
            .as_chunks::<4>()
            .0
            .iter()
            .map(|c| [c[0], c[1], c[2]])
            .collect();
        p += 6 * n;
    }
    if major == 2 && minor >= 2 {
        let m = u32_at(r, p, "transition table")? as usize;
        transition_rest_pose = Some(u16_at(r, p + 4, "transition table")?);
        p += 6;
        let tab = slice(r, p, m.saturating_mul(6), "transition table")?;
        transitions = tab
            .as_chunks::<6>()
            .0
            .iter()
            .map(|c| Transition {
                from_pose: u16::from_le_bytes([c[0], c[1]]),
                to_pose: u16::from_le_bytes([c[2], c[3]]),
                anim_id: u16::from_le_bytes([c[4], c[5]]),
            })
            .collect();
        p += 6 * m;
    }
    if p != s.pay_data {
        return Err(invalid(
            "optional block",
            format!("ends at {p:#x}, payload data starts at {:#x}", s.pay_data),
        ));
    }

    let payloads = parse_payloads(r, &s)?;
    let sounds = parse_sounds(r, &s)?;
    let (anims, unused_animation_entries) = parse_groups(r, s.anim_data, s.anim_index, s.str_data)?;
    let (strs, unused_string_entries) = if s.str_index < r.len() {
        parse_groups(r, s.str_data, s.str_index, r.len())?
    } else {
        (Vec::new(), 0)
    };

    let npay = payloads.len();
    let nsnd = sounds.len();
    let mut animations = Vec::with_capacity(anims.len());
    for (id, blobs) in anims {
        let mut variants = Vec::with_capacity(blobs.len());
        for blob in blobs {
            variants.push(parse_script(blob, id, npay, nsnd)?);
        }
        animations.push(Group { id, variants });
    }
    for t in &transitions {
        if !animations.iter().any(|g| g.id == t.anim_id) {
            return Err(invalid(
                "transition table",
                format!("animation {} missing", t.anim_id),
            ));
        }
    }
    let strings = strs
        .into_iter()
        .map(|(id, vs)| Group {
            id,
            variants: vs.into_iter().map(<[u8]>::to_vec).collect(),
        })
        .collect();

    Ok(ActFile {
        major,
        minor,
        item_name,
        header,
        color_table,
        transitions,
        transition_rest_pose,
        payloads,
        sounds,
        animations,
        strings,
        unused_animation_entries,
        unused_string_entries,
    })
}

const OFFSET_MASK: u32 = 0x3FFF_FFFF;

fn parse_payloads(r: &[u8], s: &Sections) -> Result<Vec<Payload>, ActError> {
    let len = s.snd_data - s.pay_index;
    if !len.is_multiple_of(4) || len < 4 {
        return Err(invalid("payload index", format!("size {len}")));
    }
    let ent: Vec<u32> = slice(r, s.pay_index, len, "payload index")?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let npay = ent.len() - 1;
    let last = ent[npay];
    if last >> 30 != 0 || s.pay_data.checked_add(last as usize) != Some(s.pay_index) {
        return Err(invalid("payload index", "end sentinel mismatch"));
    }
    let data = &r[s.pay_data..s.pay_index];
    let mut packs: HashMap<usize, Vec<Bitmap>> = HashMap::new();
    let mut out = Vec::with_capacity(npay);
    for i in 0..npay {
        let perr = |detail: String| ActError::Payload { index: i, detail };
        let off = (ent[i] & OFFSET_MASK) as usize;
        let sub = (ent[i] >> 30) as usize;
        // Runtime rule: the blob ends at the first later entry with a different offset.
        let next = ent[i + 1..]
            .iter()
            .map(|e| (e & OFFSET_MASK) as usize)
            .find(|&o| o != off)
            .ok_or_else(|| perr("no end offset".into()))?;
        if next <= off {
            return Err(perr("offsets not ascending".into()));
        }
        let b = slice(data, off, next - off, "payload")?;
        let p = if b.starts_with(&WMF_PLACEABLE_KEY) {
            if sub != 0 {
                return Err(perr("WMF with sub index".into()));
            }
            Payload::Wmf(parse_wmf(b).map_err(perr)?)
        } else if b.len() >= 2 && u16::from_le_bytes([b[0], b[1]]) == COMPOSITE_TAG {
            if sub != 0 {
                return Err(perr("composite with sub index".into()));
            }
            Payload::Composite(parse_composite(b, npay).map_err(perr)?)
        } else if b.starts_with(b"DCIK") || b.starts_with(b"MNAK") {
            if let std::collections::hash_map::Entry::Vacant(e) = packs.entry(off) {
                e.insert(decode_pack(b).map_err(perr)?);
            }
            let imgs = &packs[&off];
            let img = imgs
                .get(sub)
                .ok_or_else(|| perr(format!("sub index {sub} >= {}", imgs.len())))?;
            Payload::Bitmap(img.clone())
        } else {
            let head: String = b.iter().take(8).map(|x| format!("{x:02x}")).collect();
            return Err(perr(format!("unknown payload kind {head}")));
        };
        out.push(p);
    }
    Ok(out)
}

fn parse_composite(b: &[u8], npay: usize) -> Result<Vec<Child>, String> {
    let n = usize::from(u16::from_le_bytes([
        b.get(2).copied().unwrap_or(0),
        b.get(3).copied().unwrap_or(0),
    ]));
    if b.len() < 4 || b.len() != 4 + 10 * n {
        return Err(format!("composite size {} for {n} children", b.len()));
    }
    let mut v = Vec::with_capacity(n);
    for c in b[4..].as_chunks::<10>().0 {
        let w = |k: usize| u16::from_le_bytes([c[k], c[k + 1]]);
        let ch = Child {
            id: w(0),
            left: w(2) as i16,
            top: w(4) as i16,
            right: w(6) as i16,
            bottom: w(8) as i16,
        };
        if ch.id != NONE && usize::from(ch.id) >= npay {
            return Err(format!("child {} out of range", ch.id));
        }
        if ch.left > ch.right || ch.top > ch.bottom {
            return Err(format!("child {} rectangle inverted", ch.id));
        }
        v.push(ch);
    }
    Ok(v)
}

fn parse_wmf(b: &[u8]) -> Result<Wmf, String> {
    if b.len() < 22 + 18 {
        return Err("WMF shorter than headers".into());
    }
    let w = |o: usize| u16::from_le_bytes([b[o], b[o + 1]]);
    let bbox = [w(6) as i16, w(8) as i16, w(10) as i16, w(12) as i16];
    let inch = w(14);
    if w(24) != 9 || !matches!(w(22), 1 | 2) {
        return Err("bad metafile header".into());
    }
    // mtSize and the placeable checksum are ignored, as by the runtime (act-format 4.6).
    let mut q = 22 + 18;
    let mut records = Vec::new();
    loop {
        if q + 6 > b.len() {
            return Err("records run past the payload without EOF".into());
        }
        let sz = u32::from_le_bytes([b[q], b[q + 1], b[q + 2], b[q + 3]]) as usize;
        let function = w(q + 4);
        if function == 0 {
            if q + 6 != b.len() {
                return Err("EOF record not at payload end".into());
            }
            records.push(WmfRecord {
                function,
                params: Vec::new(),
            });
            break;
        }
        if sz < 3 {
            return Err(format!("record size {sz} < 3"));
        }
        let end = sz
            .checked_mul(2)
            .and_then(|n| q.checked_add(n))
            .filter(|&e| e <= b.len())
            .ok_or("record runs past the payload")?;
        records.push(WmfRecord {
            function,
            params: b[q + 6..end].to_vec(),
        });
        q = end;
    }
    Ok(Wmf {
        bytes: b.to_vec(),
        bbox,
        inch,
        records,
    })
}

fn decode_pack(b: &[u8]) -> Result<Vec<Bitmap>, String> {
    let rd = |o: usize| -> Result<u32, String> {
        b.get(o..o + 4)
            .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
            .ok_or_else(|| "bitmap blob header truncated".to_string())
    };
    let total = rd(4)? as usize;
    let (starts, p) = if b.starts_with(b"DCIK") {
        (vec![0usize], 8usize)
    } else {
        let n = rd(8)? as usize;
        if n == 0 || n > 64 {
            return Err(format!("MNAK image count {n}"));
        }
        let mut st = vec![0usize];
        for k in 0..n - 1 {
            st.push(rd(12 + 4 * k)? as usize);
        }
        (st, 12 + 4 * (n - 1))
    };
    let src = b.get(p..).ok_or("bitmap blob header truncated")?;
    let out = lz::decompress(src, total).map_err(|e| format!("LZ: {e}"))?;
    let mut ends: Vec<usize> = starts[1..].to_vec();
    ends.push(total);
    let mut imgs = Vec::with_capacity(starts.len());
    for (k, (&a, &e)) in starts.iter().zip(&ends).enumerate() {
        if a > e || e > total {
            return Err(format!("image {k} bounds {a}..{e} outside {total}"));
        }
        imgs.push(decode_image(&out[a..e]).map_err(|m| format!("image {k}: {m}"))?);
    }
    Ok(imgs)
}

fn decode_image(im: &[u8]) -> Result<Bitmap, String> {
    if im.len() < 12 {
        return Err("image record shorter than 12 bytes".into());
    }
    let rd = |o: usize| u32::from_le_bytes([im[o], im[o + 1], im[o + 2], im[o + 3]]);
    let (width, height, flags) = (rd(0), rd(4), rd(8));
    let w = width as usize;
    let h = height as usize;
    let stride = w.checked_add(3).ok_or("width overflow")? & !3;
    let need = stride.checked_mul(h).ok_or("size overflow")?;
    let src = &im[12..];
    let px: Vec<u8> = if flags & 1 != 0 {
        // RLE output is at most 127 bytes per input byte.
        if need > src.len().saturating_mul(127) {
            return Err(format!(
                "{width}x{height} cannot come from {} RLE bytes",
                src.len()
            ));
        }
        rle(src, need)
    } else if flags & 2 != 0 {
        src.to_vec()
    } else {
        return Err(format!("bitmap flags {flags}"));
    };
    if px.len() < need {
        return Err(format!("{width}x{height} got {} of {need} bytes", px.len()));
    }
    let mut pixels = Vec::with_capacity(w * h);
    for y in 0..h {
        let row = (h - 1 - y) * stride;
        pixels.extend_from_slice(&px[row..row + w]);
    }
    Ok(Bitmap {
        width,
        height,
        flags,
        pixels,
    })
}

/// RLE decoder (act-format 4.7); output is truncated to `limit` bytes.
fn rle(src: &[u8], limit: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(limit);
    let mut i = 0;
    while i < src.len() && out.len() < limit {
        let c = src[i];
        i += 1;
        if c == 0 {
            break;
        }
        if c & 0x80 != 0 {
            let n = usize::from(c & 0x7F);
            let end = (i + n).min(src.len());
            out.extend_from_slice(&src[i..end]);
            i = end;
        } else if let Some(&v) = src.get(i) {
            out.extend(std::iter::repeat_n(v, usize::from(c)));
            i += 1;
        }
    }
    out.truncate(limit);
    out
}

fn parse_sounds(r: &[u8], s: &Sections) -> Result<Vec<Sound>, ActError> {
    let len = s.anim_data - s.snd_index;
    if !len.is_multiple_of(4) {
        return Err(invalid("sound index", format!("size {len}")));
    }
    if len == 0 {
        return Ok(Vec::new());
    }
    let sent: Vec<usize> = slice(r, s.snd_index, len, "sound index")?
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize)
        .collect();
    let n = sent.len() - 1;
    if s.snd_data.checked_add(sent[n]) != Some(s.snd_index) {
        return Err(invalid("sound index", "end sentinel mismatch"));
    }
    let data = &r[s.snd_data..s.snd_index];
    let mut out = Vec::with_capacity(n);
    for i in 0..n {
        let serr = |detail: &str| ActError::Sound {
            index: i,
            detail: detail.into(),
        };
        if sent[i] > sent[i + 1] {
            return Err(serr("offsets not ascending"));
        }
        let w = slice(data, sent[i], sent[i + 1] - sent[i], "sound")?;
        if w.len() < 12 || &w[..4] != b"RIFF" || &w[8..12] != b"WAVE" {
            return Err(serr("not RIFF WAVE"));
        }
        let rd32 = |o: usize| u32::from_le_bytes([w[o], w[o + 1], w[o + 2], w[o + 3]]) as usize;
        let riff = rd32(4).saturating_add(8);
        if riff != w.len() && riff + 1 != w.len() {
            return Err(serr("RIFF size does not match blob"));
        }
        let mut q = 12usize;
        let mut fmt = None;
        while q + 8 <= w.len() {
            let csz = rd32(q + 4);
            if &w[q..q + 4] == b"fmt " {
                let f = w
                    .get(q + 8..q + 24)
                    .ok_or_else(|| serr("fmt chunk truncated"))?;
                let r16 = |o: usize| u16::from_le_bytes([f[o], f[o + 1]]);
                fmt = Some(Sound {
                    bytes: w.to_vec(),
                    format_tag: r16(0),
                    channels: r16(2),
                    sample_rate: u32::from_le_bytes([f[4], f[5], f[6], f[7]]),
                    bits_per_sample: r16(14),
                });
            }
            q = match q.checked_add(8 + csz + (csz & 1)) {
                Some(v) => v,
                None => break,
            };
        }
        out.push(fmt.ok_or_else(|| serr("no fmt chunk"))?);
    }
    Ok(out)
}

type RawGroups<'a> = Vec<(u16, Vec<&'a [u8]>)>;

fn parse_groups(
    r: &[u8],
    data: usize,
    index: usize,
    end: usize,
) -> Result<(RawGroups<'_>, usize), ActError> {
    if index == end {
        return Ok((Vec::new(), 0));
    }
    let gc = u32_at(r, index, "group index")? as usize;
    let len = end - index;
    if len < 4 || !(len - 4).is_multiple_of(6) || gc > (len - 4) / 6 {
        return Err(invalid(
            "group index",
            format!("size {len} for {gc} groups"),
        ));
    }
    let nent = (len - 4) / 6 - gc;
    let mut used = vec![false; nent];
    let mut out = Vec::with_capacity(gc);
    let mut last: i32 = -1;
    for g in 0..gc {
        let h = index + 4 + 6 * g;
        let id = u16_at(r, h, "group header")?;
        let cnt = usize::from(u16_at(r, h + 2, "group header")?);
        let first = usize::from(u16_at(r, h + 4, "group header")?);
        if i32::from(id) <= last {
            return Err(invalid("group index", "group ids not sorted ascending"));
        }
        last = i32::from(id);
        if first + cnt > nent {
            return Err(invalid(
                "group index",
                format!("group {id} entries out of range"),
            ));
        }
        let mut vs = Vec::with_capacity(cnt);
        for (j, u) in used.iter_mut().enumerate().skip(first).take(cnt) {
            let e = index + 4 + 6 * (gc + j);
            let off = u32_at(r, e, "group entry")? as usize;
            let sz = usize::from(u16_at(r, e + 4, "group entry")?);
            let start = data.checked_add(off).filter(|&st| st + sz <= index);
            let start = start
                .ok_or_else(|| invalid("group index", format!("group {id} blob beyond index")))?;
            vs.push(&r[start..start + sz]);
            *u = true;
        }
        out.push((id, vs));
    }
    Ok((out, used.iter().filter(|u| !**u).count()))
}

fn parse_script(blob: &[u8], id: u16, npay: usize, nsnd: usize) -> Result<Script, ActError> {
    let aerr = |detail: String| ActError::Animation { id, detail };
    if blob.len() < 4 {
        return Err(aerr("script shorter than 4 bytes".into()));
    }
    let header = u16::from_le_bytes([blob[0], blob[1]]);
    let n = usize::from(u16::from_le_bytes([blob[2], blob[3]]));
    if blob.len() != 4 + 6 * n {
        return Err(aerr(format!("script size {} for {n} commands", blob.len())));
    }
    let mut commands = Vec::with_capacity(n);
    for c in blob[4..].as_chunks::<6>().0 {
        let cmd = Command {
            kind: u16::from_le_bytes([c[0], c[1]]),
            a: u16::from_le_bytes([c[2], c[3]]),
            b: u16::from_le_bytes([c[4], c[5]]),
        };
        let a = usize::from(cmd.a);
        match cmd.kind {
            CMD_FRAME if cmd.a != NONE && a >= npay => {
                return Err(aerr(format!("pose {a} out of range")));
            }
            CMD_RANDOM | CMD_HOST | CMD_DAYPART if a >= n => {
                return Err(aerr(format!("branch target {a} >= {n}")));
            }
            CMD_SOUND if cmd.a != NONE && a >= nsnd => {
                return Err(aerr(format!("sound {a} out of range")));
            }
            0..=4 => {}
            k => return Err(aerr(format!("unknown command {k}"))),
        }
        commands.push(cmd);
    }
    Ok(Script { header, commands })
}
