//! HMS Desktop (Tauri 2 / README §11): the same React UI served from the
//! embedded `client/web/dist` build, a local SQLite mirror, and a background
//! sync worker driving `hms_sync::SyncEngine` against the API over
//! HttpSyncTransport.
//!
//! Status plumbing: the worker pushes a [`SyncState`] down a `tokio::sync::watch`
//! channel and `sync_status` serves it to the webview — the sidebar's sync dot
//! (online/offline, last-synced time, pending count, unresolved conflicts) plus
//! the AccountLocked state that forces a re-login after a deactivated-user
//! lockout at the next sync.

// Windows GUI subsystem (issue: closing the stray CMD console killed the app):
// without this a release exe keeps a console attached and closing it kills the
// app. `windows` subsystem = pure GUI application (no terminal host process).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod dpapi;
pub mod importer;
mod local_first;
pub mod local_store;
mod offline;
mod reports;
mod sqlcipher;
mod updater;

use hms_sync::engine::{SyncEngine, SyncFailure, SyncOutcome};
use hms_sync::store::{StoreError, SyncStore};
use hms_sync::transport::HttpSyncTransport;
use local_store::LocalStore;
use serde_json::json;
use std::path::PathBuf;
use std::sync::Arc;
use tauri::Manager;
use tokio::sync::{watch, RwLock};
use updater::UpdateState;

struct AppState {
    api_base: Arc<RwLock<String>>,
    token: Arc<RwLock<Option<String>>>,
    synced_rx: watch::Receiver<SyncState>,
    sync_sender: Arc<watch::Sender<SyncState>>,
    db_path: PathBuf,
    updates: Arc<UpdateState>,
}

/// What the sidebar status pill shows (README §11.8).
#[derive(Clone, Debug, serde::Serialize)]
#[serde(tag = "state", content = "detail")]
enum SyncState {
    /// logged in and one cycle at least finished
    Synced {
        synced_at_ms: i64,
        pending: usize,
        conflicts: usize,
    },
    /// last cycle failed (offline / update / store error)
    Failing { reason: String, pending: usize },
    /// 401/403 at sync: wiped offline cache, force online re-login
    Locked,
    /// D4: the server rejected this build's sync protocol (426); an update
    /// must be installed before syncing again
    UpdateRequired { reason: String },
    /// no successful cycle yet
    Unknown,
}

#[tauri::command]
fn sync_status(state: tauri::State<'_, AppState>) -> SyncState {
    state.synced_rx.borrow().clone()
}

#[tauri::command]
async fn get_app_preferences(
    state: tauri::State<'_, AppState>,
) -> Result<serde_json::Value, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.app_preferences().map_err(|e| e.to_string())
}

#[tauri::command]
async fn complete_first_run(
    state: tauri::State<'_, AppState>,
    mode: String,
    api_base: Option<String>,
    storage_dir: String,
    theme: String,
) -> Result<(), String> {
    if !["local", "cloud"].contains(&mode.as_str()) {
        return Err("mode must be local or cloud".into());
    }
    if !["light", "dark"].contains(&theme.as_str()) {
        return Err("theme must be light or dark".into());
    }
    if mode == "local" && api_base.is_some() {
        *state.api_base.write().await = String::new();
    }
    if mode == "cloud" {
        let address = api_base.as_deref().unwrap_or("").trim();
        if !(address.starts_with("https://") || address.starts_with("http://")) {
            return Err("server address must start with https:// or http://".into());
        }
    }
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store
        .finish_first_run(&mode, api_base.as_deref(), &storage_dir, &theme)
        .map_err(|e| e.to_string())?;
    if let Some(base) = api_base {
        *state.api_base.write().await = base.trim().trim_end_matches('/').to_string();
    }
    Ok(())
}

#[tauri::command]
async fn get_offline_mode(state: tauri::State<'_, AppState>) -> Result<bool, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.offline_mode().map_err(|e| e.to_string())
}

