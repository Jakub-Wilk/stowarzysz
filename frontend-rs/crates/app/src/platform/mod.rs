//! Everything that differs between the web build and the Android app. This is the only place
//! for `cfg(target_arch)` / `cfg(target_os)`; the rest of the app calls these functions.
//!
//! Each implementation provides:
//! - `storage::{get, set}`: small key-value settings (localStorage / a private JSON file);
//! - `api_origin()`, `user_agent()`;
//! - `sleep()`;
//! - `TokenStore` for the API client (the web one locks across tabs);
//! - `set_system_bar_color()` (Android status bar; the web uses the theme-color meta);
//! - `push::{status, enable, disable}`;
//! - `updater` (Android: download and install a new APK).
//!
//! `host` is a stand-in for `cargo test` on the development machine.

use std::rc::Rc;
use std::time::Duration;

#[cfg(target_os = "android")]
mod android;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
mod host;
#[cfg(target_arch = "wasm32")]
mod web;

#[cfg(target_os = "android")]
use android as imp;
#[cfg(not(any(target_arch = "wasm32", target_os = "android")))]
use host as imp;
#[cfg(target_arch = "wasm32")]
use web as imp;

pub use imp::{push, storage, updater};

/// Which build this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Web,
    Android,
    Host,
}

pub const KIND: Kind = imp::KIND;

/// Where the API is: the page's own origin on the web, a configured one on Android.
pub fn api_origin() -> String {
    imp::api_origin()
}

pub fn user_agent() -> String {
    format!("stowarzysz/{} ({:?})", crate::config::VERSION, KIND)
}

pub async fn sleep(duration: Duration) {
    imp::sleep(duration).await;
}

pub fn token_store() -> Rc<dyn api::TokenStore> {
    imp::token_store()
}

/// Match the system bars to the page background (`rgb(r, g, b)` from `getComputedStyle`).
pub fn set_system_bar_color(css_color: &str) {
    if let Some((r, g, b)) = parse_rgb(css_color) {
        imp::set_system_bar_color(r, g, b);
    }
}

/// Run once at startup (registers the service worker on the web).
pub fn init() {
    imp::init();
}

/// The notifications toggle's states.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushStatus {
    /// This browser/device cannot receive push at all.
    Unsupported,
    /// The server has no push keys: hide the toggle.
    Unconfigured,
    /// The user denied the permission (only the system settings can undo it).
    Blocked,
    Off,
    On,
}

/// Parse `rgb(14, 8, 24)` / `rgba(14, 8, 24, 1)`.
fn parse_rgb(css: &str) -> Option<(u8, u8, u8)> {
    let inner = css
        .trim()
        .strip_prefix("rgba(")
        .or_else(|| css.trim().strip_prefix("rgb("))?
        .strip_suffix(')')?;
    let mut parts = inner
        .split([',', ' ', '/'])
        .filter(|p| !p.is_empty())
        .map(|p| p.parse::<f64>().ok());
    let mut channel = || -> Option<u8> {
        let value = parts.next()??;
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        Some(value.round().clamp(0.0, 255.0) as u8)
    };
    Some((channel()?, channel()?, channel()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_computed_colors() {
        assert_eq!(parse_rgb("rgb(14, 8, 24)"), Some((14, 8, 24)));
        assert_eq!(parse_rgb("rgba(255, 255, 255, 0.5)"), Some((255, 255, 255)));
        assert_eq!(parse_rgb("rgb(1 2 3 / 50%)"), Some((1, 2, 3)));
        assert_eq!(parse_rgb("oklch(0.5 0.1 20)"), None);
    }
}
