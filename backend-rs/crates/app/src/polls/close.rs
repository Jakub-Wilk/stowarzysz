//! Ending a poll: count the ballots once, apply what the vote decided, freeze the result.

use domain::polls::result::{Entry, compute};
use domain::polls::{Ballot, CloseReason, closing_summary};
use serde_json::Value;

use super::{PollRow, subjects};
use crate::db::Uow;
use crate::events::{Audience, Event};
use crate::push::Notification;
use crate::{AppResult, Context};

/// Close `poll` (locked in `uow`) for `reason`.
pub(crate) async fn close(
    uow: &mut Uow,
    ctx: &Context,
    poll: &PollRow,
    reason: CloseReason,
) -> AppResult<Value> {
    let kind = poll.kind()?;
    let config = poll.config()?;
    let rules = kind.rules();
    if rules.allows_abstain {
        // Whoever never answered counts as abstaining.
        sqlx::query!(
            "update poll_participants set abstained = true, reason = $2 where poll_id = $1 and voted_at is null",
            poll.id,
            reason.auto_abstain_reason(),
        )
        .execute(uow.conn())
        .await?;
    }
    let cast = sqlx::query!(
        r#"select ballot as "ballot!", vetoed from poll_participants
           where poll_id = $1 and voted_at is not null and not abstained and ballot is not null"#,
        poll.id,
    )
    .fetch_all(uow.conn())
    .await?;
    let entries = cast
        .into_iter()
        .map(|row| {
            Ok(Entry {
                ballot: serde_json::from_value::<Ballot>(row.ballot)?,
                vetoed: row.vetoed,
            })
        })
        .collect::<Result<Vec<_>, serde_json::Error>>()
        .map_err(anyhow::Error::from)?;
    let abstain_count = if rules.allows_abstain {
        sqlx::query_scalar!(
            r#"select count(*) as "n!" from poll_participants where poll_id = $1 and abstained"#,
            poll.id
        )
        .fetch_one(uow.conn())
        .await
        .map(Some)?
    } else {
        None
    };
    let mut result = compute(kind, &config, &entries, abstain_count);
    // What the vote decided (a profile change, a pact outcome) is applied in the same
    // transaction; its outcome is recorded in the result, never raised.
    let applied = subjects::on_close(uow, ctx, poll, &config, &result).await?;
    if let (Some(object), Some(extra)) = (result.as_object_mut(), applied) {
        object.extend(extra);
    }

    sqlx::query!(
        "update polls set status = 'closed', close_reason = $2, result = $3, closed_at = $4, next_reminder_at = null
         where id = $1",
        poll.id,
        reason.as_str(),
        result,
        ctx.clock.now(),
    )
    .execute(uow.conn())
    .await?;

    let everyone = sqlx::query_scalar!(
        "select user_id from poll_participants where poll_id = $1",
        poll.id
    )
    .fetch_all(uow.conn())
    .await?;
    uow.publish(Audience::All, Event::PollClosed { poll_id: poll.id })
        .await?;
    let notification = Notification {
        title: "Głosowanie zakończone".into(),
        body: closing_summary(&poll.title, &result),
        url: format!("/voting/{}", poll.id),
    };
    ctx.push.notify(uow, &everyone, &notification).await?;
    Ok(result)
}
