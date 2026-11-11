//! Round trip of the web characters (ACF + ACA) and the Agent 1.5 OLE files against the
//! Python oracle (character-format 11, A-TEST-2).

mod oracle_common;

#[test]
#[ignore]
fn acf_aca_round_trip_against_oracle() {
    oracle_common::round_trip_kind("acf");
}

#[test]
#[ignore]
fn agent15_round_trip_against_oracle() {
    oracle_common::round_trip_kind("ole15");
}
