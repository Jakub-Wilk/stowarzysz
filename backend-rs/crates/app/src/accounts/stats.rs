//! Per-member statistics for the management panel: a fixed number of aggregate queries for any
//! number of users.

use std::collections::HashMap;

use sqlx::PgPool;

use super::views::{KindStats, PactStats, VotingStats};
use crate::AppResult;

/// Voting statistics of `user_ids` (missing users have none).
pub async fn voting(db: &PgPool, user_ids: &[i64]) -> AppResult<HashMap<i64, VotingStats>> {
    let rows = sqlx::query!(
        r#"select p.user_id,
                  count(*) as "votes_cast!",
                  count(*) filter (where p.vetoed) as "veto_count!",
                  avg((p.ballot ->> 'value')::float8) as average
           from poll_participants p join polls o on o.id = p.poll_id
           where o.status = 'closed' and o.kind = 'score' and p.ballot is not null
             and p.user_id = any($1)
           group by p.user_id"#,
        user_ids,
    )
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| {
            #[allow(clippy::cast_precision_loss)] // counts are small
            let percent = if row.votes_cast > 0 {
                round(row.veto_count as f64 / row.votes_cast as f64 * 100.0, 1)
            } else {
                0.0
            };
            let stats = VotingStats {
                votes_cast: row.votes_cast,
                average_score: row.average.map(|avg| round(avg, 2)),
                veto_count: row.veto_count,
                veto_percent: percent,
            };
            (row.user_id, stats)
        })
        .collect())
}

/// Pact records of `user_ids`: verdicts of confirmed claims (void ones count for nobody) and the
/// money of confirmed pact settlements (goods are not money).
pub async fn pacts(db: &PgPool, user_ids: &[i64]) -> AppResult<HashMap<i64, PactStats>> {
    let verdicts = sqlx::query!(
        r#"select v.user_id, k.kind,
                  count(*) filter (where v.verdict = 'won') as "won!",
                  count(*) filter (where v.verdict = 'lost') as "lost!",
                  count(*) filter (where v.verdict = 'draw') as "draw!"
           from pact_verdicts v
           join pact_claims c on c.id = v.claim_id
           join pacts k on k.id = c.pact_id
           where c.state = 'confirmed' and v.verdict <> 'void' and v.user_id = any($1)
           group by v.user_id, k.kind"#,
        user_ids,
    )
    .fetch_all(db)
    .await?;
    let money = sqlx::query!(
        r#"select u.id as "user_id!",
                  coalesce(sum(o.amount) filter (where o.creditor_id = u.id), 0)::bigint as "won!",
                  coalesce(sum(o.amount) filter (where o.debtor_id = u.id), 0)::bigint as "lost!"
           from unnest($1::bigint[]) as u(id)
           join ledger_obligations o on u.id in (o.creditor_id, o.debtor_id) and o.item = ''
           join ledger_entries e on e.id = o.entry_id and e.source = 'pact' and e.status = 'confirmed'
           group by u.id"#,
        user_ids,
    )
    .fetch_all(db)
    .await?;

    let mut stats: HashMap<i64, PactStats> = HashMap::new();
    for row in verdicts {
        let record = stats.entry(row.user_id).or_default();
        record.won += row.won;
        record.lost += row.lost;
        record.draw += row.draw;
        record.by_kind.insert(
            row.kind,
            KindStats {
                won: row.won,
                lost: row.lost,
                draw: row.draw,
            },
        );
    }
    for row in money {
        let record = stats.entry(row.user_id).or_default();
        record.money_won = row.won;
        record.money_lost = row.lost;
    }
    Ok(stats)
}

/// Half away from zero, like the figures people expect (Python's `round` differs only on exact
/// binary halves, which averages of small integers hardly produce).
fn round(value: f64, decimals: i32) -> f64 {
    let scale = 10_f64.powi(decimals);
    (value * scale).round() / scale
}
