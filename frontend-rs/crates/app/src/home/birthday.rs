//! The birthday banner on the home screen: bunting, confetti, a gift and a handwritten line.

use api::types::Person;
use dioxus::prelude::*;

use crate::polish::names::join_names;

const BUNTING: Asset = asset!("/assets/decor/bunting.svg");
const CONFETTI: Asset = asset!("/assets/decor/confetti.svg");
const GIFT: Asset = asset!("/assets/decor/gift.svg");

/// `Dzisiaj Ola ma urodziny!`, `Dzisiaj Ola i Kuba mają urodziny!`, `Dzisiaj masz urodziny!`,
/// `Dzisiaj Ty i Ola macie urodziny!`.
pub fn birthday_line(others: &[&str], mine: bool) -> Option<String> {
    let line = match (others, mine) {
        ([], false) => return None,
        ([], true) => "Dzisiaj masz urodziny!".to_owned(),
        ([one], false) => format!("Dzisiaj {one} ma urodziny!"),
        (others, false) => format!("Dzisiaj {} mają urodziny!", join_names(others)),
        (others, true) => format!("Dzisiaj Ty i {} macie urodziny!", join_names(others)),
    };
    Some(line)
}

#[component]
pub fn BirthdayBanner(people: Vec<Person>, me: i64) -> Element {
    let others: Vec<&str> = people
        .iter()
        .filter(|p| p.id != me)
        .map(|p| p.username.as_str())
        .collect();
    let mine = people.iter().any(|p| p.id == me);
    let Some(line) = birthday_line(&others, mine) else {
        return rsx! {};
    };
    rsx! {
        div { class: "birthday-banner", role: "status",
            img { class: "birthday-bunting", src: BUNTING, alt: "", "aria-hidden": "true" }
            img { class: "birthday-confetti", src: CONFETTI, alt: "", "aria-hidden": "true" }
            img { class: "birthday-gift", src: GIFT, alt: "", "aria-hidden": "true" }
            span { class: "birthday-line", "{line}" }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_who_has_a_birthday() {
        assert_eq!(birthday_line(&[], false), None);
        assert_eq!(
            birthday_line(&[], true).as_deref(),
            Some("Dzisiaj masz urodziny!")
        );
        assert_eq!(
            birthday_line(&["Ola"], false).as_deref(),
            Some("Dzisiaj Ola ma urodziny!")
        );
        assert_eq!(
            birthday_line(&["Ola", "Kuba"], false).as_deref(),
            Some("Dzisiaj Ola i Kuba mają urodziny!")
        );
        assert_eq!(
            birthday_line(&["Ola"], true).as_deref(),
            Some("Dzisiaj Ty i Ola macie urodziny!")
        );
    }
}
