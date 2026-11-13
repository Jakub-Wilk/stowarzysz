//! The Secret Santa draw: offsets match the Python implementation, and every draw keeps its
//! promises (everyone gives and receives once per tier, nobody draws themselves, nobody swaps,
//! nobody gets the same victim twice).
#![allow(clippy::unwrap_used)]

mod differential;

use std::collections::{BTreeMap, BTreeSet};

use differential::cases;
use domain::santa::{Mode, draw, max_per_tier, per_tier_shifts};
use proptest::prelude::*;

#[test]
fn shifts_match_python() {
    for case in cases("santa_shifts") {
        let size = usize::try_from(case["size"].as_u64().unwrap()).unwrap();
        let expected: Vec<usize> = case["output"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| usize::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        assert_eq!(per_tier_shifts(size), expected, "{case}");
    }
}

/// A shuffled circle of 3 to 40 people and some of its valid offsets.
fn circle() -> impl Strategy<Value = (Vec<i64>, Vec<usize>)> {
    (3..40usize).prop_flat_map(|n| {
        let order = Just((1..=i64::try_from(n).unwrap()).collect::<Vec<_>>()).prop_shuffle();
        let shifts = Just(per_tier_shifts(n))
            .prop_shuffle()
            .prop_flat_map(move |all| (1..=max_per_tier(n)).prop_map(move |k| all[..k].to_vec()));
        (order, shifts)
    })
}

fn check_pairing(pairs: &BTreeMap<i64, i64>, people: &BTreeSet<i64>) -> Result<(), TestCaseError> {
    prop_assert_eq!(&pairs.keys().copied().collect::<BTreeSet<_>>(), people);
    prop_assert_eq!(&pairs.values().copied().collect::<BTreeSet<_>>(), people);
    for (giver, receiver) in pairs {
        prop_assert_ne!(giver, receiver, "nobody draws themselves");
        prop_assert_ne!(pairs[receiver], *giver, "nobody swaps");
    }
    Ok(())
}

proptest! {
    #[test]
    fn a_single_draw_is_valid((order, _) in circle()) {
        let people: BTreeSet<i64> = order.iter().copied().collect();
        let pairs: BTreeMap<i64, i64> =
            draw(Mode::Single, &order, &[]).iter().map(|d| (d.giver, d.receiver)).collect();
        check_pairing(&pairs, &people)?;
    }

    #[test]
    fn a_per_tier_draw_is_valid((order, shifts) in circle()) {
        let people: BTreeSet<i64> = order.iter().copied().collect();
        let draws = draw(Mode::PerTier, &order, &shifts);
        prop_assert_eq!(draws.len(), order.len() * shifts.len());
        for tier in 0..shifts.len() {
            let pairs: BTreeMap<i64, i64> = draws
                .iter()
                .filter(|d| d.tier == Some(tier))
                .map(|d| (d.giver, d.receiver))
                .collect();
            check_pairing(&pairs, &people)?;
        }
        for giver in &people {
            let victims: BTreeSet<i64> =
                draws.iter().filter(|d| d.giver == *giver).map(|d| d.receiver).collect();
            prop_assert_eq!(victims.len(), shifts.len(), "a different victim for every tier");
        }
    }
}