/// Settings ▸ Remote server: change the API base without reinstalling. The
/// saved value takes effect on the next cycle; nothing here contacts the
/// server (that only happens on the next sync/login).
#[tauri::command]
async fn set_api_base(state: tauri::State<'_, AppState>, api_base: String) -> Result<(), String> {
    let address = api_base.trim().trim_end_matches('/').to_string();
    if !(address.starts_with("https://") || address.starts_with("http://")) {
        return Err("server address must start with https:// or http://".into());
    }
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.set_api_base(&address).map_err(|e| e.to_string())?;
    *state.api_base.write().await = address;
    Ok(())
}

/// Settings ▸ Sync mode: "auto" runs the background worker, "manual" pauses
/// automatic cycles and only "Sync now" performs a sync. The outbox and local
/// mirror are never touched either way.
#[tauri::command]
async fn set_sync_mode(state: tauri::State<'_, AppState>, mode: String) -> Result<(), String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.set_sync_mode(&mode).map_err(|e| e.to_string())
}

/// First-run chose "Local only": the login screen offers this entry point,
/// which opens the workspace without any server. It returns a synthetic local
/// session config (the UI builds an offline session from it). Guarded: only
/// available when setup actually selected local mode (offline_mode on and no
/// server configured), so a cloud install cannot bypass its login.
#[tauri::command]
async fn local_mode_session(
    state: tauri::State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    let prefs = store.app_preferences().map_err(|e| e.to_string())?;
    let local_chosen = prefs["mode"].as_str() == Some("local");
    let offline = store.offline_mode().map_err(|e| e.to_string())?;
    if local_chosen && offline {
        Ok(Some(json!({ "username": "local", "role": "admin" })))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn set_offline_mode(state: tauri::State<'_, AppState>, enabled: bool) -> Result<(), String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.set_offline_mode(enabled).map_err(|e| e.to_string())?;
    if enabled {
        let _ = state.synced_tx().send(SyncState::Failing {
            reason: "Offline mode is enabled by the user".into(),
            pending: store.pending_count().map_err(|e| e.to_string())?,
        });
    }
    Ok(())
}

#[tauri::command]
async fn offline_login_identity(
    state: tauri::State<'_, AppState>,
) -> Result<Option<serde_json::Value>, String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    let Some((hotel_id, username, _hash)) =
        store.cached_login_identity().map_err(|e| e.to_string())?
    else {
        return Ok(None);
    };
    Ok(Some(json!({ "hotel_id": hotel_id, "username": username })))
}

#[tauri::command]
async fn set_report_currency(
    state: tauri::State<'_, AppState>,
    currency: String,
) -> Result<(), String> {
    if currency.len() != 3 || !currency.chars().all(|c| c.is_ascii_uppercase()) {
        return Err("invalid currency code".into());
    }
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store
        .set_report_currency(&currency)
        .map_err(|e| e.to_string())
}

/// Webview calls after each online login / refresh so the worker can sync.
#[tauri::command]
async fn set_auth_token(
    state: tauri::State<'_, AppState>,
    token: String,
    hotel_id: i64,
) -> Result<(), String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.set_hotel_id(hotel_id).map_err(|e| e.to_string())?;
    store.save_token(&token).map_err(|e| e.to_string())?;
    *state.token.write().await = Some(token);
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
    let role =
        hms_core::roles::Role::from_str(&role).ok_or_else(|| format!("unknown role '{}'", role))?;
    let user_id: uuid::Uuid = user_id
        .parse::<uuid::Uuid>()
        .map_err(|e: uuid::Error| e.to_string())?;
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    offline::cache_from_online_login(
        &mut store,
        user_id,
        hotel_id,
        &username,
        &password_hash,
        role,
    )
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
        offline::OfflineAuth::Verified { user_id, role, hotel_id } => Ok(json!({
            "offline": true,
            "user_id": user_id.to_string(),
            "role": role,
            "hotel_id": hotel_id,
        })),
        offline::OfflineAuth::NoCache => {
            Err("offline login not available; unknown user or offline mirror empty".into())
        }
        offline::OfflineAuth::Expired => {
            Err("offline login expired; connect and sign in again".into())
        }
        offline::OfflineAuth::BadPassword => Err("wrong password".into()),
    }
}

