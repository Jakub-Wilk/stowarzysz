//! Reading polls. The detail view is the one place that decides what is hidden while a poll is
//! open: ballots, vetoes, abstentions, reasons and reactions appear only once it has ended
//! (everyone always sees their own ballot).

use chrono::{DateTime, Utc};
use domain::polls::policy::{self, Action};
use serde::Serialize;
use serde_json::Value;
use utoipa::ToSchema;

use super::{Mine, PollRow, facts};
use crate::accounts::Person;
use crate::auth::Actor;
use crate::db::Uow;
use crate::media::Image;
use crate::pagination::{Cursor, page};
use crate::{AppError, AppResult, Context};

const PAGE_SIZE: i64 = 20;

/// The caller's part in a poll.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[allow(clippy::struct_excessive_bools)] // independent facts about the caller
pub struct MyState {
    pub participating: bool,
    /// Voted, vetoed or abstained.
    pub has_voted: bool,
    pub vetoed: bool,
    pub abstained: bool,
}

/// A poll in a list.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PollSummary {
    pub id: i64,
    pub title: String,
    /// `score`, `single_choice`, `multi_choice`, `ranked_choice`, `nickname`, `avatar`,
    /// `pact_ruling` or `resolution_judgment`.
    pub kind: String,
    /// `open` or `closed`.
    pub status: String,
    pub creator: Person,
    pub created_at: DateTime<Utc>,
    /// When it ends at the latest.
    pub deadline_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub participant_count: i64,
    /// Answered so far (voted, vetoed or abstained).
    pub voted_count: i64,
    pub my: MyState,
    /// The frozen result once closed; its shape depends on the kind.
    #[schema(value_type = Option<Object>)]
    pub result: Option<Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PollPage {
    pub items: Vec<PollSummary>,
    /// Pass as `cursor` for the next page; `null` on the last one.
    pub next_cursor: Option<String>,
}

/// What the caller can do right now.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, ToSchema)]
#[allow(clippy::struct_excessive_bools)]
pub struct PollActions {
    pub vote: bool,
    pub veto: bool,
    pub abstain: bool,
    pub close: bool,
    pub tie_break: bool,
}

/// A participant's answer, shown once the poll has ended.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct Revealed {
    #[schema(value_type = Option<Object>)]
    pub ballot: Option<Value>,
    pub vetoed: bool,
    pub abstained: bool,
    /// Why they vetoed or abstained (empty if they gave none).
    pub reason: String,
    /// Their last reaction to the result.
    pub reaction: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct ParticipantView {
    pub user: Person,
    pub has_voted: bool,
    /// Present only once the poll has ended.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub revealed: Option<Revealed>,
}

/// One poll with everything the caller may see.
#[derive(Debug, Clone, PartialEq, Serialize, ToSchema)]
pub struct PollDetail {
    #[serde(flatten)]
    pub summary: PollSummary,
    /// Kind-specific settings (e.g. `options` with their ids and labels).
    #[schema(value_type = Object)]
    pub config: Value,
    /// `auto` (everyone answered), `creator` (ended early) or `expired`.
    pub close_reason: Option<String>,
    /// By username.
    pub participants: Vec<ParticipantView>,
    /// Your own ballot, always visible to you.
    #[schema(value_type = Option<Object>)]
    pub my_ballot: Option<Value>,
    pub my_reason: String,
    pub actions: PollActions,
    /// The poll this one breaks a tie of.
    pub tie_break_of: Option<i64>,
    /// The follow-up vote called after this one tied.
    pub tie_break_poll: Option<i64>,
    /// Avatar votes: the proposed picture, and the one it replaced once applied.
    pub proposed_image: Option<Image>,
    pub previous_image: Option<Image>,
}

/// Which polls to list.
#[derive(Debug, Clone, Default)]
pub struct ListFilter {
    /// `open` or `closed`; open polls are few and come unpaged.
    pub status: Option<String>,
    /// Only polls the caller takes part in.
    pub participating: bool,
    pub cursor: Option<String>,
}

