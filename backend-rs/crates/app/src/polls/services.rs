//! Changing polls. Each function is one transaction with the poll row locked.

use domain::DomainError;
use domain::accounts::username;
use domain::polls::ballot::{self, BallotInput};
use domain::polls::config::new_options;
use domain::polls::policy::{self, Action};
use domain::polls::{
    Ballot, CloseReason, DURATION, PollConfig, PollKind, REACTIONS, REASON_MAX, REMINDERS,
    TITLE_MAX,
};
use serde::Deserialize;
use utoipa::ToSchema;
use uuid::Uuid;

use super::{close::close, denied, facts, lock, mine};
use crate::auth::Actor;
use crate::db::Uow;
use crate::events::{Audience, Event};
use crate::media::{self, Purpose};
use crate::push::Notification;
use crate::{AppError, AppResult, Context};

/// A new poll, by kind. Members create every kind but the pact ones (those come from pacts).
#[derive(Debug, Clone, Deserialize, ToSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum NewPoll {
    /// A score from -5 to +5.
    Score {
        title: String,
        /// Who votes besides the creator (who always does).
        #[serde(default)]
        participant_ids: Vec<i64>,
    },
    SingleChoice {
        title: String,
        /// 2 to 10 labels.
        options: Vec<String>,
        /// Participants may add an option with their vote.
        #[serde(default)]
        open_options: bool,
        #[serde(default)]
        participant_ids: Vec<i64>,
    },
    MultiChoice {
        title: String,
        options: Vec<String>,
        #[serde(default)]
        open_options: bool,
        /// At most this many picks per ballot (1 to options - 1).
        max_choices: Option<usize>,
        #[serde(default)]
        participant_ids: Vec<i64>,
    },
    RankedChoice {
        title: String,
        options: Vec<String>,
        /// Not allowed for ranked votes (refused if true).
        #[serde(default)]
        open_options: bool,
        #[serde(default)]
        participant_ids: Vec<i64>,
    },
    /// Rename another member. Everyone votes; passes at a positive average.
    Nickname {
        target_user_id: i64,
        new_username: String,
    },
    /// Change another member's picture (`upload_id` of an avatar upload) or remove it.
    Avatar {
        target_user_id: i64,
        #[serde(default)]
        remove: bool,
        upload_id: Option<Uuid>,
    },
}

/// What a new poll will be, before it is stored.
pub(crate) struct Draft {
    pub kind: PollKind,
    pub config: PollConfig,
    pub title: String,
    pub participants: Vec<i64>,
    pub proposed_image_key: Option<String>,
    pub tie_break_of: Option<i64>,
    pub subject_user_id: Option<i64>,
    pub subject_claim_id: Option<i64>,
    pub subject_pact_id: Option<i64>,
}

fn title(raw: &str) -> Result<String, DomainError> {
    let title = raw.trim();
    if title.is_empty() {
        return Err(DomainError::new("title_required", "Podaj tytuł głosowania.").on("title"));
    }
    if title.chars().count() > TITLE_MAX {
        return Err(DomainError::new(
            "title_too_long",
            format!("Tytuł może mieć najwyżej {TITLE_MAX} znaków."),
        )
        .on("title"));
    }
    Ok(title.to_owned())
}

/// Create a poll; returns its id.
pub async fn create(ctx: &Context, actor: &Actor, request: NewPoll) -> AppResult<i64> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let draft = draft(&mut uow, ctx, actor, request).await?;
    let id = insert(&mut uow, ctx, actor.user_id, draft).await?;
    uow.commit().await?;
    Ok(id)
}

