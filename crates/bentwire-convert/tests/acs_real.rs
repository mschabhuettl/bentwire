//! Round trip of the ACS files (docs/sources.md 1.1) against the Python oracle (character-format 11, A-TEST-2).
//! `cargo test -p bentwire-convert --test acs_real -- --ignored` (or scripts/oracle-check.sh).

mod oracle_common;

#[test]
#[ignore]
fn acs_round_trip_against_oracle() {
    let (n, _, _) = oracle_common::round_trip_kind("acs");
    let listed = agent_fixtures::real::listed("1.1").len();
    assert_eq!(n, listed, "expected every listed ACS file");
    assert!(n >= 12, "expected at least the 12 ACS files");
}
