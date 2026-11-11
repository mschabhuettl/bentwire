//! Real-data test (A-TEST-1): every ACT/ACP sample parses completely and agrees with the
//! research parser (`actparse.py`, driven by `scripts/oracle/act_dump.py`) on
//! structure and decoded bitmap pixels. Run with `cargo test -- --ignored`.

mod act_common;

use act_common::{dump, run_oracle, samples};
use agent_formats::act;
use serde_json::Value;

#[test]
#[ignore = "real data: needs BENTWIRE_SOURCES and BENTWIRE_REFERENCE"]
fn act_real_samples_match_research_parser() {
    let (list, out) = run_oracle(&samples());
    assert!(!list.is_empty(), "no samples enumerated");
    let mut bad = Vec::new();
    for (key, path) in &list {
        let bytes = std::fs::read(path).unwrap();
        let f = match act::parse(&bytes) {
            Ok(f) => f,
            Err(e) => {
                bad.push(format!("{key}: parse error {e}"));
                continue;
            }
        };
        let want: Value =
            serde_json::from_slice(&std::fs::read(out.join(format!("json/{key}.json"))).unwrap())
                .unwrap();
        let got = dump(&f);
        if got != want {
            let obj = want.as_object().unwrap();
            let diff: Vec<&String> = obj
                .keys()
                .filter(|k| got[k.as_str()] != obj[k.as_str()])
                .collect();
            bad.push(format!("{key}: differs in {diff:?}"));
        }
    }
    println!("{} files compared, {} bad", list.len(), bad.len());
    assert!(bad.is_empty(), "{bad:#?}");
    std::fs::remove_dir_all(&out).ok();
}
