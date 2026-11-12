//! The password policy: Django's four validators, in Django's order and with its Polish
//! messages (similarity to the username, minimum length, common passwords, all digits), plus an
//! upper bound so hashing cost stays bounded.

use std::collections::{HashMap, HashSet};
use std::io::Read as _;
use std::sync::LazyLock;

use crate::DomainError;

pub const MIN_LENGTH: usize = 8;
pub const MAX_LENGTH: usize = 256;
const MAX_SIMILARITY: f64 = 0.7;

/// Django's list of 19 640 common passwords (`django/contrib/auth/common-passwords.txt.gz`,
/// BSD-3-Clause; compiled by Royce Williams), lowercase, one per line.
static COMMON: LazyLock<HashSet<String>> = LazyLock::new(|| {
    let gz = include_bytes!("common-passwords.txt.gz");
    let mut text = String::new();
    flate2::read::GzDecoder::new(&gz[..])
        .read_to_string(&mut text)
        .expect("the embedded password list is valid gzip UTF-8");
    text.lines().map(str::to_owned).collect()
});

/// Every reason `password` is unacceptable for `username`, tied to the `password` field.
pub fn validate(password: &str, username: &str) -> Vec<DomainError> {
    let mut errors = Vec::new();
    let error = |code, message: String| DomainError::new(code, message).on("password");
    if too_similar(password, username) {
        errors.push(error(
            "password_too_similar",
            "Hasło jest zbyt podobne do nazwy użytkownika.".into(),
        ));
    }
    let length = password.chars().count();
    if length < MIN_LENGTH {
        errors.push(error(
            "password_too_short",
            format!("To hasło jest za krótkie. Musi zawierać co najmniej {MIN_LENGTH} znaków."),
        ));
    }
    if length > MAX_LENGTH {
        errors.push(error(
            "password_too_long",
            format!("Hasło może mieć najwyżej {MAX_LENGTH} znaków."),
        ));
    }
    if COMMON.contains(password.trim().to_lowercase().as_str()) {
        errors.push(error(
            "password_too_common",
            "To hasło jest zbyt powszechne.".into(),
        ));
    }
    if !password.is_empty() && password.chars().all(char::is_numeric) {
        errors.push(error(
            "password_entirely_numeric",
            "Hasło składa się wyłącznie z cyfr.".into(),
        ));
    }
    errors
}

/// Django's `UserAttributeSimilarityValidator` for the username: the password is too similar if
/// `difflib.SequenceMatcher.quick_ratio` against the username, or any of its words, is >= 0.7.
fn too_similar(password: &str, username: &str) -> bool {
    let password: Vec<char> = password.to_lowercase().chars().collect();
    let username = username.to_lowercase();
    let mut parts: Vec<&str> = username
        .split(|c: char| !(c.is_alphanumeric() || c == '_'))
        .collect();
    parts.push(&username);
    parts.into_iter().any(|part| {
        let part: Vec<char> = part.chars().collect();
        !exceeds_length_ratio(password.len(), part.len())
            && quick_ratio(&password, &part) >= MAX_SIMILARITY
    })
}

/// Django skips parts so much shorter than the password that they cannot reach the threshold.
fn exceeds_length_ratio(password_len: usize, part_len: usize) -> bool {
    #[allow(clippy::cast_precision_loss)] // lengths are tiny
    let bound = MAX_SIMILARITY / 2.0 * password_len as f64;
    #[allow(clippy::cast_precision_loss)]
    let part = part_len as f64;
    password_len >= 10 * part_len && part < bound
}

/// `difflib.SequenceMatcher(a, b).quick_ratio()`: twice the multiset intersection of the
/// characters over the total length (1.0 for two empty sequences).
fn quick_ratio(a: &[char], b: &[char]) -> f64 {
    let total = a.len() + b.len();
    if total == 0 {
        return 1.0;
    }
    let mut available: HashMap<char, usize> = HashMap::new();
    for &c in b {
        *available.entry(c).or_default() += 1;
    }
    let mut matches = 0_usize;
    for c in a {
        if let Some(left) = available.get_mut(c).filter(|left| **left > 0) {
            *left -= 1;
            matches += 1;
        }
    }
    #[allow(clippy::cast_precision_loss)]
    let ratio = 2.0 * matches as f64 / total as f64;
    ratio
}

#[cfg(test)]
mod tests {
    use super::*;

    fn codes(password: &str, username: &str) -> Vec<&'static str> {
        validate(password, username)
            .into_iter()
            .map(|e| e.code)
            .collect()
    }

    /// The same inputs and results as Django 6 (`validate_password` with the app's settings).
    #[test]
    fn matches_django() {
        assert_eq!(
            codes("abc", "kowalski"),
            ["password_too_short", "password_too_common"]
        );
        assert_eq!(
            codes("12345678", "kowalski"),
            ["password_too_common", "password_entirely_numeric"]
        );
        assert_eq!(codes("password", "kowalski"), ["password_too_common"]);
        assert_eq!(codes("kowalski1", "kowalski"), ["password_too_similar"]);
        assert_eq!(codes("Zażółć gęślą 9", "kowalski"), Vec::<&str>::new());
        assert_eq!(codes("s3cret-pass-123", "alice"), Vec::<&str>::new());
    }

    #[test]
    fn messages_are_polish_and_on_the_password_field() {
        let errors = validate("abc", "x");
        assert_eq!(
            errors[0].message,
            "To hasło jest za krótkie. Musi zawierać co najmniej 8 znaków."
        );
        assert_eq!(errors[0].field.as_deref(), Some("password"));
    }

    #[test]
    fn common_list_is_case_and_whitespace_insensitive() {
        assert!(codes(" QWERTY123 ", "x").contains(&"password_too_common"));
        assert_eq!(COMMON.len(), 19_640);
    }

    #[test]
    fn similarity_checks_words_of_the_username() {
        assert!(codes("kowalskijan", "Jan Kowalski").contains(&"password_too_similar"));
        // Much longer than the username part: skipped, as in Django.
        assert!(
            !codes("j-and-a-very-long-unrelated-phrase", "j").contains(&"password_too_similar")
        );
    }

    #[test]
    fn quick_ratio_matches_difflib() {
        let chars = |s: &str| s.chars().collect::<Vec<_>>();
        assert!((quick_ratio(&chars("abcd"), &chars("bcde")) - 0.75).abs() < 1e-12);
        assert!((quick_ratio(&chars("aab"), &chars("abb")) - 2.0 * 2.0 / 6.0).abs() < 1e-12);
        assert!((quick_ratio(&[], &[]) - 1.0).abs() < 1e-12);
    }

    #[test]
    fn has_an_upper_bound() {
        assert!(codes(&"Zx9!".repeat(70), "x").contains(&"password_too_long"));
    }
}
