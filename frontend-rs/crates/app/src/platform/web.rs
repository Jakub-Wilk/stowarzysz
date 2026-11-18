//! The browser (PWA). Browser APIs without a sync answer go through `document::eval`.

use std::rc::Rc;
use std::time::Duration;

use api::{LockGuard, TokenStore};
use dioxus::prelude::*;
use futures_util::future::LocalBoxFuture;

use super::Kind;

pub const KIND: Kind = Kind::Web;

/// The refresh token's key: the React frontend's, so a signed-in PWA stays signed in.
const REFRESH_KEY: &str = "auth.refresh";

pub fn api_origin() -> String {
    web_sys::window()
        .and_then(|w| w.location().origin().ok())
        .unwrap_or_default()
}

pub async fn sleep(duration: Duration) {
    gloo_timers::future::sleep(duration).await;
}

pub fn init() {
    // Push-only service worker (public/sw.js); no caching, see the comment there.
    document::eval(
        "if ('serviceWorker' in navigator) navigator.serviceWorker.register('/sw.js').catch(() => {});",
    );
}

pub fn set_system_bar_color(_r: u8, _g: u8, _b: u8) {
    // The theme-color meta (set by the theme) is the browser's equivalent.
}

pub mod storage {
    fn local() -> Option<web_sys::Storage> {
        web_sys::window()?.local_storage().ok()?
    }

    pub fn get(key: &str) -> Option<String> {
        local()?.get_item(key).ok()?
    }

    pub fn set(key: &str, value: Option<&str>) {
        // Storage may be unavailable (private mode); the setting then lasts for this session.
        if let Some(storage) = local() {
            let _ = match value {
                Some(value) => storage.set_item(key, value),
                None => storage.remove_item(key),
            };
        }
    }
}

struct WebTokens;

impl TokenStore for WebTokens {
    fn load(&self) -> Option<String> {
        storage::get(REFRESH_KEY)
    }

    fn save(&self, refresh: Option<&str>) {
        storage::set(REFRESH_KEY, refresh);
    }

    /// `navigator.locks` across tabs (the same lock name as the React frontend, so an old and a
    /// new tab never refresh at once). Without it, no lock.
    fn lock(&self) -> LocalBoxFuture<'_, LockGuard> {
        Box::pin(async {
            let mut eval = document::eval(
                r"
                if (!navigator.locks) { dioxus.send(false); return; }
                await navigator.locks.request('token-refresh', async () => {
                    dioxus.send(true);
                    await dioxus.recv();
                });
                ",
            );
            match eval.recv::<bool>().await {
                Ok(true) => LockGuard::new(Release(eval)),
                _ => LockGuard::none(),
            }
        })
    }
}

/// Lets the lock's callback return, which releases it.
struct Release(document::Eval);

impl Drop for Release {
    fn drop(&mut self) {
        let _ = self.0.send(true);
    }
}

pub fn token_store() -> Rc<dyn TokenStore> {
    Rc::new(WebTokens)
}

pub mod push {
    use api::Client;
    use api::types::PublicKey;
    use dioxus::prelude::*;
    use serde_json::Value;

    use crate::platform::PushStatus;

    /// `unsupported` / `blocked` / `off` / `on` from the browser's point of view.
    const STATE: &str = r"
        if (!('serviceWorker' in navigator) || !('PushManager' in window) || !('Notification' in window))
            return 'unsupported';
        if (Notification.permission === 'denied') return 'blocked';
        const registration = await navigator.serviceWorker.getRegistration();
        const subscription = registration && await registration.pushManager.getSubscription();
        return subscription ? 'on' : 'off';
    ";

    async fn public_key(client: &Client) -> Result<Option<String>, String> {
        let key: PublicKey = client
            .get("/api/push/public-key")
            .await
            .map_err(|e| e.message().to_owned())?;
        Ok(key.public_key)
    }

    pub async fn status(client: &Client) -> PushStatus {
        let state = document::eval(STATE)
            .join::<String>()
            .await
            .unwrap_or_else(|_| "unsupported".to_owned());
        match state.as_str() {
            "unsupported" => return PushStatus::Unsupported,
            "blocked" => return PushStatus::Blocked,
            _ => {}
        }
        match public_key(client).await {
            Ok(None) => PushStatus::Unconfigured,
            _ if state == "on" => PushStatus::On,
            _ => PushStatus::Off,
        }
    }

    /// Ask for permission (call from a click), subscribe and register with the server.
    pub async fn enable(client: &Client) -> Result<(), String> {
        let key = public_key(client)
            .await?
            .ok_or("Powiadomienia nie są skonfigurowane na serwerze.")?;
        let script = format!(
            r"
            const permission = await Notification.requestPermission();
            if (permission !== 'granted') return null;
            const registration = await navigator.serviceWorker.register('/sw.js');
            await navigator.serviceWorker.ready;
            const key = '{key}'.replace(/-/g, '+').replace(/_/g, '/');
            const raw = atob(key + '='.repeat((4 - key.length % 4) % 4));
            const applicationServerKey = Uint8Array.from(raw, (c) => c.charCodeAt(0));
            const subscription = await registration.pushManager.subscribe({{
                userVisibleOnly: true, applicationServerKey,
            }});
            return subscription.toJSON();
            "
        );
        let subscription = document::eval(&script)
            .join::<Value>()
            .await
            .map_err(|_| "Przeglądarka nie pozwoliła włączyć powiadomień.".to_owned())?;
        if subscription.is_null() {
            return Err("Nie udzielono zgody na powiadomienia.".to_owned());
        }
        client
            .put::<_, ()>("/api/push/subscription", &subscription)
            .await
            .map_err(|e| e.message().to_owned())
    }

    pub async fn disable(client: &Client) -> Result<(), String> {
        let endpoint = document::eval(
            r"
            const registration = await navigator.serviceWorker.getRegistration();
            const subscription = registration && await registration.pushManager.getSubscription();
            if (!subscription) return null;
            const endpoint = subscription.endpoint;
            await subscription.unsubscribe();
            return endpoint;
            ",
        )
        .join::<Option<String>>()
        .await
        .map_err(|_| "Nie udało się wyłączyć powiadomień.".to_owned())?;
        if let Some(endpoint) = endpoint {
            client
                .delete::<_, ()>(
                    "/api/push/subscription",
                    Some(&serde_json::json!({ "endpoint": endpoint })),
                )
                .await
                .map_err(|e| e.message().to_owned())?;
        }
        Ok(())
    }
}

/// The web reloads instead (see `updates`).
#[allow(clippy::unused_async)] // same signatures as the Android updater
pub mod updater {
    use crate::updates::Manifest;

    pub const SUPPORTED: bool = false;

    pub async fn download_and_install(
        _client: &api::Client,
        _manifest: &Manifest,
        _progress: impl FnMut(u64, u64),
    ) -> Result<(), String> {
        Err("Aktualizacje instaluje tylko aplikacja na Androida.".to_owned())
    }

    pub fn can_install() -> bool {
        false
    }

    pub fn open_install_settings() {}

    pub fn take_error() -> Option<String> {
        None
    }
}