async fn draft(uow: &mut Uow, ctx: &Context, actor: &Actor, request: NewPoll) -> AppResult<Draft> {
    let plain = |kind, config, raw_title: &str, ids: Vec<i64>| -> Result<Draft, DomainError> {
        Ok(Draft {
            kind,
            config,
            title: title(raw_title)?,
            participants: ids,
            proposed_image_key: None,
            tie_break_of: None,
            subject_user_id: None,
            subject_claim_id: None,
            subject_pact_id: None,
        })
    };
    let mut draft = match request {
        NewPoll::Score {
            title,
            participant_ids,
        } => plain(
            PollKind::Score,
            PollConfig::Score {},
            &title,
            participant_ids,
        )?,
        NewPoll::SingleChoice {
            title,
            options,
            open_options,
            participant_ids,
        } => {
            let config = new_options(PollKind::SingleChoice, &options, open_options, None)?;
            plain(
                PollKind::SingleChoice,
                PollConfig::Options(config),
                &title,
                participant_ids,
            )?
        }
        NewPoll::MultiChoice {
            title,
            options,
            open_options,
            max_choices,
            participant_ids,
        } => {
            let config = new_options(PollKind::MultiChoice, &options, open_options, max_choices)?;
            plain(
                PollKind::MultiChoice,
                PollConfig::Options(config),
                &title,
                participant_ids,
            )?
        }
        NewPoll::RankedChoice {
            title,
            options,
            open_options,
            participant_ids,
        } => {
            let config = new_options(PollKind::RankedChoice, &options, open_options, None)?;
            plain(
                PollKind::RankedChoice,
                PollConfig::Options(config),
                &title,
                participant_ids,
            )?
        }
        NewPoll::Nickname {
            target_user_id,
            new_username,
        } => {
            let target = profile_target(uow, actor, target_user_id).await?;
            let new_username =
                check_new_username(uow, target_user_id, &target, &new_username).await?;
            let config = PollConfig::Nickname {
                target_user_id,
                target_username: target,
                new_username,
            };
            profile_draft(PollKind::Nickname, config, target_user_id)
        }
        NewPoll::Avatar {
            target_user_id,
            remove,
            upload_id,
        } => {
            let target = profile_target(uow, actor, target_user_id).await?;
            let config = PollConfig::Avatar {
                target_user_id,
                target_username: target,
                remove,
            };
            let mut draft = profile_draft(PollKind::Avatar, config, target_user_id);
            match (remove, upload_id) {
                (true, Some(_)) => {
                    return Err(DomainError::new(
                        "image_with_removal",
                        "Usuwanie zdjęcia nie przyjmuje nowego obrazu.",
                    )
                    .on("upload_id")
                    .into());
                }
                (false, None) => {
                    return Err(DomainError::new(
                        "image_required",
                        "Dołącz zdjęcie albo zaproponuj jego usunięcie.",
                    )
                    .on("upload_id")
                    .into());
                }
                (false, Some(upload)) => {
                    let image =
                        media::take_upload(uow, ctx, actor.user_id, upload, Purpose::Avatar)
                            .await?;
                    draft.proposed_image_key = Some(image.key);
                }
                (true, None) => {}
            }
            draft
        }
    };
    draft.participants = participants(
        uow,
        actor.user_id,
        draft.kind,
        std::mem::take(&mut draft.participants),
    )
    .await?;
    Ok(draft)
}

fn profile_draft(kind: PollKind, config: PollConfig, target: i64) -> Draft {
    Draft {
        kind,
        title: config.generated_title().unwrap_or_default(),
        config,
        participants: Vec::new(),
        proposed_image_key: None,
        tie_break_of: None,
        subject_user_id: Some(target),
        subject_claim_id: None,
        subject_pact_id: None,
    }
}