#[allow(clippy::struct_excessive_bools)] // a database row
struct ListRow {
    id: i64,
    title: String,
    kind: String,
    status: String,
    created_at: DateTime<Utc>,
    deadline_at: DateTime<Utc>,
    closed_at: Option<DateTime<Utc>>,
    result: Option<Value>,
    creator_id: i64,
    creator_username: String,
    creator_avatar: Option<String>,
    participant_count: i64,
    voted_count: i64,
    participating: bool,
    has_voted: bool,
    vetoed: bool,
    abstained: bool,
}

impl ListRow {
    fn into_summary(self, ctx: &Context, now: DateTime<Utc>) -> PollSummary {
        PollSummary {
            id: self.id,
            title: self.title,
            kind: self.kind,
            status: self.status,
            creator: Person {
                id: self.creator_id,
                username: self.creator_username,
                avatar: self.creator_avatar.map(|key| ctx.media.image(&key, now)),
            },
            created_at: self.created_at,
            deadline_at: self.deadline_at,
            closed_at: self.closed_at,
            participant_count: self.participant_count,
            voted_count: self.voted_count,
            my: MyState {
                participating: self.participating,
                has_voted: self.has_voted,
                vetoed: self.vetoed,
                abstained: self.abstained,
            },
            result: self.result,
        }
    }
}

/// Polls newest first, one query per page. Everyone may read every poll.
pub async fn list(ctx: &Context, actor: &Actor, filter: &ListFilter) -> AppResult<PollPage> {
    let status = match filter.status.as_deref() {
        None | Some("") => None,
        Some(status @ ("open" | "closed")) => Some(status.to_owned()),
        Some(_) => {
            return Err(
                domain::DomainError::new("invalid_status", "Nieznany status głosowania.")
                    .on("status")
                    .into(),
            );
        }
    };
    let cursor = filter.cursor.as_deref().map(Cursor::decode).transpose()?;
    let unpaged = status.as_deref() == Some("open");
    let limit = if unpaged { i64::MAX - 1 } else { PAGE_SIZE };
    let rows = sqlx::query_as!(
        ListRow,
        r#"select p.id, p.title, p.kind, p.status, p.created_at, p.deadline_at, p.closed_at, p.result,
                  u.id as creator_id, u.username as creator_username, u.avatar_key as creator_avatar,
                  (select count(*) from poll_participants x where x.poll_id = p.id) as "participant_count!",
                  (select count(*) from poll_participants x where x.poll_id = p.id and x.voted_at is not null) as "voted_count!",
                  me.user_id is not null as "participating!",
                  coalesce(me.voted_at is not null, false) as "has_voted!",
                  coalesce(me.vetoed, false) as "vetoed!",
                  coalesce(me.abstained, false) as "abstained!"
           from polls p
           join users u on u.id = p.creator_id
           left join poll_participants me on me.poll_id = p.id and me.user_id = $1
           where ($2::text is null or p.status = $2)
             and (not $3 or me.user_id is not null)
             and ($4::timestamptz is null or (p.created_at, p.id) < ($4, $5))
           order by p.created_at desc, p.id desc
           limit $6"#,
        actor.user_id,
        status,
        filter.participating,
        cursor.map(|c| c.at),
        cursor.map_or(0, |c| c.id),
        limit + 1,
    )
    .fetch_all(&ctx.db)
    .await?;
    let now = ctx.clock.now();
    let (rows, next_cursor) = if unpaged {
        (rows, None)
    } else {
        page(rows, usize::try_from(PAGE_SIZE).unwrap_or(20), |row| {
            Cursor {
                at: row.created_at,
                id: row.id,
            }
        })
    };
    Ok(PollPage {
        items: rows
            .into_iter()
            .map(|row| row.into_summary(ctx, now))
            .collect(),
        next_cursor,
    })
}

