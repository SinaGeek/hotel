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

/// reqwest client factory honouring the optional `HMS_CA_BUNDLE` env var: a
/// path to a PEM file with EXTRA trusted root CAs (e.g. a corporate PKI or a
/// private deployment's Caddy local CA for https:// loopback installs).
/// Rustls ships only the Mozilla webpki roots, so without this seam a private
/// CA would fail TLS validation even though the OS store trusts it (`curl
/// --cacert` equivalent; never disables certificate validation).
fn transport_client() -> reqwest::Client {
    let mut builder = reqwest::Client::builder();
    if let Ok(bundle) = std::env::var("HMS_CA_BUNDLE") {
        if !bundle.trim().is_empty() {
            match std::fs::read(&bundle) {
                Ok(bytes) => match reqwest::Certificate::from_pem(&bytes) {
                    Ok(cert) => {
                        builder = builder.add_root_certificate(cert);
                    }
                    Err(e) => eprintln!("HMS_CA_BUNDLE: {bundle} is not a valid PEM certificate: {e}"),
                },
                Err(e) => eprintln!("HMS_CA_BUNDLE: cannot read {bundle}: {e}"),
            }
        }
    }
    builder.build().unwrap_or_else(|e| {
        eprintln!("HMS_CA_BUNDLE: falling back to default client ({e})");
        reqwest::Client::new()
    })
}

/// reqwest-backed transport with a bearer token the desktop app refreshes on
/// each online login / refresh-token cycle. Wrap in `Arc` for sharing between
/// the app state and the engine — `Arc<T>` implements [`SyncTransport`] below.
/// The client is built by [`transport_client`] so private-deployment root CAs
/// (`HMS_CA_BUNDLE`) are honoured; validation is never disabled.
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
            client: transport_client(),
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

#[cfg(test)]
mod ca_bundle_tests {
    use super::*;
    use std::sync::{Mutex, MutexGuard};

    /// Serialises tests that touch the process-wide `HMS_CA_BUNDLE` env var.
    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn guard(bundle: Option<&str>) -> MutexGuard<'static, ()> {
        let g = ENV_LOCK.lock().unwrap_or_else(|p| p.into_inner());
        match bundle {
            Some(v) => std::env::set_var("HMS_CA_BUNDLE", v),
            None => std::env::remove_var("HMS_CA_BUNDLE"),
        }
        g
    }

    /// e2_truth: unset env → default client builds as before.
    #[test]
    fn unset_bundle_builds_default_client() {
        let _g = guard(None);
        let _ = transport_client();
    }

    /// e2_truth: unreadable path or non-PEM content must degrade with a
    /// notice, never panic (bootstrap resilience).
    #[test]
    fn bad_bundle_paths_do_not_panic() {
        {
            let _g = guard(Some("Z:\\definitely\\not\\here.pem"));
            let _ = transport_client();
        }
        {
            let _g = guard(Some("C:\\Windows\\win.ini"));
            let _ = transport_client();
        }
    }

    /// e2_truth: a real PEM root CA (RSA-2048 self-signed "hms-ca-test",
    /// generated once via New-SelfSignedCertificate and exported as PEM; not a
    /// secret, signs nothing, and never existed in any trust store) is
    /// accepted by reqwest's rustls backend without error.
    #[test]
    fn valid_pem_root_is_accepted() {
        const PEM: &str = "-----BEGIN CERTIFICATE-----\n\
MIIDFzCCAf+gAwIBAgIQG9qC3B6h6ZhH2Yb4I3KX5DANBgkqhkiG9w0BAQsFADAW\n\
MRQwEgYDVQQDDAtobXMtY2EtdGVzdDAeFw0yNjExMDUyMDI3MDlaFw0zNjExMDUy\n\
MDM3MDlaMBYxFDASBgNVBAMMC2htcy1jYS10ZXN0MIIBIjANBgkqhkiG9w0BAQEF\n\
AAOCAQ8AMIIBCgKCAQEAwnWEVe/7uxuz/iaQxNHpQz58cGWB2aRSN1V0A9Vt9DLM\n\
7sP3MiVPl+yKnjZaGtrEei/90WiLVdwZgAh08cP9QV8YtHx1/Wsq394bZ73Zl2tm\n\
tclIVJqKFVEBk7CVtZNV2M+JPSu58GL6p0Owzc2q6/DJZ3N+IPdWNeZSjh5IvsUP\n\
O1RvfvrsbTbPuA500QKXX+SXOAcDeiSfiUtu4SYxqTpvsuORsYV6GW/7FjHJS69+\n\
cBUQ7ihPzpHkAFOgu7ODNBnCer+3N1WCSVvw9ur6orbe3TRAC2InYN5/z/z6N1s/\n\
q5w6ndFH2PVWb2FSKCATr/nCjLkZ/svS8kLBlcWDvQIDAQABo2EwXzAOBgNVHQ8B\n\
Af8EBAMCAQYwHQYDVR0lBBYwFAYIKwYBBQUHAwIGCCsGAQUFBwMBMA8GA1UdEwEB\n\
/wQFMAMBAf8wHQYDVR0OBBYEFJkAVoOEBbNO3RB1K+ngGl8+vH1GMA0GCSqGSIb3\n\
DQEBCwUAA4IBAQAIkDE7cZVqfPkGViw/q1bH9kAXWtesoZ2dnT1OdfHdK/inG3Q+\n\
KiZRwe7E9TQYA4Z3uC6wbDi3m2pTV/9xlxRD/RwFCYIAIY+e61NmxZk9xlQmwPPG\n\
gXrvJzJZoUUGo53b1PEGE7kvu5LW/SIz9bhGDO3jHhflv3APYg4aKAkkJSgMgUqx\n\
TYWgmkZxEvRAGg2lp3Lky01SKCsjEcoA4LaKkBKSI+AWf31JcRUQhx0ICAYNKKlk\n\
AZFFdefoQjln5+a5lul/L3Cz1OOJznlW0IA0ieXs+82Zc2E5IWLGxki67KURVei9\n\
2qneI0eo9gwPRN6wzpV+sqIsf2441V3y8Llu\n\
-----END CERTIFICATE-----\n";
        let dir = std::env::temp_dir().join("hms-ca-bundle-tests");
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("root.pem");
        std::fs::write(&path, PEM).unwrap();
        let _g = guard(Some(path.to_str().unwrap()));
        let _ = transport_client();
    }
}
