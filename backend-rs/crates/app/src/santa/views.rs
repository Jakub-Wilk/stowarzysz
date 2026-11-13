//! Reading Secret Santa. While a round is active its pairings are revealed only here, and only
//! to their giver; help requests show the victim which gift they are about, never who asked.

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, Utc};
use domain::santa::Mode;
use serde::Serialize;
use utoipa::ToSchema;

use super::{active, victims};
use crate::accounts::{Person, persons};
use crate::auth::Actor;
use crate::pagination::{Cursor, page};
use crate::{AppResult, Context};

const HISTORY_PAGE: usize = 10;

/// The active round as everyone sees it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ActiveEvent {
    pub id: i64,
    pub mode: Mode,
    pub deadline: DateTime<Utc>,
    /// Whole złoty, highest first.
    pub gift_tiers: Vec<i64>,
    /// Everyone taking part, once each.
    pub participants: Vec<Person>,
    pub is_participant: bool,
}

/// One of your victims in a per-tier round.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct TierVictim {
    pub amount: i64,
    pub victim: Person,
}

/// Help you asked for, about one of your victims.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct GiverHelp {
    pub victim_id: i64,
    /// The tier's amount; `null` in a single-victim round.
    pub amount: Option<i64>,
    /// Waiting for an answer.
    pub pending: bool,
    /// The latest answer (kept while a new question waits).
    pub ideas: Vec<String>,
}

/// A request for ideas about you. Nothing in it says who asked.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ReceiverHelp {
    pub id: i64,
    pub amount: Option<i64>,
    pub pending: bool,
    pub ideas: Vec<String>,
}

/// Secret Santa as the caller sees it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct SantaState {
    pub active: bool,
    pub event: Option<ActiveEvent>,
    /// Single-victim rounds: whom you give to (every tier).
    pub my_victim: Option<Person>,
    /// Per-tier rounds: whom you give to for each amount, highest first.
    pub my_tier_victims: Vec<TierVictim>,
    pub my_help_requests: Vec<GiverHelp>,
    pub help_requests_for_me: Vec<ReceiverHelp>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct GiftView {
    pub id: i64,
    pub amount: i64,
    pub note: String,
}

