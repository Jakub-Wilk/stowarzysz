//! Use cases, persistence and integrations of stowarzysz.
//!
//! This crate knows nothing about HTTP: services take plain inputs and return plain outputs or an
//! [`AppError`], which the server crate maps to a response.

pub mod clock;
pub mod config;
mod context;
pub mod db;
mod error;
pub mod events;
pub mod listener;
pub mod push;
pub mod scheduler;

pub use context::Context;
pub use error::{AppError, AppResult};
