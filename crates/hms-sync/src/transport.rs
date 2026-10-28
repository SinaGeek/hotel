//! HTTP transport seam. [`HttpSyncTransport`] talks to the real server
//! (`POST /api/v1/sync/push`, `GET /api/v1/sync/pull`); tests swap in fakes.

use hms_core::sync::{SyncPullResponse, SyncPushRequest, SyncPushResponse, SYNC_PROTOCOL_VERSION};
use serde_json::Value;
use std::sync::Arc;

/// Distinguishes failure kinds so the engine can react:
/// network errors back off and retry; auth failures lock the offline account;
/// protocol rejections need an app update.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportError {
    /// Connection failed / timeout / 5xx: server unreachable or unhappy.
    Network(String),
    /// 401/403: token invalid or the user was deactivated on the server —
    /// lock the offline login at this sync (README §11.7).
    AuthExpired(String),
    /// 426 or version rejection: client protocol older than
    /// SYNC_PROTOCOL_MIN_VERSION; user must update the desktop app.
    ProtocolTooOld(String),
}

impl std::fmt::Display for TransportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransportError::Network(m) => write!(f, "network: {m}"),
            TransportError::AuthExpired(m) => write!(f, "auth expired: {m}"),
            TransportError::ProtocolTooOld(m) => write!(f, "protocol too old: {m}"),
        }
    }
}

pub trait SyncTransport: Send + Sync {
    fn push(
        &self,
        req: SyncPushRequest,
    ) -> impl std::future::Future<Output = Result<SyncPushResponse, TransportError>> + Send;

    fn pull(
        &self,
        hotel_id: i64,
        since: i64,
        limit: usize,
    ) -> impl std::future::Future<Output = Result<SyncPullResponse, TransportError>> + Send;
}

/// Lets callers share one transport behind `Arc` (app state + engine).
impl<T: SyncTransport + ?Sized> SyncTransport for std::sync::Arc<T> {
    fn push(
        &self,
        req: SyncPushRequest,
    ) -> impl std::future::Future<Output = Result<SyncPushResponse, TransportError>> + Send {
        (**self).push(req)
    }

    fn pull(
        &self,
        hotel_id: i64,
        since: i64,
        limit: usize,
    ) -> impl std::future::Future<Output = Result<SyncPullResponse, TransportError>> + Send {
        (**self).pull(hotel_id, since, limit)
    }
}

/// reqwest-backed transport with a bearer token the desktop app refreshes on
/// each online login / refresh-token cycle. Wrap in `Arc` for sharing between
/// the app state and the engine — `Arc<T>` implements [`SyncTransport`] below.
#[derive(Clone)]
pub struct HttpSyncTransport {
    base_url: String,
    token: Arc<dyn Fn() -> Option<String> + Send + Sync>,
    client: reqwest::Client,
}

impl HttpSyncTransport {
    pub fn new(
        base_url: impl Into<String>,
        token: impl Fn() -> Option<String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            base_url: base_url.into(),
            token: Arc::new(token),
            client: reqwest::Client::new(),
        }
    }

    fn bearer(&self) -> Option<String> {
        (self.token)()
    }

    /// Unwraps the uniform ApiResponse envelope; 401/403 map to AuthExpired,
    /// 426/400 containing "too old" to ProtocolTooOld, other failures to
    /// Network.
    async fn call(&self, resp: reqwest::Response) -> Result<Value, TransportError> {
        let status = resp.status();
        if status == reqwest::StatusCode::UNAUTHORIZED || status == reqwest::StatusCode::FORBIDDEN {
            return Err(TransportError::AuthExpired(format!("HTTP {status}")));
        }
        if status == reqwest::StatusCode::UPGRADE_REQUIRED {
            return Err(TransportError::ProtocolTooOld("HTTP 426".into()));
        }
        let body: Value = resp.json().await.map_err(|e| TransportError::Network(e.to_string()))?;
        if !(200..300).contains(&(status.as_u16())) {
            if body["error"].as_str().is_some_and(|m| m.contains("too old")) {
                return Err(TransportError::ProtocolTooOld(
                    body["error"].as_str().unwrap_or_default().to_owned(),
                ));
            }
            return Err(TransportError::Network(format!(
                "HTTP {}: {}",
                status,
                body["error"].as_str().unwrap_or("server error")
            )));
        }
        if !body["success"].as_bool().unwrap_or(false) {
            return Err(TransportError::Network(
                body["error"].as_str().unwrap_or("sync failed").to_owned(),
            ));
        }
        Ok(body["data"].clone())
    }
}

impl SyncTransport for HttpSyncTransport {
    async fn push(&self, req: SyncPushRequest) -> Result<SyncPushResponse, TransportError> {
        let token = self.bearer().ok_or_else(|| {
            TransportError::AuthExpired("no bearer token: login required".into())
        })?;
        let url = format!("{}/api/v1/sync/push", self.base_url.trim_end_matches('/'));
        let resp = self
            .client
            .post(url)
            .bearer_auth(token)
            .json(&req)
            .send()
            .await
            .map_err(|e| TransportError::Network(e.to_string()))?;
        serde_json::from_value(self.call(resp).await?)
            .map_err(|e| TransportError::Network(format!("bad push payload: {e}")))
    }

    async fn pull(&self, _hotel_id: i64, since: i64, limit: usize) -> Result<SyncPullResponse, TransportError> {
        let token = self.bearer().ok_or_else(|| {
            TransportError::AuthExpired("no bearer token: login required".into())
        })?;
        let url = self.pull_url(since, limit);
        let resp = self
            .client
            .get(&url)
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| TransportError::Network(e.to_string()))?;
        serde_json::from_value(self.call(resp).await?)
            .map_err(|e| TransportError::Network(format!("bad pull payload: {e}")))
    }
}

impl HttpSyncTransport {
    fn pull_url(&self, since: i64, limit: usize) -> String {
        format!(
            "{}/api/v1/sync/pull?since={since}&limit={limit}&v={}",
            self.base_url.trim_end_matches('/'),
            SYNC_PROTOCOL_VERSION
        )
    }
}
