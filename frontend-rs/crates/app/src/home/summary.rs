//! What the home cards count, from the module endpoints (there is no dashboard endpoint). The
//! counting is plain functions over the API types, so it is tested without a server.

use api::types::{Balances, PactPage, ParticipantState, PollPage, SantaState};
use api::{ApiError, Client};
use chrono::{DateTime, Utc};

/// Open polls the caller takes part in and hasn't answered yet (voted, vetoed or abstained).
pub fn polls_to_answer(page: &PollPage) -> usize {
    page.items
        .iter()
        .filter(|poll| poll.my.participating && !poll.my.has_voted)
        .count()
}

/// The ledger card: payments waiting for the caller to confirm, and the caller's money balance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Ledger {
    pub to_confirm: usize,
    /// Grosze; negative: the caller owes the group.
    pub balance: i64,
}

pub fn ledger(balances: &Balances, me: i64) -> Ledger {
    Ledger {
        // A payment is pending until its receiver (the creditor) confirms it.
        to_confirm: balances
            .pending
            .iter()
            .filter(|payment| payment.creditor.id == me && payment.item.is_empty())
            .count(),
        // Members with nothing to settle are left out; goods have rows of their own.
        balance: balances
            .members
            .iter()
            .find(|row| row.user.id == me && row.item.is_empty())
            .map_or(0, |row| row.net),
    }
}

/// Pacts the caller is still in whose deadline passed with no result.
pub fn overdue_pacts(pages: &[PactPage]) -> usize {
    pages
        .iter()
        .flat_map(|page| &page.items)
        .filter(|pact| {
            pact.my
                .as_ref()
                .is_some_and(|my| my.state == ParticipantState::Active)
        })
        .count()
}

/// The Secret Santa card, while an event runs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Santa {
    pub deadline: DateTime<Utc>,
    /// Questions about the caller's wishes they haven't answered.
    pub help_requests: usize,
}

pub fn santa(state: &SantaState) -> Option<Santa> {
    let event = state.event.as_ref().filter(|_| state.active)?;
    Some(Santa {
        deadline: event.deadline,
        help_requests: state
            .help_requests_for_me
            .iter()
            .filter(|request| request.pending)
            .count(),
    })
}

pub async fn fetch_polls(client: &Client) -> Result<usize, ApiError> {
    let page: PollPage = client
        .get("/api/polls?status=open&participating=true")
        .await?;
    Ok(polls_to_answer(&page))
}

pub async fn fetch_ledger(client: &Client, me: i64) -> Result<Ledger, ApiError> {
    let balances: Balances = client.get("/api/ledger/balances").await?;
    Ok(ledger(&balances, me))
}

pub async fn fetch_overdue_pacts(client: &Client) -> Result<usize, ApiError> {
    const PATH: &str = "/api/pacts?mine=true&status=awaiting_result";
    let mut pages = vec![client.get::<PactPage>(PATH).await?];
    // Rarely more than one page; follow them all so the count is right.
    while let Some(cursor) = pages.last().and_then(|p| p.next_cursor.clone()) {
        pages.push(client.get(&format!("{PATH}&cursor={cursor}")).await?);
    }
    Ok(overdue_pacts(&pages))
}

pub async fn fetch_santa(client: &Client) -> Result<Option<Santa>, ApiError> {
    let state: SantaState = client.get("/api/secret-santa").await?;
    Ok(santa(&state))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn person(id: i64) -> serde_json::Value {
        json!({ "id": id, "username": format!("u{id}") })
    }

    fn decode<T: serde::de::DeserializeOwned>(value: serde_json::Value) -> T {
        serde_json::from_value(value).expect("matches the API types")
    }

    fn poll(participating: bool, has_voted: bool) -> serde_json::Value {
        json!({
            "id": 1, "kind": "score", "title": "t", "status": "open", "creator": person(2),
            "created_at": "2026-11-18T10:00:00Z", "deadline_at": "2026-11-21T10:00:00Z",
            "participant_count": 3, "voted_count": 1,
            "my": { "participating": participating, "has_voted": has_voted, "vetoed": false, "abstained": false },
        })
    }

    #[test]
    fn counts_polls_to_answer() {
        let page: PollPage = decode(json!({
            "items": [poll(true, false), poll(true, true), poll(false, false), poll(true, false)],
        }));
        assert_eq!(polls_to_answer(&page), 2);
    }

    #[test]
    fn reads_the_ledger_for_me() {
        let balances: Balances = decode(json!({
            "currency": "PLN",
            "members": [
                { "user": person(1), "item": "", "net": -4680 },
                { "user": person(1), "item": "piwo", "net": 2 },
                { "user": person(2), "item": "", "net": 4680 },
            ],
            "settlements": [],
            "pending": [
                { "entry_id": 1, "debtor": person(2), "creditor": person(1), "item": "", "amount": 500 },
                { "entry_id": 2, "debtor": person(1), "creditor": person(2), "item": "", "amount": 500 },
                { "entry_id": 3, "debtor": person(2), "creditor": person(1), "item": "piwo", "amount": 1 },
            ],
        }));
        assert_eq!(
            ledger(&balances, 1),
            Ledger {
                to_confirm: 1,
                balance: -4680
            }
        );
        // Someone with nothing to settle has no row: 0.
        assert_eq!(
            ledger(&balances, 3),
            Ledger {
                to_confirm: 0,
                balance: 0
            }
        );
    }

    fn pact(state: Option<&str>) -> serde_json::Value {
        json!({
            "id": 1, "kind": "bet", "title": "t", "condition": "c", "status": "awaiting_result",
            "creator": person(2), "created_at": "2026-11-01T10:00:00Z", "active_count": 2,
            "my": state.map(|state| json!({ "participant_id": 5, "role": "host", "state": state })),
        })
    }

    #[test]
    fn counts_overdue_pacts_i_am_still_in() {
        let first: PactPage = decode(json!({
            "items": [pact(Some("active")), pact(Some("withdrawn"))], "next_cursor": "x",
        }));
        let second: PactPage = decode(json!({ "items": [pact(Some("active")), pact(None)] }));
        assert_eq!(overdue_pacts(&[first, second]), 2);
    }

    #[test]
    fn shows_santa_only_while_it_runs() {
        let running: SantaState = decode(json!({
            "active": true,
            "event": {
                "id": 1, "deadline": "2026-12-12T18:00:00Z", "gift_tiers": [], "is_participant": true,
                "mode": "single", "participants": [],
            },
            "help_requests_for_me": [
                { "id": 1, "pending": true, "ideas": [] },
                { "id": 2, "pending": false, "ideas": ["książka"] },
            ],
            "my_help_requests": [], "my_tier_victims": [],
        }));
        let santa = santa(&running).expect("running");
        assert_eq!(santa.help_requests, 1);
        assert_eq!(santa.deadline.to_rfc3339(), "2026-12-12T18:00:00+00:00");

        let idle: SantaState = decode(json!({
            "active": false, "help_requests_for_me": [], "my_help_requests": [], "my_tier_victims": [],
        }));
        assert_eq!(super::santa(&idle), None);
    }
}
