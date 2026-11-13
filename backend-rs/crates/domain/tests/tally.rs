//! The tallies reproduce the Python implementation exactly (generated golden vectors).
#![allow(clippy::unwrap_used)]

mod differential;

use std::collections::BTreeMap;

use domain::polls::tally::{count_picks, schulze, top_ids};
use serde_json::Value;

fn strings(value: &Value) -> Vec<&str> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect()
}

fn lists(value: &Value) -> Vec<Vec<&str>> {
    value.as_array().unwrap().iter().map(strings).collect()
}

#[test]
fn picks_match_python() {
    for case in differential::cases("picks") {
        let ids = strings(&case["options"]);
        let counts = count_picks(&ids, lists(&case["picks"]));
        let expected: BTreeMap<String, u32> =
            serde_json::from_value(case["output"]["counts"].clone()).unwrap();
        assert_eq!(counts, expected, "{case}");
        assert_eq!(
            top_ids(&ids, &counts),
            strings(&case["output"]["top"]),
            "{case}"
        );
    }
}

#[test]
fn schulze_matches_python() {
    let cases = differential::cases("schulze");
    for case in &cases {
        let ids = strings(&case["options"]);
        let outcome = schulze(&ids, &lists(&case["rankings"]));
        let expected = &case["output"];
        assert_eq!(outcome.winners, strings(&expected["winners"]), "{case}");
        assert_eq!(
            outcome.condorcet_winner.as_deref(),
            expected["condorcet_winner"].as_str(),
            "{case}"
        );
        let wins: BTreeMap<String, u32> = serde_json::from_value(expected["wins"].clone()).unwrap();
        assert_eq!(outcome.wins, wins, "{case}");
        let pairwise: BTreeMap<String, BTreeMap<String, u32>> =
            serde_json::from_value(expected["pairwise"].clone()).unwrap();
        assert_eq!(outcome.pairwise, pairwise, "{case}");
    }
    assert!(cases.len() > 200);
}
