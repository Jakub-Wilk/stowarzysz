//! A stand-in for the development machine, so `cargo test` can build the app's modules. It is
//! not a supported platform: storage is in memory and push/updates are unsupported.

use std::rc::Rc;
use std::time::Duration;

use api::{LockGuard, TokenStore};
use futures_util::future::LocalBoxFuture;

use super::Kind;

pub const KIND: Kind = Kind::Host;

pub fn api_origin() -> String {
    "http://localhost:8000".to_owned()
}

pub async fn sleep(duration: Duration) {
    tokio::time::sleep(duration).await;
}

pub fn init() {}

pub fn set_system_bar_color(_r: u8, _g: u8, _b: u8) {}

pub mod storage {
    use std::collections::HashMap;
    use std::sync::{Mutex, PoisonError};

    static VALUES: Mutex<Option<HashMap<String, String>>> = Mutex::new(None);

    pub fn get(key: &str) -> Option<String> {
        let values = VALUES.lock().unwrap_or_else(PoisonError::into_inner);
        values.as_ref()?.get(key).cloned()
    }

    pub fn set(key: &str, value: Option<&str>) {
        let mut values = VALUES.lock().unwrap_or_else(PoisonError::into_inner);
        let values = values.get_or_insert_with(HashMap::new);
        match value {
            Some(value) => values.insert(key.to_owned(), value.to_owned()),
            None => values.remove(key),
        };
    }
}

struct HostTokens;

impl TokenStore for HostTokens {
    fn load(&self) -> Option<String> {
        storage::get("auth.refresh")
    }

    fn save(&self, refresh: Option<&str>) {
        storage::set("auth.refresh", refresh);
    }

    fn lock(&self) -> LocalBoxFuture<'_, LockGuard> {
        Box::pin(async { LockGuard::none() })
    }
}

pub fn token_store() -> Rc<dyn TokenStore> {
    Rc::new(HostTokens)
}

#[allow(clippy::unused_async)] // same signatures as the other platforms
pub mod push {
    use api::Client;

    use crate::platform::PushStatus;

    pub async fn status(_client: &Client) -> PushStatus {
        PushStatus::Unsupported
    }

    pub async fn enable(_client: &Client) -> Result<(), String> {
        Err("Brak powiadomień na tej platformie.".to_owned())
    }

    pub async fn disable(_client: &Client) -> Result<(), String> {
        Ok(())
    }
}

#[allow(clippy::unused_async)] // same signatures as the Android updater
pub mod updater {
    use crate::updates::Manifest;

    pub const SUPPORTED: bool = false;

    pub async fn download_and_install(
        _client: &api::Client,
        _manifest: &Manifest,
        _progress: impl FnMut(u64, u64),
    ) -> Result<(), String> {
        Err("Brak aktualizacji na tej platformie.".to_owned())
    }

    pub fn can_install() -> bool {
        false
    }

    pub fn open_install_settings() {}

    pub fn take_error() -> Option<String> {
        None
    }
}
