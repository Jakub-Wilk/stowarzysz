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

mod properties {
    use std::collections::BTreeMap;

    use domain::ledger::money::{Breakdown, allocate, settle_up, split_items};
    use proptest::prelude::*;
    use rust_decimal::Decimal;

    /// Up to 8 people (ids 1..=8) with nets that sum to zero.
    fn balanced_nets() -> impl Strategy<Value = BTreeMap<i64, i64>> {
        prop::collection::vec(-1_000_000_i64..1_000_000, 1..8).prop_map(|mut nets| {
            let last = -nets.iter().sum::<i64>();
            nets.push(last);
            (1..).zip(nets).collect()
        })
    }

    /// An item: an amount and `{user: weight}` for up to 6 of 8 people.
    fn item() -> impl Strategy<Value = (i64, BTreeMap<i64, i64>)> {
        (
            0_i64..10_000_000,
            prop::collection::btree_map(1_i64..=8, 1_i64..1000, 1..6),
        )
    }

    proptest! {
        #[test]
        fn allocate_sums_exactly_and_stays_within_one_of_the_exact_share(
            total in 0_i64..1_000_000_000,
            weights in prop::collection::vec(0_i64..1_000_000, 1..12),
        ) {
            let shares = allocate(total, &weights);
            let weight_sum: i128 = weights.iter().map(|w| i128::from(*w)).sum();
            if weight_sum == 0 {
                prop_assert!(shares.iter().all(|s| *s == 0));
            } else {
                prop_assert_eq!(shares.iter().sum::<i64>(), total);
                for (share, weight) in shares.iter().zip(&weights) {
                    let exact_floor = i128::from(total) * i128::from(*weight) / weight_sum;
                    let share = i128::from(*share);
                    prop_assert!(share == exact_floor || share == exact_floor + 1);
                }
            }
        }

        #[test]
        fn split_items_owes_exactly_the_receipt(items in prop::collection::vec(item(), 1..20)) {
            let owed = split_items(&items);
            let total: i64 = items.iter().map(|(amount, _)| amount).sum();
            prop_assert_eq!(owed.values().sum::<i64>(), total);
            prop_assert!(owed.values().all(|o| *o >= 0));
            // Nobody owes anything for items they are not on.
            for user in owed.keys() {
                prop_assert!(items.iter().any(|(_, weights)| weights.contains_key(user)));
            }
        }

        #[test]
        fn settle_up_clears_every_net_in_at_most_n_minus_one_transfers(nets in balanced_nets()) {
            let debts = settle_up(&nets, "").unwrap();
            let mut left = nets.clone();
            for debt in &debts {
                prop_assert!(debt.amount > 0);
                *left.get_mut(&debt.debtor_id).unwrap() += debt.amount;
                *left.get_mut(&debt.creditor_id).unwrap() -= debt.amount;
            }
            prop_assert!(left.values().all(|n| *n == 0), "{:?}", left);
            let people = nets.values().filter(|n| **n != 0).count();
            prop_assert!(debts.len() <= people.saturating_sub(1));
        }

        #[test]
        fn a_converted_breakdown_still_balances(
            items in prop::collection::vec(item(), 1..10),
            payer in 1_i64..=8,
            rate in 1_i64..100_000_000, // up to 1000 PLN per unit, 5 decimals
        ) {
            let owed = split_items(&items);
            let total: i64 = owed.values().sum();
            let breakdown = Breakdown { paid: BTreeMap::from([(payer, total)]), owed };
            let base = breakdown.to_base(Decimal::new(rate, 5), 2);
            prop_assert_eq!(base.paid.values().sum::<i64>(), base.owed.values().sum::<i64>());
            prop_assert_eq!(base.nets().values().sum::<i64>(), 0);
        }
    }
}
