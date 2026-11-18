//! Authenticated requests.
//!
//! The access token lives in memory only; the refresh token is persisted by a [`TokenStore`].
//! A request without an access token, or one answered 401, refreshes first (once) and retries.
//! Refreshes are single-flight within the process, and serialized across processes/tabs by
//! [`TokenStore::lock`], with the stored refresh token read only after the lock is held: the
//! backend rotates the token on every refresh and keeps only the previous one valid, so two tabs
//! refreshing from the same token would leave one of them holding a dead one.
//!
//! A refresh the backend rejects (401/400) ends the session: the store is cleared and the
//! [`Client::on_signed_out`] listeners run. Any other failure (offline, 5xx) keeps it.

use std::any::Any;
use std::cell::RefCell;
use std::fmt;
use std::rc::Rc;

use futures_util::future::LocalBoxFuture;
use futures_util::lock::Mutex;
use reqwest::{Method, RequestBuilder, Response, StatusCode};
use serde::Serialize;
use serde::de::DeserializeOwned;

use crate::error::ApiError;
use crate::sse::{self, ServerEvent};
use crate::types::{
    LoginRequest, LoginResponse, Me, Purpose, RefreshRequest, TokenPair, UploadView,
};

/// Keeps the refresh token between launches (localStorage on the web, a private file on Android).
pub trait TokenStore {
    fn load(&self) -> Option<String>;
    /// `None` clears it (signed out).
    fn save(&self, refresh: Option<&str>);
    /// An exclusive lock across every instance of the app sharing this store (browser tabs),
    /// held for the duration of a refresh. Single-process platforms return [`LockGuard::none`].
    fn lock(&self) -> LocalBoxFuture<'_, LockGuard>;
}

/// Releases a [`TokenStore::lock`] when dropped.
pub struct LockGuard {
    _inner: Option<Box<dyn Any>>,
}

impl LockGuard {
    /// A guard whose `inner` releases the lock when dropped.
    pub fn new(inner: impl Any) -> Self {
        Self {
            _inner: Some(Box::new(inner)),
        }
    }

    pub fn none() -> Self {
        Self { _inner: None }
    }
}

impl fmt::Debug for LockGuard {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("LockGuard")
    }
}

/// A handle to the API; cheap to clone, all clones share one session.
#[derive(Clone)]
pub struct Client(Rc<Inner>);

type Listener = Rc<dyn Fn()>;

struct Inner {
    http: reqwest::Client,
    origin: String,
    store: Rc<dyn TokenStore>,
    access: RefCell<Option<String>>,
    refreshing: Mutex<()>,
    signed_out: RefCell<Vec<Listener>>,
}

impl fmt::Debug for Client {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Client")
            .field("origin", &self.0.origin)
            .finish_non_exhaustive()
    }
}

impl PartialEq for Client {
    fn eq(&self, other: &Self) -> bool {
        Rc::ptr_eq(&self.0, &other.0)
    }
}

impl Client {
    /// `origin` is where the API lives, e.g. `https://example.com` (on the web,
    /// `window.location.origin`). `user_agent` names the app on native platforms.
    pub fn new(origin: &str, store: Rc<dyn TokenStore>, user_agent: &str) -> Self {
        let builder = reqwest::Client::builder();
        #[cfg(not(target_arch = "wasm32"))]
        let builder = builder
            .user_agent(user_agent)
            .connect_timeout(std::time::Duration::from_secs(15));
        #[cfg(target_arch = "wasm32")]
        let _ = user_agent; // browsers send their own
        Self(Rc::new(Inner {
            http: builder.build().expect("the HTTP client builds"),
            origin: origin.trim_end_matches('/').to_owned(),
            store,
            access: RefCell::new(None),
            refreshing: Mutex::new(()),
            signed_out: RefCell::new(Vec::new()),
        }))
    }

    pub fn origin(&self) -> &str {
        &self.0.origin
    }

    /// Whether a session is stored (it may still turn out to be expired).
    pub fn has_session(&self) -> bool {
        self.0.store.load().is_some()
    }

    /// Run `listener` whenever the session ends (signed out here, or rejected by the server).
    pub fn on_signed_out(&self, listener: impl Fn() + 'static) {
        self.0.signed_out.borrow_mut().push(Rc::new(listener));
    }

    /// Absolute URL of an API path, or of a (relative) media URL the API returned.
    pub fn url(&self, path: &str) -> String {
        if path.starts_with("http://") || path.starts_with("https://") {
            path.to_owned()
        } else {
            format!("{}{path}", self.0.origin)
        }
    }

    pub async fn login(&self, username: &str, password: &str) -> Result<Me, ApiError> {
        let body = LoginRequest {
            username: username.to_owned(),
            password: password.to_owned(),
        };
        let response = self
            .public(Method::POST, "/api/auth/login")
            .json(&body)
            .send()
            .await
            .map_err(|e| ApiError::network(&e))?;
        let login: LoginResponse = decode(check(response).await?).await?;
        self.0.store.save(Some(&login.refresh));
        *self.0.access.borrow_mut() = Some(login.access);
        Ok(login.user)
    }

    /// End this device's session: tell the server (best effort), then forget the tokens.
    pub async fn logout(&self) {
        let access = self.0.access.borrow().clone();
        if let Some(access) = access {
            let _ = self
                .public(Method::POST, "/api/auth/logout")
                .bearer_auth(access)
                .send()
                .await;
        }
        self.end_session();
    }

    /// Forget the session locally (e.g. after `session.revoked`) and tell the listeners.
    pub fn end_session(&self) {
        self.0.store.save(None);
        *self.0.access.borrow_mut() = None;
        let listeners = self.0.signed_out.borrow().clone();
        for listener in listeners {
            listener();
        }
    }

