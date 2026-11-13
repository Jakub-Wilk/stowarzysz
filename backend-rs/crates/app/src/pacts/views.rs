//! Reading pacts. Every member can read every pact; only participants act on one. The list is
//! one query (plus the people in it); a detail loads the whole pact.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use domain::pacts::policy::{self, Actions};
use domain::pacts::{ClaimState, Outcome, PactKind, PactOutcome, ParticipantState, Role, Status};
use serde::Serialize;
use utoipa::ToSchema;

use super::load;
use crate::accounts::{Person, persons};
use crate::auth::Actor;
use crate::media::Image;
use crate::pagination::{Cursor, page};
use crate::{AppResult, Context};

const PAGE_SIZE: usize = 30;

/// The caller's place in a pact.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct MyPart {
    pub participant_id: i64,
    pub role: Role,
    pub state: ParticipantState,
}

/// A pact in the list.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PactSummary {
    pub id: i64,
    pub kind: PactKind,
    pub title: String,
    pub condition: String,
    pub due_at: Option<DateTime<Utc>>,
    pub status: Status,
    pub creator: Person,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    /// Participants who are in it now.
    pub active_count: i64,
    /// `null` if you have never been part of it.
    pub my: Option<MyPart>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PactPage {
    pub items: Vec<PactSummary>,
    /// Pass as `cursor` for the next page; `null` on the last one.
    pub next_cursor: Option<String>,
}

/// One approver's say on a join request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
pub struct ApprovalView {
    pub approver_id: i64,
    /// `null` until they decide.
    pub approved: Option<bool>,
}

/// One person in a pact. In a bet every opponent is a separate wager against the host.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ParticipantView {
    pub id: i64,
    pub user: Person,
    pub role: Role,
    pub state: ParticipantState,
    /// Group bets.
    pub side: String,
    /// Grosze.
    pub stake_amount: Option<i64>,
    /// A stake in kind.
    pub stake_note: String,
    /// When they were invited or asked to join.
    pub created_at: DateTime<Utc>,
    /// When an unanswered invite or join request lapses.
    pub expires_at: Option<DateTime<Utc>>,
    /// A join request's approvers.
    pub approvals: Vec<ApprovalView>,
}

/// A claim being decided: pending (waiting for confirmations) or disputed.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ClaimView {
    pub id: i64,
    /// The wager (participant id) it is about; `null`: the whole pact.
    pub wager_id: Option<i64>,
    pub proposed_by: Person,
    pub result: Outcome,
    pub state: ClaimState,
    pub created_at: DateTime<Utc>,
    pub decided_at: Option<DateTime<Utc>>,
    /// Who has confirmed it so far (user ids).
    pub confirmed_by: Vec<i64>,
    /// The Sejmik vote it went to, if disputed and escalated.
    pub ruling_poll_id: Option<i64>,
}

/// A picture on a pact.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct AttachmentView {
    pub id: i64,
    pub image: Image,
    pub width: i32,
    pub height: i32,
    pub caption: String,
    pub uploaded_by: Person,
    pub created_at: DateTime<Utc>,
}

/// A pact with everything about it.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PactDetail {
    #[serde(flatten)]
    pub summary: PactSummary,
    pub notes: String,
    /// Group bets: the sides to pick from.
    pub sides: Vec<String>,
    /// Once resolved or void: how it ended.
    pub outcome: Option<PactOutcome>,
    /// The host first, then in the order they came.
    pub participants: Vec<ParticipantView>,
    /// Claims being decided (settled and replaced ones are history).
    pub claims: Vec<ClaimView>,
    pub attachments: Vec<AttachmentView>,
    /// Resolutions: the Sejmik vote judging it now.
    pub judgment_poll_id: Option<i64>,
    pub actions: Actions,
}

/// Which pacts to list.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    pub status: Option<Status>,
    /// Only pacts you are or were part of.
    pub mine: bool,
    pub cursor: Option<String>,
}

fn parse<T>(value: Option<T>, what: &str, raw: &str) -> AppResult<T> {
    value.ok_or_else(|| anyhow::anyhow!("unknown {what} {raw:?}").into())
}

fn someone(id: i64) -> Person {
    Person {
        id,
        username: String::new(),
        avatar: None,
    }
}

