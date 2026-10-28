//! HMS Desktop (Tauri 2 / README §11): the same React UI served from the
//! embedded `apps/web/dist` build, a local SQLite mirror, and a background
//! sync worker driving `hms_sync::SyncEngine` against the API over
//! HttpSyncTransport.
//!
//! Status plumbing: the worker pushes a [`SyncState`] down a `tokio::sync::watch`
//! channel and `sync_status` serves it to the webview — the sidebar's sync dot
//! (online/offline, last-synced time, pending count, unresolved conflicts) plus
//! the AccountLocked state that forces a re-login after a deactivated-user
//! lockout at the next sync.

mod dpapi;
mod local_store;
mod offline;

use hms_sync::engine::{SyncEngine, SyncFailure, SyncOutcome};
use hms_sync::store::{StoreError, SyncStore};
use hms_sync::transport::HttpSyncTransport;
use local_store::LocalStore;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::{watch, RwLock};

struct AppState {
    api_base: String,
    token: Arc<RwLock<Option<String>>>,
    synced_rx: watch::Receiver<SyncState>,
    sync_sender: Arc<watch::Sender<SyncState>>,
    db_path: PathBuf,
}

/// What the sidebar status pill shows (README §11.8).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "state", content = "detail")]
enum SyncState {
    /// logged in and one cycle at least finished
    Synced { synced_at_ms: i64, pending: usize, conflicts: usize },
    /// last cycle failed (offline / update / store error)
    Failing { reason: String, pending: usize },
    /// 401/403 at sync: wiped offline cache, force online re-login
    Locked,
    /// no successful cycle yet
    Unknown,
}

#[tauri::command]
fn sync_status(state: tauri::State<'_, AppState>) -> SyncState {
    state.synced_rx.borrow().clone()
}

/// Webview calls after each online login / refresh so the worker can sync.
#[tauri::command]
async fn set_auth_token(
    state: tauri::State<'_, AppState>,
    token: String,
    hotel_id: i64,
) -> Result<(), String> {
    *state.token.write().await = Some(token.clone());
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.save_token(&token).map_err(|e| e.to_string())?;
    store.set_hotel_id(hotel_id).map_err(|e| e.to_string())?;
    let msg = summarize(&state.db_path, None).map_err(|e| e.to_string())?;
    let _ = state.synced_tx().send(msg);
    Ok(())
}

/// Online login success → cache the credentials for offline verification.
#[tauri::command]
async fn remember_offline_credentials(
    state: tauri::State<'_, AppState>,
    user_id: String,
    hotel_id: i64,
    username: String,
    password_hash: String,
    role: String,
) -> Result<(), String> {
    let role = hms_core::roles::Role::from_str(&role)
        .ok_or_else(|| format!("unknown role '{}'", role))?;
    let user_id: uuid::Uuid = user_id
        .parse::<uuid::Uuid>()
        .map_err(|e: uuid::Error| e.to_string())?;
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    offline::cache_from_online_login(&mut store, user_id, hotel_id, &username, &password_hash, role)
}

/// Offline login against the DPAPI-protected cache (README §11.7).
#[tauri::command]
async fn offline_login(
    state: tauri::State<'_, AppState>,
    hotel_id: i64,
    username: String,
    password: String,
) -> Result<serde_json::Value, String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    match offline::offline_login(&mut store, hotel_id, &username, &password)? {
        offline::OfflineAuth::Verified { user_id, role } => Ok(json!({
            "offline": true,
            "user_id": user_id.to_string(),
            "role": role,
        })),
        offline::OfflineAuth::NoCache => {
            Err("offline login not available; connect and sign in once first".into())
        }
        offline::OfflineAuth::Expired => {
            Err("offline login expired; connect and sign in again".into())
        }
        offline::OfflineAuth::BadPassword => Err("wrong password".into()),
    }
}

/// Manual sync trigger (the worker also runs every SYNC_INTERVAL_SECS).
#[tauri::command]
async fn sync_now(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let token = state.token.clone();
    let outcome = run_cycle(state.api_base.clone(), token, state.db_path.clone()).await;
    let msg = match &outcome {
        Ok(_) => summarize(&state.db_path, None).map_err(|e| e.to_string())?,
        Err(SyncFailure::AccountLocked(_)) => SyncState::Locked,
        Err(e) => summarize(&state.db_path, Some(e.to_string())).map_err(|x| x.to_string())?,
    };
    let _ = state.synced_tx().send(msg);
    outcome
        .map(|o| cycle_summary(&o))
        .map_err(|e| e.to_string())
}

