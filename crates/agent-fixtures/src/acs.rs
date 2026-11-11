//! Writer for synthetic ACS 2.0 files (acs-format 3). Owner: formats teammate.

use crate::common::{Compression, Out, Picture, lz_compress, packed_dib, wav_pcm};

#[derive(Debug, Clone)]
pub struct Voice {
    pub engine: [u8; 16],
    pub mode: [u8; 16],
    pub speed: u32,
    pub pitch: u16,
    /// (language, dialect, gender, age, style)
    pub extra: Option<(u16, String, u16, u16, String)>,
}

#[derive(Debug, Clone)]
pub struct Balloon {
    pub lines: u8,
    pub chars_per_line: u8,
    pub foreground: u32,
    pub background: u32,
    pub border: u32,
    pub font: String,
    pub font_height: i32,
    pub font_weight: u32,
    pub italic: u8,
    pub flag2: u8,
}

#[derive(Debug, Clone)]
pub struct Overlay {
    pub mouth: u8,
    pub replace_top: u8,
    pub image: u32,
    pub x: i16,
    pub y: i16,
    pub point_x: u16,
    pub point_y: u16,
}

#[derive(Debug, Clone, Default)]
pub struct Frame {
    /// (image index, x, y), top-most first.
    pub images: Vec<(u32, i16, i16)>,
    pub sound: u16,
    pub duration: u16,
    pub exit: i16,
    /// (target, probability)
    pub branches: Vec<(u16, u16)>,
    pub overlays: Vec<Overlay>,
}

#[derive(Debug, Clone)]
pub struct Animation {
    pub name: String,
    pub transition: u8,
    pub return_animation: String,
    pub frames: Vec<Frame>,
}

#[derive(Debug, Clone)]
pub struct Image {
    /// `None` writes a kind-0 ("no image") record.
    pub picture: Option<Picture>,
    pub pixels: Compression,
    /// `Raw` stores the region uncompressed.
    pub region: Compression,
    /// Write an empty region (size 0) instead of the picture's mask.
    pub empty_region: bool,
}

#[derive(Debug, Clone)]
pub struct Acs {
    pub version: u32,
    pub guid: [u8; 16],
    pub width: u16,
    pub height: u16,
    pub transparent: u8,
    /// Written as given, except that bit 0x20 is set iff `voice` is present and bit 0x100
    /// is set iff `balloon` is absent.
    pub flags: u32,
    pub animation_set: (u16, u16),
    pub voice: Option<Voice>,
    pub balloon: Option<Balloon>,
    /// RGBQUADs (B, G, R, reserved).
    pub palette: Vec<[u8; 4]>,
    /// (mask DIB, colour DIB)
    pub tray_icon: Option<(Vec<u8>, Vec<u8>)>,
    pub states: Vec<(String, Vec<String>)>,
    /// (LANGID, name, description, extra)
    pub localized: Vec<(u16, String, String, String)>,
    pub animations: Vec<Animation>,
    pub images: Vec<Image>,
    /// Complete RIFF files.
    pub sounds: Vec<Vec<u8>>,
}

/// Value written into the image and audio list "checksum" dword of entry `i`.
pub fn checksum(i: usize) -> u32 {
    0x5EED_0000 ^ (i as u32).wrapping_mul(0x9E37_79B9)
}

impl Acs {
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

    /// Serializes the character as an ACS file. Physical order as in Microsoft's files:
    /// header, animations, images, sounds, the three lists, character info.
    pub fn write(&self) -> Vec<u8> {
        let mut o = Out(vec![0; 36]);
        let mut anim_locs = Vec::new();
        for a in &self.animations {
            let start = o.len();
            o.acs_string(&a.name.to_uppercase());
            o.u8(a.transition);
            o.acs_string(&a.return_animation);
            o.u16(a.frames.len() as u16);
            for f in &a.frames {
                o.u16(f.images.len() as u16);
                for &(i, x, y) in &f.images {
                    o.u32(i);
                    o.i16(x);
                    o.i16(y);
                }
                o.u16(f.sound);
                o.u16(f.duration);
                o.i16(f.exit);
                o.u8(f.branches.len() as u8);
                for &(t, p) in &f.branches {
                    o.u16(t);
                    o.u16(p);
                }
                o.u8(f.overlays.len() as u8);
                for v in &f.overlays {
                    o.u8(v.mouth);
                    o.u8(v.replace_top);
                    o.u32(v.image);
                    o.i16(v.x);
                    o.i16(v.y);
                    o.u16(v.point_x);
                    o.u16(v.point_y);
                }
            }
            anim_locs.push((start, o.len() - start));
        }
        let mut image_locs = Vec::new();
        for img in &self.images {
            let start = o.len();
            write_image(&mut o, img, self.transparent);
            image_locs.push((start, o.len() - start));
        }
        let mut sound_locs = Vec::new();
        for s in &self.sounds {
            sound_locs.push((o.len(), s.len()));
            o.bytes(s);
        }
        let anim_list = o.len();
        o.u32(self.animations.len() as u32);
        for (a, &(off, size)) in self.animations.iter().zip(&anim_locs) {
            o.acs_string(&a.name);
            o.u32(off as u32);
            o.u32(size as u32);
        }
        let image_list = o.len();
        write_list(&mut o, &image_locs);
        let sound_list = o.len();
        write_list(&mut o, &sound_locs);
        let char_info = o.len();
        let loc_slot = self.write_character(&mut o);
        let end = o.len();
        let mut h = Out::default();
        h.u32(0xABCD_ABC3);
        for (a, b) in [
            (char_info, end),
            (anim_list, image_list),
            (image_list, sound_list),
            (sound_list, char_info),
        ] {
            h.u32(a as u32);
            h.u32((b - a) as u32);
        }
        o.0[..36].copy_from_slice(&h.0);
        let (loc_at, loc_start) = loc_slot;
        o.0[loc_at..loc_at + 4].copy_from_slice(&(loc_start as u32).to_le_bytes());
        o.0[loc_at + 4..loc_at + 8].copy_from_slice(&((end - loc_start) as u32).to_le_bytes());
        o.0
    }

