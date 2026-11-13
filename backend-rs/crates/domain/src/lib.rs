//! Pure business rules of stowarzysz.
//!
//! Nothing in this crate performs I/O, touches the database or is `async`: it takes plain values
//! and returns plain values (or a [`DomainError`]), so every rule can be unit- and
//! property-tested in isolation. User-facing messages are Polish.

#[macro_use]
mod macros;

pub mod accounts;
mod error;
pub mod ledger;
pub mod pacts;
pub mod polls;

pub use error::DomainError;
