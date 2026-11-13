//! Changing Secret Santa. Each function is one transaction with the active round locked.

use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use domain::DomainError;
use domain::santa::{
    self, Draw, MIN_PARTICIPANTS, Mode, NOTE_MAX, clean_ideas, clean_tiers, help_gift,
    max_per_tier, per_tier_shifts,
};
use rand::seq::{IndexedRandom as _, SliceRandom as _};
use serde::Deserialize;
use utoipa::ToSchema;

use super::views::GiftView;
use super::{lock_active, victims};
use crate::auth::Actor;
use crate::db::Uow;
use crate::events::{Audience, Event};
use crate::push::Notification;
use crate::{AppError, AppResult, Context};

const URL: &str = "/secret-santa";

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct StartRequest {
    /// At least 3 members (duplicates are ignored).
    pub participant_ids: Vec<i64>,
    /// Informational: a superuser ends the round by hand.
    pub deadline: DateTime<Utc>,
    /// Gift amounts in whole złoty, 1 to 10 of them.
    pub gift_tiers: Vec<i64>,
    /// `single` when absent.
    pub mode: Option<Mode>,
}

#[derive(Debug, Clone, Default, Deserialize, ToSchema)]
pub struct UpdateRequest {
    pub deadline: Option<DateTime<Utc>>,
    /// New amounts; a per-tier round keeps its number of tiers.
    pub gift_tiers: Option<Vec<i64>>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct HelpRequest {
    /// One of your victims.
    pub victim_id: i64,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct AnswerRequest {
    /// 1 to 5 ideas, up to 200 characters each (blank ones are dropped).
    pub ideas: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, ToSchema)]
pub struct GiftNote {
    /// What was given; up to 300 characters.
    pub note: String,
}

fn future(deadline: DateTime<Utc>, now: DateTime<Utc>) -> Result<(), DomainError> {
    if deadline <= now {
        return Err(
            DomainError::new("deadline_in_past", "Termin musi być w przyszłości.").on("deadline"),
        );
    }
    Ok(())
}

async fn push(
    uow: &mut Uow,
    ctx: &Context,
    to: &[i64],
    title: &str,
    body: String,
) -> AppResult<()> {
    let notification = Notification {
        title: title.into(),
        body,
        url: URL.into(),
    };
    ctx.push.notify(uow, to, &notification).await?;
    Ok(())
}

/// Start a round and draw it. Every giver is pushed whom they drew.
pub async fn start(ctx: &Context, actor: &Actor, request: &StartRequest) -> AppResult<()> {
    let now = ctx.clock.now();
    let mode = request.mode.unwrap_or(Mode::Single);
    let tiers = clean_tiers(&request.gift_tiers)?;
    future(request.deadline, now)?;
    let mut wanted = request.participant_ids.clone();
    wanted.sort_unstable();
    wanted.dedup();

    let mut uow = Uow::begin(&ctx.db).await?;
    let members = sqlx::query!(
        "select id, username from users
         where id = any($1) and is_active and password_hash is not null order by id",
        &wanted,
    )
    .fetch_all(uow.conn())
    .await?;
    if members.len() != wanted.len() {
        return Err(
            DomainError::new("unknown_participant", "Nieznany lub nieaktywny użytkownik.")
                .on("participant_ids")
                .into(),
        );
    }
    if members.len() < MIN_PARTICIPANTS {
        return Err(DomainError::new(
            "too_few_participants",
            format!("Potrzeba co najmniej {MIN_PARTICIPANTS} uczestników."),
        )
        .on("participant_ids")
        .into());
    }
    if mode == Mode::PerTier && tiers.len() > max_per_tier(members.len()) {
        return Err(DomainError::new(
            "too_many_tiers",
            "Za mało uczestników, by każda kwota miała innego podopiecznego.",
        )
        .on("gift_tiers")
        .into());
    }
    let running = sqlx::query_scalar!(
        r#"select exists(select 1 from santa_events where status = 'active') as "running!""#
    )
    .fetch_one(uow.conn())
    .await?;
    if running {
        return Err(already_running());
    }

    let draws = {
        let mut rng = rand::rng();
        let mut order: Vec<i64> = members.iter().map(|m| m.id).collect();
        order.shuffle(&mut rng);
        let shifts: Vec<usize> = per_tier_shifts(order.len())
            .choose_multiple(&mut rng, tiers.len())
            .copied()
            .collect();
        let mut draws = santa::draw(mode, &order, &shifts);
        // Stored in a fresh random order, so row ids say nothing about the draw.
        draws.shuffle(&mut rng);
        draws
    };

    let event_id = sqlx::query_scalar!(
        "insert into santa_events (mode, deadline, gift_tiers, created_by, created_at)
         values ($1, $2, $3, $4, $5) returning id",
        mode.as_str(),
        request.deadline,
        &tiers,
        actor.user_id,
        now,
    )
    .fetch_one(uow.conn())
    .await
    .map_err(|error| match &error {
        // Lost a race with another start.
        sqlx::Error::Database(db) if db.is_unique_violation() => already_running(),
        _ => error.into(),
    })?;
    let tier_of = |d: &Draw| d.tier.and_then(|t| i16::try_from(t).ok());
    let givers: Vec<i64> = draws.iter().map(|d| d.giver).collect();
    let tier_indexes: Vec<Option<i16>> = draws.iter().map(tier_of).collect();
    let sealed: Vec<Vec<u8>> = draws
        .iter()
        .map(|d| ctx.santa.seal(event_id, d.giver, tier_of(d), d.receiver))
        .collect();
    sqlx::query!(
        "insert into santa_assignments (event_id, giver_id, tier_index, sealed)
         select $1, giver, tier, sealed
         from unnest($2::bigint[], $3::smallint[], $4::bytea[]) with ordinality as t(giver, tier, sealed, n)
         order by n",
        event_id,
        &givers,
        &tier_indexes as &[Option<i16>],
        &sealed,
    )
    .execute(uow.conn())
    .await?;
    uow.publish(Audience::All, Event::SantaUpdated { event_id })
        .await?;
    let names: BTreeMap<i64, String> = members.into_iter().map(|m| (m.id, m.username)).collect();
    for (giver, body) in santa::start_messages(&draws, &names, &tiers) {
        push(&mut uow, ctx, &[giver], "Secret Santa — wylosowano!", body).await?;
    }
    uow.commit().await
}

fn already_running() -> AppError {
    AppError::conflict("santa_active", "Secret Santa już trwa.")
}

/// Change the deadline or the amounts; the draw stays as it is.
pub async fn update(ctx: &Context, request: &UpdateRequest) -> AppResult<()> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let event = lock_active(&mut uow).await?;
    if let Some(deadline) = request.deadline {
        future(deadline, now)?;
    }
    let tiers = match &request.gift_tiers {
        None => event.gift_tiers.clone(),
        Some(raw) => {
            let clean = clean_tiers(raw)?;
            if event.mode == Mode::PerTier && clean.len() != event.gift_tiers.len() {
                return Err(DomainError::new(
                    "tier_count_fixed",
                    "Po losowaniu nie można zmienić liczby kwot.",
                )
                .on("gift_tiers")
                .into());
            }
            clean
        }
    };
    sqlx::query!(
        "update santa_events set deadline = $2, gift_tiers = $3 where id = $1",
        event.id,
        request.deadline.unwrap_or(event.deadline),
        &tiers,
    )
    .execute(uow.conn())
    .await?;
    uow.publish(Audience::All, Event::SantaUpdated { event_id: event.id })
        .await?;
    if tiers != event.gift_tiers {
        let givers = sqlx::query_scalar!(
            "select distinct giver_id from santa_assignments where event_id = $1 order by giver_id",
            event.id,
        )
        .fetch_all(uow.conn())
        .await?;
        let body = format!("Nowe kwoty: {}.", santa::amounts(&tiers));
        push(&mut uow, ctx, &givers, "Secret Santa — zmiana kwot", body).await?;
    }
    uow.commit().await
}

