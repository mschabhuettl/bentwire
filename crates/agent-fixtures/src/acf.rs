//! Writer for synthetic ACF + ACA/AAF characters at any accepted version (acf-aaf-format 5
//! to 7, agent15-format 4). Owner: formats teammate.

use crate::acs::{Balloon, Voice};
use crate::common::{Compression, Out, Picture, lz_compress, packed_dib, wav_pcm};

#[derive(Debug, Clone)]
pub struct Overlay {
    pub mouth: u8,
    /// `None` writes an empty overlay (size 0).
    pub picture: Option<(i16, i16, Picture)>,
    pub reserved: u8,
    /// Written only above 0x1001F. Nonzero also writes the overlay's region.
    pub replace_base: u8,
}

#[derive(Debug, Clone)]
pub enum Base {
    Absent,
    Empty,
    Picture(Picture),
}

#[derive(Debug, Clone)]
pub struct Frame {
    pub image: u16,
    pub sound: u16,
    pub duration: u16,
    pub offset: (i16, i16),
    /// Written only from 0x10020.
    pub exit: i16,
    pub branches: Vec<(u16, u16)>,
    /// Written only above 0x1001F when the frame has overlays.
    pub base: Base,
    pub overlays: Vec<Overlay>,
}

#[derive(Debug, Clone)]
pub struct Animation {
    pub name: String,
    pub file_name: String,
    pub return_animation: String,
    pub checksum: u32,
    /// Written only from 0x10020.
    pub transition: u8,
    pub sounds: Vec<Vec<u8>>,
    /// Full-frame pictures at the character size; `None` = empty image (size 0).
    pub images: Vec<Option<Picture>>,
    /// The byte after each image size.
    pub image_unknown: u8,
    pub frames: Vec<Frame>,
    /// `Raw` writes compression flag 0.
    pub compression: Compression,
    /// Omit the header although the ACF version has one; the reader then sees an
    /// unrecognized header version and reads the file as headerless.
    pub headerless: bool,
}

#[derive(Debug, Clone)]
pub struct Acf {
    pub magic: u32,
    pub version: u32,
    pub guid: [u8; 16],
    /// (LANGID, name, description, extra). Below 0x20001 only the first entry is written,
    /// without its LANGID.
    pub localized: Vec<(u16, String, String, String)>,
    pub width: u16,
    pub height: u16,
    pub transparent: u8,
    /// Bits 0x20 and 0x100 are derived from `voice` / `balloon` as in the ACS writer.
    pub flags: u32,
    pub animation_set: (u16, u16),
    /// The extra block is written only above 0x20000.
    pub voice: Option<Voice>,
    /// The trailing byte is written only above 0x1001E.
    pub balloon: Option<Balloon>,
    pub palette: Vec<[u8; 4]>,
    /// Written only above 0x20000.
    pub tray_icon: Option<(Vec<u8>, Vec<u8>)>,
    pub states: Vec<(String, Vec<String>)>,
    pub animations: Vec<Animation>,
}

impl Acf {
    pub fn flags_written(&self) -> u32 {
        let mut f = self.flags & !0x120;
        if self.voice.is_some() {
            f |= 0x20;
        }
        if self.balloon.is_none() {
            f |= 0x100;
        }
        f
    }

    /// The decompressed ACF body.
    pub fn body(&self) -> Vec<u8> {
        let v = self.version;
        let mut o = Out::default();
        o.u32(v);
        o.u16(self.animations.len() as u16);
        for a in &self.animations {
            o.acf_string(&a.name);
            o.acf_string(&a.file_name);
            o.acf_string(&a.return_animation);
            if v > 0x1_001D {
                o.u32(a.checksum);
            }
        }
        o.bytes(&self.guid);
        if v >= 0x2_0001 {
            o.u16(self.localized.len() as u16);
            for (lang, name, desc, extra) in &self.localized {
                o.u16(*lang);
                o.acf_string(name);
                o.acf_string(desc);
                o.acf_string(extra);
            }
        } else {
            let (_, name, desc, extra) = &self.localized[0];
            o.acf_string(name);
            o.acf_string(desc);
            o.acf_string(extra);
        }
        o.u16(self.width);
        o.u16(self.height);
        o.u8(self.transparent);
        o.u32(self.flags_written());
        if v >= 0x2_0001 {
            o.u16(self.animation_set.0);
            o.u16(self.animation_set.1);
        }
        if let Some(vc) = &self.voice {
            o.bytes(&vc.engine);
            o.bytes(&vc.mode);
            o.u32(vc.speed);
            o.u16(vc.pitch);
            if v > 0x2_0000 {
                match &vc.extra {
                    None => o.u8(0),
                    Some((lang, dialect, gender, age, style)) => {
                        o.u8(1);
                        o.u16(*lang);
                        o.acf_string(dialect);
                        o.u16(*gender);
                        o.u16(*age);
                        o.acf_string(style);
                    }
                }
            }
        }
        if let Some(b) = &self.balloon {
            o.u8(b.lines);
            o.u8(b.chars_per_line);
            o.u32(b.foreground);
            o.u32(b.background);
            o.u32(b.border);
            o.acf_string(&b.font);
            o.i32(b.font_height);
            o.u32(b.font_weight);
            o.u8(b.italic);
            if v > 0x1_001E {
                o.u8(b.flag2);
            }
        }
        o.u32(self.palette.len() as u32);
        for q in &self.palette {
            o.bytes(q);
        }
        if v > 0x2_0000 {
            match &self.tray_icon {
                None => o.u8(0),
                Some((mask, color)) => {
                    o.u8(1);
                    o.u32(mask.len() as u32);
                    o.bytes(mask);
                    o.u32(color.len() as u32);
                    o.bytes(color);
                }
            }
        }
        o.u16(self.states.len() as u16);
        for (name, anims) in &self.states {
            o.acf_string(name);
            o.u16(anims.len() as u16);
            for a in anims {
                o.acf_string(a);
            }
        }
        o.0
    }

