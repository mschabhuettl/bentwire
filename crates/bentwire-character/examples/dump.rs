//! `cargo run -p bentwire-character --example dump -- CHARACTER [OUT.json]`: canonical dump and
//! interpreter B traces of a character as JSON (same layout as scripts/oracle/dump.py).

use bentwire_character::Character;
use bentwire_character::dump::{Dumper, all_traces};

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let c = Character::load(&args[0]).unwrap_or_else(|e| panic!("{}: {e}", args[0]));
    let d = Dumper::new(&c);
    let v = serde_json::json!({ "dump": d.dump(), "traces": all_traces(&d) });
    let s = serde_json::to_string(&v).unwrap();
    match args.get(1) {
        Some(p) => std::fs::write(p, s).unwrap(),
        None => println!("{s}"),
    }
}
