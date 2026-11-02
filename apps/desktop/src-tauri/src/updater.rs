//! Self-updater (todo D1–D5): a thin, testable layer over
//! `tauri-plugin-updater`.
//!
//! Policy decisions live here, not in the webview:
//! - the check runs on app start and then every few hours (the worker below);
//! - downloads happen in the background, installs only on an explicit user
//!   action, so a form is never yanked from under the user (D3);
//! - a server `426` (protocol too old) makes the update *mandatory*: the sync
//!   engine reports `UpdateRequired`, this module flips the forced flag and the
//!   webview blocks syncing until the install completes (D4);
//! - before installing we always attempt one final sync, so the outbox drains
//!   while the current build is still running (D5). The SQLCipher key and the
//!   database file live outside the install directory, and migrations run on
//!   the first start of the new build, so local data survives (D5).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;
use tauri::AppHandle;
use tauri_plugin_updater::UpdaterExt;

/// How often the background worker re-checks for a new release.
pub const CHECK_INTERVAL: Duration = Duration::from_secs(4 * 60 * 60);

/// What the webview shows in the "update available" pill.
#[derive(Debug, Clone, serde::Serialize)]
pub struct UpdateInfo {
    pub available: bool,
    pub current_version: String,
    pub version: Option<String>,
    pub notes: Option<String>,
    /// D4: the server refused to sync this build; installing is not optional.
    pub mandatory: bool,
    /// the raw manifest, replayed to `restore` so a restart can still install
    #[serde(skip_serializing_if = "Option::is_none")]
    pub raw_json: Option<serde_json::Value>,
}

/// Shared updater state behind a mutex-free enum-less design: the plugin's
/// `Updater` is created per call (it holds an HTTP client), while the two
/// sticky facts (a pending release and the forced flag) live here.
#[derive(Default)]
pub struct UpdateState {
    pub pending: std::sync::Mutex<Option<serde_json::Value>>,
    pub forced: AtomicBool,
}

impl UpdateState {
    pub fn is_forced(&self) -> bool {
        self.forced.load(Ordering::Relaxed)
    }

    pub fn force(&self) {
        self.forced.store(true, Ordering::Relaxed);
    }
}

fn build_updater(app: &AppHandle) -> Result<tauri_plugin_updater::Updater, String> {
    app.updater_builder()
        .build()
        .map_err(|e| format!("updater init: {e}"))
}

/// Check the configured endpoint for a newer release. Does NOT download.
pub async fn check(app: &AppHandle, state: &Arc<UpdateState>) -> Result<UpdateInfo, String> {
    let updater = build_updater(app)?;
    let current_version = app.package_info().version.to_string();
    match updater.check().await.map_err(|e| format!("update check: {e}"))? {
        Some(update) => {
            *state.pending.lock().map_err(|e| e.to_string())? = Some(update.raw_json.clone());
            Ok(UpdateInfo {
                available: true,
                current_version,
                version: Some(update.version.clone()),
                notes: update.body.clone(),
                mandatory: state.is_forced(),
                raw_json: Some(update.raw_json.clone()),
            })
        }
        None => {
            *state.pending.lock().map_err(|e| e.to_string())? = None;
            Ok(UpdateInfo {
                available: false,
                current_version,
                version: None,
                notes: None,
                mandatory: state.is_forced(),
                raw_json: None,
            })
        }
    }
}

/// Download + verify + install the pending release. `sync_first` (D5) drains
/// the outbox while the old build is still alive; its failure blocks the
/// install so no unsynced write is ever stranded.
pub async fn download_and_install<F, Fut>(
    app: &AppHandle,
    state: &Arc<UpdateState>,
    sync_first: F,
) -> Result<String, String>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<(), String>>,
{
    let raw = state
        .pending
        .lock()
        .map_err(|e| e.to_string())?
        .clone()
        .ok_or_else(|| "no pending release; run a check first".to_string())?;

    // D5: never install over an undrained outbox
    sync_first().await?;

    let updater = build_updater(app)?;
    let update = updater
        .restore_update(raw)
        .map_err(|e| format!("restore release: {e}"))?
        .ok_or_else(|| "the pending release is no longer offered by the server".to_string())?;

    // signature verification happens inside download() against the pubkey in
    // tauri.conf.json: a tampered package aborts here with an error (D6)
    let bytes = update
        .download(|_chunk, _total| {}, || {})
        .await
        .map_err(|e| format!("download: {e}"))?;
    update.install(bytes).map_err(|e| format!("install: {e}"))?;
    Ok(update.version.clone())
}

/// Background worker: check at startup and every [`CHECK_INTERVAL`]. A failed
/// check is not fatal — the app keeps working offline.
pub fn spawn_checker(app: AppHandle, state: Arc<UpdateState>) {
    tauri::async_runtime::spawn(async move {
        let mut ticker = tokio::time::interval(CHECK_INTERVAL);
        loop {
            ticker.tick().await;
            match check(&app, &state).await {
                Ok(info) if info.available => {
                    tracing::info!("update available: {:?}", info.version);
                }
                Ok(_) => tracing::debug!("no update available"),
                Err(e) => tracing::warn!("update check failed: {e}"),
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn d4_forced_flag_is_sticky_once_the_server_refuses_the_protocol() {
        let state = UpdateState::default();
        assert!(!state.is_forced());
        state.force();
        assert!(state.is_forced(), "a 426 must keep the update mandatory");
    }

    #[test]
    fn d3_pending_release_round_trips_through_the_state() {
        let state = UpdateState::default();
        assert!(state.pending.lock().unwrap().is_none());
        *state.pending.lock().unwrap() =
            Some(serde_json::json!({"version": "9.9.9", "pub_date": "2026-01-01"}));
        let raw = state.pending.lock().unwrap().clone().unwrap();
        assert_eq!(raw["version"], "9.9.9");
    }
}