/// The member a profile vote is about (their current username).
async fn profile_target(uow: &mut Uow, actor: &Actor, target_user_id: i64) -> AppResult<String> {
    let target = sqlx::query_scalar!(
        "select username from users where id = $1 and is_active and password_hash is not null",
        target_user_id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or_else(|| {
        DomainError::new("unknown_target", "Nieznany lub nieaktywny poseł.").on("target_user_id")
    })?;
    if target_user_id == actor.user_id {
        return Err(
            DomainError::new("own_profile", "Nie możesz głosować nad własnym profilem.").into(),
        );
    }
    Ok(target)
}

async fn check_new_username(
    uow: &mut Uow,
    target_id: i64,
    current: &str,
    raw: &str,
) -> AppResult<String> {
    let name = username::normalize(raw);
    if name == current {
        return Err(DomainError::new("same_username", "To już jest ten nick.")
            .on("new_username")
            .into());
    }
    username::validate(&name).map_err(|error| error.on("new_username"))?;
    let taken = sqlx::query_scalar!(
        r#"select exists(select 1 from users where username = $1 and id <> $2) as "taken!""#,
        name,
        target_id,
    )
    .fetch_one(uow.conn())
    .await?;
    if taken {
        return Err(username::taken().on("new_username").into());
    }
    Ok(name)
}

/// Who votes: every member for some kinds; else the chosen members plus the creator.
async fn participants(
    uow: &mut Uow,
    creator: i64,
    kind: PollKind,
    chosen: Vec<i64>,
) -> AppResult<Vec<i64>> {
    if kind.rules().everyone_participates {
        return Ok(sqlx::query_scalar!(
            "select id from users where is_active and password_hash is not null order by id"
        )
        .fetch_all(uow.conn())
        .await?);
    }
    let mut wanted = chosen;
    wanted.push(creator);
    wanted.sort_unstable();
    wanted.dedup();
    let members = sqlx::query_scalar!(
        "select id from users where id = any($1) and is_active and password_hash is not null order by id",
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
    Ok(members)
}

/// Store `draft` as a new poll by `creator`, announce it, and tell its voters.
pub(crate) async fn insert(
    uow: &mut Uow,
    ctx: &Context,
    creator: i64,
    draft: Draft,
) -> AppResult<i64> {
    if draft.participants.is_empty() {
        return Err(
            DomainError::new("no_voters", "Nikt nie może zagłosować w tej sprawie.").into(),
        );
    }
    let now = ctx.clock.now();
    let id = sqlx::query_scalar!(
        "insert into polls (creator_id, title, kind, config, created_at, deadline_at, next_reminder_at,
                            proposed_image_key, tie_break_of, subject_user_id, subject_claim_id, subject_pact_id)
         values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) returning id",
        creator,
        draft.title,
        draft.kind.as_str(),
        draft.config.to_json(),
        now,
        now + DURATION,
        now + REMINDERS[0],
        draft.proposed_image_key,
        draft.tie_break_of,
        draft.subject_user_id,
        draft.subject_claim_id,
        draft.subject_pact_id,
    )
    .fetch_one(uow.conn())
    .await?;
    sqlx::query!(
        "insert into poll_participants (poll_id, user_id) select $1, unnest($2::bigint[])",
        id,
        &draft.participants,
    )
    .execute(uow.conn())
    .await?;
    uow.publish(Audience::All, Event::PollCreated { poll_id: id })
        .await?;
    let others: Vec<i64> = draft
        .participants
        .iter()
        .copied()
        .filter(|user| *user != creator)
        .collect();
    let notification = Notification {
        title: "Nowe głosowanie w Sejmiku".into(),
        body: draft.title,
        url: format!("/voting/{id}"),
    };
    ctx.push.notify(uow, &others, &notification).await?;
    Ok(id)
}

/// The policy check for `action` on poll `id` (locked); returns the poll.
async fn guarded(
    uow: &mut Uow,
    actor: &Actor,
    id: i64,
    action: Action,
) -> AppResult<super::PollRow> {
    let poll = lock(uow, id).await?;
    let mine = mine(uow, id, actor.user_id).await?;
    policy::check(action, &facts(&poll, actor.user_id, mine, false)?).map_err(denied)?;
    Ok(poll)
}

