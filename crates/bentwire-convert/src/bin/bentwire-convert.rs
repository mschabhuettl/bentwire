//! Convert Microsoft Agent and Office actor files into bentwire characters.
//!
//! ```text
//! bentwire-convert [--office-palette FILE] [--lang TAG] [--codepage LABEL] SOURCE DEST
//! bentwire-convert --theme OBALLOON.DLL DIR      (Office balloon theme, A-THEME-1)
//! ```
//! DEST ending in `.bwc` writes a ZIP, otherwise a folder.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bentwire_convert::ConvertOptions;

const USAGE: &str = "usage: bentwire-convert [--office-palette FILE] [--lang TAG] [--codepage LABEL] SOURCE DEST\n       bentwire-convert --theme OBALLOON.DLL DIR";

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut opts = ConvertOptions::default();
    let mut pos = Vec::new();
    let mut theme = false;
    while let Some(a) = args.next() {
        let mut val = || args.next();
        match a.as_str() {
            "--office-palette" => opts.office_palette = val().map(PathBuf::from),
            "--lang" => opts.lang = val(),
            "--codepage" => opts.codepage = val(),
            "--theme" => theme = true,
            _ => pos.push(a),
        }
    }
    if pos.len() != 2 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let (src, dst) = (Path::new(&pos[0]), Path::new(&pos[1]));
    let r = if theme {
        std::fs::read(src)
            .map_err(bentwire_convert::Error::from)
            .and_then(|d| bentwire_convert::theme::extract_theme(&d, dst, "Office"))
    } else {
        bentwire_convert::convert_with(src, dst, &opts)
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e}", src.display());
            ExitCode::FAILURE
        }
    }
}
