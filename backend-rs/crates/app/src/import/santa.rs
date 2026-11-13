//! Secret Santa. Ended rounds are copied as they are. An active round's pairings are Fernet
//! tokens (`"<giver>:<receiver>"`, keyed by Django's `SECRET_SANTA_KEY`): each is opened, checked
//! against its giver, sealed again with the new key, and the new seal is opened back to make
//! sure it holds the same receiver.

use domain::santa::{Mode, Status};
use serde_json::Value;

use crate::Context;
use crate::db::Uow;

use super::Problems;
use super::legacy::Legacy;

/// Pairings re-sealed.
pub async fn import(
    uow: &mut Uow,
    ctx: &Context,
    data: &Legacy,
    legacy_key: Option<&str>,
    problems: &mut Problems,
) -> anyhow::Result<usize> {
    let mut active = std::collections::HashSet::new();
    for event in &data.santa_events {
        let (Some(status), Some(mode)) = (Status::parse(&event.status), Mode::parse(&event.mode))
        else {
            problems.push(format!(
                "Santa round {} has an unknown status or mode",
                event.id
            ));
            continue;
        };
        let tiers: Option<Vec<i64>> = event
            .gift_tiers
            .as_array()
            .and_then(|list| list.iter().map(Value::as_i64).collect());
        let Some(tiers) = tiers else {
            problems.push(format!(
                "Santa round {} has unreadable gift tiers",
                event.id
            ));
            continue;
        };
        if status == Status::Active {
            active.insert(event.id);
        }
        sqlx::query!(
            "insert into santa_events (id, status, mode, deadline, gift_tiers, created_by, created_at, ended_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
            event.id,
            status.as_str(),
            mode.as_str(),
            event.deadline,
            &tiers,
            event.created_by_id,
            event.created_at,
            event.ended_at,
        )
        .execute(uow.conn())
        .await?;
    }
    let fernet = match (legacy_key, active.is_empty()) {
        (_, true) => None,
        (None, false) => anyhow::bail!(
            "a Secret Santa round is running: set LEGACY_SANTA_KEY (Django's SECRET_SANTA_KEY) so its draw can be re-sealed"
        ),
        (Some(key), false) => Some(
            fernet::Fernet::new(key)
                .ok_or_else(|| anyhow::anyhow!("LEGACY_SANTA_KEY is not a Fernet key"))?,
        ),
    };
    let mut resealed = 0;
    for a in &data.santa_assignments {
        let (sealed, receiver) = match (&fernet, active.contains(&a.event_id)) {
            (Some(fernet), true) => {
                let opened = fernet
                    .decrypt(&a.payload)
                    .ok()
                    .and_then(|plain| String::from_utf8(plain).ok())
                    .and_then(|text| {
                        let (giver, receiver) = text.split_once(':')?;
                        Some((giver.parse::<i64>().ok()?, receiver.parse::<i64>().ok()?))
                    });
                let Some((giver, receiver)) = opened.filter(|(giver, _)| *giver == a.giver_id)
                else {
                    problems.push(format!(
                        "Santa pairing {} cannot be opened with LEGACY_SANTA_KEY (or is not its giver's)",
                        a.id
                    ));
                    continue;
                };
                let sealed = ctx.santa.seal(a.event_id, giver, a.tier_index, receiver);
                if ctx.santa.open(a.event_id, giver, a.tier_index, &sealed) != Ok(receiver) {
                    problems.push(format!(
                        "Santa pairing {} does not survive re-sealing",
                        a.id
                    ));
                }
                resealed += 1;
                (Some(sealed), None)
            }
            _ => (None, a.receiver_id),
        };
        sqlx::query!(
            "insert into santa_assignments (id, event_id, giver_id, tier_index, sealed, receiver_id)
             values ($1, $2, $3, $4, $5, $6)",
            a.id,
            a.event_id,
            a.giver_id,
            a.tier_index,
            sealed,
            receiver,
        )
        .execute(uow.conn())
        .await?;
    }
    for g in &data.santa_gifts {
        sqlx::query!(
            "insert into santa_gifts (id, assignment_id, amount, note) values ($1, $2, $3, $4)",
            g.id,
            g.assignment_id,
            i64::from(g.amount),
            g.note,
        )
        .execute(uow.conn())
        .await?;
    }
    for h in &data.santa_help {
        let ideas: Option<Vec<String>> = h
            .ideas
            .as_array()
            .and_then(|list| list.iter().map(|v| v.as_str().map(str::to_owned)).collect());
        let Some(ideas) = ideas else {
            problems.push(format!("Santa help request {} has unreadable ideas", h.id));
            continue;
        };
        sqlx::query!(
            "insert into santa_help_requests (id, event_id, receiver_id, tier_index, ideas, asked_at, answered_at)
             values ($1, $2, $3, $4, $5, $6, $7)",
            h.id,
            h.event_id,
            h.receiver_id,
            h.tier_index,
            &ideas,
            h.asked_at,
            h.answered_at,
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(resealed)
}
