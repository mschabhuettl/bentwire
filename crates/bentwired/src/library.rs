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
