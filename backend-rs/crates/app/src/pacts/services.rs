//! Changing pacts. Each function is one transaction with the pact row locked; what the caller
//! may do is decided by [`policy::check`] on the loaded pact. The in-memory pact is kept in
//! step with every write, so later rules in the same transaction see the new state.

use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use domain::DomainError;
use domain::ledger::money::{BASE_CURRENCY, format_amount};
use domain::pacts::policy::{self, Action};
use domain::pacts::terms::{self, TermsInput};
use domain::pacts::{
    CAPTION_MAX, CONDITION_MAX, Claim, ClaimState, INVITE_TTL, NOTES_MAX, Outcome, Pact, PactKind,
    ParticipantState, Status, TITLE_MAX, Terms, settle,
};
use domain::polls::{PollConfig, PollKind};
use serde::Deserialize;
use serde_json::Value;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{denied, lock};
use crate::auth::Actor;
use crate::clock::local_today;
use crate::db::Uow;
use crate::events::{Audience, Event};
use crate::listener::Wake;
use crate::media::{self, Purpose};
use crate::polls::{Draft, insert as insert_poll};
use crate::push::Notification;
use crate::{AppError, AppResult, Context};

/// A new pact. What applies depends on `kind`: a bet invites opponents with their stakes, a
/// group bet takes `sides` and everyone's side (yours in `host`), a resolution invites nobody
/// and needs `due_at`, a shared goal invites people and needs `due_at`.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct NewPact {
    pub kind: PactKind,
    /// Up to 200 characters.
    pub title: String,
    /// What has to happen (in a bet: for the host to win).
    pub condition: String,
    #[serde(default)]
    pub notes: String,
    /// Must be in the future; required for resolutions and shared goals. Nobody can call a
    /// result before it (only call it off).
    pub due_at: Option<DateTime<Utc>>,
    /// Group bets: 2 to 6 sides (default `tak`, `nie`).
    pub sides: Option<Vec<String>>,
    /// Your own side and stake (group bets).
    #[serde(default)]
    pub host: TermsInput,
    /// Who to invite, with the terms you propose for them (a bet: their stake, required).
    #[serde(default)]
    pub opponents: Vec<Invitee>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct Invitee {
    pub user_id: i64,
    #[serde(flatten)]
    pub terms: TermsInput,
}

/// An answer to an invite.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct Respond {
    pub accept: bool,
    /// Group bets: your side and stake; what you leave out keeps the creator's proposal. In a
    /// bet the stake is the one you were invited with.
    #[serde(flatten)]
    pub terms: TermsInput,
}

/// A claim of how it ended.
#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct ClaimRequest {
    /// Bets: the wager (participant id) the claim is about; ignored by the other kinds.
    pub wager_id: Option<i64>,
    /// `{"winner": "host" | "opponent" | "draw"}` (bet), `{"winner": <side> | "draw"}` (group
    /// bet), `{"done": bool}` (shared goal), or `{"void": true}` to call it off.
    #[schema(value_type = Outcome)]
    pub result: Value,
}

/// What a closed vote did to the pact it decides.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Applied {
    Yes,
    /// Nothing to apply (overruled, or the vote decided nothing).
    No,
    /// It could not be applied, and why.
    Refused(String),
}

/// `field.inner` for an error about one part of the request.
fn under(prefix: String) -> impl Fn(DomainError) -> DomainError {
    move |error| {
        let field = match &error.field {
            Some(inner) => format!("{prefix}.{inner}"),
            None => prefix.clone(),
        };
        error.on(field)
    }
}

fn text(
    raw: &str,
    field: &'static str,
    max: usize,
    missing: Option<&'static str>,
    too_long: String,
) -> Result<String, DomainError> {
    let text = raw.trim();
    if let Some(missing) = missing.filter(|_| text.is_empty()) {
        return Err(DomainError::new("required", missing).on(field));
    }
    if text.chars().count() > max {
        return Err(DomainError::new("too_long", too_long).on(field));
    }
    Ok(text.to_owned())
}

