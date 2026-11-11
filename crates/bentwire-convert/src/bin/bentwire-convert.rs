//! `bentwire-convert SOURCE DEST`: convert one Microsoft Agent or Office actor file into a
//! bentwire character (DEST ending in `.bwc` writes a ZIP, otherwise a folder).

use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        eprintln!("usage: bentwire-convert SOURCE DEST");
        return ExitCode::from(2);
    }
    match bentwire_convert::convert(Path::new(&args[0]), Path::new(&args[1])) {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("{}: {e}", args[0]);
            ExitCode::FAILURE
        }
    }
}
