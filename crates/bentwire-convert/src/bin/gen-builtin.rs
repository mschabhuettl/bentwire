//! `cargo run -p bentwire-convert --bin gen-builtin [ROOT]`: regenerate `characters/builtin`
//! (pack.toml, LICENSE and the folders of `wire`, `wire-retro`, `quack` and `quack-retro`)
//! under ROOT (default: the repository root).

use bentwire_convert::own::{BUILTIN_PACK_TOML, LICENCE, Style};
use bentwire_convert::quack::QUACK;
use bentwire_convert::wire::WIRE;
use std::path::PathBuf;

fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let pack = root.join("characters/builtin");
    std::fs::create_dir_all(&pack).expect("create pack folder");
    std::fs::write(pack.join("pack.toml"), BUILTIN_PACK_TOML).expect("write pack.toml");
    std::fs::write(pack.join("LICENSE"), LICENCE).expect("write LICENSE");
    for (design, style) in [WIRE, QUACK]
        .iter()
        .flat_map(|d| [(d, Style::Modern), (d, Style::Retro)])
    {
        let dir = pack.join(design.id(style));
        if dir.exists() {
            std::fs::remove_dir_all(&dir).expect("remove old character");
        }
        let pkg = design.build(style).expect("build character");
        pkg.write_folder(&dir).expect("write character");
        println!("wrote {}", dir.display());
    }
}
