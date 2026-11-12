//! Birthdays: whose is today, and when the reminder goes out.

use chrono::{Datelike as _, NaiveDate, NaiveTime};

use crate::DomainError;

/// Local (Europe/Warsaw) time of day from which the birthday reminder is sent.
pub const REMINDER_TIME: NaiveTime = match NaiveTime::from_hms_opt(20, 0, 0) {
    Some(time) => time,
    None => panic!("20:00 is a valid time"),
};

/// Whether `today` is `birthday`'s day of the year. 29 February is celebrated on 28 February
/// in years without one.
pub fn is_birthday(birthday: NaiveDate, today: NaiveDate) -> bool {
    if (birthday.month(), birthday.day()) == (2, 29) {
        return (today.month(), today.day()) == (2, 29)
            || ((today.month(), today.day()) == (2, 28) && !today.leap_year());
    }
    (birthday.month(), birthday.day()) == (today.month(), today.day())
}

/// The `(month, day)` pairs whose birthdays fall on `today`, for a database filter.
pub fn birthdays_on(today: NaiveDate) -> Vec<(u32, u32)> {
    let mut days = vec![(today.month(), today.day())];
    if (today.month(), today.day()) == (2, 28) && !today.leap_year() {
        days.push((2, 29));
    }
    days
}

/// A birthday may not be in the future (`today` in Warsaw time). Tied to the `birthday` field.
pub fn validate(birthday: NaiveDate, today: NaiveDate) -> Result<(), DomainError> {
    if birthday > today {
        return Err(DomainError::new(
            "birthday_in_future",
            "Data urodzin nie może być z przyszłości.",
        )
        .on("birthday"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn ignores_the_year() {
        assert!(is_birthday(day(1990, 5, 3), day(2026, 5, 3)));
        assert!(!is_birthday(day(1990, 5, 3), day(2026, 5, 4)));
    }

    #[test]
    fn leap_day_is_kept_on_28_february_in_other_years() {
        let leap = day(2000, 2, 29);
        assert!(is_birthday(leap, day(2027, 2, 28)));
        assert!(!is_birthday(leap, day(2028, 2, 28)));
        assert!(is_birthday(leap, day(2028, 2, 29)));
        assert!(!is_birthday(leap, day(2027, 3, 1)));
        assert_eq!(birthdays_on(day(2027, 2, 28)), [(2, 28), (2, 29)]);
        assert_eq!(birthdays_on(day(2028, 2, 28)), [(2, 28)]);
    }

    #[test]
    fn cannot_be_in_the_future() {
        assert!(validate(day(2026, 11, 12), day(2026, 11, 12)).is_ok());
        assert_eq!(
            validate(day(2026, 11, 13), day(2026, 11, 12))
                .unwrap_err()
                .code,
            "birthday_in_future"
        );
    }
}
