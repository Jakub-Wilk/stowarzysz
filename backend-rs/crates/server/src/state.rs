use std::sync::Arc;

use app::Context;
use tokio_util::sync::CancellationToken;

use crate::http::guard::Limiters;

/// Shared, cheaply clonable state handed to every handler.
#[derive(Debug, Clone)]
pub struct AppState {
    pub ctx: Arc<Context>,
    pub limiters: Arc<Limiters>,
    /// Cancelled on shutdown; long-lived responses (SSE) end when it fires so the server can
    /// drain.
    pub shutdown: CancellationToken,
}

impl AppState {
    pub fn new(ctx: Arc<Context>) -> Self {
        Self::with_shutdown(ctx, CancellationToken::new())
    }

    pub fn with_shutdown(ctx: Arc<Context>, shutdown: CancellationToken) -> Self {
        let limiters = Arc::new(Limiters::new(&ctx.config));
        Self {
            ctx,
            limiters,
            shutdown,
        }
    }
}
