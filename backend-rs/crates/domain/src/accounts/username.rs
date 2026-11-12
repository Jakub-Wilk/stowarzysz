//! Usernames: the only thing that identifies a person in the app.

use unicode_normalization::UnicodeNormalization as _;

use crate::DomainError;

pub const MAX_LENGTH: usize = 150;

const INVALID: &str = "Nazwa może zawierać litery, cyfry oraz znaki @ . + - _ \
                       i pojedyncze spacje (nie na początku ani na końcu).";

/// What a typed username is stored as: NFKC-normalised (so look-alike forms collapse) and
/// trimmed.
pub fn normalize(raw: &str) -> String {
    raw.nfkc().collect::<String>().trim().to_owned()
}

/// Letters, digits and `@ . + - _`, with single spaces between words. Call on a [`normalize`]d
/// value. Errors are tied to the `username` field.
pub fn validate(username: &str) -> Result<(), DomainError> {
    if username.is_empty() {
        return Err(
            DomainError::new("username_required", "Podaj nazwę użytkownika.").on("username"),
        );
    }
    if username.chars().count() > MAX_LENGTH {
        return Err(DomainError::new(
            "username_too_long",
            format!("Nazwa może mieć najwyżej {MAX_LENGTH} znaków."),
        )
        .on("username"));
    }
    let words_ok = username
        .split(' ')
        .all(|word| !word.is_empty() && word.chars().all(allowed));
    if !words_ok {
        return Err(DomainError::new("username_invalid", INVALID).on("username"));
    }
    Ok(())
}

/// Python's `\w` (letters, digits, underscore in any script) plus `.@+-`.
fn allowed(c: char) -> bool {
    c.is_alphanumeric() || matches!(c, '_' | '.' | '@' | '+' | '-')
}

/// The message for a username that is already taken (a unique-constraint violation).
pub fn taken() -> DomainError {
    DomainError::new("username_taken", "Użytkownik o takiej nazwie już istnieje.").on("username")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalises_compatibility_forms_and_trims() {
        assert_eq!(normalize("  ﬁlip  "), "filip"); // the "fi" ligature
        assert_eq!(normalize("Ａla"), "Ala"); // full-width A
    }

    #[test]
    fn accepts_letters_digits_symbols_and_single_spaces() {
        for ok in [
            "ala",
            "Żaneta",
            "jan.kowalski@x",
            "a+b-c_d",
            "Jan Kowalski",
            "1",
        ] {
            assert_eq!(validate(ok), Ok(()), "{ok}");
        }
    }

    #[test]
    fn rejects_bad_usernames() {
        for (bad, code) in [
            ("", "username_required"),
            ("a  b", "username_invalid"),
            (" a", "username_invalid"),
            ("a ", "username_invalid"),
            ("a!b", "username_invalid"),
            ("a\tb", "username_invalid"),
        ] {
            let error = validate(bad).unwrap_err();
            assert_eq!(
                (error.code, error.field.as_deref()),
                (code, Some("username")),
                "{bad:?}"
            );
        }
        assert_eq!(
            validate(&"a".repeat(151)).unwrap_err().code,
            "username_too_long"
        );
        assert_eq!(validate(&"ą".repeat(150)), Ok(()));
    }
}