/// List local hotel records from encrypted SQLite.
#[tauri::command]
async fn list_local_hotels(state: tauri::State<'_, AppState>) -> Result<Vec<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.list_local_hotels().map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_local_hotel(
    state: tauri::State<'_, AppState>, old_id: i64, id: i64, code: String, name: String, profile: serde_json::Value,
) -> Result<(), String> {
    if name.trim().is_empty() || code.trim().is_empty() { return Err("hotel code and name are required".into()); }
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.save_local_hotel(old_id, id, &code, &name, &profile).map_err(|e| e.to_string())
}

/// Persist hotel profile metadata inside the encrypted local database.
#[tauri::command]
async fn save_hotel_profile(state: tauri::State<'_, AppState>, hotel_id: i64, profile: serde_json::Value) -> Result<(), String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.save_hotel_profile(hotel_id, &profile).map_err(|e| e.to_string())
}

#[tauri::command]
async fn get_hotel_profile(state: tauri::State<'_, AppState>, hotel_id: i64) -> Result<Option<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.get_hotel_profile(hotel_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn create_shift_transfer(
    state: tauri::State<'_, AppState>, id: String, hotel_id: i64, from_user: String,
    to_user: String, amount_cents: i64, currency: String, notes: Option<String>,
) -> Result<(), String> {
    if amount_cents < 0 { return Err("cash amount cannot be negative".into()); }
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.create_shift_transfer(&id, hotel_id, &from_user, &to_user, amount_cents, &currency, notes.as_deref()).map_err(|e| e.to_string())
}

#[tauri::command]
async fn pending_shift_transfer(state: tauri::State<'_, AppState>, hotel_id: i64, to_user: String) -> Result<Option<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.pending_shift_transfer(hotel_id, &to_user).map_err(|e| e.to_string())
}

#[tauri::command]
async fn decide_shift_transfer(state: tauri::State<'_, AppState>, id: String, decided_by: String, approved: bool) -> Result<(), String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.decide_shift_transfer(&id, &decided_by, approved).map_err(|e| e.to_string())
}

/// Local users provisioned via Settings ▸ Bulk import (always available
/// offline, no expiry). The Login page lists them for offline sign-in.
#[tauri::command]
async fn list_domain_records(state: tauri::State<'_, AppState>, entity: String, hotel_id: i64) -> Result<Vec<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.list_domain_records(&entity, hotel_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn save_domain_record(state: tauri::State<'_, AppState>, entity: String, hotel_id: i64, id: String, value: serde_json::Value) -> Result<(), String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.save_domain_record(&entity, hotel_id, &id, &value).map_err(|e| e.to_string())
}

#[tauri::command]
async fn delete_domain_record(state: tauri::State<'_, AppState>, entity: String, hotel_id: i64, id: String) -> Result<(), String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.delete_domain_record(&entity, hotel_id, &id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn restore_domain_record(state: tauri::State<'_, AppState>, entity: String, hotel_id: i64, id: String) -> Result<(), String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.restore_domain_record(&entity, hotel_id, &id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn update_local_user(state: tauri::State<'_, AppState>, user_id: String, password: String) -> Result<(), String> {
    if password.len() < 3 { return Err("password must be at least 3 characters".into()); }
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.update_local_user_password(&user_id, &password).map_err(|e| e.to_string())
}

#[tauri::command]
async fn list_local_users(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.list_local_users().map_err(|e| e.to_string())
}

/// Settings ▸ Bulk import: load one legacy dataset into the standalone local
/// database. Idempotent — re-running updates instead of duplicating.
#[tauri::command]
async fn bulk_import(
    state: tauri::State<'_, AppState>,
    kind: String,
    path: String,
    anchor: Option<String>,
) -> Result<serde_json::Value, String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    let p = std::path::PathBuf::from(&path);
    if !p.exists() {
        return Err(format!("file not found: {path}"));
    }
    match kind.as_str() {
        "guests" => importer::import_guest_register(&mut store, &p),
        "collections" => importer::import_collection_ledger(&mut store, &p),
        "reservations" => {
            importer::import_reservation_workbook(&mut store, &p, anchor.as_deref().unwrap_or("2026-06-01"))
        }
        "users" => importer::import_users(&mut store, &p),
        other => Err(format!("unknown import kind '{other}' (guests|collections|reservations|users)")),
    }
}

