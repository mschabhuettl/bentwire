//! Installed characters: the core's loader (B-CHAR-2) plus the loaded packages the
//! daemon needs for pictures, sounds and the tray icon.

use crate::discovery::Entry;
use crate::sheets::SheetCache;
use bentwire_character::Character;
use bentwire_character::label::{PackLabels, label_info};
use bentwire_core::api::{CatalogEntry, CharacterLoader, Picture};
use bentwire_core::l10n::startup_candidates;
use bentwire_core::model::{CharacterModel, InfoEntry};
use bentwire_render::canvas::RgbaImage;
use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub loaded: HashMap<String, Arc<Character>>,
    /// The core's models of the loaded characters (Test1.Snapshot `rest_opaque`).
    pub models: HashMap<String, Arc<CharacterModel>>,
    /// Search roots (A-PACK-1), scanned again on use (B-CHAR-1).
    pub roots: Vec<PathBuf>,
    /// The A-THEME-1 balloon theme under the roots at the last scan.
    pub theme: Option<PathBuf>,
    pub cache: crate::discovery::Cache,
    /// Where `cache` is kept between daemon starts; `None` keeps it in memory only.
    cache_file: Option<PathBuf>,
    /// Decoded sheets on disk (B-CHAR-7); `None` decodes every load (tests, tools).
    pub sheets: Option<SheetCache>,
    /// The startup character loading in a thread (`preload`).
    preload: Option<(String, std::thread::JoinHandle<Result<Character, String>>)>,
}

pub type Shared = Rc<RefCell<Library>>;

impl Library {
    #[cfg(test)]
    pub fn new(roots: Vec<PathBuf>) -> Library {
        let mut lib = Library {
            roots,
            ..Default::default()
        };
        lib.scan();
        lib
    }

    /// `new` with the discovery cache kept in `file` between starts (B-CHAR-1).
    pub fn with_cache_file(roots: Vec<PathBuf>, file: PathBuf) -> Library {
        let mut lib = Library {
            roots,
            cache: crate::discovery::load_cache(&file),
            cache_file: Some(file),
            ..Default::default()
        };
        lib.scan();
        lib
    }

    /// Discover the characters and the theme under the roots again.
    pub fn scan(&mut self) {
        let t = std::time::Instant::now();
        let before = self.cache_file.is_some().then(|| self.cache.clone());
        let entries = crate::discovery::discover_cached(&self.roots, &mut self.cache);
        if let (Some(file), Some(before)) = (&self.cache_file, before)
            && before != self.cache
        {
            crate::discovery::save_cache(file, &self.cache);
        }
        if entries.len() != self.entries.len() {
            log::info!("{} characters found", entries.len());
        }
        log::debug!("scan: {} characters in {:?}", entries.len(), t.elapsed());
        self.entries = entries;
        self.theme = crate::discovery::find_theme(&self.roots);
    }