/// Create a pact for `actor`; returns its id.
pub async fn create(ctx: &Context, actor: &Actor, request: NewPact) -> AppResult<i64> {
    let now = ctx.clock.now();
    let kind = request.kind;
    let rules = kind.rules();
    let title = text(
        &request.title,
        "title",
        TITLE_MAX,
        Some("Podaj tytuł umowy."),
        format!("Tytuł może mieć najwyżej {TITLE_MAX} znaków."),
    )?;
    let condition = text(
        &request.condition,
        "condition",
        CONDITION_MAX,
        Some("Podaj warunek umowy."),
        format!("Warunek może mieć najwyżej {CONDITION_MAX} znaków."),
    )?;
    let notes = text(
        &request.notes,
        "notes",
        NOTES_MAX,
        None,
        format!("Notatki mogą mieć najwyżej {NOTES_MAX} znaków."),
    )?;
    if rules.judged_by_everyone && !request.opponents.is_empty() {
        return Err(DomainError::new(
            "no_invitees",
            "Tego rodzaju umowy oceniają wszyscy posłowie, nie zapraszaj nikogo.",
        )
        .on("opponents")
        .into());
    }
    match request.due_at {
        None if rules.needs_due_date => {
            return Err(DomainError::new("due_required", "Podaj termin.")
                .on("due_at")
                .into());
        }
        Some(due) if due <= now => {
            return Err(
                DomainError::new("due_in_past", "Termin musi być w przyszłości.")
                    .on("due_at")
                    .into(),
            );
        }
        _ => {}
    }
    let config = terms::config(kind, request.sides.as_deref())?;
    let host = request
        .host
        .clean()
        .and_then(|t| terms::validate(kind, t, &config, true, true))
        .map_err(under("host".into()))?;
    let mut invitees: Vec<(i64, Terms)> = Vec::with_capacity(request.opponents.len());
    for (i, invitee) in request.opponents.iter().enumerate() {
        let terms = invitee
            .terms
            .clean()
            .and_then(|t| terms::validate(kind, t, &config, false, false))
            .map_err(under(format!("opponents.{i}")))?;
        invitees.push((invitee.user_id, terms));
    }
    let ids: Vec<i64> = invitees.iter().map(|(id, _)| *id).collect();
    let distinct: BTreeSet<i64> = ids.iter().copied().collect();
    if distinct.len() != ids.len() {
        return Err(DomainError::new(
            "invited_twice",
            "Ta sama osoba jest zaproszona więcej niż raz.",
        )
        .on("opponents")
        .into());
    }
    if distinct.contains(&actor.user_id) {
        return Err(
            DomainError::new("self_invite", "Nie możesz zaprosić samego siebie.")
                .on("opponents")
                .into(),
        );
    }

    let mut uow = Uow::begin(&ctx.db).await?;
    let members = sqlx::query_scalar!(
        r#"select count(*) as "count!" from users
           where id = any($1) and is_active and password_hash is not null"#,
        &ids,
    )
    .fetch_one(uow.conn())
    .await?;
    if usize::try_from(members).ok() != Some(ids.len()) {
        return Err(DomainError::new(
            "unknown_invitee",
            "Któraś z zaproszonych osób nie istnieje albo jest nieaktywna.",
        )
        .on("opponents")
        .into());
    }
    let status = if rules.judged_by_everyone {
        Status::Active
    } else {
        Status::Proposed
    };
    let id = sqlx::query_scalar!(
        "insert into pacts (creator_id, kind, title, condition, notes, due_at, config, status,
                            created_at, next_nudge_at)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $6) returning id",
        actor.user_id,
        kind.as_str(),
        title,
        condition,
        notes,
        request.due_at,
        config.to_json(),
        status.as_str(),
        now,
    )
    .fetch_one(uow.conn())
    .await?;
    sqlx::query!(
        "insert into pact_participants (pact_id, user_id, role, state, side, stake_amount, stake_note, created_at)
         values ($1, $2, 'host', 'active', $3, $4, $5, $6)",
        id,
        actor.user_id,
        host.side,
        host.stake_amount,
        host.stake_note,
        now,
    )
    .execute(uow.conn())
    .await?;
    let stakes: Vec<Option<i64>> = invitees.iter().map(|(_, t)| t.stake_amount).collect();
    let notes: Vec<String> = invitees.iter().map(|(_, t)| t.stake_note.clone()).collect();
    let sides: Vec<String> = invitees.iter().map(|(_, t)| t.side.clone()).collect();
    sqlx::query!(
        "insert into pact_participants (pact_id, user_id, state, stake_amount, stake_note, side, created_at, expires_at)
         select $1, user_id, 'invited', stake_amount, stake_note, side, $6, $7
         from unnest($2::bigint[], $3::bigint[], $4::text[], $5::text[])
              as t(user_id, stake_amount, stake_note, side)",
        id,
        &ids,
        &stakes as &[Option<i64>],
        &notes,
        &sides,
        now,
        now + INVITE_TTL,
    )
    .execute(uow.conn())
    .await?;
    uow.publish(Audience::All, Event::PactCreated { pact_id: id })
        .await?;
    let notification = Notification {
        title: "Nowa umowa".into(),
        body: format!("{} zaprasza Cię: {title}", actor.username),
        url: url(id),
    };
    ctx.push.notify(&mut uow, &ids, &notification).await?;
    // New due times (the deadline, the invites' expiry) may be earlier than anything the
    // scheduler is waiting for.
    uow.wake(Wake::Jobs).await?;
    uow.commit().await?;
    Ok(id)
}

