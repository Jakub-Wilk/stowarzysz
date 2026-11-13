//! Use cases, persistence and integrations of stowarzysz.
//!
//! This crate knows nothing about HTTP: services take plain inputs and return plain outputs or an
//! [`AppError`], which the server crate maps to a response. Response views derive `ToSchema` so
//! the API documents them as they are.

pub mod accounts;
pub mod auth;
pub mod clock;
pub mod config;
mod context;
pub mod db;
mod error;
pub mod events;
pub mod ledger;
pub mod listener;
pub mod media;
pub mod ocr;
mod orchestration;
pub mod pacts;
pub mod pagination;
pub mod polls;
pub mod push;
pub mod santa;
pub mod scheduler;

pub use context::Context;
pub use error::{AppError, AppResult};