    pub async fn get<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        self.json(Method::GET, path, None::<&()>).await
    }

    pub async fn post<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.json(Method::POST, path, Some(body)).await
    }

    pub async fn put<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.json(Method::PUT, path, Some(body)).await
    }

    pub async fn patch<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        self.json(Method::PATCH, path, Some(body)).await
    }

    pub async fn delete<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, ApiError> {
        self.json(Method::DELETE, path, body).await
    }

    /// An unauthenticated GET (`/api/login-users`, `/api/health`, ...).
    pub async fn get_public<T: DeserializeOwned>(&self, path: &str) -> Result<T, ApiError> {
        let response = self
            .public(Method::GET, path)
            .send()
            .await
            .map_err(|e| ApiError::network(&e))?;
        decode(check(response).await?).await
    }

    /// An unauthenticated POST (activation).
    pub async fn post_public<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        path: &str,
        body: &B,
    ) -> Result<T, ApiError> {
        let response = self
            .public(Method::POST, path)
            .json(body)
            .send()
            .await
            .map_err(|e| ApiError::network(&e))?;
        decode(check(response).await?).await
    }

    /// An authenticated request with a JSON body (or none). An empty answer decodes as `null`,
    /// so `()` and `Option<_>` work for 204.
    pub async fn json<B: Serialize + ?Sized, T: DeserializeOwned>(
        &self,
        method: Method,
        path: &str,
        body: Option<&B>,
    ) -> Result<T, ApiError> {
        let response = self
            .authorized(path, |http, url| {
                let request = http.request(method.clone(), url);
                match body {
                    Some(body) => request.json(body),
                    None => request,
                }
            })
            .await?;
        decode(response).await
    }

    /// Upload an image (raw body); reference the result by its `id`.
    pub async fn upload(
        &self,
        bytes: Vec<u8>,
        content_type: &str,
        purpose: Purpose,
    ) -> Result<UploadView, ApiError> {
        let path = format!("/api/uploads?purpose={purpose}");
        let response = self
            .authorized(&path, |http, url| {
                http.post(url)
                    .header(reqwest::header::CONTENT_TYPE, content_type)
                    .body(bytes.clone())
            })
            .await?;
        decode(response).await
    }

    /// Open `/api/events`. The stream ends when the connection does; reconnecting (with
    /// backoff) is the caller's job.
    pub async fn events(
        &self,
    ) -> Result<impl futures_util::Stream<Item = Result<ServerEvent, ApiError>> + use<>, ApiError>
    {
        let response = self
            .authorized("/api/events", |http, url| {
                http.get(url)
                    .header(reqwest::header::ACCEPT, "text/event-stream")
            })
            .await?;
        Ok(sse::stream(response))
    }

    fn public(&self, method: Method, path: &str) -> RequestBuilder {
        self.0.http.request(method, self.url(path))
    }

    /// Send with the access token, refreshing it first if there is none and once more on a 401.
    async fn authorized(
        &self,
        path: &str,
        build: impl Fn(&reqwest::Client, &str) -> RequestBuilder,
    ) -> Result<Response, ApiError> {
        let url = self.url(path);
        let mut retried = false;
        loop {
            let access = self.0.access.borrow().clone();
            let Some(access) = access else {
                self.refresh(None).await?;
                continue;
            };
            let response = build(&self.0.http, &url)
                .bearer_auth(&access)
                .send()
                .await
                .map_err(|e| ApiError::network(&e))?;
            if response.status() == StatusCode::UNAUTHORIZED {
                if retried {
                    // A freshly issued token was rejected too: the session is gone.
                    self.end_session();
                    return Err(ApiError::SignedOut);
                }
                retried = true;
                self.refresh(Some(&access)).await?;
                continue;
            }
            return check(response).await;
        }
    }

    /// Get a new access token, unless another task already replaced `stale` meanwhile.
    async fn refresh(&self, stale: Option<&str>) -> Result<(), ApiError> {
        let _local = self.0.refreshing.lock().await;
        let current = self.0.access.borrow().clone();
        if current.is_some() && current.as_deref() != stale {
            return Ok(());
        }
        let _shared = self.0.store.lock().await;
        let Some(refresh) = self.0.store.load() else {
            self.end_session();
            return Err(ApiError::SignedOut);
        };
        let response = self
            .public(Method::POST, "/api/auth/refresh")
            .json(&RefreshRequest { refresh })
            .send()
            .await
            .map_err(|e| ApiError::network(&e))?;
        let status = response.status();
        if status == StatusCode::UNAUTHORIZED || status == StatusCode::BAD_REQUEST {
            self.end_session();
            return Err(ApiError::SignedOut);
        }
        let pair: TokenPair = decode(check(response).await?).await?;
        self.0.store.save(Some(&pair.refresh));
        *self.0.access.borrow_mut() = Some(pair.access);
        Ok(())
    }
}

async fn check(response: Response) -> Result<Response, ApiError> {
    if response.status().is_success() {
        return Ok(response);
    }
    let status = response.status().as_u16();
    let body = response.bytes().await.unwrap_or_default();
    Err(ApiError::from_body(status, &body))
}

async fn decode<T: DeserializeOwned>(response: Response) -> Result<T, ApiError> {
    let body = response.bytes().await.map_err(|e| ApiError::network(&e))?;
    let body: &[u8] = if body.is_empty() { b"null" } else { &body };
    serde_json::from_slice(body).map_err(|e| ApiError::Decode(e.to_string()))
}
