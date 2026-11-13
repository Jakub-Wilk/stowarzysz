//! The ledger arithmetic reproduces the Python implementation exactly (generated golden vectors).
#![allow(clippy::unwrap_used)]

mod differential;

use std::collections::BTreeMap;
use std::str::FromStr as _;

use differential::{cases, pairs};
use domain::ledger::money::{Breakdown, allocate, format_amount, settle_up, split_items, to_base};
use rust_decimal::Decimal;
use serde_json::Value;

fn ints(value: &Value) -> Vec<i64> {
    value
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_i64().unwrap())
        .collect()
}

fn map(value: &Value) -> BTreeMap<i64, i64> {
    pairs(value).into_iter().collect()
}

fn rate(value: &Value) -> Decimal {
    Decimal::from_str(value.as_str().unwrap()).unwrap()
}

fn exponent(value: &Value) -> u32 {
    u32::try_from(value.as_u64().unwrap()).unwrap()
}

#[test]
fn allocate_matches_python() {
    for case in cases("allocate") {
        let shares = allocate(case["total"].as_i64().unwrap(), &ints(&case["weights"]));
        assert_eq!(shares, ints(&case["output"]), "{case}");
    }
}

#[test]
fn split_items_matches_python() {
    let all = cases("split_items");
    for case in &all {
        let items: Vec<(i64, BTreeMap<i64, i64>)> = case["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|item| (item[0].as_i64().unwrap(), map(&item[1])))
            .collect();
        assert_eq!(split_items(&items), map(&case["output"]), "{case}");
    }
    assert!(all.len() > 200);
}

#[test]
fn settle_up_matches_python() {
    for case in cases("settle_up") {
        let debts = settle_up(&map(&case["nets"]), "").unwrap();
        let got: Vec<Vec<i64>> = debts
            .iter()
            .map(|d| vec![d.debtor_id, d.creditor_id, d.amount])
            .collect();
        let expected: Vec<Vec<i64>> = case["output"]
            .as_array()
            .unwrap()
            .iter()
            .map(ints)
            .collect();
        assert_eq!(got, expected, "{case}");
    }
}

#[test]
fn to_base_matches_python() {
    for case in cases("to_base") {
        let got = to_base(
            case["amount"].as_i64().unwrap(),
            rate(&case["rate"]),
            exponent(&case["exponent"]),
        );
        assert_eq!(got, case["output"].as_i64().unwrap(), "{case}");
    }
}

#[test]
fn breakdown_to_base_matches_python() {
    for case in cases("breakdown_to_base") {
        let breakdown = Breakdown {
            paid: map(&case["paid"]),
            owed: map(&case["owed"]),
        };
        let base = breakdown.to_base(rate(&case["rate"]), exponent(&case["exponent"]));
        assert_eq!(base.paid, map(&case["output"]["paid"]), "{case}");
        assert_eq!(base.owed, map(&case["output"]["owed"]), "{case}");
    }
}

#[test]
fn format_amount_matches_python() {
    for case in cases("format_amount") {
        let got = format_amount(
            case["amount"].as_i64().unwrap(),
            case["currency"].as_str().unwrap(),
            case["item"].as_str().unwrap(),
        );
        assert_eq!(got, case["output"].as_str().unwrap(), "{case}");
    }
}
