//! HTTP API, SSE, CLI and wiring of stowarzysz.
//!
//! The binary (`src/main.rs`) is a thin CLI over this library, so integration tests can build
//! the exact production router with [`http::router`].

pub mod http;
pub mod shutdown;
pub mod state;
pub mod telemetry;

pub use state::AppState;
