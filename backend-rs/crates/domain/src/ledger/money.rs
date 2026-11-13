//! The ledger's arithmetic. Pure and exact: amounts are integers in a currency's minor units
//! (grosze for PLN, whole yen for JPY) or, for goods, a quantity of an item, and every function
//! sums exactly, so nothing is ever lost to rounding. Checked against the Python implementation
//! (`fixtures/differential/{allocate,split_items,settle_up,to_base,breakdown_to_base,
//! format_amount}.json`).

use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap};

use num_bigint::BigInt;
use num_integer::Integer as _;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

/// What balances are kept in. A constant: changing it would reinterpret history.
pub const BASE_CURRENCY: &str = "PLN";

/// The largest amount (minor units or pieces) anything may have: 10 billion zł. Keeps every
/// intermediate product within `i128`.
pub const MAX_AMOUNT: i64 = 1_000_000_000_000;

/// ISO 4217 minor-unit exponents of the currencies the ECB publishes reference rates for; the
/// common ones first (the order the currency picker shows).
pub const CURRENCIES: [(&str, u32); 30] = [
    ("PLN", 2),
    ("EUR", 2),
    ("USD", 2),
    ("GBP", 2),
    ("CHF", 2),
    ("CZK", 2),
    ("AUD", 2),
    ("BRL", 2),
    ("CAD", 2),
    ("CNY", 2),
    ("DKK", 2),
    ("HKD", 2),
    ("HUF", 2),
    ("IDR", 2),
    ("ILS", 2),
    ("INR", 2),
    ("ISK", 0),
    ("JPY", 0),
    ("KRW", 0),
    ("MXN", 2),
    ("MYR", 2),
    ("NOK", 2),
    ("NZD", 2),
    ("PHP", 2),
    ("RON", 2),
    ("SEK", 2),
    ("SGD", 2),
    ("THB", 2),
    ("TRY", 2),
    ("ZAR", 2),
];

/// The minor-unit digits of a supported currency.
pub fn exponent(currency: &str) -> Option<u32> {
    CURRENCIES
        .iter()
        .find(|(code, _)| *code == currency)
        .map(|(_, exponent)| *exponent)
}

/// The one fact the whole ledger is made of: `debtor` owes `creditor` `amount` minor units of
/// the base currency or, with `item`, that many of the item ("piwo").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
pub struct Debt {
    pub debtor_id: i64,
    pub creditor_id: i64,
    pub amount: i64,
    /// Empty for money.
    #[serde(default)]
    pub item: String,
}

impl Debt {
    pub fn money(debtor_id: i64, creditor_id: i64, amount: i64) -> Self {
        Self {
            debtor_id,
            creditor_id,
            amount,
            item: String::new(),
        }
    }
}

/// Split `total` in proportion to `weights` in whole units, summing exactly to `total` (largest
/// remainder; ties go to the earlier entry, so callers order their inputs).
pub fn allocate(total: i64, weights: &[i64]) -> Vec<i64> {
    let weight_sum: i128 = weights.iter().map(|w| i128::from(*w)).sum();
    if total == 0 || weight_sum == 0 {
        return vec![0; weights.len()];
    }
    let total_wide = i128::from(total);
    let mut shares: Vec<i64> = weights
        .iter()
        .map(|w| narrow(total_wide * i128::from(*w) / weight_sum))
        .collect();
    let leftover = usize::try_from(total - shares.iter().sum::<i64>()).unwrap_or(0);
    let mut by_remainder: Vec<usize> = (0..weights.len()).collect();
    by_remainder.sort_by_key(|&i| (Reverse(total_wide * i128::from(weights[i]) % weight_sum), i));
    for &i in by_remainder.iter().take(leftover) {
        shares[i] += 1;
    }
    shares
}

