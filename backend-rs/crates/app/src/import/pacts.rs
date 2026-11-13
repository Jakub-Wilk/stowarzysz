//! Pacts with their participants, claims (verdicts become rows), confirmations, join consents and
//! pictures. The stored `outcome` is not copied: it is derived from the confirmed claims now,
//! and the importer checks that the derivation gives what Django had stored.

use chrono::{DateTime, Utc};
use domain::pacts::terms::PactConfig;
use domain::pacts::{
    ClaimState, INVITE_TTL, NUDGE_EVERY, Outcome, PactKind, ParticipantState, Role, Status, Verdict,
};
use serde_json::Value;

use crate::db::Uow;
use crate::media::Purpose;

use super::Problems;
use super::legacy::Legacy;
use super::media::MediaImport;

/// Pictures dropped because their file was missing.
pub async fn import(
    uow: &mut Uow,
    data: &Legacy,
    media: &mut MediaImport,
    problems: &mut Problems,
    now: DateTime<Utc>,
) -> anyhow::Result<usize> {
    for pact in &data.pacts {
        let Some(kind) = PactKind::parse(&pact.kind) else {
            problems.push(format!("pact {} has unknown kind {:?}", pact.id, pact.kind));
            continue;
        };
        // Django declared a `declined` status but never set it.
        let status = match pact.status.as_str() {
            "declined" => Some(Status::Cancelled),
            other => Status::parse(other),
        };
        let Some(status) = status else {
            problems.push(format!(
                "pact {} has unknown status {:?}",
                pact.id, pact.status
            ));
            continue;
        };
        let config = match PactConfig::from_json(&pact.config) {
            Ok(config) => config,
            Err(error) => {
                problems.push(format!(
                    "pact {} has an unreadable config: {error}",
                    pact.id
                ));
                continue;
            }
        };
        // When the scheduler next looks at it.
        let next_nudge = match status {
            Status::Proposed | Status::Active => pact.due_at,
            Status::AwaitingResult => Some(pact.last_nudged_at.unwrap_or(now) + NUDGE_EVERY),
            Status::Resolved | Status::Cancelled | Status::Void => None,
        };
        sqlx::query!(
            "insert into pacts (id, creator_id, kind, title, condition, notes, due_at, config, status,
                                created_at, resolved_at, next_nudge_at, judgment_poll_id)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)",
            pact.id,
            pact.creator_id,
            kind.as_str(),
            pact.title,
            pact.condition,
            pact.notes,
            pact.due_at,
            config.to_json(),
            status.as_str(),
            pact.created_at,
            pact.resolved_at,
            next_nudge,
            pact.judgment_poll_id,
        )
        .execute(uow.conn())
        .await?;
    }
    for p in &data.pact_participants {
        let (Some(role), Some(state)) = (Role::parse(&p.role), ParticipantState::parse(&p.state))
        else {
            problems.push(format!(
                "pact participant {} has an unknown role or state",
                p.id
            ));
            continue;
        };
        let expires = state.waiting().then(|| p.created_at + INVITE_TTL);
        sqlx::query!(
            "insert into pact_participants (id, pact_id, user_id, role, side, stake_amount, stake_note,
                                            state, created_at, expires_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)",
            p.id,
            p.pact_id,
            p.user_id,
            role.as_str(),
            p.side,
            p.stake_amount.filter(|a| *a > 0).map(i64::from),
            p.stake_note,
            state.as_str(),
            p.created_at,
            expires,
        )
        .execute(uow.conn())
        .await?;
    }
    for claim in &data.claims {
        let (Ok(outcome), Some(state)) = (
            Outcome::from_json(&claim.result),
            ClaimState::parse(&claim.state),
        ) else {
            problems.push(format!(
                "claim {} has an unreadable result or state",
                claim.id
            ));
            continue;
        };
        sqlx::query!(
            "insert into pact_claims (id, pact_id, wager_id, proposed_by, result, state, created_at, decided_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
            claim.id,
            claim.pact_id,
            claim.wager_id,
            claim.proposed_by_id,
            outcome.to_json(),
            state.as_str(),
            claim.created_at,
            claim.decided_at,
        )
        .execute(uow.conn())
        .await?;
        let Some(verdicts) = claim.verdicts.as_object() else {
            problems.push(format!("claim {} has unreadable verdicts", claim.id));
            continue;
        };
        for (user, verdict) in verdicts {
            let user = user.parse::<i64>().ok();
            let verdict = verdict.as_str().and_then(Verdict::parse);
            let (Some(user), Some(verdict)) = (user, verdict) else {
                problems.push(format!("claim {} has an unreadable verdict", claim.id));
                continue;
            };
            sqlx::query!(
                "insert into pact_verdicts (claim_id, user_id, verdict) values ($1, $2, $3)",
                claim.id,
                user,
                verdict.as_str(),
            )
            .execute(uow.conn())
            .await?;
        }
    }
    for c in &data.confirmations {
        sqlx::query!(
            "insert into pact_claim_confirmations (claim_id, user_id, created_at) values ($1, $2, $3)",
            c.proposal_id,
            c.user_id,
            c.created_at,
        )
        .execute(uow.conn())
        .await?;
    }
    for c in &data.consents {
        sqlx::query!(
            "insert into pact_join_consents (participant_id, approver_id, approved) values ($1, $2, $3)",
            c.participant_id,
            c.approver_id,
            c.approved,
        )
        .execute(uow.conn())
        .await?;
    }
    let mut dropped = 0;
    for a in &data.pact_attachments {
        let what = format!("picture {} of pact {}", a.id, a.owner_id);
        let Some(image) = media.adopt(what, &a.image, Purpose::Photo).await? else {
            dropped += 1;
            continue;
        };
        sqlx::query!(
            "insert into pact_attachments (id, pact_id, uploaded_by, image_key, width, height, caption, created_at)
             values ($1, $2, $3, $4, $5, $6, $7, $8)",
            a.id,
            a.owner_id,
            a.uploaded_by_id,
            image.key,
            image.width,
            image.height,
            a.caption,
            a.created_at,
        )
        .execute(uow.conn())
        .await?;
    }
    Ok(dropped)
}

/// Each finished pact's outcome, derived from its claims, must be what Django stored.
pub async fn verify_outcomes(
    uow: &mut Uow,
    data: &Legacy,
    problems: &mut Problems,
) -> anyhow::Result<()> {
    for pact in &data.pacts {
        if !matches!(pact.status.as_str(), "resolved" | "void") {
            continue;
        }
        let (_, loaded) = crate::pacts::load(uow.conn(), pact.id).await?;
        let derived = loaded.outcome().map(|o| serde_json::json!(o));
        let stored = pact.outcome.clone().filter(|o| !o.is_null());
        if derived != stored {
            problems.push(format!(
                "pact {}: the outcome derived from its claims ({}) differs from the stored one ({})",
                pact.id,
                derived.unwrap_or(Value::Null),
                stored.unwrap_or(Value::Null)
            ));
        }
    }
    Ok(())
}
