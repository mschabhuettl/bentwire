//! `cargo run -p bentwire-convert --bin gen-builtin [ROOT]`: regenerate `characters/builtin`
//! (pack.toml and the placeholder folder) under ROOT (default: the repository root).

use std::path::PathBuf;

fn main() {
    let root = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../.."));
    let pack = root.join("characters/builtin");
    let dir = pack.join("bentwire-placeholder");
    if dir.exists() {
        std::fs::remove_dir_all(&dir).expect("remove old placeholder");
    }
    std::fs::create_dir_all(&pack).expect("create pack folder");
    std::fs::write(
        pack.join("pack.toml"),
        bentwire_convert::placeholder::BUILTIN_PACK_TOML,
    )
    .expect("write pack.toml");
    let pkg = bentwire_convert::placeholder::placeholder().expect("build placeholder");
    pkg.write_folder(&dir).expect("write placeholder");
    println!("wrote {}", dir.display());
}