/// End the round: reveal the pairings and record one gift per pairing and tier.
pub async fn end(ctx: &Context) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let event = lock_active(&mut uow).await?;
    let rows = sqlx::query!(
        r#"select id, giver_id, tier_index, sealed as "sealed!" from santa_assignments
           where event_id = $1 and sealed is not null"#,
        event.id,
    )
    .fetch_all(uow.conn())
    .await?;
    let mut ids = Vec::with_capacity(rows.len());
    let mut receivers = Vec::with_capacity(rows.len());
    let (mut gift_rows, mut gift_amounts) = (Vec::new(), Vec::new());
    for row in rows {
        let receiver = ctx
            .santa
            .open(event.id, row.giver_id, row.tier_index, &row.sealed)
            .map_err(anyhow::Error::from)?;
        ids.push(row.id);
        receivers.push(receiver);
        let amounts = match event.amount(row.tier_index) {
            Some(amount) => vec![amount],
            None => event.gift_tiers.clone(),
        };
        for amount in amounts {
            gift_rows.push(row.id);
            gift_amounts.push(amount);
        }
    }
    sqlx::query!(
        "update santa_assignments a set receiver_id = t.receiver, sealed = null
         from unnest($1::bigint[], $2::bigint[]) as t(id, receiver) where a.id = t.id",
        &ids,
        &receivers,
    )
    .execute(uow.conn())
    .await?;
    sqlx::query!(
        "insert into santa_gifts (assignment_id, amount) select * from unnest($1::bigint[], $2::bigint[])",
        &gift_rows,
        &gift_amounts,
    )
    .execute(uow.conn())
    .await?;
    sqlx::query!(
        "update santa_events set status = 'ended', ended_at = $2 where id = $1",
        event.id,
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;
    uow.publish(Audience::All, Event::SantaUpdated { event_id: event.id })
        .await?;
    uow.commit().await
}