    /// The flat ACF file (magic, sizes, compressed body).
    pub fn write_acf(&self) -> Vec<u8> {
        let body = self.body();
        let mut o = Out::default();
        o.u32(self.magic);
        block(&mut o, &body, Compression::Lz);
        o.0
    }

    /// The decompressed body of animation `i`.
    pub fn aca_body(&self, i: usize) -> Vec<u8> {
        let v = self.version;
        let a = &self.animations[i];
        let t = self.transparent;
        let mut o = Out::default();
        o.u16(a.sounds.len() as u16);
        for s in &a.sounds {
            o.u32(s.len() as u32);
            o.bytes(s);
        }
        o.u16(a.images.len() as u16);
        for img in &a.images {
            match img {
                None => o.u32(0),
                Some(p) => {
                    let dib = p.dib();
                    o.u32(dib.len() as u32);
                    o.u8(a.image_unknown);
                    o.bytes(&dib);
                    let rgn = p.region(t);
                    o.u32(rgn.len() as u32);
                    o.bytes(&rgn);
                }
            }
        }
        if v >= 0x1_0020 {
            o.u8(a.transition);
        }
        o.u16(a.frames.len() as u16);
        for f in &a.frames {
            o.u16(f.image);
            o.u16(f.sound);
            o.u16(f.duration);
            o.i16(f.offset.0);
            o.i16(f.offset.1);
            if v >= 0x1_0020 {
                o.i16(f.exit);
            }
            o.u8(f.branches.len() as u8);
            for &(target, p) in &f.branches {
                o.u16(target);
                o.u16(p);
            }
            o.u8(f.overlays.len() as u8);
            if v > 0x1_001F && !f.overlays.is_empty() {
                match &f.base {
                    Base::Absent => o.u8(0),
                    Base::Empty => {
                        o.u8(1);
                        o.u32(0);
                    }
                    Base::Picture(p) => {
                        o.u8(1);
                        let dib = p.dib();
                        o.u32(dib.len() as u32);
                        o.bytes(&dib);
                        let rgn = p.region(t);
                        o.u32(rgn.len() as u32);
                        o.bytes(&rgn);
                    }
                }
            }
            for ov in &f.overlays {
                o.u8(ov.mouth);
                let Some((x, y, p)) = &ov.picture else {
                    o.u32(0);
                    continue;
                };
                let dib = p.dib();
                o.u32(dib.len() as u32);
                o.u8(ov.reserved);
                if v > 0x1_001F {
                    o.u8(ov.replace_base);
                }
                o.i16(*x);
                o.i16(*y);
                o.u16(p.width);
                o.u16(p.height);
                o.bytes(&dib);
                if v > 0x1_001F && ov.replace_base != 0 {
                    let rgn = p.region(t);
                    o.u32(rgn.len() as u32);
                    o.bytes(&rgn);
                }
            }
        }
        o.0
    }

    /// The ACA/AAF file (or OLE stream) of animation `i`.
    pub fn write_aca(&self, i: usize) -> Vec<u8> {
        let a = &self.animations[i];
        let mut o = Out::default();
        if self.version >= 0x1_001D && !a.headerless {
            o.u32(self.version);
            o.u32(a.checksum);
        }
        let body = self.aca_body(i);
        match a.compression {
            Compression::Raw => {
                o.u8(0);
                o.bytes(&body);
            }
            c => {
                o.u8(1);
                block(&mut o, &body, c);
            }
        }
        o.0
    }
}

fn block(o: &mut Out, body: &[u8], c: Compression) {
    let z = lz_compress(body, c != Compression::Literals);
    o.u32(body.len() as u32);
    o.u32(z.len() as u32);
    o.bytes(&z);
}

