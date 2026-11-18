//! Money travels as integers in a currency's minor units (grosze for PLN, whole yen for JPY);
//! people read and type major units. `exponent` is the number of minor-unit digits (2, or 0 for
//! JPY); `/api/ledger/meta` lists it for every currency. A port of `frontend/src/lib/money.ts`.

/// pl-PL digit grouping as `Intl` does it: a no-break space, and only from five digits up
/// (`1200`, but `12 000`).
fn group(major: u64) -> String {
    let digits = major.to_string();
    if digits.len() < 5 {
        return digits;
    }
    let mut out = String::with_capacity(digits.len() + digits.len() / 3 * 2);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push('\u{a0}');
        }
        out.push(c);
    }
    out
}

/// `1050` -> `"10,50 zł"`; whole amounts drop the decimals: `1000` -> `"10 zł"`.
pub fn format_money(minor: i64, currency: &str, exponent: u32) -> String {
    let sign = if minor < 0 { "-" } else { "" };
    let abs = minor.unsigned_abs();
    let unit = 10u64.pow(exponent);
    let major = group(abs / unit);
    let rest = abs % unit;
    let decimals = if rest == 0 {
        String::new()
    } else {
        format!(",{rest:0width$}", width = exponent as usize)
    };
    let symbol = if currency == "PLN" { "zł" } else { currency };
    format!("{sign}{major}{decimals} {symbol}")
}

/// Złoty in grosze.
pub fn format_pln(grosze: i64) -> String {
    format_money(grosze, "PLN", 2)
}

/// A balance with its sign spelled out, as the ledger writes it: `"− 46,80 zł"`, `"+ 12 zł"`,
/// `"0 zł"` (a real minus sign, U+2212).
pub fn format_signed_pln(grosze: i64) -> String {
    let amount = format_pln(grosze.abs());
    match grosze.signum() {
        -1 => format!("\u{2212} {amount}"),
        1 => format!("+ {amount}"),
        _ => amount,
    }
}

/// `"10"`, `"10,5"` or `"10.50"` -> minor units; `None` for blank, zero, negative or invalid text.
pub fn parse_money(text: &str, exponent: u32) -> Option<i64> {
    let cleaned: String = text
        .trim()
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect::<String>()
        .replacen(',', ".", 1);
    let (major, fraction) = match cleaned.split_once('.') {
        Some((major, fraction)) => (major, fraction),
        None => (cleaned.as_str(), ""),
    };
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(major) {
        return None;
    }
    if cleaned.contains('.') && (exponent == 0 || !digits(fraction)) {
        return None;
    }
    if fraction.len() > exponent as usize {
        return None;
    }
    let unit = 10i64.checked_pow(exponent)?;
    let major: i64 = major.parse().ok()?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        format!("{fraction:0<width$}", width = exponent as usize)
            .parse()
            .ok()?
    };
    let minor = major.checked_mul(unit)?.checked_add(fraction)?;
    (minor > 0).then_some(minor)
}

/// Minor units back to what a person would type: `1050` -> `"10,50"`, `1000` -> `"10"`.
pub fn to_money_input(minor: i64, exponent: u32) -> String {
    let unit = 10i64.pow(exponent);
    let (major, rest) = (minor / unit, minor % unit);
    if rest == 0 {
        major.to_string()
    } else {
        format!("{major},{rest:0width$}", width = exponent as usize)
    }
}

/// Money (grosze) or goods (a quantity of an item): `1050` -> `"10,50 zł"`, `3` + `"piwo"` ->
/// `"3 × piwo"`.
pub fn format_debt(amount: i64, item: &str) -> String {
    match (item, amount) {
        ("", _) => format_pln(amount),
        (item, 1) => item.to_owned(),
        (item, amount) => format!("{amount} × {item}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_intl_pl() {
        assert_eq!(format_pln(1050), "10,50 zł");
        assert_eq!(format_pln(1000), "10 zł");
        assert_eq!(format_pln(5), "0,05 zł");
        assert_eq!(format_pln(-1050), "-10,50 zł");
        assert_eq!(format_pln(120_000), "1200 zł");
        assert_eq!(format_pln(123_456_789), "1\u{a0}234\u{a0}567,89 zł");
        assert_eq!(format_money(1200, "JPY", 0), "1200 JPY");
        assert_eq!(format_money(12_000, "JPY", 0), "12\u{a0}000 JPY");
        assert_eq!(format_money(1001, "EUR", 2), "10,01 EUR");
    }

    #[test]
    fn signs_balances() {
        assert_eq!(format_signed_pln(-4680), "\u{2212} 46,80 zł");
        assert_eq!(format_signed_pln(1200), "+ 12 zł");
        assert_eq!(format_signed_pln(0), "0 zł");
    }

    #[test]
    fn parses_what_people_type() {
        assert_eq!(parse_money("10", 2), Some(1000));
        assert_eq!(parse_money("10,5", 2), Some(1050));
        assert_eq!(parse_money(" 10.50 ", 2), Some(1050));
        assert_eq!(parse_money("1 200,01", 2), Some(120_001));
        assert_eq!(parse_money("0,01", 2), Some(1));
        assert_eq!(parse_money("1200", 0), Some(1200));
        for bad in [
            "", "0", "0,00", "-5", "10,505", "abc", "1,2,3", ",5", "5,", "1.5",
        ] {
            let exponent = if bad == "1.5" { 0 } else { 2 };
            assert_eq!(parse_money(bad, exponent), None, "{bad:?}");
        }
    }

    #[test]
    fn round_trips_the_input_form() {
        assert_eq!(to_money_input(1050, 2), "10,50");
        assert_eq!(to_money_input(1000, 2), "10");
        assert_eq!(to_money_input(1005, 2), "10,05");
        assert_eq!(to_money_input(7, 0), "7");
    }

    #[test]
    fn formats_debts() {
        assert_eq!(format_debt(1050, ""), "10,50 zł");
        assert_eq!(format_debt(1, "piwo"), "piwo");
        assert_eq!(format_debt(3, "piwo"), "3 × piwo");
    }
}