fn url(pact_id: i64) -> String {
    format!("/pacts/{pact_id}")
}

/// Tell everyone the pact changed (ids only, clients refetch: everyone can read every pact)
/// and push `title` / `body` to `to`.
async fn announce(
    uow: &mut Uow,
    ctx: &Context,
    pact: &Pact,
    to: &[i64],
    title: &str,
    body: String,
) -> AppResult<()> {
    uow.publish(Audience::All, Event::PactUpdated { pact_id: pact.id })
        .await?;
    if !to.is_empty() {
        let notification = Notification {
            title: title.into(),
            body,
            url: url(pact.id),
        };
        ctx.push.notify(uow, to, &notification).await?;
    }
    Ok(())
}

async fn touched(uow: &mut Uow, pact: &Pact) -> AppResult<()> {
    uow.publish(Audience::All, Event::PactUpdated { pact_id: pact.id })
        .await
}

/// Lock and load pact `id` and check that `actor` may do `action` now.
async fn guarded(
    uow: &mut Uow,
    ctx: &Context,
    actor: &Actor,
    id: i64,
    action: Action,
) -> AppResult<Pact> {
    let (_, pact) = lock(uow, id).await?;
    policy::check(action, &pact, actor.user_id, ctx.clock.now()).map_err(denied)?;
    Ok(pact)
}

/// Move participant `id` to `state` (with new terms if given). Leaving the waiting states
/// clears the expiry.
async fn set_state(
    uow: &mut Uow,
    pact: &mut Pact,
    id: i64,
    state: ParticipantState,
    terms: Option<Terms>,
) -> AppResult<()> {
    let expires = state.waiting();
    match &terms {
        Some(terms) => {
            sqlx::query!(
                "update pact_participants
                 set state = $2, side = $3, stake_amount = $4, stake_note = $5,
                     expires_at = case when $6 then expires_at end
                 where id = $1",
                id,
                state.as_str(),
                terms.side,
                terms.stake_amount,
                terms.stake_note,
                expires,
            )
            .execute(uow.conn())
            .await?;
        }
        None => {
            sqlx::query!(
                "update pact_participants set state = $2, expires_at = case when $3 then expires_at end
                 where id = $1",
                id,
                state.as_str(),
                expires,
            )
            .execute(uow.conn())
            .await?;
        }
    }
    if let Some(row) = pact.participants.iter_mut().find(|p| p.id == id) {
        row.state = state;
        if !expires {
            row.expires_at = None;
        }
        if let Some(terms) = terms {
            row.terms = terms;
        }
    }
    Ok(())
}

pub(super) async fn set_status(uow: &mut Uow, pact: &mut Pact, status: Status) -> AppResult<()> {
    sqlx::query!(
        "update pacts set status = $2, next_nudge_at = case when $3 then next_nudge_at end
         where id = $1",
        pact.id,
        status.as_str(),
        !status.finished(),
    )
    .execute(uow.conn())
    .await?;
    pact.status = status;
    Ok(())
}

/// Whoever is still waiting for an answer is out (the pact is over or called off).
pub(super) async fn withdraw_waiting(uow: &mut Uow, pact: &mut Pact) -> AppResult<()> {
    sqlx::query!(
        "update pact_participants set state = 'withdrawn', expires_at = null
         where pact_id = $1 and state in ('invited', 'requested')",
        pact.id,
    )
    .execute(uow.conn())
    .await?;
    sqlx::query!(
        "delete from pact_join_consents c using pact_participants p
         where c.participant_id = p.id and p.pact_id = $1 and c.approved is null",
        pact.id,
    )
    .execute(uow.conn())
    .await?;
    for row in &mut pact.participants {
        if row.state.waiting() {
            row.state = ParticipantState::Withdrawn;
            row.expires_at = None;
            row.consents.retain(|c| c.approved.is_some());
        }
    }
    Ok(())
}

/// After someone came in or dropped out: start a pact that has what it needs, and resolve a
/// bet whose last running wager just went.
pub(super) async fn refresh(uow: &mut Uow, pact: &mut Pact, now: DateTime<Utc>) -> AppResult<()> {
    if pact.ready_to_start() {
        set_status(uow, pact, Status::Active).await?;
    }
    if pact.status.deciding()
        && pact.rules().wager_based
        && pact.wagers_done()
        && pact.confirmed_claims().next().is_some()
    {
        finish(uow, pact, now).await?;
    }
    Ok(())
}

