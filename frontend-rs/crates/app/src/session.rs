//! Who is signed in. The API client owns the tokens; this keeps the UI's view of them.

use api::types::Me;
use api::{ApiError, Client};
use dioxus::prelude::*;

#[derive(Debug, Clone)]
pub enum Auth {
    /// A session is stored and `/api/me` is on its way.
    Checking,
    SignedIn(Me),
    SignedOut,
    /// A session is stored but the server can't be reached (offline, 5xx): offer a retry.
    Unreachable(String),
}

#[derive(Debug, Clone, Copy)]
pub struct Session {
    pub auth: Signal<Auth>,
}

impl PartialEq for Session {
    fn eq(&self, other: &Self) -> bool {
        self.auth == other.auth
    }
}

impl Session {
    pub fn me(&self) -> Option<Me> {
        match &*self.auth.read() {
            Auth::SignedIn(me) => Some(me.clone()),
            _ => None,
        }
    }

    pub fn is_superuser(&self) -> bool {
        self.me().is_some_and(|me| me.is_superuser)
    }

    /// Fetch `/api/me` again (after a profile change, or to retry when unreachable).
    pub fn reload(mut self) {
        let client = use_client_from_root();
        if !client.has_session() {
            self.auth.set(Auth::SignedOut);
            return;
        }
        if !matches!(*self.auth.peek(), Auth::SignedIn(_)) {
            self.auth.set(Auth::Checking);
        }
        spawn(async move {
            match client.get::<Me>("/api/me").await {
                Ok(me) => self.auth.set(Auth::SignedIn(me)),
                Err(ApiError::SignedOut) => self.auth.set(Auth::SignedOut),
                Err(error) => {
                    if !matches!(*self.auth.peek(), Auth::SignedIn(_)) {
                        self.auth.set(Auth::Unreachable(error.message().to_owned()));
                    }
                }
            }
        });
    }

    pub async fn login(mut self, username: &str, password: &str) -> Result<(), ApiError> {
        let me = use_client_from_root().login(username, password).await?;
        self.auth.set(Auth::SignedIn(me));
        Ok(())
    }

    pub async fn logout(self) {
        use_client_from_root().logout().await;
    }
}

fn use_client_from_root() -> Client {
    consume_context::<Client>()
}

/// Provide the client and the session (in `App`).
pub fn use_session_provider() -> Session {
    use_context_provider(|| {
        Client::new(
            &crate::platform::api_origin(),
            crate::platform::token_store(),
            &crate::platform::user_agent(),
        )
    });
    let session = use_context_provider(|| Session {
        auth: Signal::new(Auth::SignedOut),
    });
    use_hook(move || {
        let client = use_client_from_root();
        let auth = session.auth;
        client.on_signed_out(move || {
            let mut auth = auth;
            auth.set(Auth::SignedOut);
        });
        session.reload();
        // Another tab signing in or out (the web only; nothing fires on Android).
        spawn(async move {
            let mut eval = document::eval(
                r"
                window.addEventListener('storage', (event) => {
                    if (event.key === 'auth.refresh' || event.key === null) dioxus.send(event.newValue);
                });
                ",
            );
            // Another tab refreshing rotates the token too (`Some`): nothing to do then.
            while let Ok(refresh) = eval.recv::<Option<String>>().await {
                let signed_in = matches!(*session.auth.peek(), Auth::SignedIn(_));
                match refresh {
                    None if signed_in => client.end_session(),
                    Some(_) if !signed_in => session.reload(),
                    _ => {}
                }
            }
        });
    });
    session
}

pub fn use_client() -> Client {
    use_context()
}

pub fn use_session() -> Session {
    use_context()
}

impl PartialEq for Auth {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::SignedIn(a), Self::SignedIn(b)) => a.id == b.id && a.username == b.username,
            (Self::Unreachable(a), Self::Unreachable(b)) => a == b,
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }
}
