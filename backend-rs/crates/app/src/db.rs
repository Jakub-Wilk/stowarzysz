//! Connection pool, migrations and the unit of work.

use std::time::Duration;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgConnection, PgPool, Postgres, Transaction};

use crate::AppResult;
use crate::config::Config;
use crate::events::{Audience, CHANNEL_EVENTS, Envelope, Event};
use crate::listener::Wake;

/// The schema, embedded in the binary and applied on startup.
pub static MIGRATOR: Migrator = sqlx::migrate!("../../migrations");

/// Open the pool. Connections are established lazily, so this only fails on a malformed URL.
pub fn pool(config: &Config) -> anyhow::Result<PgPool> {
    let options: PgConnectOptions = config.database_url.expose().parse()?;
    Ok(PgPoolOptions::new()
        .max_connections(20)
        .min_connections(1)
        .acquire_timeout(Duration::from_secs(5))
        .idle_timeout(Duration::from_secs(600))
        .connect_lazy_with(options.application_name("stowarzysz")))
}

pub async fn migrate(pool: &PgPool) -> anyhow::Result<()> {
    MIGRATOR.run(pool).await?;
    Ok(())
}

/// One database transaction plus the side effects it causes.
///
/// Every side effect is itself transactional, so it happens exactly when (and only if) the
/// transaction commits: SSE events are `pg_notify` calls (Postgres delivers them on commit),
/// push notifications are outbox rows, file deletions are `media_trash` rows. There is nothing
/// to run "after commit" in process memory, so nothing is lost if the process dies.
#[derive(Debug)]
pub struct Uow {
    tx: Transaction<'static, Postgres>,
}

impl Uow {
    pub async fn begin(pool: &PgPool) -> AppResult<Self> {
        Ok(Self {
            tx: pool.begin().await.map_err(anyhow::Error::from)?,
        })
    }

    /// The connection to run queries on: `query.execute(uow.conn())`.
    pub fn conn(&mut self) -> &mut PgConnection {
        &mut self.tx
    }

    /// Send an SSE event to `audience` once the transaction commits.
    pub async fn publish(&mut self, audience: Audience, event: Event) -> AppResult<()> {
        let payload =
            serde_json::to_string(&Envelope { audience, event }).map_err(anyhow::Error::from)?;
        sqlx::query!("select pg_notify($1, $2)", CHANNEL_EVENTS, payload)
            .execute(&mut *self.tx)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    /// Wake a background worker once the transaction commits (e.g. new outbox rows).
    pub async fn wake(&mut self, wake: Wake) -> AppResult<()> {
        sqlx::query!("select pg_notify($1, '')", wake.channel())
            .execute(&mut *self.tx)
            .await
            .map_err(anyhow::Error::from)?;
        Ok(())
    }

    pub async fn commit(self) -> AppResult<()> {
        self.tx.commit().await.map_err(anyhow::Error::from)?;
        Ok(())
    }
}