/// Cast or change a ballot. A ballot may add an option to an open pool.
pub async fn cast_ballot(
    ctx: &Context,
    actor: &Actor,
    id: i64,
    input: &BallotInput,
) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let poll = guarded(&mut uow, actor, id, Action::Vote).await?;
    let kind = poll.kind()?;
    let mut config = poll.config()?;
    let resolved = ballot::resolve(kind, &config, input)?;
    sqlx::query!(
        "update poll_participants set ballot = $3, abstained = false, reason = '', voted_at = $4
         where poll_id = $1 and user_id = $2",
        id,
        actor.user_id,
        resolved.ballot.to_json(),
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;
    if let (Some(added), PollConfig::Options(options)) = (&resolved.added, &mut config) {
        options.options.push(added.clone());
        sqlx::query!(
            "update polls set config = $2 where id = $1",
            id,
            config.to_json()
        )
        .execute(uow.conn())
        .await?;
    }
    let still_open = after_answer(&mut uow, ctx, &poll).await?;
    if let (Some(added), true) = (resolved.added, still_open) {
        // Those who already voted may want to change their vote. Not who added it: ballots are
        // secret until the end.
        let voters = sqlx::query_scalar!(
            "select user_id from poll_participants
             where poll_id = $1 and voted_at is not null and not abstained and user_id <> $2",
            id,
            actor.user_id,
        )
        .fetch_all(uow.conn())
        .await?;
        let notification = Notification {
            title: "Nowa opcja w głosowaniu".into(),
            body: format!(
                "Dodano opcję „{}” w: {}. Możesz zmienić swój głos.",
                added.label, poll.title
            ),
            url: format!("/voting/{id}"),
        };
        ctx.push.notify(&mut uow, &voters, &notification).await?;
    }
    uow.commit().await
}

fn clean_reason(raw: &str) -> Result<String, DomainError> {
    let reason = raw.trim();
    if reason.chars().count() > REASON_MAX {
        return Err(DomainError::new(
            "reason_too_long",
            format!("Uzasadnienie może mieć najwyżej {REASON_MAX} znaków."),
        )
        .on("reason"));
    }
    Ok(reason.to_owned())
}

