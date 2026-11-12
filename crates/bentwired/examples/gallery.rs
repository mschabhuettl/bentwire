//! Gallery images: every character of a pack in its rest pose, 1x, transparent background.
//! The picture is the one `Show(fast)` displays (B-SHOW-3): frame 0 of `RestPose` (office97:
//! the pose `office97.rest_pose`), else the last frame of the first Showing member, else an
//! empty picture of the display size. Writes `OUTDIR/<id>.png` per character file (`<id>.bwc`
//! or a `<id>/` folder with `character.toml`) in PACKDIR. Output is deterministic.
//! Prints one line per character: id, English name, product and display label (A-LABEL-1),
//! tab-separated (product empty when there is none), for scripts/gallery-table.py.
//! usage: gallery PACKDIR OUTDIR

#[allow(dead_code)]
#[path = "../src/model.rs"]
mod model;

use std::path::{Path, PathBuf};

use bentwire_character::Character;
use bentwire_character::label::{PackLabels, label, name_and_product};
use bentwire_core::fallback::state_members;
use bentwire_core::model::Family;
use bentwire_render::canvas::RgbaImage;

fn rest_picture(id: &str, c: &Character) -> RgbaImage {
    let m = model::model_from(id, c);
    let manifest = &c.manifest;
    let img = if m.family == Family::Office97 {
        m.office97
            .as_ref()
            .and_then(|o| o.rest_pose)
            .and_then(|p| manifest.poses.get(&p))
            .map(|l| c.compose(l, 1))
    } else {
        m.animation_index("RestPose")
            .filter(|&i| !m.animations[i].frames.is_empty())
            .and_then(|i| manifest.animation(&m.animations[i].name))
            .map(|a| c.compose_frame(&a.frames[0], None, 1))
    };
    let img = img.or_else(|| {
        let members = state_members(&m, "Showing")?;
        let i = m.animation_index(members.first()?)?;
        let a = manifest.animation(&m.animations[i].name)?;
        a.frames.last().map(|f| c.compose_frame(f, None, 1))
    });
    match img.filter(|i| i.width > 0 && i.height > 0) {
        Some(i) => RgbaImage {
            width: i.width,
            height: i.height,
            data: i.data,
        },
        None => {
            let [w, h] = manifest.display();
            RgbaImage::new(w, h)
        }
    }
}

fn main() {
    let args: Vec<PathBuf> = std::env::args_os().skip(1).map(PathBuf::from).collect();
    let [pack, out] = args.as_slice() else {
        eprintln!("usage: gallery PACKDIR OUTDIR");
        std::process::exit(2);
    };
    let mut chars: Vec<(String, PathBuf)> = std::fs::read_dir(pack)
        .unwrap_or_else(|e| panic!("{}: {e}", pack.display()))
        .map(|e| e.unwrap().path())
        .filter_map(|p| {
            let name = p.file_name()?.to_str()?.to_owned();
            if let Some(id) = name.strip_suffix(".bwc") {
                Some((id.to_owned(), p))
            } else if p.join("character.toml").is_file() {
                Some((name, p))
            } else {
                None
            }
        })
        .collect();
    chars.sort();
    std::fs::create_dir_all(out).unwrap();
    let pack = PackLabels::of_pack(pack).unwrap_or_else(|e| panic!("{e}"));
    for (id, path) in &chars {
        let c = Character::load(Path::new(path)).unwrap_or_else(|e| panic!("{id}: {e}"));
        let img = rest_picture(id, &c);
        bentwire_render::scene::write_png(&out.join(format!("{id}.png")), &img).unwrap();
        let info = &c.manifest.character.info;
        let names: Vec<(&str, &str)> = info.iter().map(|i| (&*i.lang, &*i.name)).collect();
        let format = (c.manifest.source.as_ref())
            .and_then(|s| s.get("format"))
            .and_then(|f| f.as_str());
        let (name, product) = name_and_product(id, &names, format, &pack);
        let l = label(&name, product.as_deref());
        println!("{id}\t{name}\t{}\t{l}", product.unwrap_or_default());
    }
}
