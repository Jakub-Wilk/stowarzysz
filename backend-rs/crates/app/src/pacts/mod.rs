//! Umowy: creating pacts, answering invites, joining, claiming outcomes, disputes, judgments and
//! deadlines. Every change runs in one transaction with the pact row locked, against the whole
//! pact loaded as [`domain::pacts::Pact`], and is authorised by [`domain::pacts::policy`].
//!
//! Money never lives here: settling hands the debts to [`crate::ledger::record_debts`]. Pacts
//! call votes through [`crate::polls`]; what a closed vote decides comes back through
//! [`crate::orchestration`] as [`apply_ruling`] / [`apply_judgment`].

pub mod deadlines;
mod services;
mod views;

use std::collections::HashMap;

use chrono::{DateTime, Utc};
use domain::pacts::policy::Denial;
use domain::pacts::terms::PactConfig;
use domain::pacts::{
    Claim, ClaimState, Consent, Outcome, Pact, PactKind, Participant, ParticipantState, Role,
    Ruling, Status, Terms,
};
use sqlx::PgConnection;

use crate::db::Uow;
use crate::{AppError, AppResult};

pub(crate) use services::{Applied, apply_judgment, apply_ruling};
pub use services::{
    ClaimRequest, Invitee, NewPact, Respond, add_attachment, call_judgment, cancel, claim, confirm,
    create, decide, delete_attachment, dispute, escalate, join, respond, withdraw,
};
pub use views::{
    AttachmentView, ClaimView, ListFilter, MyPart, PactDetail, PactPage, PactSummary,
    ParticipantView, detail, list,
};

/// The parts of a pact's row the aggregate does not need.
#[derive(Debug, Clone)]
pub(crate) struct PactRow {
    pub condition: String,
    pub notes: String,
    pub created_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub next_nudge_at: Option<DateTime<Utc>>,
}

fn corrupt(what: &str, id: i64, value: &str) -> AppError {
    anyhow::anyhow!("{what} {id} has unknown value {value:?}").into()
}

/// Lock pact `id` for the rest of `uow` and load it (404 if it does not exist).
pub(crate) async fn lock(uow: &mut Uow, id: i64) -> AppResult<(PactRow, Pact)> {
    sqlx::query_scalar!("select id from pacts where id = $1 for update", id)
        .fetch_optional(uow.conn())
        .await?
        .ok_or(AppError::NotFound)?;
    load(uow.conn(), id).await
}

/// Pact `id` with its participants, join consents, claims (with their confirmations and
/// rulings) and number of pictures.
pub(crate) async fn load(conn: &mut PgConnection, id: i64) -> AppResult<(PactRow, Pact)> {
    let row = sqlx::query!(
        "select id, kind, title, condition, notes, due_at, config, status, creator_id, created_at,
                resolved_at, next_nudge_at, judgment_poll_id
         from pacts where id = $1",
        id,
    )
    .fetch_optional(&mut *conn)
    .await?
    .ok_or(AppError::NotFound)?;
    let participants = sqlx::query!(
        "select id, user_id, role, state, side, stake_amount, stake_note, created_at, expires_at
         from pact_participants where pact_id = $1 order by id",
        id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let consents = sqlx::query!(
        "select c.participant_id, c.approver_id, c.approved
         from pact_join_consents c join pact_participants p on p.id = c.participant_id
         where p.pact_id = $1 order by c.approver_id",
        id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let claims = sqlx::query!(
        r#"select c.id, c.wager_id, c.proposed_by, c.result, c.state, c.created_at, c.decided_at,
                  r.id as "ruling_id?", r.status as "ruling_status?"
           from pact_claims c left join polls r on r.subject_claim_id = c.id
           where c.pact_id = $1 order by c.id"#,
        id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let confirmations = sqlx::query!(
        "select f.claim_id, f.user_id
         from pact_claim_confirmations f join pact_claims c on c.id = f.claim_id
         where c.pact_id = $1 order by f.created_at, f.user_id",
        id,
    )
    .fetch_all(&mut *conn)
    .await?;
    let attachments = sqlx::query_scalar!(
        r#"select count(*) as "count!" from pact_attachments where pact_id = $1"#,
        id
    )
    .fetch_one(&mut *conn)
    .await?;

    let mut consents_of: HashMap<i64, Vec<Consent>> = HashMap::new();
    for c in consents {
        consents_of
            .entry(c.participant_id)
            .or_default()
            .push(Consent {
                approver_id: c.approver_id,
                approved: c.approved,
            });
    }
    let mut confirmed_by: HashMap<i64, Vec<i64>> = HashMap::new();
    for f in confirmations {
        confirmed_by.entry(f.claim_id).or_default().push(f.user_id);
    }
    let participants = participants
        .into_iter()
        .map(|p| {
            Ok(Participant {
                role: Role::parse(&p.role).ok_or_else(|| corrupt("participant", p.id, &p.role))?,
                state: ParticipantState::parse(&p.state)
                    .ok_or_else(|| corrupt("participant", p.id, &p.state))?,
                terms: Terms {
                    stake_amount: p.stake_amount,
                    stake_note: p.stake_note,
                    side: p.side,
                },
                consents: consents_of.remove(&p.id).unwrap_or_default(),
                id: p.id,
                user_id: p.user_id,
                created_at: p.created_at,
                expires_at: p.expires_at,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let claims = claims
        .into_iter()
        .map(|c| {
            Ok(Claim {
                result: Outcome::from_json(&c.result).map_err(anyhow::Error::from)?,
                state: ClaimState::parse(&c.state)
                    .ok_or_else(|| corrupt("claim", c.id, &c.state))?,
                confirmed_by: confirmed_by.remove(&c.id).unwrap_or_default(),
                ruling: c.ruling_id.map(|poll_id| Ruling {
                    poll_id,
                    open: c.ruling_status.as_deref() == Some("open"),
                }),
                id: c.id,
                wager_id: c.wager_id,
                proposed_by: c.proposed_by,
                created_at: c.created_at,
                decided_at: c.decided_at,
            })
        })
        .collect::<AppResult<Vec<_>>>()?;
    let pact = Pact {
        id: row.id,
        kind: PactKind::parse(&row.kind).ok_or_else(|| corrupt("pact", row.id, &row.kind))?,
        status: Status::parse(&row.status).ok_or_else(|| corrupt("pact", row.id, &row.status))?,
        creator_id: row.creator_id,
        title: row.title,
        due_at: row.due_at,
        config: PactConfig::from_json(&row.config).map_err(anyhow::Error::from)?,
        judgment_poll_id: row.judgment_poll_id,
        participants,
        claims,
        attachments: usize::try_from(attachments).unwrap_or(usize::MAX),
    };
    let extra = PactRow {
        condition: row.condition,
        notes: row.notes,
        created_at: row.created_at,
        resolved_at: row.resolved_at,
        next_nudge_at: row.next_nudge_at,
    };
    Ok((extra, pact))
}

/// Map a policy refusal to the API error.
pub(crate) fn denied(denial: Denial) -> AppError {
    match denial {
        Denial::NotFound => AppError::NotFound,
        Denial::Conflict { code, message } => AppError::conflict(code, message),
        Denial::Forbidden { code, message } => AppError::forbidden(code, message),
        Denial::Invalid { code, message } => domain::DomainError::new(code, message).into(),
    }
}