/// Resolve (or void) a pact whose claims are all settled.
async fn finish(uow: &mut Uow, pact: &mut Pact, now: DateTime<Utc>) -> AppResult<()> {
    withdraw_waiting(uow, pact).await?;
    let status = pact.final_status();
    set_status(uow, pact, status).await?;
    sqlx::query!(
        "update pacts set resolved_at = $2 where id = $1",
        pact.id,
        now
    )
    .execute(uow.conn())
    .await?;
    Ok(())
}

/// Accept or decline an invite. Group bets take the invitee's side and stake here.
pub async fn respond(ctx: &Context, actor: &Actor, id: i64, request: &Respond) -> AppResult<()> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let mut pact = guarded(&mut uow, ctx, actor, id, Action::Respond).await?;
    let mine = pact
        .row_of(actor.user_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    if request.accept {
        let terms = if pact.rules().wager_based {
            mine.terms.clone() // the host fixed each opponent's stake
        } else {
            let merged = request.terms.over(&mine.terms)?;
            terms::validate(pact.kind, merged, &pact.config, false, true)?
        };
        set_state(
            &mut uow,
            &mut pact,
            mine.id,
            ParticipantState::Active,
            Some(terms),
        )
        .await?;
    } else {
        set_state(
            &mut uow,
            &mut pact,
            mine.id,
            ParticipantState::Declined,
            None,
        )
        .await?;
    }
    refresh(&mut uow, &mut pact, now).await?;
    let verb = if request.accept {
        "przyjmuje"
    } else {
        "odrzuca"
    };
    let body = format!("{} {verb} umowę: {}", actor.username, pact.title);
    announce(
        &mut uow,
        ctx,
        &pact,
        &[pact.creator_id],
        "Umowa: odpowiedź",
        body,
    )
    .await?;
    uow.commit().await
}

/// The creator calls off a pact that has not started.
pub async fn cancel(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let mut pact = guarded(&mut uow, ctx, actor, id, Action::Cancel).await?;
    let told: Vec<i64> = pact
        .participants
        .iter()
        .filter(|p| p.user_id != actor.user_id)
        .filter(|p| p.state.waiting() || p.state == ParticipantState::Active)
        .map(|p| p.user_id)
        .collect();
    withdraw_waiting(&mut uow, &mut pact).await?;
    set_status(&mut uow, &mut pact, Status::Cancelled).await?;
    let body = format!("{} anuluje umowę: {}", actor.username, pact.title);
    announce(&mut uow, ctx, &pact, &told, "Umowa anulowana", body).await?;
    uow.commit().await
}

/// Ask to join. Who has to approve depends on the kind.
pub async fn join(ctx: &Context, actor: &Actor, id: i64, input: &TermsInput) -> AppResult<()> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let pact = guarded(&mut uow, ctx, actor, id, Action::Join).await?;
    let terms = terms::validate(pact.kind, input.clean()?, &pact.config, false, true)?;
    let expires = now + INVITE_TTL;
    let participant_id = match pact.row_of(actor.user_id) {
        Some(earlier) => {
            sqlx::query!(
                "update pact_participants
                 set role = 'opponent', state = 'requested', side = $2, stake_amount = $3,
                     stake_note = $4, created_at = $5, expires_at = $6
                 where id = $1",
                earlier.id,
                terms.side,
                terms.stake_amount,
                terms.stake_note,
                now,
                expires,
            )
            .execute(uow.conn())
            .await?;
            sqlx::query!(
                "delete from pact_join_consents where participant_id = $1",
                earlier.id
            )
            .execute(uow.conn())
            .await?;
            earlier.id
        }
        None => {
            sqlx::query_scalar!(
                "insert into pact_participants (pact_id, user_id, state, side, stake_amount, stake_note, created_at, expires_at)
                 values ($1, $2, 'requested', $3, $4, $5, $6, $7) returning id",
                id,
                actor.user_id,
                terms.side,
                terms.stake_amount,
                terms.stake_note,
                now,
                expires,
            )
            .fetch_one(uow.conn())
            .await?
        }
    };
    let approvers: Vec<i64> = pact
        .kind
        .join_approvers(pact.creator_id, &pact.active_ids())
        .into_iter()
        .collect();
    sqlx::query!(
        "insert into pact_join_consents (participant_id, approver_id) select $1, unnest($2::bigint[])",
        participant_id,
        &approvers,
    )
    .execute(uow.conn())
    .await?;
    let body = format!("{} chce dołączyć do umowy: {}", actor.username, pact.title);
    announce(
        &mut uow,
        ctx,
        &pact,
        &approvers,
        "Prośba o dołączenie",
        body,
    )
    .await?;
    uow.wake(Wake::Jobs).await?; // the request's expiry
    uow.commit().await
}

