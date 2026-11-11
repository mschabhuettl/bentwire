//! Installed characters: the core's loader (B-CHAR-2) plus the loaded packages the
//! daemon needs for pictures, sounds and the tray icon.

use crate::discovery::Entry;
use bentwire_character::Character;
use bentwire_core::api::{CatalogEntry, CharacterLoader, Picture};
use bentwire_core::model::{CharacterModel, InfoEntry};
use bentwire_render::canvas::RgbaImage;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use std::sync::Arc;

#[derive(Default)]
pub struct Library {
    pub entries: Vec<Entry>,
    pub loaded: HashMap<String, Arc<Character>>,
}

pub type Shared = Rc<RefCell<Library>>;

impl Library {
    pub fn catalog(&self) -> Vec<CatalogEntry> {
        self.entries
            .iter()
            .map(|e| CatalogEntry {
                id: e.id.clone(),
                info: e
                    .info
                    .iter()
                    .map(|i| InfoEntry {
                        lang: i.lang.clone(),
                        name: i.name.clone(),
                        description: i.description.clone(),
                    })
                    .collect(),
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<&Arc<Character>> {
        self.loaded.get(id)
    }
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
        let c = Character::load(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        for w in &c.warnings {
            log::warn!("{id}: {w}");
        }
        let model = Arc::new(crate::model::model_from(id, &c));
        lib.loaded.insert(id.to_string(), Arc::new(c));
        Ok(model)
    }
}

/// The composed character picture at `density` times the display size, straight RGBA.
/// Mouth overlays are never requested in v1 (B-LIP-2), so they are not drawn.
pub fn picture(c: &Character, p: &Picture, density: u32) -> RgbaImage {
    let img = match p {
        Picture::Empty => None,
        Picture::Frame {
            animation, frame, ..
        } => c
            .manifest
            .animation(animation)
            .and_then(|a| a.frames.get(*frame))
            .map(|f| c.compose_frame(f, None, density)),
        Picture::Pose(id) => c.manifest.poses.get(id).map(|l| c.compose(l, density)),
    };
    // A-CHAR-4: compose returns 0x0 for oversized output; draw nothing then.
    match img.filter(|i| i.width > 0 && i.height > 0) {
        Some(i) => RgbaImage {
            width: i.width,
            height: i.height,
            data: i.data,
        },
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
        let entries = crate::discovery::discover(&[root.to_path_buf()]);
        let n = entries.len();
        let lib: Shared = Rc::new(RefCell::new(Library {
            entries,
            loaded: HashMap::new(),
        }));
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
        const MISSING: [&str; 3] = ["IdleSpin", "Wave", "GestureLeft"];
        let src = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../characters/builtin/bentwire-placeholder");
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