/// Open a file picker dialog and return the selected file path.
#[tauri::command]
async fn pick_file(
    app: tauri::AppHandle,
    file_types: Vec<(String, Vec<String>)>,
) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::{DialogExt, FileDialogBuilder};
    
    let mut builder = app.dialog().file();
    
    for (name, extensions) in file_types {
        let ext_refs: Vec<&str> = extensions.iter().map(|s| s.as_str()).collect();
        builder = builder.add_filter(&name, &ext_refs);
    }
    
    let result = builder.blocking_pick_file();
    match result {
        Some(file_path) => {
            match file_path.into_path() {
                Ok(path) => Ok(Some(path.to_string_lossy().to_string())),
                Err(_) => Ok(None)
            }
        },
        None => Ok(None)
    }
}

/// Offline report reads from the local mirror (README §10 / todo 201): the
/// webview calls this when the REST endpoints are unreachable. Returns the
/// four aggregates in one payload (`summary`, `by_day`, `by_channel`,
/// `by_room`) with the REST DTO field names.
#[tauri::command]
async fn reservation_guests(
    state: tauri::State<'_, AppState>,
    hotel_id: i64,
    reservation_id: String,
) -> Result<Vec<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.reservation_guests(hotel_id, &reservation_id).map_err(|e| e.to_string())
}

/// Guest ids linked to a reservation (primary first) — the webview appends
/// newly added guests by re-saving the reservation with the full id list.
#[tauri::command]
async fn reservation_guest_ids(
    state: tauri::State<'_, AppState>,
    reservation_id: String,
) -> Result<Vec<String>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.reservation_guest_ids(&reservation_id).map_err(|e| e.to_string())
}

#[tauri::command]
async fn offline_reservations(
    state: tauri::State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    let store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store.list_reservations_json().map_err(|e| e.to_string())
}

#[tauri::command]
async fn offline_reports(
    state: tauri::State<'_, AppState>,
    from: String,
    to: String,
) -> Result<serde_json::Value, String> {
    let Some(f) = chrono::NaiveDate::parse_from_str(&from, "%Y-%m-%d").ok() else {
        return Err("invalid from date".into());
    };
    let Some(t) = chrono::NaiveDate::parse_from_str(&to, "%Y-%m-%d").ok() else {
        return Err("invalid 'to' date".into());
    };
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    let hotel_id = store.hotel_id().map_err(|e| e.to_string())?;
    if hotel_id == 0 {
        return Err("no hotel logged in on this device; sign in online once first".into());
    }
    reports::compute(&store, f, t)
}

/// G4: save LAN hub configuration for peer sync routing.
#[tauri::command]
async fn set_hub_config(
    state: tauri::State<'_, AppState>,
    hub_url: Option<String>,
    hub_token: Option<String>,
) -> Result<(), String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    store
        .save_hub_config(hub_url, hub_token)
        .map_err(|e| e.to_string())
}

/// G4: load LAN hub configuration.
#[tauri::command]
async fn get_hub_config(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    let (url, has_token) = store.load_hub_config().map_err(|e| e.to_string())?;
    Ok(json!({
        "hub_url": url,
        "has_token": has_token.is_some(),
    }))
}

/// Manual sync trigger (the worker also runs every SYNC_INTERVAL_SECS).
#[tauri::command]
async fn sync_now(state: tauri::State<'_, AppState>) -> Result<serde_json::Value, String> {
    let token = state.token.clone();
    let outcome = run_cycle(
        state.api_base.read().await.clone(),
        token,
        state.db_path.clone(),
    )
    .await;
    let msg = match &outcome {
        Ok(_) => summarize(&state.db_path, None).map_err(|e| e.to_string())?,
        Err(SyncFailure::AccountLocked(_)) => SyncState::Locked,
        Err(SyncFailure::UpdateRequired(reason)) => {
            state.updates.force();
            SyncState::UpdateRequired {
                reason: reason.clone(),
            }
        }
        Err(e) => summarize(&state.db_path, Some(e.to_string())).map_err(|x| x.to_string())?,
    };
    let _ = state.synced_tx().send(msg);
    outcome
        .map(|o| cycle_summary(&o))
        .map_err(|e| e.to_string())
}

