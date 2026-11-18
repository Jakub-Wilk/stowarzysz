//! The home screen: the birthday banner (when someone has a birthday) and a card per module
//! with what waits for the signed-in member. Each card has its own query, so a slow or failing
//! module never holds up the others; SSE events refresh them through their topics.

mod birthday;
mod cards;
mod garland;
mod summary;

use api::ApiError;
use api::types::Person;
use birthday::BirthdayBanner;
use cards::{KasaCard, MikolajkiCard, SejmikCard, UmowyCard};
use dioxus::prelude::*;

use crate::components::DeskScroll;
use crate::query::{Topic, use_query};
use crate::session::{use_client, use_session};

/// The value of a finished, successful query.
fn ready<T: Clone>(resource: &Resource<Result<T, ApiError>>) -> Option<T> {
    match &*resource.read() {
        Some(Ok(value)) => Some(value.clone()),
        _ => None,
    }
}

#[component]
pub fn Home() -> Element {
    let client = use_client();
    let session = use_session();
    let me = session.me().map_or(0, |me| me.id);

    let polls = {
        let client = client.clone();
        use_query(&[Topic::Polls], move || {
            let client = client.clone();
            async move { summary::fetch_polls(&client).await }
        })
    };
    let ledger = {
        let client = client.clone();
        use_query(&[Topic::Ledger], move || {
            let client = client.clone();
            async move { summary::fetch_ledger(&client, me).await }
        })
    };
    let pacts = {
        let client = client.clone();
        use_query(&[Topic::Pacts], move || {
            let client = client.clone();
            async move { summary::fetch_overdue_pacts(&client).await }
        })
    };
    let santa = {
        let client = client.clone();
        use_query(&[Topic::Santa], move || {
            let client = client.clone();
            async move { summary::fetch_santa(&client).await }
        })
    };
    let birthdays = use_query(&[Topic::Birthdays, Topic::People], move || {
        let client = client.clone();
        async move { client.get::<Vec<Person>>("/api/birthdays/today").await }
    });

    rsx! {
        DeskScroll { class: "home",
            if let Some(people) = ready(&birthdays) {
                BirthdayBanner { people, me }
            }
            div { class: "home-cards",
                SejmikCard { to_answer: ready(&polls) }
                div { class: "home-grid",
                    KasaCard { ledger: ready(&ledger) }
                    UmowyCard { overdue: ready(&pacts) }
                }
                if let Some(Some(santa)) = ready(&santa) {
                    MikolajkiCard { santa }
                }
            }
        }
    }
}
