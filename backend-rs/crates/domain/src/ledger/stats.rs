//! Spending statistics: periods, their buckets and averages.
//!
//! What counts (confirmed only): expenses, incomes (a negative expense, so a refunded ticket
//! counts as nothing) and money debts, a category of their own, [`DEBTS`]: one is an expense
//! the creditor paid for the debtor. Goods debts and payments are not spending.

use chrono::{Datelike as _, Months, NaiveDate};
use serde::Serialize;

/// The stats-only category of money debts: key, label, emoji.
pub const DEBTS: (&str, &str, &str) = ("debts", "Długi", "🤝");
/// A period this long or longer is charted by month.
pub const DAY_BUCKETS_UP_TO: i64 = 62;

/// What the series is bucketed by.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[cfg_attr(feature = "schema", derive(utoipa::ToSchema))]
#[serde(rename_all = "snake_case")]
pub enum Bucket {
    Day,
    Month,
}

impl Bucket {
    pub fn for_period(begin: NaiveDate, finish: NaiveDate) -> Self {
        if (finish - begin).num_days() < DAY_BUCKETS_UP_TO {
            Self::Day
        } else {
            Self::Month
        }
    }

    /// The label of the bucket `day` falls in: `YYYY-MM-DD` or `YYYY-MM`.
    pub fn label(self, day: NaiveDate) -> String {
        match self {
            Self::Day => day.format("%Y-%m-%d").to_string(),
            Self::Month => day.format("%Y-%m").to_string(),
        }
    }

    /// Every bucket from `begin` to `finish`, in order (none if `finish` is before `begin`).
    pub fn all(self, begin: NaiveDate, finish: NaiveDate) -> Vec<String> {
        match self {
            Self::Day => begin
                .iter_days()
                .take_while(|day| *day <= finish)
                .map(|day| self.label(day))
                .collect(),
            Self::Month => {
                let first = begin.with_day(1).unwrap_or(begin);
                (0..u32::try_from(months(begin, finish)).unwrap_or(0))
                    .filter_map(|n| first.checked_add_months(Months::new(n)))
                    .map(|day| self.label(day))
                    .collect()
            }
        }
    }
}

/// Calendar months touched from `start` to `end`, both included (≤ 0 if `end` is earlier).
pub fn months(start: NaiveDate, end: NaiveDate) -> i64 {
    i64::from(end.year() - start.year()) * 12 + i64::from(end.month()) - i64::from(start.month())
        + 1
}

/// The average per month over more than one month, rounded half to even; `None` otherwise.
pub fn monthly_average(spent: i64, months: i64) -> Option<i64> {
    if months <= 1 {
        return None;
    }
    let (quotient, remainder) = (spent.div_euclid(months), spent.rem_euclid(months));
    let twice = 2 * remainder;
    Some(
        if twice > months || (twice == months && quotient % 2 != 0) {
            quotient + 1
        } else {
            quotient
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn buckets_cover_the_period_without_gaps() {
        let days = Bucket::Day.all(day(2026, 2, 27), day(2026, 3, 2));
        assert_eq!(
            days,
            ["2026-02-27", "2026-02-28", "2026-03-01", "2026-03-02"]
        );
        let months = Bucket::Month.all(day(2025, 11, 30), day(2026, 2, 1));
        assert_eq!(months, ["2025-11", "2025-12", "2026-01", "2026-02"]);
        assert_eq!(
            Bucket::Day.all(day(2026, 3, 2), day(2026, 3, 1)),
            Vec::<String>::new()
        );
        assert_eq!(
            Bucket::for_period(day(2026, 1, 1), day(2026, 3, 3)),
            Bucket::Day
        );
        assert_eq!(
            Bucket::for_period(day(2026, 1, 1), day(2026, 3, 4)),
            Bucket::Month
        );
    }

    #[test]
    fn averages_round_half_to_even_like_python() {
        assert_eq!(monthly_average(400, 1), None);
        assert_eq!(monthly_average(5, 2), Some(2)); // 2.5
        assert_eq!(monthly_average(7, 2), Some(4)); // 3.5
        assert_eq!(monthly_average(400, 7), Some(57));
        assert_eq!(monthly_average(-5, 2), Some(-2)); // -2.5
    }
}