/// Divide each item (`amount`, `{user id: weight}`) among its people and total what each owes.
///
/// Every item sums exactly, and its odd grosze go to whoever is furthest behind their exact
/// share so far on the whole receipt (ties: lower user id), so the rounding evens out from item
/// to item (four people sharing 108 zł of lines each owe 27 zł) instead of piling up on anyone.
/// A single item is plain largest-remainder [`allocate`]. How far behind each person is is kept
/// exactly, in units of `1/scale` grosz (`scale` a common multiple of the items' weight sums).
pub fn split_items(items: &[(i64, BTreeMap<i64, i64>)]) -> BTreeMap<i64, i64> {
    let weight_sum =
        |weights: &BTreeMap<i64, i64>| -> i128 { weights.values().map(|w| i128::from(*w)).sum() };
    // Only items that leave a remainder for somebody need to be expressible in the unit; any
    // common multiple works, as every error scales with it.
    let scale = items
        .iter()
        .filter_map(|(amount, weights)| {
            let total = weight_sum(weights);
            let uneven = weights
                .values()
                .any(|w| total != 0 && i128::from(*amount) * i128::from(*w) % total != 0);
            uneven.then(|| BigInt::from(total))
        })
        .fold(BigInt::from(1), |scale, total| scale.lcm(&total));
    let mut behind: BTreeMap<i64, BigInt> = BTreeMap::new();
    let mut owed: BTreeMap<i64, i64> = BTreeMap::new();
    for (amount, weights) in items {
        let total = weight_sum(weights);
        if total == 0 {
            continue;
        }
        let amount = i128::from(*amount);
        let mut shares: BTreeMap<i64, i64> = weights
            .iter()
            .map(|(user, w)| (*user, narrow(amount * i128::from(*w) / total)))
            .collect();
        let leftover = amount - shares.values().map(|s| i128::from(*s)).sum::<i128>();
        let mut error: BTreeMap<i64, BigInt> = BTreeMap::new();
        if leftover > 0 {
            let unit = &scale / BigInt::from(total);
            for (user, w) in weights {
                let exact = amount * i128::from(*w);
                if exact % total != 0 {
                    let rounded_down = i128::from(shares[user]) * total - exact;
                    let before = behind.get(user).cloned().unwrap_or_default();
                    error.insert(*user, before + BigInt::from(rounded_down) * &unit);
                }
            }
            let mut order: Vec<(&BigInt, i64)> = error.iter().map(|(u, e)| (e, *u)).collect();
            order.sort();
            let lucky: Vec<i64> = order
                .into_iter()
                .take(usize::try_from(leftover).unwrap_or(usize::MAX))
                .map(|(_, user)| user)
                .collect();
            for user in lucky {
                *shares.entry(user).or_default() += 1;
                if let Some(e) = error.get_mut(&user) {
                    *e += &scale;
                }
            }
        }
        for (user, share) in shares {
            *owed.entry(user).or_default() += share;
            if let Some(e) = error.remove(&user) {
                behind.insert(user, e);
            }
        }
    }
    owed
}

/// The nets given to [`settle_up`] do not sum to zero.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Unbalanced;

/// Transfers that bring every net to zero: the biggest debtor pays the biggest creditor, again
/// and again. Each step clears at least one person, so n people need at most n-1 transfers, and
/// ties go to the lower user id, so the same nets always give the same answer.
///
/// `nets` maps user id to net (positive: is owed) and must sum to zero.
pub fn settle_up(nets: &BTreeMap<i64, i64>, item: &str) -> Result<Vec<Debt>, Unbalanced> {
    if nets.values().map(|n| i128::from(*n)).sum::<i128>() != 0 {
        return Err(Unbalanced);
    }
    // Min-heaps: most negative net first, largest credit first; then the lower id.
    let mut debtors: BinaryHeap<Reverse<(i64, i64)>> = nets
        .iter()
        .filter(|(_, net)| **net < 0)
        .map(|(user, net)| Reverse((*net, *user)))
        .collect();
    let mut creditors: BinaryHeap<Reverse<(i64, i64)>> = nets
        .iter()
        .filter(|(_, net)| **net > 0)
        .map(|(user, net)| Reverse((-*net, *user)))
        .collect();
    let mut debts = Vec::new();
    while let Some(Reverse((owes, debtor))) = debtors.pop() {
        let Some(Reverse((is_owed, creditor))) = creditors.pop() else {
            return Err(Unbalanced);
        };
        let amount = (-owes).min(-is_owed);
        debts.push(Debt {
            debtor_id: debtor,
            creditor_id: creditor,
            amount,
            item: item.to_owned(),
        });
        if -owes > amount {
            debtors.push(Reverse((owes + amount, debtor)));
        }
        if -is_owed > amount {
            creditors.push(Reverse((is_owed + amount, creditor)));
        }
    }
    Ok(debts)
}

