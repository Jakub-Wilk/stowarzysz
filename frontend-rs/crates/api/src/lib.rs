//! The backend-rs API as the frontend uses it.
//!
//! - [`types`]: every schema of `openapi.json` (the committed snapshot of backend-rs's contract,
//!   refreshed with `scripts/update-openapi.sh`), generated at build time.
//! - [`Client`]: authenticated requests with token refresh; see [`TokenStore`].
//! - [`sse`]: the `/api/events` stream.
//!
//! The crate knows nothing about Dioxus or the platform it runs on: it compiles to wasm and to
//! native code alike, and its futures are not `Send` (nothing needs them to be).

mod client;
mod error;
pub mod sse;

#[allow(
    clippy::all,
    clippy::pedantic,
    missing_debug_implementations,
    unused_qualifications
)]
pub mod types {
    include!(concat!(env!("OUT_DIR"), "/types.rs"));
}

pub use client::{Client, LockGuard, TokenStore};
pub use error::{ApiError, FALLBACK_MESSAGE};
