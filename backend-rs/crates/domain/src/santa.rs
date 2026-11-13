//! Secret Santa: one round at a time, drawn once, revealed when a superuser ends it.
//!
//! The draw is a shuffled circle: in `single` mode everyone gives to the next person on it;
//! in `per_tier` mode every gift tier uses a different offset around the same circle, so a
//! giver never gets the same victim twice, nobody draws themselves and nobody swaps with the
//! person who drew them. Randomness stays outside: callers pass the shuffled order and the
//! offsets they picked, so every rule here is deterministic and testable.

use std::collections::BTreeMap;

use crate::DomainError;

pub const MIN_PARTICIPANTS: usize = 3;
pub const MAX_TIERS: usize = 10;
/// Gift amounts are whole złoty.
pub const MAX_AMOUNT: i64 = 100_000;
pub const MAX_IDEAS: usize = 5;
pub const IDEA_MAX: usize = 200;
pub const NOTE_MAX: usize = 300;

text_enum!(
    /// How victims are drawn.
    Mode {
        /// One victim per giver, who gets every tier.
        Single = "single",
        /// A different victim for each tier.
        PerTier = "per_tier",
    }
);

text_enum!(
    Status {
        Active = "active",
        /// Revealed: pairings are public history.
        Ended = "ended",
    }
);

/// Distinct amounts, highest first (a tier is its position in this list).
pub fn clean_tiers(tiers: &[i64]) -> Result<Vec<i64>, DomainError> {
    let mut clean = tiers.to_vec();
    clean.sort_unstable_by(|a, b| b.cmp(a));
    clean.dedup();
    let invalid = |code, message: String| Err(DomainError::new(code, message).on("gift_tiers"));
    if clean.is_empty() {
        return invalid("tiers_required", "Dodaj przynajmniej jedną kwotę.".into());
    }
    if clean.len() > MAX_TIERS {
        return invalid("too_many_tiers", format!("Maksymalnie {MAX_TIERS} kwot."));
    }
    if clean.iter().any(|a| !(1..=MAX_AMOUNT).contains(a)) {
        return invalid(
            "invalid_tier",
            "Kwoty muszą być dodatnie i rozsądne.".into(),
        );
    }
    Ok(clean)
}

/// A victim's gift ideas: trimmed, blanks dropped, 1 to 5 of at most 200 characters.
pub fn clean_ideas(ideas: &[String]) -> Result<Vec<String>, DomainError> {
    let clean: Vec<String> = ideas
        .iter()
        .map(|i| i.trim().to_owned())
        .filter(|i| !i.is_empty())
        .collect();
    let invalid = |code, message: String| Err(DomainError::new(code, message).on("ideas"));
    if clean.is_empty() {
        return invalid("ideas_required", "Podaj przynajmniej jeden pomysł.".into());
    }
    if clean.len() > MAX_IDEAS {
        return invalid(
            "too_many_ideas",
            format!("Maksymalnie {MAX_IDEAS} pomysłów."),
        );
    }
    if clean.iter().any(|i| i.chars().count() > IDEA_MAX) {
        return invalid(
            "idea_too_long",
            format!("Pomysł może mieć najwyżej {IDEA_MAX} znaków."),
        );
    }
    Ok(clean)
}

/// Offsets along a circle of `size` people that make a valid tier: never yourself (0) and
/// never a swap with the person who draws you (half the circle).
pub fn per_tier_shifts(size: usize) -> Vec<usize> {
    (1..size).filter(|shift| (2 * shift) % size != 0).collect()
}

/// How many tiers a per-tier draw among `size` people can have.
pub fn max_per_tier(size: usize) -> usize {
    per_tier_shifts(size).len()
}

/// One pairing: `giver` gives to `receiver`, for tier `tier` (`None`: every tier).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Draw {
    pub giver: i64,
    pub tier: Option<usize>,
    pub receiver: i64,
}

/// Everyone gives to the person `shift` places after them on the circle `order`.
pub fn shifted(order: &[i64], shift: usize) -> Vec<(i64, i64)> {
    let n = order.len();
    order
        .iter()
        .enumerate()
        .map(|(i, giver)| (*giver, order[(i + shift) % n]))
        .collect()
}

