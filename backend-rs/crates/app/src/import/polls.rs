//! Votes. What a vote decides moves from its JSON config to typed columns: `tie_break_of`, and
//! the subject (the member a profile vote is about, the claim a ruling decides, the resolution
//! a judgment is about). The pact subjects are linked once pacts exist.

use domain::polls::{Ballot, DURATION, PollConfig, PollKind, REMINDERS};
use serde_json::Value;

use crate::db::Uow;
use crate::media::Purpose;

use super::Problems;
use super::legacy::Legacy;
use super::media::MediaImport;

/// Links that can only be written once their targets exist.
#[derive(Debug, Default)]
pub struct Links {
    /// (poll, the poll it breaks a tie of)
    pub tie_breaks: Vec<(i64, i64)>,
    /// (poll, claim)
    pub claims: Vec<(i64, i64)>,
    /// (poll, pact)
    pub pacts: Vec<(i64, i64)>,
}

pub async fn import(
    uow: &mut Uow,
    data: &Legacy,
    media: &mut MediaImport,
    problems: &mut Problems,
) -> anyhow::Result<Links> {
    let mut links = Links::default();
    for poll in &data.polls {
        let Some(kind) = PollKind::parse(&poll.kind) else {
            problems.push(format!("poll {} has unknown kind {:?}", poll.id, poll.kind));
            continue;
        };
        let config = match PollConfig::from_json(kind, &poll.config) {
            Ok(config) => config,
            Err(error) => {
                problems.push(format!(
                    "poll {} has an unreadable config: {error}",
                    poll.id
                ));
                continue;
            }
        };
        if let Some(of) = poll.config.get("tie_break_of").and_then(Value::as_i64) {
            links.tie_breaks.push((poll.id, of));
        }
        let subject_user = match &config {
            PollConfig::Nickname { target_user_id, .. }
            | PollConfig::Avatar { target_user_id, .. } => Some(*target_user_id),
            _ => None,
        };
        match &config {
            PollConfig::PactRuling { proposal_id, .. } => {
                links.claims.push((poll.id, *proposal_id));
            }
            PollConfig::ResolutionJudgment { pact_id, .. } => links.pacts.push((poll.id, *pact_id)),
            _ => {}
        }
        let open = poll.status == "open";
        let next_reminder = if open {
            usize::try_from(poll.reminders_sent)
                .ok()
                .and_then(|sent| REMINDERS.get(sent))
                .map(|offset| poll.created_at + *offset)
        } else {
            None
        };
        let mut keys = [None, None];
        for (slot, (legacy, what)) in [
            (&poll.proposed_avatar, "proposed"),
            (&poll.previous_avatar, "previous"),
        ]
        .into_iter()
        .enumerate()
        {
            if legacy.is_empty() {
                continue;
            }
            let what = format!("{what} picture of poll {}", poll.id);
            keys[slot] = media
                .adopt(what, legacy, Purpose::Avatar)
                .await?
                .map(|stored| stored.key);
        }
        let [proposed, previous] = keys;
        sqlx::query!(
            "insert into polls (id, creator_id, title, kind, config, status, close_reason, result,
                                created_at, deadline_at, next_reminder_at, reminders_sent, closed_at,
                                proposed_image_key, previous_image_key, subject_user_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)",
            poll.id,
            poll.creator_id,
            poll.title,
            kind.as_str(),
            config.to_json(),
            poll.status,
            poll.close_reason,
            poll.result,
            poll.created_at,
            poll.created_at + DURATION,
            next_reminder,
            poll.reminders_sent,
            poll.closed_at,
            proposed,
            previous,
            subject_user,
        )
        .execute(uow.conn())
        .await?;
    }
    for p in &data.poll_participants {
        if let Some(ballot) = &p.ballot
            && let Err(error) = serde_json::from_value::<Ballot>(ballot.clone())
        {
            problems.push(format!(
                "the ballot of user {} in poll {} is unreadable: {error}",
                p.user_id, p.poll_id
            ));
        }
        sqlx::query!(
            "insert into poll_participants (poll_id, user_id, ballot, vetoed, abstained, reason, voted_at, last_reaction)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
            p.poll_id,
            p.user_id,
            p.ballot,
            p.vetoed,
            p.abstained,
            p.reason,
            p.voted_at,
            p.last_reaction,
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(links)
}

/// Write the links once every poll, pact and claim exists.
pub async fn link(uow: &mut Uow, links: &Links) -> anyhow::Result<()> {
    for (poll, of) in &links.tie_breaks {
        sqlx::query!("update polls set tie_break_of = $2 where id = $1", poll, of)
            .execute(uow.conn())
            .await?;
    }
    for (poll, claim) in &links.claims {
        sqlx::query!(
            "update polls set subject_claim_id = $2 where id = $1",
            poll,
            claim
        )
        .execute(uow.conn())
        .await?;
    }
    for (poll, pact) in &links.pacts {
        sqlx::query!(
            "update polls set subject_pact_id = $2 where id = $1",
            poll,
            pact
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(())
}
