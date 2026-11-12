use app::config::LogFormat;
use tracing_subscriber::EnvFilter;
use tracing_subscriber::prelude::*;

/// Install the global tracing subscriber. `RUST_LOG` overrides the default filter.
pub fn init(format: LogFormat) {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("info,tower_http=info,sqlx=warn"));
    let registry = tracing_subscriber::registry().with(filter);
    match format {
        LogFormat::Pretty => registry.with(tracing_subscriber::fmt::layer()).init(),
        LogFormat::Json => registry
            .with(
                tracing_subscriber::fmt::layer()
                    .json()
                    .flatten_event(true)
                    .with_current_span(true)
                    .with_span_list(false),
            )
            .init(),
    }
}
