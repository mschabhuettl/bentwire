//! Round trip of the ACT actors against the Python oracle (character-format 11, A-TEST-2).
//! The oracle reads the Office palette from CLIPPIT.ACS, so the converter does too here.

mod oracle_common;

use bentwire_convert::ConvertOptions;

fn opts() -> ConvertOptions {
    ConvertOptions {
        office_palette: Some(oracle_common::sources().join("CLIPPIT.ACS")),
        ..Default::default()
    }
}

#[test]
#[ignore]
fn act_round_trip_against_oracle() {
    oracle_common::round_trip_with("act", &opts());
}