/// Take back one's join request.
pub async fn withdraw(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let mut pact = guarded(&mut uow, ctx, actor, id, Action::Withdraw).await?;
    let mine = pact
        .row_of(actor.user_id)
        .map(|p| p.id)
        .ok_or(AppError::NotFound)?;
    set_state(&mut uow, &mut pact, mine, ParticipantState::Withdrawn, None).await?;
    sqlx::query!(
        "delete from pact_join_consents where participant_id = $1",
        mine
    )
    .execute(uow.conn())
    .await?;
    touched(&mut uow, &pact).await?;
    uow.commit().await
}

/// Approve or reject participant `participant_id`'s join request. One "no" rejects it; it
/// is accepted once every approver has said yes.
pub async fn decide(
    ctx: &Context,
    actor: &Actor,
    id: i64,
    participant_id: i64,
    approve: bool,
) -> AppResult<()> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let mut pact = guarded(&mut uow, ctx, actor, id, Action::Decide(participant_id)).await?;
    sqlx::query!(
        "update pact_join_consents set approved = $3 where participant_id = $1 and approver_id = $2",
        participant_id,
        actor.user_id,
        approve,
    )
    .execute(uow.conn())
    .await?;
    let requester = pact
        .participant(participant_id)
        .cloned()
        .ok_or(AppError::NotFound)?;
    if !approve {
        set_state(
            &mut uow,
            &mut pact,
            participant_id,
            ParticipantState::Rejected,
            None,
        )
        .await?;
        sqlx::query!(
            "delete from pact_join_consents where participant_id = $1 and approved is null",
            participant_id,
        )
        .execute(uow.conn())
        .await?;
        let body = format!(
            "{} odrzuca Twoją prośbę o dołączenie: {}",
            actor.username, pact.title
        );
        announce(
            &mut uow,
            ctx,
            &pact,
            &[requester.user_id],
            "Prośba odrzucona",
            body,
        )
        .await?;
    } else if requester
        .consents
        .iter()
        .all(|c| c.approver_id == actor.user_id || c.approved == Some(true))
    {
        set_state(
            &mut uow,
            &mut pact,
            participant_id,
            ParticipantState::Active,
            None,
        )
        .await?;
        refresh(&mut uow, &mut pact, now).await?;
        let body = format!("Przyjęto Cię do umowy: {}", pact.title);
        announce(
            &mut uow,
            ctx,
            &pact,
            &[requester.user_id],
            "Prośba przyjęta",
            body,
        )
        .await?;
    } else {
        touched(&mut uow, &pact).await?;
    }
    uow.commit().await
}

/// Claim how a wager (bet) or the whole pact ended; the other side then confirms. A newer
/// claim replaces a pending or disputed one. Returns the claim's id.
pub async fn claim(
    ctx: &Context,
    actor: &Actor,
    id: i64,
    request: &ClaimRequest,
) -> AppResult<i64> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let (_, mut pact) = lock(&mut uow, id).await?;
    let scope = if pact.rules().wager_based {
        request.wager_id
    } else {
        None
    };
    policy::check(Action::Claim(scope), &pact, actor.user_id, now).map_err(denied)?;
    let outcome = Outcome::parse(pact.kind, &pact.config, &request.result)?;
    policy::claim_timing(&pact, &outcome, now).map_err(denied)?;
    sqlx::query!(
        "update pact_claims set state = 'superseded'
         where pact_id = $1 and wager_id is not distinct from $2 and state in ('pending', 'disputed')",
        id,
        scope,
    )
    .execute(uow.conn())
    .await?;
    for earlier in &mut pact.claims {
        if earlier.wager_id == scope && earlier.state.live() {
            earlier.state = ClaimState::Superseded;
        }
    }
    let claim_id = insert_claim(&mut uow, &mut pact, scope, actor.user_id, outcome, now).await?;
    let claim = pact.claim(claim_id).cloned().ok_or(AppError::NotFound)?;
    let confirmers: Vec<i64> = pact.confirmers(&claim).into_iter().collect();
    if confirmers.is_empty() {
        settle_claim(&mut uow, ctx, &mut pact, claim_id, now).await?;
    } else {
        let body = format!("{} podaje wynik: {}", actor.username, pact.title);
        announce(
            &mut uow,
            ctx,
            &pact,
            &confirmers,
            "Potwierdź wynik umowy",
            body,
        )
        .await?;
    }
    uow.commit().await?;
    Ok(claim_id)
}

