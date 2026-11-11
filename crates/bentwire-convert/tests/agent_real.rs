//! Round trip of the web characters (ACF + ACA) and the Agent 1.5 OLE files against the
//! Python oracle (character-format 11, A-TEST-2).

mod oracle_common;

use std::path::PathBuf;

#[test]
fn output_stems_unique_for_same_base_name() {
    let paths: Vec<PathBuf> = [
        "a/genie/data",
        "b/merlin/Data",
        "c/genie.acs",
        "d/x/genie.acs",
    ]
    .iter()
    .map(PathBuf::from)
    .collect();
    let stems = oracle_common::output_stems(&paths);
    assert_eq!(stems, ["data", "data-1", "genie", "genie-3"]);
}

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