/// Veto (counts as the lowest score, flagged); cannot be undone.
pub async fn veto(ctx: &Context, actor: &Actor, id: i64, reason: &str) -> AppResult<()> {
    let reason = clean_reason(reason)?;
    let mut uow = Uow::begin(&ctx.db).await?;
    let poll = guarded(&mut uow, actor, id, Action::Veto).await?;
    sqlx::query!(
        "update poll_participants set ballot = $3, vetoed = true, abstained = false, reason = $4, voted_at = $5
         where poll_id = $1 and user_id = $2",
        id,
        actor.user_id,
        Ballot::veto().to_json(),
        reason,
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;
    after_answer(&mut uow, ctx, &poll).await?;
    uow.commit().await
}

/// Abstain: left out of the result (not the same as a zero). A later vote replaces it.
pub async fn abstain(ctx: &Context, actor: &Actor, id: i64, reason: &str) -> AppResult<()> {
    let reason = clean_reason(reason)?;
    let mut uow = Uow::begin(&ctx.db).await?;
    let poll = guarded(&mut uow, actor, id, Action::Abstain).await?;
    sqlx::query!(
        "update poll_participants set ballot = null, abstained = true, reason = $3, voted_at = $4
         where poll_id = $1 and user_id = $2",
        id,
        actor.user_id,
        reason,
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;
    after_answer(&mut uow, ctx, &poll).await?;
    uow.commit().await
}

/// After someone answered: close if everyone has, else announce the progress. Returns whether
/// the poll is still open.
async fn after_answer(uow: &mut Uow, ctx: &Context, poll: &super::PollRow) -> AppResult<bool> {
    let waiting = sqlx::query_scalar!(
        r#"select exists(select 1 from poll_participants where poll_id = $1 and voted_at is null) as "waiting!""#,
        poll.id,
    )
    .fetch_one(uow.conn())
    .await?;
    if waiting {
        uow.publish(Audience::All, Event::PollUpdated { poll_id: poll.id })
            .await?;
    } else {
        close(uow, ctx, poll, CloseReason::Auto).await?;
    }
    Ok(waiting)
}

/// End the poll now (its creator; not for every kind).
pub async fn close_early(ctx: &Context, actor: &Actor, id: i64) -> AppResult<()> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let poll = guarded(&mut uow, actor, id, Action::Close).await?;
    close(&mut uow, ctx, &poll, CloseReason::Creator).await?;
    uow.commit().await
}

/// A follow-up vote between the options a finished poll tied on, with the same people (who are
/// still members). Returns `(id, created)`: asking again returns the existing one.
pub async fn tie_break(ctx: &Context, actor: &Actor, id: i64) -> AppResult<(i64, bool)> {
    let mut uow = Uow::begin(&ctx.db).await?;
    let poll = guarded(&mut uow, actor, id, Action::TieBreak).await?;
    if let Some(existing) = sqlx::query_scalar!("select id from polls where tie_break_of = $1", id)
        .fetch_optional(uow.conn())
        .await?
    {
        return Ok((existing, false));
    }
    let kind = poll.kind()?;
    let follow_up = domain::polls::tie_break_kind(kind)
        .ok_or_else(|| anyhow::anyhow!("tied poll of kind {kind}"))?;
    let tied = domain::polls::tied_winners(kind, poll.result.as_ref());
    let config = poll.config()?;
    let labels: Vec<String> = config
        .options()
        .map(|o| {
            o.options
                .iter()
                .filter(|o| tied.contains(&o.id))
                .map(|o| o.label.clone())
                .collect()
        })
        .unwrap_or_default();
    let voters = sqlx::query_scalar!(
        "select p.user_id from poll_participants p join users u on u.id = p.user_id
         where p.poll_id = $1 and u.is_active and u.password_hash is not null order by p.user_id",
        id,
    )
    .fetch_all(uow.conn())
    .await?;
    let title: String = format!("Dogrywka: {}", poll.title)
        .chars()
        .take(TITLE_MAX)
        .collect();
    let draft = Draft {
        kind: follow_up,
        config: PollConfig::Options(new_options(follow_up, &labels, false, None)?),
        title,
        participants: voters,
        proposed_image_key: None,
        tie_break_of: Some(id),
        subject_user_id: None,
        subject_claim_id: None,
        subject_pact_id: None,
    };
    let new_id = insert(&mut uow, ctx, poll.creator_id, draft).await?;
    uow.commit().await?;
    Ok((new_id, true))
}

/// React to a finished vote. Meant to be spammed: one conditional update (a repeated emoji
/// writes nothing), stored only for participants, and an event everyone sees.
pub async fn react(ctx: &Context, actor: &Actor, id: i64, emoji: &str) -> AppResult<()> {
    if !REACTIONS.contains(&emoji) {
        return Err(
            DomainError::new("invalid_reaction", "Niedozwolona reakcja.")
                .on("emoji")
                .into(),
        );
    }
    let mut uow = Uow::begin(&ctx.db).await?;
    let status = sqlx::query_scalar!("select status from polls where id = $1", id)
        .fetch_optional(uow.conn())
        .await?
        .ok_or(AppError::NotFound)?;
    if status != "closed" {
        return Err(DomainError::new(
            "poll_open",
            "Reakcje są dostępne dla zakończonych głosowań.",
        )
        .into());
    }
    sqlx::query!(
        "update poll_participants set last_reaction = $3 where poll_id = $1 and user_id = $2 and last_reaction <> $3",
        id,
        actor.user_id,
        emoji,
    )
    .execute(uow.conn())
    .await?;
    let event = Event::PollReaction {
        poll_id: id,
        user_id: actor.user_id,
        emoji: emoji.to_owned(),
    };
    uow.publish(Audience::All, event).await?;
    uow.commit().await
}
