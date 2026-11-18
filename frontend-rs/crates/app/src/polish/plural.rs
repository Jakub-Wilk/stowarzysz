//! Polish plurals: 1 głos, 2 głosy, 5 głosów (the CLDR rules for whole numbers).

/// The three noun forms a Polish count needs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Forms {
    pub one: &'static str,
    pub few: &'static str,
    pub many: &'static str,
}

/// The form for `count`: `one` for 1, `few` for 2-4, 22-24, ... (but not 12-14), else `many`.
pub fn plural(count: i64, forms: Forms) -> &'static str {
    let n = count.unsigned_abs();
    if n == 1 {
        forms.one
    } else if (2..=4).contains(&(n % 10)) && !(12..=14).contains(&(n % 100)) {
        forms.few
    } else {
        forms.many
    }
}

/// `"3 głosy"`.
pub fn count(count: i64, forms: Forms) -> String {
    format!("{count} {}", plural(count, forms))
}

pub const VOTE: Forms = Forms {
    one: "głos",
    few: "głosy",
    many: "głosów",
};
pub const DAY: Forms = Forms {
    one: "dzień",
    few: "dni",
    many: "dni",
};
pub const PARTICIPANT: Forms = Forms {
    one: "uczestnik",
    few: "uczestników",
    many: "uczestników",
};
pub const PACT: Forms = Forms {
    one: "umowa",
    few: "umowy",
    many: "umów",
};
pub const PICTURE: Forms = Forms {
    one: "zdjęcie",
    few: "zdjęcia",
    many: "zdjęć",
};
pub const PERSON: Forms = Forms {
    one: "osoba",
    few: "osoby",
    many: "osób",
};
pub const ITEM: Forms = Forms {
    one: "pozycja",
    few: "pozycje",
    many: "pozycji",
};
/// After "dla": dla 1 osoby, dla 3 osób.
pub const PERSON_GENITIVE: Forms = Forms {
    one: "osoby",
    few: "osób",
    many: "osób",
};
pub const REQUEST: Forms = Forms {
    one: "prośba",
    few: "prośby",
    many: "próśb",
};
pub const ENTRY: Forms = Forms {
    one: "wpis",
    few: "wpisy",
    many: "wpisów",
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_the_polish_form() {
        let cases = [
            (0, "głosów"),
            (1, "głos"),
            (2, "głosy"),
            (4, "głosy"),
            (5, "głosów"),
            (11, "głosów"),
            (12, "głosów"),
            (14, "głosów"),
            (21, "głosów"),
            (22, "głosy"),
            (101, "głosów"),
            (102, "głosy"),
            (112, "głosów"),
            (1_000_003, "głosy"),
            (-3, "głosy"),
        ];
        for (n, expected) in cases {
            assert_eq!(plural(n, VOTE), expected, "{n}");
        }
        assert_eq!(count(5, PERSON), "5 osób");
        assert_eq!(count(1, REQUEST), "1 prośba");
        assert_eq!(count(3, REQUEST), "3 prośby");
        assert_eq!(count(5, REQUEST), "5 próśb");
    }
}