/// Ask one of your victims, anonymously, for gift ideas. Asking again is possible once they
/// answered (their earlier ideas stay until they answer again).
pub async fn ask_for_help(ctx: &Context, actor: &Actor, victim_id: i64) -> AppResult<()> {
    let now = ctx.clock.now();
    let mut uow = Uow::begin(&ctx.db).await?;
    let event = lock_active(&mut uow).await?;
    // A giver never draws the same victim twice, so the victim names the pairing.
    let tier = victims(uow.conn(), ctx, event.id, actor.user_id)
        .await?
        .into_iter()
        .find(|(_, receiver)| *receiver == victim_id)
        .map(|(tier, _)| tier)
        .ok_or_else(|| {
            DomainError::new("not_your_victim", "To nie jest Twój podopieczny.").on("victim_id")
        })?;
    let earlier = sqlx::query!(
        "select id, answered_at from santa_help_requests
         where event_id = $1 and receiver_id = $2 and tier_index is not distinct from $3
         for update",
        event.id,
        victim_id,
        tier,
    )
    .fetch_optional(uow.conn())
    .await?;
    match earlier {
        None => {
            sqlx::query!(
                "insert into santa_help_requests (event_id, receiver_id, tier_index, asked_at)
                 values ($1, $2, $3, $4)",
                event.id,
                victim_id,
                tier,
                now,
            )
            .execute(uow.conn())
            .await?;
        }
        Some(request) if request.answered_at.is_none() => {
            return Err(
                DomainError::new("help_pending", "Prośba o pomoc już czeka na odpowiedź.").into(),
            );
        }
        Some(request) => {
            sqlx::query!(
                "update santa_help_requests set asked_at = $2, answered_at = null where id = $1",
                request.id,
                now,
            )
            .execute(uow.conn())
            .await?;
        }
    }
    uow.publish(
        Audience::User(victim_id),
        Event::SantaHelp { event_id: event.id },
    )
    .await?;
    let body = format!(
        "Twój Secret Santa nie wie, co Ci kupić, i prosi o pomoc z {}. Podsuń kilka pomysłów!",
        help_gift(event.amount(tier))
    );
    push(
        &mut uow,
        ctx,
        &[victim_id],
        "Secret Santa — prośba o pomoc",
        body,
    )
    .await?;
    uow.commit().await
}