/// One poll: two queries (the poll with its creator and counts, then its participants).
pub async fn detail(ctx: &Context, actor: &Actor, id: i64) -> AppResult<PollDetail> {
    let mut uow = Uow::begin(&ctx.db).await?; // one consistent snapshot of both queries
    let row = sqlx::query!(
        r#"select p.id, p.title, p.kind, p.status, p.created_at, p.deadline_at, p.closed_at, p.result,
                  p.config, p.close_reason, p.creator_id, p.reminders_sent, p.proposed_image_key,
                  p.previous_image_key, p.tie_break_of,
                  u.username as creator_username, u.avatar_key as creator_avatar,
                  (select f.id from polls f where f.tie_break_of = p.id) as tie_break_poll
           from polls p join users u on u.id = p.creator_id where p.id = $1"#,
        id,
    )
    .fetch_optional(uow.conn())
    .await?
    .ok_or(AppError::NotFound)?;
    let participants = sqlx::query!(
        "select p.user_id, u.username, u.avatar_key, p.ballot, p.vetoed, p.abstained, p.reason,
                p.voted_at is not null as has_voted, p.last_reaction
         from poll_participants p join users u on u.id = p.user_id
         where p.poll_id = $1 order by u.username",
        id,
    )
    .fetch_all(uow.conn())
    .await?;
    uow.commit().await?;

    let now = ctx.clock.now();
    let open = row.status == "open";
    let mine = participants.iter().find(|p| p.user_id == actor.user_id);
    let poll = PollRow {
        id: row.id,
        creator_id: row.creator_id,
        title: row.title.clone(),
        kind: row.kind.clone(),
        config: row.config.clone(),
        status: row.status.clone(),
        result: row.result.clone(),
        created_at: row.created_at,
        reminders_sent: row.reminders_sent,
        proposed_image_key: row.proposed_image_key.clone(),
    };
    let my = Mine {
        participating: mine.is_some(),
        vetoed: mine.is_some_and(|m| m.vetoed),
    };
    let facts = facts(&poll, actor.user_id, my, row.tie_break_poll.is_some())?;
    let actions = PollActions {
        vote: policy::can(Action::Vote, &facts),
        veto: policy::can(Action::Veto, &facts),
        abstain: policy::can(Action::Abstain, &facts),
        close: policy::can(Action::Close, &facts),
        tie_break: policy::can(Action::TieBreak, &facts),
    };
    let image = |key: Option<String>| key.map(|key| ctx.media.image(&key, now));
    let summary = PollSummary {
        id: row.id,
        title: row.title,
        kind: row.kind,
        status: row.status,
        creator: Person {
            id: row.creator_id,
            username: row.creator_username,
            avatar: image(row.creator_avatar),
        },
        created_at: row.created_at,
        deadline_at: row.deadline_at,
        closed_at: row.closed_at,
        participant_count: i64::try_from(participants.len()).unwrap_or(i64::MAX),
        voted_count: i64::try_from(
            participants
                .iter()
                .filter(|p| p.has_voted == Some(true))
                .count(),
        )
        .unwrap_or(i64::MAX),
        my: MyState {
            participating: mine.is_some(),
            has_voted: mine.is_some_and(|m| m.has_voted == Some(true)),
            vetoed: mine.is_some_and(|m| m.vetoed),
            abstained: mine.is_some_and(|m| m.abstained),
        },
        result: row.result,
    };
    let my_ballot = mine.and_then(|m| m.ballot.clone());
    let my_reason = mine.map(|m| m.reason.clone()).unwrap_or_default();
    let participants = participants
        .into_iter()
        .map(|p| ParticipantView {
            user: Person {
                id: p.user_id,
                username: p.username,
                avatar: image(p.avatar_key),
            },
            has_voted: p.has_voted == Some(true),
            revealed: (!open).then(|| Revealed {
                ballot: p.ballot,
                vetoed: p.vetoed,
                abstained: p.abstained,
                reason: p.reason,
                reaction: Some(p.last_reaction).filter(|r| !r.is_empty()),
            }),
        })
        .collect();
    Ok(PollDetail {
        summary,
        config: row.config,
        close_reason: row.close_reason,
        participants,
        my_ballot,
        my_reason,
        actions,
        tie_break_of: row.tie_break_of,
        tie_break_poll: row.tie_break_poll,
        proposed_image: image(row.proposed_image_key),
        previous_image: image(row.previous_image_key),
    })
}