/// Local-first write (todo A1): stage the optimistic mirror row + outbox op
/// in ONE SQLite transaction, then — when a session token exists —
/// immediately run one sync cycle so the push reaches the server and the
/// pull overwrites the optimistic values with the server's canonical state
/// (ids, status, price, conflict flag). Returns `confirmed` when the op left
/// the outbox in that cycle, `queued` otherwise (offline / network failure /
/// rejected) — queued writes are never lost, the background worker retries.
#[tauri::command]
async fn local_write(
    state: tauri::State<'_, AppState>,
    entity: String,
    operation: String,
    entity_id: Option<String>,
    payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    let hotel_id = store.hotel_id().map_err(|e| e.to_string())?;
    let e = local_first::entity_of(&entity)?;
    let o = local_first::operation_of(&operation)?;
    let eid = match &entity_id {
        Some(s) => Some(
            s.parse::<uuid::Uuid>()
                .map_err(|_| "entity_id is not a UUID".to_string())?,
        ),
        None => None,
    };
    let (op, warning) = local_first::stage_local_write(&mut store, hotel_id, e, o, eid, payload)?;
    let oid = op.operation_id;
    let id_s = op.entity_id.to_string();

    let online = (*state.token.read().await).is_some();
    let mut cycle_error: Option<String> = None;
    if online {
        let outcome = run_cycle(
            state.api_base.read().await.clone(),
            state.token.clone(),
            state.db_path.clone(),
        )
        .await;
        match &outcome {
            Ok(_) => {}
            Err(SyncFailure::AccountLocked(_)) => {
                // deactivated user: the next sync already wipes the cache;
                // surface Locked and keep the write queued
                let _ = state.synced_tx().send(SyncState::Locked);
                cycle_error = Some("account locked; sign in online".into());
            }
            Err(SyncFailure::UpdateRequired(reason)) => {
                // D4: this build is too old for the server's sync protocol
                state.updates.force();
                let reason = reason.clone();
                let _ = state.synced_tx().send(SyncState::UpdateRequired {
                    reason: reason.clone(),
                });
                cycle_error = Some(format!("update required: {reason}"));
            }
            Err(err) => cycle_error = Some(err.to_string()),
        }
        if !local_first::op_is_still_pending(&mut store, oid)? {
            if let Ok(msg) = summarize(&state.db_path, None) {
                let _ = state.synced_tx().send(msg);
            }
            return Ok(serde_json::to_value(local_first::WriteOutcome::Confirmed {
                operation_id: oid.to_string(),
                entity_id: id_s,
                warning,
            })
            .expect("serializable"));
        }
    }
    Ok(serde_json::to_value(local_first::WriteOutcome::Queued {
        operation_id: oid.to_string(),
        entity_id: id_s,
        warning,
        reason: cycle_error
            .unwrap_or_else(|| "offline or no session; stored locally and queued".into()),
    })
    .expect("serializable"))
}

/// Mirror lookup of a payment (offline reversal building): returns
/// `{ reservation_id, amount_cents, currency }` or an error when the payment
/// is not on this device yet (never guessed — the server stays the authority).
#[tauri::command]
async fn local_payment_lookup(
    state: tauri::State<'_, AppState>,
    payment_id: String,
) -> Result<serde_json::Value, String> {
    let mut store = LocalStore::open(&state.db_path).map_err(|e| e.to_string())?;
    match store
        .payment_mirror_lookup(&payment_id)
        .map_err(|e| e.to_string())?
    {
        Some((reservation_id, amount_cents, currency)) => Ok(json!({
            "reservation_id": reservation_id,
            "amount_cents": amount_cents,
            "currency": currency,
        })),
        None => Err("payment not found on this device; connect and sync online first".into()),
    }
}