/// A victim answers a request about them with a few ideas; their giver is told.
pub async fn answer_help(
    ctx: &Context,
    actor: &Actor,
    request_id: i64,
    ideas: &[String],
) -> AppResult<()> {
    let ideas = clean_ideas(ideas)?;
    let mut uow = Uow::begin(&ctx.db).await?;
    let event = lock_active(&mut uow).await?;
    // Someone else's request looks the same as a missing one.
    let tier = sqlx::query_scalar!(
        "select tier_index from santa_help_requests
         where id = $1 and event_id = $2 and receiver_id = $3 for update",
        request_id,
        event.id,
        actor.user_id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    sqlx::query!(
        "update santa_help_requests set ideas = $2, answered_at = $3 where id = $1",
        request_id,
        &ideas,
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;
    let giver = giver_of(&mut uow, ctx, event.id, actor.user_id, tier).await?;
    uow.publish(
        Audience::User(giver),
        Event::SantaHelp { event_id: event.id },
    )
    .await?;
    let body = format!("Masz nowe pomysły na prezent dla: {}.", actor.username);
    push(
        &mut uow,
        ctx,
        &[giver],
        "Secret Santa — są podpowiedzi",
        body,
    )
    .await?;
    uow.commit().await
}

/// Who gives to `receiver_id` in `tier`: opens that tier's seals, only to know whom to tell.
async fn giver_of(
    uow: &mut Uow,
    ctx: &Context,
    event_id: i64,
    receiver_id: i64,
    tier: Option<i16>,
) -> AppResult<i64> {
    let rows = sqlx::query!(
        r#"select giver_id, sealed as "sealed!" from santa_assignments
           where event_id = $1 and tier_index is not distinct from $2 and sealed is not null"#,
        event_id,
        tier,
    )
    .fetch_all(uow.conn())
    .await?;
    for row in rows {
        let receiver = ctx
            .santa
            .open(event_id, row.giver_id, tier, &row.sealed)
            .map_err(anyhow::Error::from)?;
        if receiver == receiver_id {
            return Ok(row.giver_id);
        }
    }
    Err(AppError::NotFound)
}

/// Note what was given (the giver, or a superuser), once the round has ended.
pub async fn set_gift_note(
    ctx: &Context,
    actor: &Actor,
    gift_id: i64,
    note: &str,
) -> AppResult<GiftView> {
    let note = note.trim();
    if note.chars().count() > NOTE_MAX {
        return Err(DomainError::new(
            "note_too_long",
            format!("Notatka może mieć najwyżej {NOTE_MAX} znaków."),
        )
        .on("note")
        .into());
    }
    let mut uow = Uow::begin(&ctx.db).await?;
    let gift = sqlx::query!(
        "select g.id, g.amount, a.giver_id, e.id as event_id, e.status
         from santa_gifts g join santa_assignments a on a.id = g.assignment_id
         join santa_events e on e.id = a.event_id
         where g.id = $1 for update of g",
        gift_id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    if gift.status != santa::Status::Ended.as_str() {
        return Err(DomainError::new(
            "santa_running",
            "Notatki można dodawać po zakończeniu Secret Santa.",
        )
        .into());
    }
    if gift.giver_id != actor.user_id && !actor.is_superuser {
        return Err(AppError::forbidden(
            "not_your_gift",
            "Możesz edytować tylko własne prezenty.",
        ));
    }
    sqlx::query!(
        "update santa_gifts set note = $2 where id = $1",
        gift_id,
        note
    )
    .execute(uow.conn())
    .await?;
    uow.publish(
        Audience::All,
        Event::SantaUpdated {
            event_id: gift.event_id,
        },
    )
    .await?;
    uow.commit().await?;
    Ok(GiftView {
        id: gift.id,
        amount: gift.amount,
        note: note.to_owned(),
    })
}