/// `amount` minor units of a currency with `exponent` digits, in base-currency minor units at
/// `rate` (base per unit), rounded half up. Exact: the rate's decimal digits are kept as an
/// integer fraction.
pub fn to_base(amount: i64, rate: Decimal, exponent: u32) -> i64 {
    let base_exponent = 2_i32; // PLN
    let shift = base_exponent - i32::try_from(exponent).unwrap_or(base_exponent);
    let mut numerator = i128::from(amount) * rate.mantissa();
    let mut denominator = 10_i128.pow(rate.scale());
    if shift >= 0 {
        numerator *= 10_i128.pow(shift.unsigned_abs());
    } else {
        denominator *= 10_i128.pow(shift.unsigned_abs());
    }
    // Half up (away from zero), as Python's ROUND_HALF_UP.
    let doubled = 2 * numerator.abs() + denominator;
    let magnitude = doubled / (2 * denominator);
    narrow(if numerator < 0 { -magnitude } else { magnitude })
}

fn narrow(value: i128) -> i64 {
    i64::try_from(value).unwrap_or(if value < 0 { i64::MIN } else { i64::MAX })
}

/// Who paid and who owes what for one expense, in one currency, as `{user id: amount}`. Both
/// sides sum to the same total; someone can be on both (they paid and ate).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Breakdown {
    pub paid: BTreeMap<i64, i64>,
    pub owed: BTreeMap<i64, i64>,
}

impl Breakdown {
    pub fn total(&self) -> i64 {
        self.paid.values().sum()
    }

    /// Paid minus owed, per person (positive: the others owe them).
    pub fn nets(&self) -> BTreeMap<i64, i64> {
        let mut nets: BTreeMap<i64, i64> = BTreeMap::new();
        for (user, amount) in &self.paid {
            *nets.entry(*user).or_default() += amount;
        }
        for (user, amount) in &self.owed {
            *nets.entry(*user).or_default() -= amount;
        }
        nets
    }

    /// Everyone on either side.
    pub fn people(&self) -> impl Iterator<Item = i64> + '_ {
        let mut people: Vec<i64> = self.paid.keys().chain(self.owed.keys()).copied().collect();
        people.sort_unstable();
        people.dedup();
        people.into_iter()
    }

    /// The same breakdown in the base currency. The total is converted once (half up) and then
    /// spread over each side, so the result still balances to the grosz.
    #[must_use]
    pub fn to_base(&self, rate: Decimal, exponent: u32) -> Self {
        let base_total = to_base(self.total(), rate, exponent);
        Self {
            paid: spread(base_total, &self.paid),
            owed: spread(base_total, &self.owed),
        }
    }
}

/// `total` divided among `parts`' users in proportion to their amounts, in user id order.
fn spread(total: i64, parts: &BTreeMap<i64, i64>) -> BTreeMap<i64, i64> {
    let weights: Vec<i64> = parts.values().copied().collect();
    parts
        .keys()
        .copied()
        .zip(allocate(total, &weights))
        .collect()
}