    /// Returns (offset of the localized-info locator, offset of the localized info).
    fn write_character(&self, o: &mut Out) -> (usize, usize) {
        o.u32(self.version);
        let loc_at = o.len();
        o.u32(0);
        o.u32(0);
        o.bytes(&self.guid);
        o.u16(self.width);
        o.u16(self.height);
        o.u8(self.transparent);
        o.u32(self.flags_written());
        o.u16(self.animation_set.0);
        o.u16(self.animation_set.1);
        if let Some(v) = &self.voice {
            o.bytes(&v.engine);
            o.bytes(&v.mode);
            o.u32(v.speed);
            o.u16(v.pitch);
            match &v.extra {
                None => o.u8(0),
                Some((lang, dialect, gender, age, style)) => {
                    o.u8(1);
                    o.u16(*lang);
                    o.acs_string(dialect);
                    o.u16(*gender);
                    o.u16(*age);
                    o.acs_string(style);
                }
            }
        }
        if let Some(b) = &self.balloon {
            o.u8(b.lines);
            o.u8(b.chars_per_line);
            o.u32(b.foreground);
            o.u32(b.background);
            o.u32(b.border);
            o.acs_string(&b.font);
            o.i32(b.font_height);
            o.u32(b.font_weight);
            o.u8(b.italic);
            o.u8(b.flag2);
        }
        o.u32(self.palette.len() as u32);
        for q in &self.palette {
            o.bytes(q);
        }
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
        o.u16(self.states.len() as u16);
        for (name, anims) in &self.states {
            o.acs_string(name);
            o.u16(anims.len() as u16);
            for a in anims {
                o.acs_string(a);
            }
        }
        let loc_start = o.len();
        o.u16(self.localized.len() as u16);
        for (lang, name, desc, extra) in &self.localized {
            o.u16(*lang);
            o.acs_string(name);
            o.acs_string(desc);
            o.acs_string(extra);
        }
        (loc_at, loc_start)
    }
}

fn write_list(o: &mut Out, locs: &[(usize, usize)]) {
    o.u32(locs.len() as u32);
    for (i, &(off, size)) in locs.iter().enumerate() {
        o.u32(off as u32);
        o.u32(size as u32);
        o.u32(checksum(i));
    }
}

fn write_image(o: &mut Out, img: &Image, transparent: u8) {
    let Some(p) = &img.picture else {
        o.u8(0);
        return;
    };
    o.u8(1);
    o.u16(p.width);
    o.u16(p.height);
    let dib = p.dib();
    match img.pixels {
        Compression::Raw => {
            o.u8(0);
            o.u32(dib.len() as u32);
            o.bytes(&dib);
        }
        c => {
            let z = lz_compress(&dib, c == Compression::Lz);
            o.u8(1);
            o.u32(z.len() as u32);
            o.bytes(&z);
        }
    }
    let rgn = if img.empty_region {
        Vec::new()
    } else {
        p.region(transparent)
    };
    match img.region {
        Compression::Raw => {
            o.u32(0);
            o.u32(rgn.len() as u32);
            o.bytes(&rgn);
        }
        c => {
            let z = lz_compress(&rgn, c == Compression::Lz);
            o.u32(z.len() as u32);
            o.u32(rgn.len() as u32);
            o.bytes(&z);
        }
    }
}

/// A palette with distinct colours, as RGBQUADs.
pub fn palette(n: usize) -> Vec<[u8; 4]> {
    (0..n)
        .map(|i| [(i * 7) as u8, (i * 13 + 5) as u8, (255 - i) as u8, 0])
        .collect()
}

fn guid(seed: u8) -> [u8; 16] {
    std::array::from_fn(|i| seed.wrapping_mul(31).wrapping_add(i as u8 * 17))
}

