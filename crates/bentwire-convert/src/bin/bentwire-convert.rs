//! Convert Microsoft Agent and Office actor files into bentwire characters.
//!
//! ```text
//! bentwire-convert [OPTIONS] SOURCE DEST
//! bentwire-convert [OPTIONS] --batch LIST OUTDIR [--per-id FILE]
//! bentwire-convert --theme BALLOON_DLL DIR [--generator TEXT]   (Office balloon theme, A-THEME-1)
//! ```
//! OPTIONS: `--office-palette FILE`, `--lang TAG`, `--codepage LABEL`. DEST ending in `.bwc`
//! writes a ZIP, otherwise a folder. `--batch` reads source paths (one per line) from LIST,
//! writes `OUTDIR/<id>.bwc` for every source that converts completely (A-PACK-2, A-CONV-5) and
//! prints one tab-separated report line per source: `included id path bytes`,
//! `excluded path reason` or `duplicate path first-path`. `--per-id` names a file of
//! `id lang codepage` lines.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use bentwire_convert::ConvertOptions;
use bentwire_convert::classics::{Outcome, generate, parse_overrides};

const USAGE: &str = "usage: bentwire-convert [--office-palette FILE] [--lang TAG] [--codepage LABEL] SOURCE DEST\n       bentwire-convert [--office-palette FILE] --batch LIST OUTDIR [--per-id FILE]\n       bentwire-convert --theme BALLOON_DLL DIR";

fn batch(
    list: &Path,
    out: &Path,
    opts: &ConvertOptions,
    per_id: Option<&Path>,
) -> Result<(), String> {
    let text = std::fs::read_to_string(list).map_err(|e| format!("{}: {e}", list.display()))?;
    let sources: Vec<PathBuf> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
        .map(PathBuf::from)
        .collect();
    let overrides = match per_id {
        Some(f) => parse_overrides(
            &std::fs::read_to_string(f).map_err(|e| format!("{}: {e}", f.display()))?,
        ),
        None => Vec::new(),
    };
    let res = generate(&sources, out, opts, &overrides).map_err(|e| e.to_string())?;
    for r in &res {
        match r {
            Outcome::Included { id, path, size } => {
                println!("included\t{id}\t{}\t{size}", path.display())
            }
            Outcome::Excluded { path, reason } => {
                println!("excluded\t{}\t{reason}", path.display())
            }
            Outcome::Duplicate { path, of } => {
                println!(
                    "duplicate\t{}\tbyte-identical to {}",
                    path.display(),
                    of.display()
                )
            }
        }
    }
    Ok(())
}

fn main() -> ExitCode {
    let mut args = std::env::args().skip(1);
    let mut opts = ConvertOptions::default();
    let mut pos = Vec::new();
    let (mut theme, mut is_batch) = (false, false);
    let mut per_id: Option<PathBuf> = None;
    let mut generator = String::from("bentwire-convert --theme");
    while let Some(a) = args.next() {
        let mut val = || args.next();
        match a.as_str() {
            "--office-palette" => opts.office_palette = val().map(PathBuf::from),
            "--lang" => opts.lang = val(),
            "--codepage" => opts.codepage = val(),
            "--per-id" => per_id = val().map(PathBuf::from),
            "--generator" => generator = val().unwrap_or_default(),
            "--theme" => theme = true,
            "--batch" => is_batch = true,
            _ => pos.push(a),
        }
    }
    if pos.len() != 2 {
        eprintln!("{USAGE}");
        return ExitCode::from(2);
    }
    let (src, dst) = (Path::new(&pos[0]), Path::new(&pos[1]));
    let r = if is_batch {
        batch(src, dst, &opts, per_id.as_deref())
    } else if theme {
        std::fs::read(src)
            .map_err(bentwire_convert::Error::from)
            .and_then(|d| {
                let source = src.file_name().unwrap_or_default().to_string_lossy();
                bentwire_convert::theme::extract_theme(&d, dst, "Office", &source, &generator)
            })
            .map_err(|e| e.to_string())
    } else {
        bentwire_convert::convert_with(src, dst, &opts).map_err(|e| e.to_string())
    };
    match r {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e}", src.display());
            ExitCode::FAILURE
        }
    }
}
