//! Use cases, persistence and integrations of stowarzysz.
//!
//! This crate knows nothing about HTTP: services take plain inputs and return plain outputs or an
//! [`AppError`], which the server crate maps to a response.

pub mod config;
mod error;

pub use error::{AppError, AppResult};