/// Pacts newest first.
pub async fn list(ctx: &Context, actor: &Actor, filter: &ListFilter) -> AppResult<PactPage> {
    let after = filter.cursor.as_deref().map(Cursor::decode).transpose()?;
    let rows = sqlx::query!(
        r#"select k.id, k.kind, k.title, k.condition, k.due_at, k.status, k.creator_id,
                  k.created_at, k.resolved_at,
                  (select count(*) from pact_participants a
                   where a.pact_id = k.id and a.state = 'active') as "active_count!",
                  m.id as "my_id?", m.role as "my_role?", m.state as "my_state?"
           from pacts k
           left join pact_participants m on m.pact_id = k.id and m.user_id = $1
           where ($2::text is null or k.status = $2)
             and (not $3 or m.id is not null)
             and ($4::timestamptz is null or (k.created_at, k.id) < ($4, $5))
           order by k.created_at desc, k.id desc
           limit $6"#,
        actor.user_id,
        filter.status.map(Status::as_str),
        filter.mine,
        after.map(|c| c.at),
        after.map_or(0, |c| c.id),
        i64::try_from(PAGE_SIZE + 1).unwrap_or(i64::MAX),
    )
    .fetch_all(&ctx.db)
    .await?;
    let (rows, next_cursor) = page(rows, PAGE_SIZE, |r| Cursor {
        at: r.created_at,
        id: r.id,
    });
    let creators: Vec<i64> = rows
        .iter()
        .map(|r| r.creator_id)
        .collect::<BTreeSet<_>>()
        .into_iter()
        .collect();
    let people = persons(ctx, &creators).await?;
    let items = rows
        .into_iter()
        .map(|r| {
            let my = match (r.my_id, r.my_role, r.my_state) {
                (Some(participant_id), Some(role), Some(state)) => Some(MyPart {
                    participant_id,
                    role: parse(Role::parse(&role), "role", &role)?,
                    state: parse(ParticipantState::parse(&state), "state", &state)?,
                }),
                _ => None,
            };
            Ok(PactSummary {
                id: r.id,
                kind: parse(PactKind::parse(&r.kind), "kind", &r.kind)?,
                status: parse(Status::parse(&r.status), "status", &r.status)?,
                title: r.title,
                condition: r.condition,
                due_at: r.due_at,
                creator: people
                    .get(&r.creator_id)
                    .cloned()
                    .unwrap_or_else(|| someone(r.creator_id)),
                created_at: r.created_at,
                resolved_at: r.resolved_at,
                active_count: r.active_count,
                my,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    Ok(PactPage { items, next_cursor })
}

pub async fn detail(ctx: &Context, actor: &Actor, id: i64) -> AppResult<PactDetail> {
    let mut conn = ctx.db.acquire().await?;
    let (row, pact) = load(&mut conn, id).await?;
    let attachments = sqlx::query!(
        "select id, uploaded_by, image_key, width, height, caption, created_at
         from pact_attachments where pact_id = $1 order by created_at, id",
        id,
    )
    .fetch_all(&mut *conn)
    .await?;
    drop(conn);

    let mut ids: BTreeSet<i64> = pact.participants.iter().map(|p| p.user_id).collect();
    ids.insert(pact.creator_id);
    ids.extend(pact.claims.iter().map(|c| c.proposed_by));
    ids.extend(attachments.iter().map(|a| a.uploaded_by));
    let people = persons(ctx, &ids.into_iter().collect::<Vec<_>>()).await?;
    let person = |id: i64| people.get(&id).cloned().unwrap_or_else(|| someone(id));
    let now = ctx.clock.now();

    let mine = pact.row_of(actor.user_id);
    let summary = PactSummary {
        id: pact.id,
        kind: pact.kind,
        title: pact.title.clone(),
        condition: row.condition,
        due_at: pact.due_at,
        status: pact.status,
        creator: person(pact.creator_id),
        created_at: row.created_at,
        resolved_at: row.resolved_at,
        active_count: i64::try_from(pact.active_ids().len()).unwrap_or(i64::MAX),
        my: mine.map(|p| MyPart {
            participant_id: p.id,
            role: p.role,
            state: p.state,
        }),
    };
    let participants = pact
        .participants
        .iter()
        .map(|p| ParticipantView {
            id: p.id,
            user: person(p.user_id),
            role: p.role,
            state: p.state,
            side: p.terms.side.clone(),
            stake_amount: p.terms.stake_amount,
            stake_note: p.terms.stake_note.clone(),
            created_at: p.created_at,
            expires_at: p.expires_at,
            approvals: p
                .consents
                .iter()
                .map(|c| ApprovalView {
                    approver_id: c.approver_id,
                    approved: c.approved,
                })
                .collect(),
        })
        .collect();
    let claims = pact
        .claims
        .iter()
        .filter(|c| c.state.live())
        .map(|c| ClaimView {
            id: c.id,
            wager_id: c.wager_id,
            proposed_by: person(c.proposed_by),
            result: c.result.clone(),
            state: c.state,
            created_at: c.created_at,
            decided_at: c.decided_at,
            confirmed_by: c.confirmed_by.clone(),
            ruling_poll_id: c.ruling.map(|r| r.poll_id),
        })
        .collect();
    let attachments = attachments
        .into_iter()
        .map(|a| AttachmentView {
            id: a.id,
            image: ctx.media.image(&a.image_key, now),
            width: a.width,
            height: a.height,
            caption: a.caption,
            uploaded_by: person(a.uploaded_by),
            created_at: a.created_at,
        })
        .collect();
    Ok(PactDetail {
        summary,
        notes: row.notes,
        sides: pact.config.sides.clone(),
        outcome: pact.outcome(),
        participants,
        claims,
        attachments,
        judgment_poll_id: pact.judgment_poll_id,
        actions: policy::actions(&pact, actor.user_id, now),
    })
}
