//! The Bilans maths. Money is settled across the whole group (the fewest transfers, whoever
//! originally lent it); goods are settled per pair and per item, since "kolacja" owed to Bob is
//! owed to Bob, and never against money.

use std::collections::BTreeMap;

use super::money::{Debt, settle_up};

/// Transfers that even out the money nets (`user -> net`, positive: is owed). Nets that do not
/// sum to zero (impossible for obligations) give no transfers.
pub fn money_transfers(nets: &BTreeMap<i64, i64>) -> Vec<Debt> {
    let nonzero: BTreeMap<i64, i64> = nets
        .iter()
        .filter(|(_, net)| **net != 0)
        .map(|(user, net)| (*user, *net))
        .collect();
    settle_up(&nonzero, "").unwrap_or_default()
}

/// Goods owed, netted per pair and unit: `(debtor, creditor, unit, total)` rows in, one
/// `(debtor, creditor, unit) -> quantity` per pair that still owes out.
pub fn goods_pairs(
    rows: impl IntoIterator<Item = (i64, i64, String, i64)>,
) -> BTreeMap<(i64, i64, String), i64> {
    let mut net: BTreeMap<(i64, i64, String), i64> = BTreeMap::new();
    for (debtor, creditor, unit, total) in rows {
        let (low, high) = (debtor.min(creditor), debtor.max(creditor));
        let signed = if debtor == low { total } else { -total };
        *net.entry((low, high, unit)).or_default() += signed;
    }
    net.into_iter()
        .filter(|(_, amount)| *amount != 0)
        .map(|((low, high, unit), amount)| {
            if amount > 0 {
                ((low, high, unit), amount)
            } else {
                ((high, low, unit), -amount)
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn goods_net_within_a_pair_only() {
        let pairs = goods_pairs([
            (2, 1, "piwo".to_owned(), 2),
            (1, 2, "piwo".to_owned(), 1),
            (3, 2, "piwo".to_owned(), 1),
            (4, 1, "kawa".to_owned(), 1),
            (1, 4, "kawa".to_owned(), 1),
        ]);
        assert_eq!(
            pairs.into_iter().collect::<Vec<_>>(),
            [
                ((2, 1, "piwo".to_owned()), 1),
                ((3, 2, "piwo".to_owned()), 1)
            ]
        );
    }

    #[test]
    fn money_is_settled_across_the_group() {
        let nets = BTreeMap::from([(1, 3000), (2, -2000), (3, -1000), (4, 0)]);
        assert_eq!(
            money_transfers(&nets),
            [Debt::money(2, 1, 2000), Debt::money(3, 1, 1000)]
        );
    }
}
