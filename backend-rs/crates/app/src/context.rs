use std::sync::Arc;

use sqlx::PgPool;

use crate::auth::password::Passwords;
use crate::auth::tokens::Signer;
use crate::clock::Clock;
use crate::config::Config;
use crate::events::Hub;
use crate::ledger::Rates;
use crate::listener::Signals;
use crate::media::Media;
use crate::ocr::Ocr;
use crate::push::Push;
use crate::santa::Sealer;

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
    pub signer: Signer,
    pub passwords: Passwords,
    pub media: Media,
    /// Exchange rates (Frankfurter, cached in the database).
    pub rates: Rates,
    /// Receipt OCR (Gemini candidates and their health).
    pub ocr: Ocr,
    /// Seals Secret Santa pairings while a round is active.
    pub santa: Sealer,
}

impl Context {
    pub fn new(config: Config, db: PgPool, clock: Arc<dyn Clock>) -> Self {
        Self {
            push: Push::new(config.vapid.is_some()),
            signer: Signer::new(&config.secrets.jwt, config.legacy.secret_key.as_ref()),
            passwords: Passwords::default(),
            media: Media::new(&config),
            rates: Rates::new(&config.rates_url),
            ocr: Ocr::new(&config),
            santa: Sealer::new(config.secrets.santa_seal.expose()),
            config: Arc::new(config),
            db,
            clock,
            hub: Hub::default(),
            signals: Signals::default(),
        }
    }
}