fn guid(seed: u8) -> [u8; 16] {
    std::array::from_fn(|i| seed.wrapping_mul(29).wrapping_add(i as u8 * 11))
}

/// A synthetic web character at `version` (0x20001, 0x1001F, 0x1001E or 0x1001C) with three
/// animations covering sounds, empty images, branches, exits, overlays (empty, plain and
/// with "replace base frame"), base image blocks, raw and compressed bodies.
pub fn sample(version: u32) -> Acf {
    let (w, h, t) = (36u16, 28u16, 10u8);
    let ext = if version >= 0x2_0001 { "aca" } else { "aaf" };
    let pic = |seed| Picture::pattern(w, h, seed, t);
    let mouth = |m: u8, seed, replace_base| Overlay {
        mouth: m,
        picture: Some((10 + m as i16, 15, Picture::pattern(7, 5, seed, t))),
        reserved: 0,
        replace_base,
    };
    let frame = |image, sound, duration, exit, branches: Vec<(u16, u16)>| Frame {
        image,
        sound,
        duration,
        offset: (0, 0),
        exit,
        branches,
        base: Base::Absent,
        overlays: vec![],
    };
    let mut talk = frame(0, 0xFFFF, 10, -2, vec![]);
    talk.overlays = (0..7).map(|m| mouth(m, 20 + m as u32, 0)).collect();
    talk.overlays[6].picture = None;
    let mut talk2 = frame(1, 0xFFFF, 10, -1, vec![]);
    talk2.base = Base::Picture(pic(9));
    talk2.overlays = vec![mouth(0, 30, 1), mouth(4, 31, 0)];
    let mut talk3 = frame(1, 0xFFFF, 10, -1, vec![]);
    talk3.base = Base::Empty;
    talk3.overlays = vec![mouth(2, 32, 1)];
    let anim = |name: &str, ret: &str, checksum, transition, frames, compression| Animation {
        name: name.into(),
        file_name: format!("{name}.{ext}"),
        return_animation: String::from(ret),
        checksum,
        transition,
        sounds: vec![wav_pcm(11025, 1, 8, 99, 3), wav_pcm(16000, 1, 16, 50, 4)],
        images: vec![Some(pic(1)), Some(pic(2)), None],
        image_unknown: 0,
        frames,
        compression,
        headerless: false,
    };
    let mut odd = frame(2, 1, 0, 2, vec![(0, 40), (1, 60)]);
    odd.offset = (3, -2);
    Acf {
        magic: if version >= 0x2_0001 {
            0xABCD_ABC4
        } else {
            0xABCD_ABC2
        },
        version,
        guid: guid(5),
        localized: vec![
            (
                9,
                "Webby".into(),
                "Synthetic web character".into(),
                "x~~y".into(),
            ),
            (0x0007, "Webbi".into(), String::new(), String::new()),
        ],
        width: w,
        height: h,
        transparent: t,
        flags: 0x0011_0200,
        animation_set: (2, 0),
        voice: Some(Voice {
            engine: guid(6),
            mode: guid(7),
            speed: 0xFFFF_FFFF,
            pitch: 0xFFFF,
            extra: Some((0x0407, "Hoch".into(), 1, 4, "neutral".into())),
        }),
        balloon: Some(Balloon {
            lines: 0,
            chars_per_line: 0,
            foreground: 0x0000_00FF,
            background: 0x00E1_FFFF,
            border: 0x0012_3456,
            font: "Arial".into(),
            font_height: -15,
            font_weight: 700,
            italic: 1,
            flag2: 1,
        }),
        palette: crate::acs::palette(128),
        tray_icon: Some((
            packed_dib(16, 16, 1, |x, _| (x % 2) as u8),
            packed_dib(16, 16, 4, |x, y| (x + y) as u8),
        )),
        states: vec![
            ("Showing".into(), vec!["Show".into()]),
            ("IdlingLevel1".into(), vec!["Blink".into()]),
        ],
        animations: vec![
            anim(
                "Show",
                "",
                0x1234_5678,
                1,
                vec![
                    frame(0xFFFF, 0xFFFF, 0, -2, vec![]),
                    frame(0, 0, 10, 3, vec![]),
                    talk,
                    odd,
                    talk2,
                ],
                Compression::Lz,
            ),
            anim(
                "Blink",
                "BlinkReturn",
                0xCAFE_F00D,
                0,
                vec![frame(1, 0xFFFF, 5, -1, vec![(0, 100)]), talk3],
                Compression::Literals,
            ),
            anim(
                "BlinkReturn",
                "",
                0,
                2,
                vec![frame(0, 7, 65535, -1, vec![])],
                Compression::Raw,
            ),
        ],
    }
}