/// The pairings of a draw over the shuffled circle `order`. `single` uses the next person;
/// `per_tier` uses `shifts[t]` for tier `t` (distinct values from [`per_tier_shifts`]).
pub fn draw(mode: Mode, order: &[i64], shifts: &[usize]) -> Vec<Draw> {
    match mode {
        Mode::Single => shifted(order, 1)
            .into_iter()
            .map(|(giver, receiver)| Draw {
                giver,
                tier: None,
                receiver,
            })
            .collect(),
        Mode::PerTier => shifts
            .iter()
            .enumerate()
            .flat_map(|(tier, shift)| {
                shifted(order, *shift)
                    .into_iter()
                    .map(move |(giver, receiver)| Draw {
                        giver,
                        tier: Some(tier),
                        receiver,
                    })
            })
            .collect(),
    }
}

/// `100 zł, 30 zł`.
pub fn amounts(tiers: &[i64]) -> String {
    tiers
        .iter()
        .map(|a| format!("{a} zł"))
        .collect::<Vec<_>>()
        .join(", ")
}

/// What each giver is told when the draw is made.
pub fn start_messages(
    draws: &[Draw],
    names: &BTreeMap<i64, String>,
    tiers: &[i64],
) -> BTreeMap<i64, String> {
    let name = |id: &i64| names.get(id).cloned().unwrap_or_default();
    if draws.iter().all(|d| d.tier.is_none()) {
        return draws
            .iter()
            .map(|d| {
                let body = format!(
                    "Obdarowujesz: {}. Kwoty: {}.",
                    name(&d.receiver),
                    amounts(tiers)
                );
                (d.giver, body)
            })
            .collect();
    }
    let mut by_giver: BTreeMap<i64, Vec<(usize, i64)>> = BTreeMap::new();
    for d in draws {
        by_giver
            .entry(d.giver)
            .or_default()
            .push((d.tier.unwrap_or(0), d.receiver));
    }
    by_giver
        .into_iter()
        .map(|(giver, mut picks)| {
            picks.sort_unstable();
            let list: Vec<String> = picks
                .iter()
                .map(|(t, r)| format!("{} ({} zł)", name(r), tiers.get(*t).unwrap_or(&0)))
                .collect();
            (giver, format!("Twoi podopieczni: {}.", list.join(", ")))
        })
        .collect()
}

/// The gift a help request is about, as the victim reads it.
pub fn help_gift(amount: Option<i64>) -> String {
    amount.map_or_else(
        || "prezentem".to_owned(),
        |a| format!("prezentem za {a} zł"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tiers_are_distinct_sorted_and_sane() {
        assert_eq!(clean_tiers(&[30, 100, 30]).unwrap(), [100, 30]);
        for bad in [
            vec![],
            vec![0],
            vec![-5],
            vec![MAX_AMOUNT + 1],
            (1..=11).collect(),
        ] {
            assert!(clean_tiers(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn ideas_are_trimmed_and_bounded() {
        let ideas = |list: &[&str]| list.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        assert_eq!(
            clean_ideas(&ideas(&[" Książka ", "", "Kubek"])).unwrap(),
            ["Książka", "Kubek"]
        );
        for bad in [
            ideas(&[]),
            ideas(&["", "  "]),
            ideas(&["x"; 6]),
            vec!["x".repeat(201)],
        ] {
            assert!(clean_ideas(&bad).is_err(), "{bad:?}");
        }
    }

    #[test]
    fn a_single_draw_is_one_circle() {
        let pairs = draw(Mode::Single, &[3, 1, 2], &[]);
        let got: Vec<(i64, i64)> = pairs.iter().map(|d| (d.giver, d.receiver)).collect();
        assert_eq!(got, [(3, 1), (1, 2), (2, 3)]);
    }

    #[test]
    fn three_people_allow_two_tiers() {
        assert_eq!(per_tier_shifts(3), [1, 2]);
        assert_eq!(per_tier_shifts(4), [1, 3]);
        assert_eq!(max_per_tier(2), 0);
    }

    #[test]
    fn givers_are_told_whom_they_drew() {
        let names: BTreeMap<i64, String> =
            [(1, "ala".into()), (2, "ola".into()), (3, "ewa".into())].into();
        let single = start_messages(&draw(Mode::Single, &[1, 2, 3], &[]), &names, &[100, 30]);
        assert_eq!(single[&1], "Obdarowujesz: ola. Kwoty: 100 zł, 30 zł.");
        let tiers = start_messages(
            &draw(Mode::PerTier, &[1, 2, 3], &[1, 2]),
            &names,
            &[100, 30],
        );
        assert_eq!(tiers[&1], "Twoi podopieczni: ola (100 zł), ewa (30 zł).");
        assert_eq!(help_gift(Some(30)), "prezentem za 30 zł");
    }
}