async fn insert_claim(
    uow: &mut Uow,
    pact: &mut Pact,
    wager_id: Option<i64>,
    by: i64,
    result: Outcome,
    now: DateTime<Utc>,
) -> AppResult<i64> {
    let id = sqlx::query_scalar!(
        "insert into pact_claims (pact_id, wager_id, proposed_by, result, created_at)
         values ($1, $2, $3, $4, $5) returning id",
        pact.id,
        wager_id,
        by,
        result.to_json(),
        now,
    )
    .fetch_one(uow.conn())
    .await?;
    pact.claims.push(Claim {
        id,
        wager_id,
        proposed_by: by,
        result,
        state: ClaimState::Pending,
        confirmed_by: Vec::new(),
        ruling: None,
        created_at: now,
        decided_at: None,
    });
    Ok(id)
}

/// A required party agrees with claim `claim_id`. Once all of them have, it is settled.
pub async fn confirm(ctx: &Context, actor: &Actor, id: i64, claim_id: i64) -> AppResult<()> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let mut pact = guarded(&mut uow, ctx, actor, id, Action::Confirm(claim_id)).await?;
    sqlx::query!(
        "insert into pact_claim_confirmations (claim_id, user_id, created_at) values ($1, $2, $3)",
        claim_id,
        actor.user_id,
        now,
    )
    .execute(uow.conn())
    .await?;
    let claim = pact
        .claims
        .iter_mut()
        .find(|c| c.id == claim_id)
        .ok_or(AppError::NotFound)?;
    claim.confirmed_by.push(actor.user_id);
    let claim = claim.clone();
    if pact
        .confirmers(&claim)
        .iter()
        .all(|user| claim.confirmed_by.contains(user))
    {
        settle_claim(&mut uow, ctx, &mut pact, claim_id, now).await?;
    } else {
        touched(&mut uow, &pact).await?;
    }
    uow.commit().await
}

/// A required party disagrees. The claim is parked: claim again, or put it to a vote.
pub async fn dispute(ctx: &Context, actor: &Actor, id: i64, claim_id: i64) -> AppResult<()> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let pact = guarded(&mut uow, ctx, actor, id, Action::Dispute(claim_id)).await?;
    sqlx::query!(
        "update pact_claims set state = 'disputed', decided_at = $2 where id = $1",
        claim_id,
        now,
    )
    .execute(uow.conn())
    .await?;
    let proposer = pact.claim(claim_id).map(|c| c.proposed_by);
    let body = format!(
        "{} nie zgadza się z wynikiem: {}",
        actor.username, pact.title
    );
    announce(
        &mut uow,
        ctx,
        &pact,
        &Vec::from_iter(proposer),
        "Wynik umowy sporny",
        body,
    )
    .await?;
    uow.commit().await
}

/// Put disputed claim `claim_id` to a vote of the members who are not part of the pact.
pub async fn escalate(ctx: &Context, actor: &Actor, id: i64, claim_id: i64) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let pact = guarded(&mut uow, ctx, actor, id, Action::Escalate(claim_id)).await?;
    let jurors = sqlx::query_scalar!(
        "select u.id from users u
         where u.is_active and u.password_hash is not null
           and not exists (select 1 from pact_participants p where p.pact_id = $1 and p.user_id = u.id)
         order by u.id",
        id,
    )
    .fetch_all(uow.conn())
    .await?;
    if jurors.is_empty() {
        return Err(DomainError::new(
            "no_jurors",
            "Nie ma bezstronnych posłów, którzy mogliby rozstrzygnąć spór.",
        )
        .into());
    }
    let claim = pact.claim(claim_id).ok_or(AppError::NotFound)?;
    let config = PollConfig::PactRuling {
        proposal_id: claim_id,
        pact_id: id,
        pact_title: pact.title.clone(),
        claim: claim.result.to_json(),
    };
    let draft = Draft {
        kind: PollKind::PactRuling,
        title: config.generated_title().unwrap_or_default(),
        config,
        participants: jurors,
        proposed_image_key: None,
        tie_break_of: None,
        subject_user_id: None,
        subject_claim_id: Some(claim_id),
        subject_pact_id: None,
    };
    insert_poll(&mut uow, ctx, actor.user_id, draft).await?;
    touched(&mut uow, &pact).await?;
    uow.commit().await
}

