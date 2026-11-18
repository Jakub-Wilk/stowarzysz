//! Dates the Polish way, in Warsaw time (the group lives there; Android has no `Intl`, so the
//! names are spelled out here instead of asking the platform).

use chrono::{DateTime, Datelike as _, NaiveDate, TimeDelta, Timelike as _, Utc, Weekday};
use chrono_tz::Europe::Warsaw;

use super::plural::{DAY, plural};

pub type Local = DateTime<chrono_tz::Tz>;

pub fn now() -> Local {
    Utc::now().with_timezone(&Warsaw)
}

pub fn local(at: DateTime<Utc>) -> Local {
    at.with_timezone(&Warsaw)
}

/// `listopad`.
pub fn month_name(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "styczeń",
        "luty",
        "marzec",
        "kwiecień",
        "maj",
        "czerwiec",
        "lipiec",
        "sierpień",
        "wrzesień",
        "październik",
        "listopad",
        "grudzień",
    ];
    NAMES[(month as usize).clamp(1, 12) - 1]
}

/// `18 listopada`.
pub fn month_genitive(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "stycznia",
        "lutego",
        "marca",
        "kwietnia",
        "maja",
        "czerwca",
        "lipca",
        "sierpnia",
        "września",
        "października",
        "listopada",
        "grudnia",
    ];
    NAMES[(month as usize).clamp(1, 12) - 1]
}

/// `18 lis` (`Intl`'s `month: 'short'` for pl-PL).
pub fn month_short(month: u32) -> &'static str {
    const NAMES: [&str; 12] = [
        "sty", "lut", "mar", "kwi", "maj", "cze", "lip", "sie", "wrz", "paź", "lis", "gru",
    ];
    NAMES[(month as usize).clamp(1, 12) - 1]
}

pub fn weekday_name(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "poniedziałek",
        Weekday::Tue => "wtorek",
        Weekday::Wed => "środa",
        Weekday::Thu => "czwartek",
        Weekday::Fri => "piątek",
        Weekday::Sat => "sobota",
        Weekday::Sun => "niedziela",
    }
}

/// `SOB.`: the weekday as a desk calendar prints it.
pub fn weekday_short(day: Weekday) -> &'static str {
    match day {
        Weekday::Mon => "PON.",
        Weekday::Tue => "WT.",
        Weekday::Wed => "ŚR.",
        Weekday::Thu => "CZW.",
        Weekday::Fri => "PT.",
        Weekday::Sat => "SOB.",
        Weekday::Sun => "NIEDZ.",
    }
}

/// `SOB. 14.11`.
pub fn desk_date(at: Local) -> String {
    format!(
        "{} {:02}.{:02}",
        weekday_short(at.weekday()),
        at.day(),
        at.month()
    )
}

/// `12 grudnia`.
pub fn day_month(at: DateTime<Utc>) -> String {
    let at = local(at);
    format!("{} {}", at.day(), month_genitive(at.month()))
}

/// `18 listopada 2026`.
pub fn format_date(date: NaiveDate) -> String {
    format!(
        "{} {} {}",
        date.day(),
        month_genitive(date.month()),
        date.year()
    )
}

/// `18 lis, 14:05`.
pub fn format_when(at: DateTime<Utc>) -> String {
    let at = local(at);
    format!(
        "{} {}, {:02}:{:02}",
        at.day(),
        month_short(at.month()),
        at.hour(),
        at.minute()
    )
}

/// Heading a history entry is filed under: `Dzisiaj`, `Wczoraj`, the weekday or the month.
pub fn day_group(at: DateTime<Utc>, now: Local) -> String {
    let date = local(at).date_naive();
    let days = (now.date_naive() - date).num_days();
    match days {
        ..=0 => "Dzisiaj".to_owned(),
        1 => "Wczoraj".to_owned(),
        2..=6 => weekday_name(date.weekday()).to_owned(),
        _ => format!("{} {}", month_name(date.month()), date.year()),
    }
}

/// Time until `deadline` as `"2 dni 3 godz."`, `"5 godz. 10 min"` or `"12 min"` (abbreviations
/// don't inflect).
pub fn time_left(deadline: DateTime<Utc>, now: DateTime<Utc>) -> String {
    let left = (deadline - now).max(TimeDelta::zero());
    // Round up to whole minutes, like the React version.
    let minutes = (left.num_seconds() + 59) / 60;
    let (days, hours, mins) = (minutes / 1440, minutes % 1440 / 60, minutes % 60);
    if days > 0 {
        let hours = if hours > 0 {
            format!(" {hours} godz.")
        } else {
            String::new()
        };
        format!("{days} {}{hours}", plural(days, DAY))
    } else if hours > 0 {
        let mins = if mins > 0 {
            format!(" {mins} min")
        } else {
            String::new()
        };
        format!("{hours} godz.{mins}")
    } else {
        format!("{mins} min")
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone as _;

    use super::*;

    fn utc(y: i32, m: u32, d: u32, h: u32, min: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(y, m, d, h, min, 0)
            .single()
            .expect("valid date")
    }

    #[test]
    fn formats_in_warsaw_time() {
        // 23:30 UTC in November is 00:30 the next day in Warsaw (UTC+1).
        assert_eq!(format_when(utc(2026, 11, 18, 23, 30)), "19 lis, 00:30");
        // Summer time: UTC+2.
        assert_eq!(format_when(utc(2026, 7, 1, 10, 5)), "1 lip, 12:05");
        assert_eq!(
            format_date(NaiveDate::from_ymd_opt(2026, 11, 18).expect("date")),
            "18 listopada 2026"
        );
    }

    #[test]
    fn prints_the_desk_date() {
        // 14 November 2026 is a Saturday.
        assert_eq!(desk_date(local(utc(2026, 11, 14, 9, 0))), "SOB. 14.11");
        assert_eq!(desk_date(local(utc(2026, 11, 16, 9, 0))), "PON. 16.11");
        // 23:30 UTC on Saturday is already Sunday in Warsaw.
        assert_eq!(desk_date(local(utc(2026, 11, 14, 23, 30))), "NIEDZ. 15.11");
        assert_eq!(day_month(utc(2026, 12, 12, 10, 0)), "12 grudnia");
    }

    #[test]
    fn groups_by_day() {
        let now = local(utc(2026, 11, 18, 12, 0)); // a Wednesday
        assert_eq!(day_group(utc(2026, 11, 18, 6, 0), now), "Dzisiaj");
        assert_eq!(day_group(utc(2026, 11, 17, 6, 0), now), "Wczoraj");
        assert_eq!(day_group(utc(2026, 11, 14, 6, 0), now), "sobota");
        assert_eq!(day_group(utc(2026, 10, 1, 6, 0), now), "październik 2026");
    }

    #[test]
    fn counts_down() {
        let now = utc(2026, 11, 18, 12, 0);
        assert_eq!(time_left(utc(2026, 11, 20, 15, 0), now), "2 dni 3 godz.");
        assert_eq!(time_left(utc(2026, 11, 19, 12, 0), now), "1 dzień");
        assert_eq!(time_left(utc(2026, 11, 18, 17, 10), now), "5 godz. 10 min");
        assert_eq!(time_left(utc(2026, 11, 18, 12, 12), now), "12 min");
        assert_eq!(time_left(utc(2026, 11, 18, 11, 0), now), "0 min");
    }
}