/// Who gave to whom in an ended round, and what.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PairingView {
    pub id: i64,
    pub giver: Person,
    pub receiver: Person,
    /// Highest amount first.
    pub gifts: Vec<GiftView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct HistoryEvent {
    pub id: i64,
    pub mode: Mode,
    pub deadline: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub gift_tiers: Vec<i64>,
    pub pairings: Vec<PairingView>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct HistoryPage {
    pub items: Vec<HistoryEvent>,
    pub next_cursor: Option<String>,
}

fn someone(id: i64) -> Person {
    Person {
        id,
        username: String::new(),
        avatar: None,
    }
}

pub async fn state(ctx: &Context, actor: &Actor) -> AppResult<SantaState> {
    let mut conn = ctx.db.acquire().await?;
    let Some(event) = active(&mut conn).await? else {
        return Ok(SantaState {
            active: false,
            event: None,
            my_victim: None,
            my_tier_victims: Vec::new(),
            my_help_requests: Vec::new(),
            help_requests_for_me: Vec::new(),
        });
    };
    let givers = sqlx::query_scalar!(
        "select distinct giver_id from santa_assignments where event_id = $1",
        event.id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mine = victims(&mut conn, ctx, event.id, actor.user_id).await?;
    let requests = sqlx::query!(
        r#"select id, receiver_id, tier_index, ideas, answered_at from santa_help_requests
           where event_id = $1 order by tier_index nulls first, id"#,
        event.id,
    )
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    let mut ids: BTreeSet<i64> = givers.iter().copied().collect();
    ids.extend(mine.iter().map(|(_, receiver)| *receiver));
    let people = persons(ctx, &ids.into_iter().collect::<Vec<_>>()).await?;
    let person = |id: i64| people.get(&id).cloned().unwrap_or_else(|| someone(id));

    let mut participants: Vec<Person> = givers.iter().map(|id| person(*id)).collect();
    participants.sort_by(|a, b| a.username.cmp(&b.username));
    let my_help_requests = mine
        .iter()
        .filter_map(|(tier, victim)| {
            requests
                .iter()
                .find(|r| r.receiver_id == *victim && r.tier_index == *tier)
                .map(|r| GiverHelp {
                    victim_id: *victim,
                    amount: event.amount(*tier),
                    pending: r.answered_at.is_none(),
                    ideas: r.ideas.clone(),
                })
        })
        .collect();
    let help_requests_for_me = requests
        .iter()
        .filter(|r| r.receiver_id == actor.user_id)
        .map(|r| ReceiverHelp {
            id: r.id,
            amount: event.amount(r.tier_index),
            pending: r.answered_at.is_none(),
            ideas: r.ideas.clone(),
        })
        .collect();
    Ok(SantaState {
        active: true,
        my_victim: mine
            .iter()
            .find(|(tier, _)| tier.is_none())
            .map(|(_, victim)| person(*victim)),
        my_tier_victims: mine
            .iter()
            .filter_map(|(tier, victim)| {
                event.amount(*tier).map(|amount| TierVictim {
                    amount,
                    victim: person(*victim),
                })
            })
            .collect(),
        event: Some(ActiveEvent {
            id: event.id,
            mode: event.mode,
            deadline: event.deadline,
            is_participant: givers.contains(&actor.user_id),
            gift_tiers: event.gift_tiers.clone(),
            participants,
        }),
        my_help_requests,
        help_requests_for_me,
    })
}

/// Ended rounds, newest first, with their pairings (public once a round is over).
pub async fn history(ctx: &Context, cursor: Option<&str>) -> AppResult<HistoryPage> {
    let after = cursor.map(Cursor::decode).transpose()?;
    let events = sqlx::query!(
        r#"select id, mode, deadline, ended_at as "ended_at!", gift_tiers from santa_events
           where status = 'ended'
             and ($1::timestamptz is null or (ended_at, id) < ($1, $2))
           order by ended_at desc, id desc limit $3"#,
        after.map(|c| c.at),
        after.map_or(0, |c| c.id),
        i64::try_from(HISTORY_PAGE + 1).unwrap_or(i64::MAX),
    )
    .fetch_all(&ctx.db)
    .await?;
    let (events, next_cursor) = page(events, HISTORY_PAGE, |e| Cursor {
        at: e.ended_at,
        id: e.id,
    });
    let ids: Vec<i64> = events.iter().map(|e| e.id).collect();
    let pairings = sqlx::query!(
        r#"select id, event_id, giver_id, receiver_id as "receiver_id!" from santa_assignments
           where event_id = any($1) and receiver_id is not null
           order by tier_index nulls first, id"#,
        &ids,
    )
    .fetch_all(&ctx.db)
    .await?;
    let gifts = sqlx::query!(
        "select g.id, g.assignment_id, g.amount, g.note from santa_gifts g
         join santa_assignments a on a.id = g.assignment_id
         where a.event_id = any($1) order by g.amount desc, g.id",
        &ids,
    )
    .fetch_all(&ctx.db)
    .await?;
    let mut people: BTreeSet<i64> = BTreeSet::new();
    for p in &pairings {
        people.extend([p.giver_id, p.receiver_id]);
    }
    let people = persons(ctx, &people.into_iter().collect::<Vec<_>>()).await?;
    let person = |id: i64| people.get(&id).cloned().unwrap_or_else(|| someone(id));
    let mut gifts_of: HashMap<i64, Vec<GiftView>> = HashMap::new();
    for g in gifts {
        gifts_of.entry(g.assignment_id).or_default().push(GiftView {
            id: g.id,
            amount: g.amount,
            note: g.note,
        });
    }
    let mut pairings_of: HashMap<i64, Vec<PairingView>> = HashMap::new();
    for p in pairings {
        pairings_of
            .entry(p.event_id)
            .or_default()
            .push(PairingView {
                id: p.id,
                giver: person(p.giver_id),
                receiver: person(p.receiver_id),
                gifts: gifts_of.remove(&p.id).unwrap_or_default(),
            });
    }
    let items = events
        .into_iter()
        .map(|e| {
            Ok(HistoryEvent {
                mode: Mode::parse(&e.mode)
                    .ok_or_else(|| anyhow::anyhow!("santa event {} has mode {:?}", e.id, e.mode))?,
                pairings: pairings_of.remove(&e.id).unwrap_or_default(),
                id: e.id,
                deadline: e.deadline,
                ended_at: e.ended_at,
                gift_tiers: e.gift_tiers,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(HistoryPage { items, next_cursor })
}
