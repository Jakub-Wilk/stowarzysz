use std::sync::Arc;

use app::Context;
use tokio_util::sync::CancellationToken;

/// Shared, cheaply clonable state handed to every handler.
#[derive(Debug, Clone)]
pub struct AppState {
    pub ctx: Arc<Context>,
    /// Cancelled on shutdown; long-lived responses (SSE) end when it fires so the server can
    /// drain.
    pub shutdown: CancellationToken,
}

impl AppState {
    pub fn new(ctx: Arc<Context>) -> Self {
        Self {
            ctx,
            shutdown: CancellationToken::new(),
        }
    }

    pub fn with_shutdown(ctx: Arc<Context>, shutdown: CancellationToken) -> Self {
        Self { ctx, shutdown }
    }
}