/// SyncOutcome is a report for the status pill, serialized by hand (the
/// engine's plain struct deliberately has no serde).
fn cycle_summary(o: &SyncOutcome) -> serde_json::Value {
    json!({
        "pushed_ok": o.pushed_ok,
        "pushed_deduped": o.pushed_deduped,
        "pushed_conflicts": o.pushed_conflicts,
        "pushed_rejected": o.pushed_rejected,
        "pulled_applied": o.pulled_applied,
        "pulled_already_known": o.pulled_already_known,
        "pending": o.pending,
        "remaining_behind": o.remaining_behind,
        "last_synced_at_ms": o.last_synced_at_ms,
    })
}

impl AppState {
    fn synced_tx(&self) -> watch::Sender<SyncState> {
        (*self.sync_sender).clone()
    }
}

/// One engine cycle against a fresh store handle (SQLite connections are not
/// shared across threads; opening per cycle is cheap on WAL).
async fn run_cycle(
    api_base: String,
    token: Arc<RwLock<Option<String>>>,
    db_path: PathBuf,
) -> Result<SyncOutcome, SyncFailure> {
    let mut store = LocalStore::open(&db_path).map_err(SyncFailure::Store)?;
    let hotel_id = store.hotel_id().map_err(SyncFailure::Store)?;
    if hotel_id == 0 {
        // no successful login recorded yet — nothing to sync
        return Ok(SyncOutcome::default());
    }
    let transport = HttpSyncTransport::new(api_base, move || {
        // sync closure (transport calls it synchronously); try_read avoids
        // blocking if a writer holds the lock for an instant
        token.try_read().ok().and_then(|g| (*g).clone())
    });
    SyncEngine::new(hotel_id, store, transport).sync_once().await
}

/// Rebuild the status snapshot from the local store after a cycle.
fn summarize(db_path: &std::path::Path, failure: Option<String>) -> Result<SyncState, StoreError> {
    let mut store = LocalStore::open(db_path)?;
    let pending = store.pending_count()?;
    let conflicts = store.conflict_count()?;
    Ok(match (store.last_synced_at_ms()?, failure) {
        (Some(_at), Some(reason)) => SyncState::Failing { reason, pending },
        (Some(at), None) => SyncState::Synced { synced_at_ms: at, pending, conflicts },
        (None, Some(reason)) => SyncState::Failing { reason, pending },
        (None, None) => SyncState::Unknown,
    })
}

fn dirs_data() -> PathBuf {
    std::env::var("APPDATA")
        .map(|p| PathBuf::from(p).join("com.hms.desktop"))
        .unwrap_or_else(|_| PathBuf::from(".").join("data"))
}

#[tokio::main]
async fn main() {
    let db_dir = dirs_data();
    std::fs::create_dir_all(&db_dir).expect("create app data dir");
    let db_path = db_dir.join("hms.sqlite");

    let token: Arc<RwLock<Option<String>>> = Arc::new(RwLock::new(
        LocalStore::open(&db_path).ok().and_then(|mut s| s.load_token().ok().flatten()),
    ));

    let (sync_tx, sync_rx) = watch::channel(
        summarize(&db_path, None).unwrap_or(SyncState::Unknown),
    );

    let api_base = std::env::var("HMS_API_BASE")
        .unwrap_or_else(|_| "http://localhost:8080".into());
    let interval = std::time::Duration::from_secs(
        std::env::var("SYNC_INTERVAL_SECS").ok().and_then(|v| v.trim().parse().ok()).unwrap_or(30),
    );

    let state = AppState {
        api_base: api_base.clone(),
        token: token.clone(),
        synced_rx: sync_rx,
        sync_sender: Arc::new(sync_tx),
        db_path: db_path.clone(),
    };

    // background worker: one engine cycle per interval once a token exists
    {
        let tx = state.sync_sender.clone();
        let tok = state.token.clone();
        let base = api_base.clone();
        let db = db_path.clone();
        tokio::spawn(async move {
            let mut timer = tokio::time::interval(interval);
            loop {
                timer.tick().await;
                if (*tok.read().await).is_none() {
                    continue;
                }
                match run_cycle(base.clone(), tok.clone(), db.clone()).await {
                    Ok(_) => {
                        if let Ok(msg) = summarize(&db, None) {
                            let _ = tx.send(msg);
                        }
                    }
                    Err(SyncFailure::AccountLocked(_)) => {
                        // deactivated user: wipe the offline cache now
                        if let Ok(mut store) = LocalStore::open(&db) {
                            let _ = store.drop_offline_cache();
                        }
                        let _ = tx.send(SyncState::Locked);
                    }
                    Err(e) => {
                        if let Ok(msg) = summarize(&db, Some(e.to_string())) {
                            let _ = tx.send(msg);
                        }
                    }
                }
            }
        });
    }

    tauri::Builder::default()
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            sync_status,
            set_auth_token,
            remember_offline_credentials,
            offline_login,
            sync_now
        ])
        .setup(move |_app| {
            tracing::info!("HMS desktop shell started; db at {:?}", db_path);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
