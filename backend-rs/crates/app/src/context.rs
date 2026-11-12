use std::sync::Arc;

use sqlx::PgPool;

use crate::clock::Clock;
use crate::config::Config;
use crate::events::Hub;
use crate::listener::Signals;
use crate::push::Push;

/// The services every use case may need. Built once at startup and shared behind an `Arc`.
#[derive(Debug)]
pub struct Context {
    pub config: Arc<Config>,
    pub db: PgPool,
    pub clock: Arc<dyn Clock>,
    /// Committed SSE events, fanned out to this process's open streams.
    pub hub: Hub,
    pub push: Push,
    /// Wake-ups for this process's background workers.
    pub signals: Signals,
}

impl Context {
    pub fn new(config: Config, db: PgPool, clock: Arc<dyn Clock>) -> Self {
        let push = Push::new(config.vapid.is_some());
        Self {
            config: Arc::new(config),
            db,
            clock,
            hub: Hub::default(),
            push,
            signals: Signals::default(),
        }
    }
}
