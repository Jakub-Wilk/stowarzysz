//! Pact settlement reproduces the Python implementation (generated golden vectors) and keeps
//! its promises for any input: debts come only from the losing side, nobody loses more than
//! their stake, and the losers' stakes are paid out in full when anyone on the winning side
//! staked.
#![allow(clippy::unwrap_used)]

mod differential;

use std::collections::BTreeMap;

use differential::cases;
use domain::pacts::{Outcome, PactKind, Party, Verdict, settle};
use proptest::prelude::*;
use serde_json::Value;

fn party(value: &Value) -> Party {
    Party {
        user_id: value["user_id"].as_i64().unwrap(),
        is_host: value["is_host"].as_bool().unwrap(),
        side: value["side"].as_str().unwrap().to_owned(),
        stake_amount: value["stake_amount"].as_i64(),
        stake_note: value["stake_note"].as_str().unwrap().to_owned(),
    }
}

#[test]
fn settle_matches_python() {
    let all = cases("pact_settle");
    for case in &all {
        let kind = PactKind::parse(case["kind"].as_str().unwrap()).unwrap();
        let parties: Vec<Party> = case["parties"]
            .as_array()
            .unwrap()
            .iter()
            .map(party)
            .collect();
        let outcome = Outcome::from_json(&case["result"]).unwrap();
        let settlement = settle(kind, &parties, &outcome);
        let debts: Vec<Value> = settlement
            .debts
            .iter()
            .map(|d| serde_json::json!([d.debtor_id, d.creditor_id, d.amount, d.item]))
            .collect();
        assert_eq!(Value::from(debts), case["output"]["debts"], "{case}");
        let verdicts: Vec<Value> = settlement
            .verdicts
            .iter()
            .map(|(user, verdict)| serde_json::json!([user, verdict.as_str()]))
            .collect();
        assert_eq!(Value::from(verdicts), case["output"]["verdicts"], "{case}");
    }
    assert!(all.len() > 200);
}

/// Up to 8 people, each on one of three sides with an optional stake (or a stake in kind).
fn group() -> impl Strategy<Value = Vec<Party>> {
    prop::collection::vec(
        (
            0..3usize,
            prop::option::of(1..1_000_000i64),
            prop::bool::ANY,
        ),
        1..8,
    )
    .prop_map(|people| {
        people
            .into_iter()
            .enumerate()
            .map(|(i, (side, stake, note))| Party {
                user_id: i64::try_from(i).unwrap() + 1,
                is_host: i == 0,
                side: ["a", "b", "c"][side].to_owned(),
                stake_amount: stake,
                stake_note: if note { "piwo".into() } else { String::new() },
            })
            .collect()
    })
}

proptest! {
    #[test]
    fn a_group_bet_pays_out_the_losers_stakes_and_nothing_more(
        parties in group(),
        winner in 0..3usize,
    ) {
        let winner = ["a", "b", "c"][winner];
        let settlement = settle(
            PactKind::GroupBet,
            &parties,
            &Outcome::Winner { winner: winner.to_owned() },
        );
        let by_id: BTreeMap<i64, &Party> = parties.iter().map(|p| (p.user_id, p)).collect();
        let mut paid: BTreeMap<i64, i64> = BTreeMap::new();
        for debt in &settlement.debts {
            let (debtor, creditor) = (by_id[&debt.debtor_id], by_id[&debt.creditor_id]);
            prop_assert!(debtor.side != winner && creditor.side == winner);
            prop_assert!(debt.amount > 0);
            if debt.item.is_empty() {
                *paid.entry(debt.debtor_id).or_default() += debt.amount;
            } else {
                prop_assert_eq!(debt.amount, 1);
                prop_assert!(debtor.stake_amount.is_none());
            }
        }
        let anyone_staked_right = parties
            .iter()
            .any(|p| p.side == winner && p.stake_amount.is_some());
        for p in &parties {
            let lost = paid.get(&p.user_id).copied().unwrap_or(0);
            prop_assert!(lost <= p.stake_amount.unwrap_or(0), "nobody loses more than their stake");
            if p.side != winner && anyone_staked_right {
                prop_assert_eq!(lost, p.stake_amount.unwrap_or(0), "the pot is paid out in full");
            }
            let expected = if p.side == winner { Verdict::Won } else { Verdict::Lost };
            prop_assert_eq!(settlement.verdicts[&p.user_id], expected);
        }
    }

    #[test]
    fn a_draw_or_calling_it_off_moves_nothing(parties in group()) {
        for outcome in [Outcome::Winner { winner: "draw".into() }, Outcome::VOID] {
            let settlement = settle(PactKind::GroupBet, &parties, &outcome);
            prop_assert!(settlement.debts.is_empty());
            prop_assert_eq!(settlement.verdicts.len(), parties.len());
        }
    }
}
