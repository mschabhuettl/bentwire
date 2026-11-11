//! `gen-classics RESEARCH SOURCES OUT OFFICE_PALETTE [OPTIONS_FILE]`: convert every complete source into
//! `OUT/<id>.bwc` (A-PACK-2, A-ID-1) and print one line per source. Used by
//! `characters/classics/regenerate.sh`, which also extracts the balloon theme and writes the
//! README from this report.

use std::path::PathBuf;
use std::process::ExitCode;

use bentwire_convert::classics::{Outcome, enumerate, generate, parse_overrides};

fn main() -> ExitCode {
    let a: Vec<PathBuf> = std::env::args().skip(1).map(PathBuf::from).collect();
    if a.len() != 4 && a.len() != 5 {
        eprintln!("usage: gen-classics RESEARCH SOURCES OUT OFFICE_PALETTE [OPTIONS_FILE]");
        return ExitCode::from(2);
    }
    let opts = bentwire_convert::ConvertOptions {
        office_palette: Some(a[3].clone()),
        ..Default::default()
    };
    let overrides = match a.get(4) {
        Some(f) => match std::fs::read_to_string(f) {
            Ok(t) => parse_overrides(&t),
            Err(e) => {
                eprintln!("{}: {e}", f.display());
                return ExitCode::FAILURE;
            }
        },
        None => Vec::new(),
    };
    let sources = enumerate(&a[0], &a[1]);
    let rel = |p: &PathBuf| {
        p.strip_prefix(&a[0])
            .map(|r| format!("$BENTWIRE_REFERENCE/{}", r.display()))
            .or_else(|_| {
                p.strip_prefix(&a[1])
                    .map(|r| format!("$BENTWIRE_SOURCES/{}", r.display()))
            })
            .unwrap_or_else(|_| p.display().to_string())
    };
    match generate(&sources, &a[2], &opts, &overrides) {
        Err(e) => {
            eprintln!("gen-classics: {e}");
            ExitCode::FAILURE
        }
        Ok(res) => {
            for r in &res {
                match r {
                    Outcome::Included { id, path, size } => {
                        println!("included\t{id}\t{}\t{size}", rel(path))
                    }
                    Outcome::Excluded { path, reason } => {
                        println!("excluded\t{}\t{reason}", rel(path))
                    }
                    Outcome::Duplicate { path, of } => {
                        println!("duplicate\t{}\tbyte-identical to {}", rel(path), rel(of))
                    }
                }
            }
            ExitCode::SUCCESS
        }
    }
}
