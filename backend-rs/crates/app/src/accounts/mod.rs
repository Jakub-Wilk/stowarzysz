//! Accounts: who people are, what members and administrators may see and change about them.

pub mod activation;
pub mod birthdays;
pub mod profile;
pub mod stats;
pub mod users;
mod views;

pub use views::{KindStats, LoginUser, ManagedUser, Me, PactStats, Person, VotingStats};
