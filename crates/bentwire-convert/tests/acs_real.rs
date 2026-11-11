//! Round trip of the 12 ACS files against the Python oracle (character-format 11, A-TEST-2).
//! `cargo test -p bentwire-convert --test acs_real -- --ignored` (or scripts/oracle-check.sh).

mod oracle_common;

#[test]
#[ignore]
fn acs_round_trip_against_oracle() {
    let (n, _, _) = oracle_common::round_trip_kind("acs");
    assert_eq!(n, 12, "expected the 12 ACS files");
}