/// For people (push texts): 750 → `7,50 zł`, 1000 → `10 zł`, 5 EUR → `0,05 EUR`, 1200 JPY →
/// `1200 JPY`. Goods read as `piwo` or `3 x piwo`.
pub fn format_amount(amount: i64, currency: &str, item: &str) -> String {
    if !item.is_empty() {
        return if amount == 1 {
            item.to_owned()
        } else {
            format!("{amount} x {item}")
        };
    }
    let exponent = exponent(currency).unwrap_or(2);
    let unit = 10_i64.pow(exponent);
    let (major, minor) = (amount.div_euclid(unit), amount.rem_euclid(unit));
    let width = usize::try_from(exponent).unwrap_or(2);
    let text = if minor == 0 {
        major.to_string()
    } else {
        format!("{major},{minor:0width$}")
    };
    if currency == BASE_CURRENCY || currency.is_empty() {
        format!("{text} zł")
    } else {
        format!("{text} {currency}")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr as _;

    use super::*;

    fn map(pairs: &[(i64, i64)]) -> BTreeMap<i64, i64> {
        pairs.iter().copied().collect()
    }

    #[test]
    fn allocate_sums_and_breaks_ties_to_the_first() {
        assert_eq!(allocate(2000, &[3000, 5000]), [750, 1250]);
        assert_eq!(allocate(100, &[1, 1, 1]), [34, 33, 33]);
        assert_eq!(allocate(0, &[1]), [0]);
        assert_eq!(allocate(5, &[]), Vec::<i64>::new());
    }

    #[test]
    fn split_items_balances_rounding_across_a_receipt() {
        let everyone = map(&[(1, 1), (2, 1), (3, 1), (4, 1)]);
        let lines: Vec<_> = [1950, 1400, 3700, 1950, 1800]
            .into_iter()
            .map(|amount| (amount, everyone.clone()))
            .collect();
        assert_eq!(
            split_items(&lines),
            map(&[(1, 2700), (2, 2700), (3, 2700), (4, 2700)])
        );
        assert_eq!(
            split_items(&[(100, map(&[(1, 1), (2, 1), (3, 1)]))]),
            map(&[(1, 34), (2, 33), (3, 33)])
        );
    }

    #[test]
    fn split_items_survives_huge_weight_sums() {
        // Primes as weight sums: their common multiple is far beyond 128 bits.
        let primes = [
            999_999_937_i64,
            999_999_929,
            999_999_893,
            999_999_883,
            999_999_797,
        ];
        let items: Vec<_> = primes
            .iter()
            .map(|p| (12_345, map(&[(1, 1), (2, p - 1)])))
            .collect();
        let owed = split_items(&items);
        assert_eq!(owed.values().sum::<i64>(), 12_345 * 5);
    }

    #[test]
    fn settle_up_pays_the_biggest_creditor_first() {
        let debts = settle_up(&map(&[(1, 6000), (2, -3000), (3, -2000), (4, -1000)]), "").unwrap();
        assert_eq!(
            debts,
            [
                Debt::money(2, 1, 3000),
                Debt::money(3, 1, 2000),
                Debt::money(4, 1, 1000)
            ]
        );
        assert_eq!(settle_up(&map(&[(1, 100), (2, -99)]), ""), Err(Unbalanced));
    }

    #[test]
    fn conversion_rounds_half_up_across_exponents() {
        let rate = |s: &str| Decimal::from_str(s).unwrap();
        assert_eq!(to_base(1000, rate("4.25"), 2), 4250);
        assert_eq!(to_base(1, rate("4.325"), 2), 4);
        assert_eq!(to_base(1, rate("4.5"), 2), 5);
        assert_eq!(to_base(100, rate("0.0265"), 0), 265);
        let breakdown = Breakdown {
            paid: map(&[(1, 10000)]),
            owed: map(&[(1, 3334), (2, 3333), (3, 3333)]),
        };
        let base = breakdown.to_base(rate("4.2537"), 2);
        assert_eq!(base.owed, map(&[(1, 14182), (2, 14178), (3, 14177)]));
        assert_eq!(base.nets().values().sum::<i64>(), 0);
    }

    #[test]
    fn amounts_read_as_people_write_them() {
        assert_eq!(format_amount(1000, "PLN", ""), "10 zł");
        assert_eq!(format_amount(750, "PLN", ""), "7,50 zł");
        assert_eq!(format_amount(5, "EUR", ""), "0,05 EUR");
        assert_eq!(format_amount(1200, "JPY", ""), "1200 JPY");
        assert_eq!(format_amount(1, "", "piwo"), "piwo");
        assert_eq!(format_amount(3, "", "piwo"), "3 x piwo");
    }
}
