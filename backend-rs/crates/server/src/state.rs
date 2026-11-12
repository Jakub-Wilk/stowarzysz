use std::sync::Arc;

use app::config::Config;

/// Shared, cheaply clonable state handed to every handler.
#[derive(Debug, Clone)]
pub struct AppState {
    pub config: Arc<Config>,
}

impl AppState {
    pub fn new(config: Config) -> Self {
        Self {
            config: Arc::new(config),
        }
    }
}
