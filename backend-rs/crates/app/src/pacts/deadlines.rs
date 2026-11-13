//! Pact deadlines, at their exact due times: unanswered invites and join requests lapse after
//! [`INVITE_TTL`]; at its deadline a pact that never started lapses and an active one becomes
//! overdue; the parties of an overdue pact are reminded every [`NUDGE_EVERY`] until they
//! settle it.

use chrono::{DateTime, Utc};
use domain::pacts::{NUDGE_EVERY, ParticipantState, Status};
use futures_util::future::BoxFuture;

#[cfg(doc)]
use domain::pacts::INVITE_TTL;

use super::load;
use super::services::{refresh, set_status, withdraw_waiting};
use crate::Context;
use crate::db::Uow;
use crate::events::{Audience, Event};
use crate::push::Notification;
use crate::scheduler::{Job, JobResult};

const BATCH: i64 = 50;

#[derive(Debug, Clone, Copy)]
pub struct PactDeadlines;

impl PactDeadlines {
    async fn process(ctx: &Context, now: DateTime<Utc>) -> JobResult {
        loop {
            let mut uow = Uow::begin(&ctx.db).await?;
            // Claimed with SKIP LOCKED: a pact someone is changing right now waits for the
            // next run instead of blocking this one.
            let due = sqlx::query_scalar!(
                "select k.id from pacts k
                 where k.status in ('proposed', 'active', 'awaiting_result')
                   and (k.next_nudge_at <= $1
                        or exists (select 1 from pact_participants p
                                   where p.pact_id = k.id and p.state in ('invited', 'requested')
                                     and p.expires_at <= $1))
                 order by k.id limit $2 for update of k skip locked",
                now,
                BATCH,
            )
            .fetch_all(uow.conn())
            .await?;
            for id in &due {
                Self::one(&mut uow, ctx, *id, now).await?;
            }
            uow.commit().await?;
            if due.len() < usize::try_from(BATCH)? {
                break;
            }
        }
        Ok(sqlx::query_scalar!(
            "select least(
                 (select min(next_nudge_at) from pacts
                  where status in ('proposed', 'active', 'awaiting_result')),
                 (select min(p.expires_at) from pact_participants p join pacts k on k.id = p.pact_id
                  where p.state in ('invited', 'requested')
                    and k.status in ('proposed', 'active', 'awaiting_result')))"
        )
        .fetch_one(&ctx.db)
        .await?)
    }

    /// Everything due for pact `id` (locked).
    async fn one(uow: &mut Uow, ctx: &Context, id: i64, now: DateTime<Utc>) -> anyhow::Result<()> {
        let (row, mut pact) = load(uow.conn(), id).await?;
        let lapsed: Vec<i64> = pact
            .participants
            .iter()
            .filter(|p| p.state.waiting() && p.expires_at.is_some_and(|at| at <= now))
            .map(|p| p.id)
            .collect();
        if !lapsed.is_empty() {
            sqlx::query!(
                "update pact_participants set state = 'expired', expires_at = null where id = any($1)",
                &lapsed,
            )
            .execute(uow.conn())
            .await?;
            sqlx::query!(
                "delete from pact_join_consents where participant_id = any($1) and approved is null",
                &lapsed,
            )
            .execute(uow.conn())
            .await?;
            for p in &mut pact.participants {
                if lapsed.contains(&p.id) {
                    p.state = ParticipantState::Expired;
                    p.expires_at = None;
                }
            }
            // A group bet may start once the stragglers are gone; a bet may be over.
            refresh(uow, &mut pact, now).await?;
        }
        let mut nudge = false;
        if row.next_nudge_at.is_some_and(|at| at <= now) {
            match pact.status {
                Status::Proposed => {
                    // It never started and its date has passed.
                    withdraw_waiting(uow, &mut pact).await?;
                    set_status(uow, &mut pact, Status::Cancelled).await?;
                }
                Status::Active => {
                    set_status(uow, &mut pact, Status::AwaitingResult).await?;
                    nudge = true;
                }
                Status::AwaitingResult => nudge = true,
                Status::Resolved | Status::Cancelled | Status::Void => {}
            }
        }
        if nudge {
            sqlx::query!(
                "update pacts set next_nudge_at = $2 where id = $1",
                id,
                now + NUDGE_EVERY,
            )
            .execute(uow.conn())
            .await?;
            let notification = Notification {
                title: "Termin umowy minął".into(),
                body: format!("„{}”: ustalcie wynik", pact.title),
                url: format!("/pacts/{id}"),
            };
            ctx.push
                .notify(uow, &pact.active_ids(), &notification)
                .await?;
        }
        uow.publish(Audience::All, Event::PactUpdated { pact_id: id })
            .await?;
        Ok(())
    }
}

impl Job for PactDeadlines {
    fn name(&self) -> &'static str {
        "pact-deadlines"
    }

    fn run<'a>(&'a self, ctx: &'a Context, now: DateTime<Utc>) -> BoxFuture<'a, JobResult> {
        Box::pin(Self::process(ctx, now))
    }
}