/// A small synthetic character that exercises every ACS feature the reader handles: voice
/// with extra block, balloon, tray icon, several languages and states, layered frames with
/// negative offsets, branches, every kind of exit, mouth overlays, compressed and raw images
/// and regions, an empty region, out-of-range indices, PCM sounds (8 and 16 bit, odd length).
/// (No kind-0 image: the Python research parser cannot read one; set `picture: None` on an
/// entry to get it.)
pub fn sample() -> Acs {
    let (w, h, t) = (40u16, 30u16, 10u8);
    let pic = |pw, ph, seed| Some(Picture::pattern(pw, ph, seed, t));
    let img = |picture, pixels, region| Image {
        picture,
        pixels,
        region,
        empty_region: false,
    };
    let images = vec![
        img(pic(w, h, 1), Compression::Lz, Compression::Lz),
        img(pic(w, h, 2), Compression::Literals, Compression::Literals),
        img(pic(13, 9, 3), Compression::Raw, Compression::Raw),
        img(pic(7, 21, 4), Compression::Lz, Compression::Raw),
        img(pic(w, h, 8), Compression::Lz, Compression::Literals),
        Image {
            picture: pic(10, 6, 5),
            pixels: Compression::Lz,
            region: Compression::Lz,
            empty_region: true,
        },
        img(pic(9, 5, 6), Compression::Lz, Compression::Lz), // mouth
        img(pic(9, 5, 7), Compression::Lz, Compression::Lz), // mouth
    ];
    let sounds = vec![wav_pcm(11025, 1, 8, 301, 1), wav_pcm(22050, 2, 16, 200, 2)];
    let f = |images: Vec<(u32, i16, i16)>, sound, duration, exit, branches| Frame {
        images,
        sound,
        duration,
        exit,
        branches,
        overlays: vec![],
    };
    let mouths = |x, y| {
        (0..7u8)
            .map(|m| Overlay {
                mouth: m,
                replace_top: (m == 3) as u8,
                image: 6 + (m % 2) as u32,
                x,
                y,
                point_x: 4,
                point_y: 2,
            })
            .collect::<Vec<_>>()
    };
    let mut speak = f(vec![(0, 0, 0)], 0xFFFF, 10, -2, vec![]);
    speak.overlays = mouths(15, 20);
    let mut dup = speak.clone();
    dup.overlays.push(Overlay {
        mouth: 2,
        replace_top: 0,
        image: 7,
        x: -1,
        y: 3,
        point_x: 0,
        point_y: 0,
    });
    let animations = vec![
        Animation {
            name: "RestPose".into(),
            transition: 2,
            return_animation: String::new(),
            frames: vec![f(vec![(0, 0, 0)], 0xFFFF, 10, -1, vec![])],
        },
        Animation {
            name: "Greet".into(),
            transition: 1,
            return_animation: String::new(),
            frames: vec![
                f(vec![], 0xFFFF, 0, -2, vec![]),
                f(
                    vec![(2, -3, 4), (3, 30, -5), (1, 0, 0)],
                    0,
                    10,
                    3,
                    vec![(0, 30), (2, 70)],
                ),
                f(
                    vec![(4, 0, 0), (5, 1, 1), (99, 0, 0)],
                    1,
                    5,
                    -1,
                    vec![(1, 100)],
                ),
                speak,
                dup,
                f(vec![(1, 0, 0)], 0xFFFF, 65535, -2, vec![(9, 20), (0, 0)]),
            ],
        },
        Animation {
            name: "GreetReturn".into(),
            transition: 0,
            return_animation: "RestPose".into(),
            frames: vec![f(vec![(0, 0, 0)], 7, 12, -1, vec![])],
        },
    ];
    Acs {
        version: 0x2_0001,
        guid: guid(1),
        width: w,
        height: h,
        transparent: t,
        flags: 0x0011_0210,
        animation_set: (2, 0),
        voice: Some(Voice {
            engine: guid(2),
            mode: guid(3),
            speed: 150,
            pitch: 100,
            extra: Some((0x0409, "Standard".into(), 2, 3, "Ünïcødé".into())),
        }),
        balloon: Some(Balloon {
            lines: 2,
            chars_per_line: 28,
            foreground: 0,
            background: 0x00E1_FFFF,
            border: 0,
            font: "Tahoma".into(),
            font_height: -13,
            font_weight: 400,
            italic: 0,
            flag2: 0,
        }),
        palette: palette(256),
        tray_icon: Some((
            packed_dib(16, 16, 1, |x, y| ((x + y) % 2) as u8),
            packed_dib(16, 16, 4, |x, y| (x ^ y) as u8),
        )),
        states: vec![
            ("SHOWING".into(), vec!["Greet".into()]),
            (
                "IDLINGLEVEL1".into(),
                vec!["RestPose".into(), "greet".into()],
            ),
            ("EMPTY".into(), vec![]),
        ],
        localized: vec![
            (
                0x0009,
                "Testy".into(),
                "A synthetic character".into(),
                "Hello~~Hi".into(),
            ),
            (0x0407, "Testi".into(), String::new(), String::new()),
        ],
        animations,
        images,
        sounds,
    }
}