/// Put a resolution to a Sejmik vote of every member except its author. Anyone may once the
/// deadline has passed; before it only the author may (claiming it is done early).
pub async fn call_judgment(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let (row, pact) = lock(&mut uow, id).await?;
    policy::check(Action::CallJudgment, &pact, actor.user_id, ctx.clock.now()).map_err(denied)?;
    let jurors = sqlx::query_scalar!(
        "select id from users where is_active and password_hash is not null and id <> $1 order by id",
        pact.creator_id,
    )
    .fetch_all(uow.conn())
    .await?;
    let author = sqlx::query_scalar!("select username from users where id = $1", pact.creator_id)
        .fetch_one(uow.conn())
        .await?;
    let config = PollConfig::ResolutionJudgment {
        pact_id: id,
        pact_title: pact.title.clone(),
        condition: row.condition,
        author_id: pact.creator_id,
        author_username: author,
    };
    let draft = Draft {
        kind: PollKind::ResolutionJudgment,
        title: config.generated_title().unwrap_or_default(),
        config,
        participants: jurors,
        proposed_image_key: None,
        tie_break_of: None,
        subject_user_id: None,
        subject_claim_id: None,
        subject_pact_id: Some(id),
    };
    let poll_id = insert_poll(&mut uow, ctx, actor.user_id, draft).await?;
    sqlx::query!(
        "update pacts set judgment_poll_id = $2 where id = $1",
        id,
        poll_id
    )
    .execute(uow.conn())
    .await?;
    touched(&mut uow, &pact).await?;
    uow.commit().await
}

/// Apply claim `claim_id`, which everyone agreed to (or a ruling upheld): verdicts, debts in
/// the ledger, the end of the wager or pact.
async fn settle_claim(
    uow: &mut Uow,
    ctx: &Context,
    pact: &mut Pact,
    claim_id: i64,
    now: DateTime<Utc>,
) -> AppResult<()> {
    let claim = pact.claim(claim_id).cloned().ok_or(AppError::NotFound)?;
    let (parties, ended) = pact.settled_by(&claim);
    let inactive = ended.iter().any(|id| {
        pact.participant(*id)
            .is_none_or(|p| p.state != ParticipantState::Active)
    });
    if inactive {
        return Err(AppError::conflict(
            "pact_inactive",
            "Ta umowa nie jest już aktywna.",
        ));
    }
    let settlement = settle(pact.kind, &parties, &claim.result);
    crate::ledger::record_debts(
        uow,
        ctx,
        settlement.debts.clone(),
        &pact.title,
        pact.id,
        local_today(now),
    )
    .await?;
    sqlx::query!(
        "update pact_claims set state = 'confirmed', decided_at = $2 where id = $1",
        claim_id,
        now,
    )
    .execute(uow.conn())
    .await?;
    let (users, verdicts): (Vec<i64>, Vec<String>) = settlement
        .verdicts
        .iter()
        .map(|(user, verdict)| (*user, verdict.as_str().to_owned()))
        .unzip();
    sqlx::query!(
        "insert into pact_verdicts (claim_id, user_id, verdict)
         select $1, user_id, verdict from unnest($2::bigint[], $3::text[]) as t(user_id, verdict)",
        claim_id,
        &users,
        &verdicts,
    )
    .execute(uow.conn())
    .await?;
    let void = claim.result.is_void();
    let end = if void {
        ParticipantState::Void
    } else {
        ParticipantState::Settled
    };
    sqlx::query!(
        "update pact_participants set state = $2, expires_at = null where id = any($1)",
        &ended,
        end.as_str(),
    )
    .execute(uow.conn())
    .await?;
    for row in &mut pact.participants {
        if ended.contains(&row.id) {
            row.state = end;
        }
    }
    if let Some(stored) = pact.claims.iter_mut().find(|c| c.id == claim_id) {
        stored.state = ClaimState::Confirmed;
        stored.decided_at = Some(now);
    }
    if pact.wagers_done() {
        finish(uow, pact, now).await?;
    }
    let summary = if settlement.debts.is_empty() {
        "bez rozliczenia".to_owned()
    } else {
        settlement
            .debts
            .iter()
            .map(|d| {
                let currency = if d.item.is_empty() { BASE_CURRENCY } else { "" };
                format_amount(d.amount, currency, &d.item)
            })
            .collect::<Vec<_>>()
            .join(", ")
    };
    let title = if void {
        "Umowa unieważniona"
    } else {
        "Umowa rozstrzygnięta"
    };
    let body = format!("{}: {summary}", pact.title);
    announce(uow, ctx, pact, &[claim.proposed_by], title, body).await
}