    /// Names are display labels (A-LABEL-1).
    pub fn catalog(&self) -> Vec<CatalogEntry> {
        self.entries
            .iter()
            .map(|e| {
                let pack = PackLabels::for_character(&e.path).unwrap_or_else(|err| {
                    log::warn!("{err}");
                    PackLabels::default()
                });
                let info: Vec<_> = e
                    .info
                    .iter()
                    .map(|i| (i.lang.clone(), i.name.clone(), i.description.clone()))
                    .collect();
                let agent_15 = e.format.as_deref().is_some_and(|f| f.starts_with("acs-1."));
                CatalogEntry {
                    id: e.id.clone(),
                    // label_info keeps the order of the entries; an added English entry
                    // has no ExtraData. Agent 1.5 ExtraData is a licence string, not
                    // intro phrases (B-GAL-1).
                    info: label_info(&e.id, &info, e.format.as_deref(), &pack)
                        .into_iter()
                        .enumerate()
                        .map(|(k, (lang, name, description))| InfoEntry {
                            lang,
                            name,
                            description,
                            extra: e
                                .info
                                .get(k)
                                .filter(|_| !agent_15)
                                .map(|i| i.extra.clone())
                                .unwrap_or_default(),
                        })
                        .collect(),
                    display: e.display,
                }
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Arc<Character>> {
        self.loaded.get(id)
    }

    /// Start loading the first B-CHAR-4 candidate in a thread, so that it overlaps the rest
    /// of the daemon start; the core's first load of that id takes the result.
    pub fn preload(&mut self, chosen: Option<&str>, config: Option<&str>) {
        let mut ids: Vec<String> = self.entries.iter().map(|e| e.id.clone()).collect();
        ids.sort();
        let Some(id) = startup_candidates(chosen, config, &ids).into_iter().next() else {
            return;
        };
        let Some(path) = self
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.path.clone())
        else {
            return;
        };
        let sheets = self.sheets.clone();
        let h = std::thread::spawn(move || load_character(&path, sheets.as_ref()));
        self.preload = Some((id, h));
    }
}

fn load_character(path: &Path, sheets: Option<&SheetCache>) -> Result<Character, String> {
    match sheets {
        Some(s) => Character::load_with(path, &mut |file, png| s.decode(path, file, png)),
        None => Character::load(path),
    }
    .map_err(|e| format!("{}: {e}", path.display()))
}

pub struct Loader(pub Shared);

impl CharacterLoader for Loader {
    fn load(&mut self, id: &str) -> Result<Arc<CharacterModel>, String> {
        let mut lib = self.0.borrow_mut();
        let path = lib
            .entries
            .iter()
            .find(|e| e.id == id)
            .map(|e| e.path.clone())
            .ok_or_else(|| format!("no character {id:?}"))?;
        let t = std::time::Instant::now();
        let pre = lib.preload.take().filter(|(p, _)| p == id);
        let c = match pre.and_then(|(_, h)| h.join().ok()) {
            Some(r) => r,
            None => load_character(&path, lib.sheets.as_ref()),
        }?;
        log::debug!("loaded {id} in {:?}", t.elapsed());
        for w in &c.warnings {
            log::warn!("{id}: {w}");
        }
        let model = Arc::new(crate::model::model_from(id, &c));
        lib.loaded.insert(id.to_string(), Arc::new(c));
        lib.models.insert(id.to_string(), model.clone());
        Ok(model)
    }

    fn rescan(&mut self) -> Option<Vec<CatalogEntry>> {
        let mut lib = self.0.borrow_mut();
        lib.scan();
        Some(lib.catalog())
    }
}

/// The rest picture (B-SHOW-3) of a loaded character, as the renderer draws it.
pub fn rest_picture(lib: &Library, id: &str) -> Option<RgbaImage> {
    use bentwire_core::player::{Pic, rest_picture};
    let m = lib.models.get(id)?;
    let p = match rest_picture(m) {
        Pic::Empty => Picture::Empty,
        Pic::Frame(a, f) => Picture::Frame {
            animation: m.animations[a].name.clone(),
            frame: f,
            mouth: None,
        },
        Pic::Pose(p) => Picture::Pose(p),
    };
    Some(picture(lib.get(id)?, &p, 1))
}

/// B-SCALE-1: the density to compose a picture at, for buffer scale `k` and size factor
/// `scale`. A character with `hidpi` sheets (vector actors, drawn pictures) is composed at
/// the target density, where A-CHAR-3 picks its 2x files; a bitmap character only at `k`,
/// so the buffer repeats each of its pixels `scale` times (nearest neighbour, exact).
pub fn density(c: &Character, k: u32, scale: u32) -> u32 {
    if c.manifest.sheets.iter().any(|s| !s.hidpi.is_empty()) {
        k * scale
    } else {
        k
    }
}

/// B-SMOOTH-1: the density a smoothed picture for `density` is composed at: the densest
/// `hidpi` key of the character's sheets when it is a multiple of `density` above it.
/// `None`: no denser pictures, the picture is drawn as without smoothing.
pub fn smooth_density(c: &Character, density: u32) -> Option<u32> {
    let k = c
        .manifest
        .sheets
        .iter()
        .flat_map(|s| s.hidpi.keys())
        .copied()
        .max()?;
    (density > 0 && k > density && k % density == 0).then_some(k)
}

/// The composed character picture at `density` times the display size, straight RGBA,
/// with its mouth (speech output only, R-LIP-1) clipped to the frame (B-PIX-2).
pub fn picture(c: &Character, p: &Picture, density: u32) -> RgbaImage {
    let rgba = |i: bentwire_character::Rgba| RgbaImage {
        width: i.width,
        height: i.height,
        data: i.data,
    };
    let img = match p {
        Picture::Empty => None,
        Picture::Frame {
            animation,
            frame,
            mouth,
        } => c
            .manifest
            .animation(animation)
            .and_then(|a| a.frames.get(*frame))
            .map(|f| {
                let i = mouth.and_then(|k| f.mouths.iter().position(|m| m.kind == k));
                match i.map(|i| &f.mouths[i]) {
                    // A mouth with `base` replaces the frame and is not clipped.
                    Some(m) if m.base.is_none() => bentwire_render::picture::clip_mouth(
                        &rgba(c.compose_frame(f, None, density)),
                        &rgba(c.compose(std::slice::from_ref(&m.layer), density)),
                    ),
                    _ => rgba(c.compose_frame(f, i, density)),
                }
            }),
        Picture::Pose(id) => c
            .manifest
            .poses
            .get(id)
            .map(|l| rgba(c.compose(l, density))),
    };
    // A-CHAR-4: compose returns 0x0 for oversized output; draw nothing then.
    match img.filter(|i| i.width > 0 && i.height > 0) {
        Some(i) => i,
        None => {
            let [w, h] = c.manifest.display();
            RgbaImage::new(w * density.max(1), h * density.max(1))
        }
    }
}

#[cfg(test)]
mod tests {
    //! Release check: the daemon's own discovery, loader and model adapter load
    //! every character of a pack, and the core plays it.
    use super::*;
    use bentwire_core::Core;
    use bentwire_core::api::{
        Call, Config, CoreInit, Event, LocaleEnv, Output, OutputInfo, Rect, ReplyValue, Signal,
    };
    use std::path::{Path, PathBuf};

    /// Loads every character under `root` the way bentwired does, selects each in a
    /// headless core, plays Showing (or the first animation) for a while and returns the
    /// number of characters and the failures.
    fn load_and_play_all(root: &Path) -> (usize, Vec<String>) {
        let lib: Shared = Rc::new(RefCell::new(Library::new(vec![root.to_path_buf()])));
        let n = lib.borrow().entries.len();
        let catalog = lib.borrow().catalog();
        let ids: Vec<String> = catalog.iter().map(|c| c.id.clone()).collect();
        let mut core = Core::new(CoreInit {
            config: Config::default(),
            locale: LocaleEnv::default(),
            state: None,
            catalog,
            loader: Box::new(Loader(lib.clone())),
            metrics: Box::new(bentwire_render::font::Fonts::bundled()),
            draws: Box::new(bentwire_core::draws::SplitMix64::new(1)),
            outputs: vec![OutputInfo {
                id: 0,
                rect: Rect::new(0, 0, 1920, 1080),
                scale: 1.0,
            }],
            local_hour: 12,
        });
        let mut now = 0;
        core.start(now);
        let mut failures = Vec::new();
        let mut token = 0;
        let mut call = |core: &mut Core, now: u64, c: Call| {
            token += 1;
            let out = core.handle(
                now,
                Event::Call {
                    token,
                    sender: ":test".into(),
                    call: c,
                },
            );
            let started: Vec<u32> = out
                .iter()
                .filter_map(|o| match o {
                    Output::Signal {
                        signal: Signal::RequestStart { id },
                        ..
                    } => Some(*id),
                    _ => None,
                })
                .collect();
            let reply = out.into_iter().find_map(|o| match o {
                Output::Reply { token: t, result } if t == token => Some(result),
                _ => None,
            });
            (reply, started)
        };
        for id in ids {
            now += 10_000;
            core.advance_to(now);
            match call(&mut core, now, Call::Select(id.clone())).0 {
                Some(Ok(_)) => {}
                other => {
                    failures.push(format!("{id}: Select gave {other:?}"));
                    continue;
                }
            }
            if lib.borrow().get(&id).is_none() {
                failures.push(format!("{id}: not loaded through the daemon's loader"));
                continue;
            }
            let props = core.properties();
            let name = ["Showing", "Show"]
                .into_iter()
                .find(|n| {
                    props
                        .animation_names
                        .iter()
                        .any(|a| a.eq_ignore_ascii_case(n))
                })
                .map(String::from)
                .or_else(|| props.animation_names.first().cloned());
            let Some(name) = name else {
                failures.push(format!("{id}: no animations"));
                continue;
            };
            let (reply, at_once) = call(&mut core, now, Call::Play(name.clone()));
            let rid = match reply {
                Some(Ok(ReplyValue::RequestId(r))) => r,
                other => {
                    failures.push(format!("{id}: Play {name} gave {other:?}"));
                    continue;
                }
            };
            // Up to 30 s for the request to start behind the switch's Show, then 3 s of frames; looping animations may not complete, so only a
            // started request and a drawable picture are required.
            let mut started = at_once.contains(&rid);
            for step in 0..300 {
                if started && step >= 30 {
                    break;
                }
                now += 100;
                for o in core.advance_to(now) {
                    if let Output::Signal {
                        signal: Signal::RequestStart { id: r },
                        ..
                    } = o
                        && r == rid
                    {
                        started = true;
                    }
                }
                let scene = core.scene();
                if let Some(ch) = &scene.character {
                    let c = lib.borrow().get(&id).cloned().unwrap();
                    let pic = picture(&c, &ch.picture, 1);
                    if pic.width != ch.width || pic.height != ch.height {
                        failures.push(format!(
                            "{id}: picture {}x{} for display {}x{}",
                            pic.width, pic.height, ch.width, ch.height
                        ));
                        break;
                    }
                }
            }
            if !started {
                failures.push(format!("{id}: Play {name} never started"));
            }
            call(&mut core, now, Call::Stop(0));
        }
        (n, failures)
    }

    #[test]
    fn a_char_1_done_when_4_every_builtin_character_loads_and_plays() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../characters/builtin");
        let (n, failures) = load_and_play_all(&root);
        assert!(n >= 1, "no characters in {}", root.display());
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// R-LIP-1, B-PIX-2: a mouth is drawn over the frame, only where the frame is opaque.
    #[test]
    fn r_lip_1_b_pix_2_mouth_drawn_clipped_to_the_frame() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../characters/builtin");
        let c = load_character(&root.join("wire"), None).unwrap();
        let pic = |mouth| {
            picture(
                &c,
                &Picture::Frame {
                    animation: "Speak".into(),
                    frame: 0,
                    mouth,
                },
                1,
            )
        };
        let base = pic(None);
        for k in 0..7 {
            let m = pic(Some(k));
            assert_eq!((m.width, m.height), (base.width, base.height));
            let changed: Vec<usize> = (0..base.data.len() / 4)
                .filter(|&i| base.data[i * 4..i * 4 + 4] != m.data[i * 4..i * 4 + 4])
                .collect();
            assert!(!changed.is_empty(), "mouth {k} drawn");
            assert!(
                changed.iter().all(|&i| base.data[i * 4 + 3] > 0),
                "mouth {k}"
            );
        }
        // A mouth type the frame does not have draws the plain frame.
        let last = picture(
            &c,
            &Picture::Frame {
                animation: "Wave".into(),
                frame: 0,
                mouth: Some(3),
            },
            1,
        );
        let plain = picture(
            &c,
            &Picture::Frame {
                animation: "Wave".into(),
                frame: 0,
                mouth: None,
            },
            1,
        );
        assert_eq!(last, plain);
    }

    /// B-CHAR-1: the daemon writes its discovery cache and the next start uses it.
    #[test]
    fn b_char_1_library_keeps_the_cache_file() {
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../characters/builtin");
        let file = crate::config::tests::tempdir().join("discovery.json");
        let a = Library::with_cache_file(vec![root.clone()], file.clone());
        assert!(!a.entries.is_empty());
        assert_eq!(crate::discovery::load_cache(&file), a.cache);
        let b = Library::with_cache_file(vec![root], file);
        assert_eq!(b.entries, a.entries);
    }

    /// Every character of every pack under $BENTWIRE_PACKS (a `characters/` directory,
    /// e.g. the repo's) through the daemon's loader and the core (A-TEST-1).
    #[test]
    #[ignore = "needs BENTWIRE_PACKS=<characters dir>"]
    fn done_when_4_every_pack_character_loads_and_plays() {
        let Some(dir) = std::env::var_os("BENTWIRE_PACKS") else {
            panic!("NOT RUN: set BENTWIRE_PACKS to a characters/ directory with packs");
        };
        let (n, failures) = load_and_play_all(&PathBuf::from(dir));
        eprintln!(
            "{n} characters loaded and played, {} failures",
            failures.len()
        );
        assert!(n > 0, "no characters found");
        assert!(failures.is_empty(), "{failures:#?}");
    }

    /// A-LABEL-1: every character under $BENTWIRE_PACKS has a non-empty display label,
    /// unique across all packs, in both UI languages (as listed, in the gallery and tray).
    #[test]
    #[ignore = "needs BENTWIRE_PACKS=<characters dir>"]
    fn a_label_1_every_pack_label_is_non_empty_and_unique() {
        use bentwire_core::api::Lang;
        let Some(dir) = std::env::var_os("BENTWIRE_PACKS") else {
            panic!("NOT RUN: set BENTWIRE_PACKS to a characters/ directory with packs");
        };
        let lib = Library::new(vec![PathBuf::from(dir)]);
        let catalog = lib.catalog();
        assert!(!catalog.is_empty(), "no characters found");
        for lang in [Lang::En, Lang::De] {
            let mut seen: HashMap<String, &str> = HashMap::new();
            for c in &catalog {
                let name = bentwire_core::l10n::display_names(&c.info, lang).0;
                assert!(!name.is_empty(), "{}: empty label ({lang:?})", c.id);
                if let Some(other) = seen.insert(name.clone(), &c.id) {
                    panic!("{other} and {} share the label {name:?} ({lang:?})", c.id);
                }
            }
        }
        eprintln!("{} characters, labels non-empty and unique", catalog.len());
    }

    /// B-GAL-1: the catalog carries the ExtraData of each entry to the core, also when
    /// label_info adds an English entry.
    #[test]
    fn b_gal_1_catalog_carries_extra_data() {
        use bentwire_core::api::Lang;
        let root = crate::config::tests::tempdir();
        for (id, source) in [("x", ""), ("y", "[source]\nformat = \"acs-1.5\"\n")] {
            let d = root.join("characters").join(id);
            std::fs::create_dir_all(&d).unwrap();
            std::fs::write(
                d.join("character.toml"),
                format!(
                    "format = 1\nfamily = \"agent\"\n{source}[[character.info]]\nlang = \"de\"\n\
                     name = \"X\"\nextra = \"Hallo.~~Servus.^^Merken.\"\n"
                ),
            )
            .unwrap();
        }
        let catalog = Library::new(vec![root]).catalog();
        let info = &catalog[0].info;
        assert_eq!(info.len(), 2, "an English entry is added");
        assert_eq!(
            bentwire_core::l10n::intro_phrases(info, Lang::De),
            ["Hallo.", "Servus."]
        );
        // Agent 1.5: the ExtraData is a licence string, not intro phrases.
        assert!(bentwire_core::l10n::intro_phrases(&catalog[1].info, Lang::De).is_empty());
    }

    fn copy_dir(from: &Path, to: &Path) {
        std::fs::create_dir_all(to).unwrap();
        for e in std::fs::read_dir(from).unwrap() {
            let p = e.unwrap().path();
            let dst = to.join(p.file_name().unwrap());
            if p.is_dir() {
                copy_dir(&p, &dst);
            } else {
                std::fs::copy(&p, &dst).unwrap();
            }
        }
    }

    /// A character whose `source.missing_animations` names animations (converter output for
    /// damaged or absent sources) behaves exactly like the same character that never had
    /// them: the loader reports them, B-FB-1 resolves every name to the same result, and the
    /// daemon loads and plays it without failing. Also for a manifest that still carries
    /// the listed animations.
    #[test]
    fn missing_animations_behave_like_animations_never_had() {
        use bentwire_core::fallback::resolve;
        const MISSING: [&str; 3] = ["Idle2_2", "Wave", "GestureLeft"];
        let src = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../characters/builtin/wire");
        let tmp = std::env::temp_dir().join(format!("bentwire-missing-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).unwrap();
        std::fs::write(tmp.join("pack.toml"), "name = \"missing-test\"\n").unwrap();
        // listed: name them in source.missing_animations; keep: leave them in animations.
        let make = |id: &str, listed: bool, keep: bool| -> PathBuf {
            let dir = tmp.join(id);
            copy_dir(&src, &dir);
            let path = dir.join("character.toml");
            let mut doc: toml::Table = std::fs::read_to_string(&path).unwrap().parse().unwrap();
            if !keep {
                let anims = doc["animations"].as_array_mut().unwrap();
                anims.retain(|a| !MISSING.contains(&a["name"].as_str().unwrap()));
            }
            if listed {
                let list: Vec<toml::Value> = MISSING.iter().map(|m| (*m).into()).collect();
                doc["source"]
                    .as_table_mut()
                    .unwrap()
                    .insert("missing_animations".into(), list.into());
            } else {
                let states = doc["states"].as_table_mut().unwrap();
                for (_, v) in states.iter_mut() {
                    let v = v.as_array_mut().unwrap();
                    v.retain(|m| !MISSING.contains(&m.as_str().unwrap()));
                }
                states.retain(|_, v| !v.as_array().unwrap().is_empty());
            }
            std::fs::write(&path, toml::to_string(&doc).unwrap()).unwrap();
            dir
        };
        let never = Character::load(make("never-had", false, false)).unwrap();
        let b = crate::model::model_from("x", &never);
        for (id, keep) in [("missing", false), ("missing-kept", true)] {
            let with = Character::load(make(id, true, keep)).unwrap();
            assert!(with.warnings.is_empty(), "{id}: {:?}", with.warnings);
            assert_eq!(with.manifest.missing_animations(), MISSING, "{id}");
            for m in MISSING {
                assert!(!with.is_available(m), "{id}: {m} available");
            }
            // The daemon's model is what the core sees: identical means every B-FB-1
            // resolution, state draw and idle choice is the same.
            let a = crate::model::model_from("x", &with);
            assert_eq!(a, b, "{id}");
            for m in MISSING {
                assert_eq!(resolve(&a, m), resolve(&b, m), "{id}: Play {m}");
            }
        }
        let (n, failures) = load_and_play_all(&tmp);
        assert_eq!(n, 3);
        assert!(failures.is_empty(), "{failures:#?}");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
