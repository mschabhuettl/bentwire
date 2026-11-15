//! Load time per character (issue #15): median of RUNS runs in ms, split into zip read,
//! manifest, PNG decode of the sheets, sounds and icon, the whole `Character::load`, the
//! core model and the first frame (frame 0 of the first Showing member, composed at 1x);
//! then `Character::load` through the sheet cache (B-CHAR-7), cold (empty cache dir) and warm.
//! usage: loadtime RUNS FILE...

#[allow(dead_code)]
#[path = "../src/model.rs"]
mod model;
#[path = "../src/sheets.rs"]
mod sheets;

use std::path::PathBuf;
use std::time::Instant;

use bentwire_character::{Character, Package, audio, image, parse, validate};
use bentwire_core::fallback::state_members;

fn ms(t: Instant) -> f64 {
    t.elapsed().as_secs_f64() * 1000.0
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(|a, b| a.total_cmp(b));
    v[v.len() / 2]
}

fn main() {
    let mut args = std::env::args().skip(1);
    let runs: usize = args.next().and_then(|r| r.parse().ok()).unwrap_or(5);
    println!("character zip manifest png sounds+icon load model first_frame cold warm");
    for path in args.map(PathBuf::from) {
        let mut t = vec![Vec::new(); 9];
        for _ in 0..runs {
            let s = Instant::now();
            let pkg = Package::open(&path).unwrap();
            t[0].push(ms(s));
            let s = Instant::now();
            let text = String::from_utf8(pkg.read("character.toml").unwrap()).unwrap();
            let m = parse::parse_manifest(&text).unwrap();
            let _ = validate(&m);
            t[1].push(ms(s));
            let s = Instant::now();
            for sh in &m.sheets {
                for f in std::iter::once(&sh.file).chain(sh.hidpi.values()) {
                    let _ = image::decode_png(&pkg.read(f).unwrap());
                }
            }
            t[2].push(ms(s));
            let s = Instant::now();
            for (_, f) in &m.sounds {
                let _ = audio::decode_sound(&pkg.read(f).unwrap());
            }
            if let Some(i) = &m.icon {
                let _ = image::decode_png(&pkg.read(&i.file).unwrap());
            }
            t[3].push(ms(s));
            let s = Instant::now();
            let c = Character::load(&path).unwrap();
            t[4].push(ms(s));
            let id = path.file_stem().unwrap().to_string_lossy();
            let s = Instant::now();
            let cm = model::model_from(&id, &c);
            t[5].push(ms(s));
            let s = Instant::now();
            let first = state_members(&cm, "Showing")
                .and_then(|v| v.first().cloned())
                .and_then(|n| c.manifest.animation(&n))
                .or_else(|| c.manifest.animations.first())
                .and_then(|a| a.frames.first());
            if let Some(f) = first {
                let _ = c.compose_frame(f, None, 1);
            }
            t[6].push(ms(s));
            let dir = std::env::temp_dir().join(format!("loadtime-{}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            let cache = sheets::SheetCache { dir: dir.clone() };
            let cached = || {
                let s = Instant::now();
                Character::load_with(&path, &mut |f, png| cache.decode(&path, f, png)).unwrap();
                ms(s)
            };
            t[7].push(cached());
            // The entries are written in the background: wait for all of them.
            let files: usize = c.manifest.sheets.iter().map(|s| 1 + s.hidpi.len()).sum();
            let count = || {
                let d = std::fs::read_dir(&dir).unwrap();
                d.filter(|e| e.as_ref().unwrap().path().extension() == Some("sheet".as_ref()))
                    .count()
            };
            while count() < files {
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
            t[8].push(cached());
            std::fs::remove_dir_all(&dir).unwrap();
        }
        let cols: Vec<String> = t.into_iter().map(|v| format!("{:.1}", median(v))).collect();
        println!(
            "{} {}",
            path.file_stem().unwrap().to_string_lossy(),
            cols.join(" ")
        );
    }
}