/// A `pact_ruling` vote closed: settle the disputed claim if upheld, else overrule it (the
/// parties may claim again).
pub(crate) async fn apply_ruling(
    uow: &mut Uow,
    ctx: &Context,
    claim_id: i64,
    upheld: bool,
) -> AppResult<Applied> {
    let now = ctx.clock.now();
    let Some(pact_id) =
        sqlx::query_scalar!("select pact_id from pact_claims where id = $1", claim_id)
            .fetch_optional(uow.conn())
            .await?
    else {
        return Ok(Applied::Refused("Ten wynik już nie istnieje.".into()));
    };
    let (_, mut pact) = lock(uow, pact_id).await?;
    if pact.claim(claim_id).map(|c| c.state) != Some(ClaimState::Disputed) {
        return Ok(Applied::Refused("Ten wynik nie jest już sporny.".into()));
    }
    if !upheld {
        sqlx::query!(
            "update pact_claims set state = 'overruled', decided_at = $2 where id = $1",
            claim_id,
            now,
        )
        .execute(uow.conn())
        .await?;
        let body = format!("Sejmik odrzucił wynik umowy: {}", pact.title);
        let active = pact.active_ids();
        announce(uow, ctx, &pact, &active, "Wynik odrzucony", body).await?;
        return Ok(Applied::No);
    }
    if !pact.status.deciding() {
        return Ok(Applied::Refused("Umowa jest już zamknięta.".into()));
    }
    settle_claim(uow, ctx, &mut pact, claim_id, now).await?;
    Ok(Applied::Yes)
}

/// A `resolution_judgment` vote closed: settle the resolution, or (a tie, no votes, or an
/// early vote that did not carry) leave it open so it can be judged again.
pub(crate) async fn apply_judgment(
    uow: &mut Uow,
    ctx: &Context,
    pact_id: i64,
    kept: bool,
    decided: bool,
) -> AppResult<Applied> {
    let now = ctx.clock.now();
    let (_, mut pact) = match lock(uow, pact_id).await {
        Err(AppError::NotFound) => {
            return Ok(Applied::Refused("Ta umowa już nie istnieje.".into()));
        }
        loaded => loaded?,
    };
    sqlx::query!(
        "update pacts set judgment_poll_id = null where id = $1",
        pact_id
    )
    .execute(uow.conn())
    .await?;
    pact.judgment_poll_id = None;
    let creator = [pact.creator_id];
    if !decided {
        let body = format!("Posłowie nie rozstrzygnęli: {}", pact.title);
        announce(uow, ctx, &pact, &creator, "Brak rozstrzygnięcia", body).await?;
        return Ok(Applied::No);
    }
    if !pact.status.deciding() {
        return Ok(Applied::Refused("Umowa jest już zamknięta.".into()));
    }
    if !kept && !pact.result_allowed(now) {
        // Called early and not carried: there is still time to keep their word.
        let body = format!("Posłowie nie uznali jeszcze postanowienia: {}", pact.title);
        announce(uow, ctx, &pact, &creator, "Brak rozstrzygnięcia", body).await?;
        return Ok(Applied::No);
    }
    let author = pact.creator_id;
    let claim_id = insert_claim(uow, &mut pact, None, author, Outcome::Kept { kept }, now).await?;
    settle_claim(uow, ctx, &mut pact, claim_id, now).await?;
    Ok(Applied::Yes)
}

/// Add a picture (an upload of purpose `photo`) to a pact's notes: the original agreement,
/// proof of the result.
pub async fn add_attachment(
    ctx: &Context,
    actor: &Actor,
    id: i64,
    upload_id: Uuid,
    caption: &str,
) -> AppResult<()> {
    let caption = text(
        caption,
        "caption",
        CAPTION_MAX,
        None,
        format!("Podpis może mieć najwyżej {CAPTION_MAX} znaków."),
    )?;
    let mut uow = Uow::begin(&ctx.db).await?;
    let pact = guarded(&mut uow, ctx, actor, id, Action::Attach).await?;
    let image = media::take_upload(&mut uow, ctx, actor.user_id, upload_id, Purpose::Photo).await?;
    sqlx::query!(
        "insert into pact_attachments (pact_id, uploaded_by, image_key, width, height, caption, created_at)
         values ($1, $2, $3, $4, $5, $6, $7)",
        id,
        actor.user_id,
        image.key,
        image.width,
        image.height,
        caption,
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;
    touched(&mut uow, &pact).await?;
    uow.commit().await
}

/// Take a picture down (its uploader or the pact's creator). The file goes once this commits.
pub async fn delete_attachment(
    ctx: &Context,
    actor: &Actor,
    id: i64,
    attachment_id: i64,
) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let (_, pact) = lock(&mut uow, id).await?;
    let attachment = sqlx::query!(
        "select uploaded_by, image_key from pact_attachments where id = $1 and pact_id = $2",
        attachment_id,
        id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    let action = Action::RemoveAttachment {
        uploaded_by: attachment.uploaded_by,
    };
    policy::check(action, &pact, actor.user_id, ctx.clock.now()).map_err(denied)?;
    sqlx::query!("delete from pact_attachments where id = $1", attachment_id)
        .execute(uow.conn())
        .await?;
    media::trash(&mut uow, Some(&attachment.image_key)).await?;
    touched(&mut uow, &pact).await?;
    uow.commit().await
}