/// Updater (D3): check the configured endpoint now; the worker also polls.
#[tauri::command]
async fn update_check(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<updater::UpdateInfo, String> {
    updater::check(&app, &state.updates).await
}

/// Updater (D3/D5): download, verify and install the pending release. A final
/// sync runs first so the outbox is drained while the old build is alive.
#[tauri::command]
async fn update_install(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<String, String> {
    let version = updater::download_and_install(&app, &state.updates, || async {
        run_cycle(
            state.api_base.read().await.clone(),
            state.token.clone(),
            state.db_path.clone(),
        )
        .await
        .map(|_| ())
        .map_err(|e| format!("sync before update failed ({e}); not installing"))
    })
    .await?;
    Ok(version)
}

/// Updater (D4): true while the server refuses this build's sync protocol.
#[tauri::command]
fn update_required(state: tauri::State<'_, AppState>) -> bool {
    state.updates.is_forced()
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
    if store.offline_mode().map_err(SyncFailure::Store)? {
        return Ok(SyncOutcome::default());
    }
    let hotel_id = store.hotel_id().map_err(SyncFailure::Store)?;
    if hotel_id == 0 {
        // no successful login recorded yet — nothing to sync
        return Ok(SyncOutcome::default());
    }

    // G4: try LAN hub first if configured
    let (hub_url, hub_token) = store.load_hub_config().map_err(SyncFailure::Store)?;
    if let (Some(url), Some(hub_tok)) = (hub_url, hub_token) {
        let hub_tok_clone = hub_tok.clone();
        let hub_transport =
            HttpSyncTransport::new(url.clone(), move || Some(hub_tok_clone.clone()));

        match SyncEngine::new(hotel_id, store, hub_transport)
            .sync_once()
            .await
        {
            Ok(outcome) => {
                tracing::debug!("LAN hub sync succeeded: {outcome:?}");
                return Ok(outcome);
            }
            Err(e) => {
                tracing::warn!("LAN hub unreachable at {url}, falling back to cloud: {e}");
                // Reopen store for cloud attempt
                store = LocalStore::open(&db_path).map_err(SyncFailure::Store)?;
            }
        }
    }

    // Cloud sync (or hub was not configured)
    let transport = HttpSyncTransport::new(api_base, move || {
        // sync closure (transport calls it synchronously); try_read avoids
        // blocking if a writer holds the lock for an instant
        token.try_read().ok().and_then(|g| (*g).clone())
    });
    SyncEngine::new(hotel_id, store, transport)
        .sync_once()
        .await
}

/// Rebuild the status snapshot from the local store after a cycle.
fn summarize(db_path: &std::path::Path, failure: Option<String>) -> Result<SyncState, StoreError> {
    let mut store = LocalStore::open(db_path)?;
    let pending = store.pending_count()?;
    let conflicts = store.conflict_count()?;
    Ok(match (store.last_synced_at_ms()?, failure) {
        (Some(_at), Some(reason)) => SyncState::Failing { reason, pending },
        (Some(at), None) => SyncState::Synced {
            synced_at_ms: at,
            pending,
            conflicts,
        },
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
        LocalStore::open(&db_path)
            .ok()
            .and_then(|mut s| s.load_token().ok().flatten()),
    ));

    let (sync_tx, sync_rx) =
        watch::channel(summarize(&db_path, None).unwrap_or(SyncState::Unknown));

    let stored_api = LocalStore::open(&db_path)
        .ok()
        .and_then(|s| s.app_preferences().ok())
        .and_then(|p| p["api_base"].as_str().map(str::to_owned));
    let api_base = stored_api
        .or_else(|| std::env::var("HMS_API_BASE").ok())
        .unwrap_or_else(|| "http://localhost:8080/api/v1".into());
    let api_base = Arc::new(RwLock::new(api_base));
    let interval = std::time::Duration::from_secs(
        std::env::var("SYNC_INTERVAL_SECS")
            .ok()
            .and_then(|v| v.trim().parse().ok())
            .unwrap_or(30),
    );

    let state = AppState {
        api_base: api_base.clone(),
        token: token.clone(),
        synced_rx: sync_rx,
        sync_sender: Arc::new(sync_tx),
        db_path: db_path.clone(),
        updates: Arc::new(UpdateState::default()),
    };

    // background worker (todo A5 automatic sync triggers): one engine cycle
    // per interval once a token exists, firing immediately at app start;
    // while offline it re-polls fast (2 s, the practical network-up event —
    // a restore of connectivity is picked up within this window), and on the
    // first success the cadence returns to the configured interval. Push
    // retries additionally honor the engine's per-op exponential backoff;
    // outbox order is FIFO by enqueued time and ops are never dropped.
    {
        let tx = state.sync_sender.clone();
        let tok = state.token.clone();
        let base = api_base.clone();
        let db = db_path.clone();
        let updates = state.updates.clone();
        tokio::spawn(async move {
            // 0 delay = attempt right away (app-start trigger)
            let mut delay_secs: u64 = 0;
            loop {
                tokio::time::sleep(std::time::Duration::from_secs(delay_secs)).await;
                if (*tok.read().await).is_none() {
                    delay_secs = interval.as_secs();
                    continue;
                }
                let manual_only = LocalStore::open(&db)
                    .map(|s| s.sync_mode().map(|m| m == "manual").unwrap_or(false))
                    .unwrap_or(false);
                if manual_only {
                    // Settings ▸ manual sync: only "Sync now" cycles
                    delay_secs = interval.as_secs();
                    continue;
                }
                if LocalStore::open(&db)
                    .and_then(|s| s.offline_mode())
                    .unwrap_or(false)
                {
                    delay_secs = interval.as_secs();
                    continue;
                }
                let base_url = base.read().await.clone();
                match run_cycle(base_url, tok.clone(), db.clone()).await {
                    Ok(_) => {
                        delay_secs = interval.as_secs();
                        if let Ok(msg) = summarize(&db, None) {
                            let _ = tx.send(msg);
                        }
                    }
                    Err(SyncFailure::AccountLocked(_)) => {
                        // deactivated user: wipe the offline cache now
                        delay_secs = interval.as_secs();
                        if let Ok(mut store) = LocalStore::open(&db) {
                            let _ = store.drop_offline_cache();
                        }
                        let _ = tx.send(SyncState::Locked);
                    }
                    Err(SyncFailure::UpdateRequired(reason)) => {
                        // D4: stop hammering the server; the build must update
                        updates.force();
                        delay_secs = interval.as_secs();
                        let _ = tx.send(SyncState::UpdateRequired { reason });
                    }
                    Err(_) => {
                        // offline / store error: poll fast so a restored
                        // network triggers a cycle within ~2 s (per-op
                        // exponential backoff still gates pushes)
                        delay_secs = 2;
                        if let Ok(msg) = summarize(&db, Some("offline; retrying".into())) {
                            let _ = tx.send(msg);
                        }
                    }
                }
            }
        });
    }

    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .manage(state)
        .invoke_handler(tauri::generate_handler![
            sync_status,
            get_app_preferences,
            complete_first_run,
            get_offline_mode,
            set_offline_mode,
            set_api_base,
            set_sync_mode,
            local_mode_session,
            offline_login_identity,
            list_local_hotels,
            save_local_hotel,
            list_domain_records,
            save_domain_record,
            delete_domain_record,
            restore_domain_record,
            save_hotel_profile,
            get_hotel_profile,
            create_shift_transfer,
            pending_shift_transfer,
            decide_shift_transfer,
            update_local_user,
            list_local_users,
            bulk_import,
            pick_file,
            set_auth_token,
            set_report_currency,
            remember_offline_credentials,
            offline_login,
            offline_reservations,
            reservation_guests,
            reservation_guest_ids,
            offline_reports,
            local_write,
            local_payment_lookup,
            sync_now,
            set_hub_config,
            get_hub_config,
            update_check,
            update_install,
            update_required
        ])
        .setup(move |app| {
            tracing::info!("HMS desktop shell started; db at {:?}", db_path);
            // D3: background update check on start and every few hours
            let updates = app.state::<AppState>().updates.clone();
            updater::spawn_checker(app.handle().clone(), updates);
            Ok(())
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
