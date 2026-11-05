//! G4: LAN Hub for offline multi-PC reception sync.
//!
//! When internet is down in a hotel, multiple reception PCs (e.g. Front Desk 1,
//! Front Desk 2) can connect over the local network to this LAN Hub. The Hub:
//!   1. Receives and queues offline push operations from any local PC.
//!   2. Maintains a shared local change sequence (`lan_change_log`) so peers
//!      immediately see each other's offline bookings and cleanliness updates.
//!   3. Bridges upstream when internet connectivity returns, pushing aggregated
//!      outbox batches to the cloud server and pulling canonical state.

use axum::extract::{Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::{DateTime, Utc};
use hms_core::sync::{
    ChangeOperation, EntityType, OperationType, ServerChangeRecord, SyncOpResult, SyncOpStatus,
    SyncPullResponse, SyncPushRequest, SyncPushResponse, SYNC_PROTOCOL_VERSION,
};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

#[derive(Clone)]
pub struct LanHubState {
    pub hotel_id: i64,
    pub db_path: PathBuf,
    conn: Arc<Mutex<Connection>>,
    pub upstream_url: Option<String>,
    /// Shared secret for peer authentication. Required for HTTP endpoints; do
    /// not expose the hub on an untrusted network without setting it.
    auth_token: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct LanStatus {
    pub hotel_id: i64,
    pub pending_upstream_ops: usize,
    pub latest_lan_seq: i64,
    pub upstream_url: Option<String>,
    pub status: String,
}

#[derive(Debug, Deserialize)]
pub struct LanPullQuery {
    pub since: i64,
    pub limit: Option<usize>,
}

impl LanHubState {
    pub fn new(hotel_id: i64, db_path: impl AsRef<Path>, upstream_url: Option<String>) -> Self {
        Self::new_with_token(hotel_id, db_path, upstream_url, None)
    }

    pub fn new_with_token(
        hotel_id: i64,
        db_path: impl AsRef<Path>,
        upstream_url: Option<String>,
        auth_token: Option<String>,
    ) -> Self {
        let path = db_path.as_ref().to_path_buf();
        let conn = Connection::open(&path).expect("failed to open LAN Hub SQLite database");
        init_db(&conn).expect("failed to initialize LAN Hub database schema");
        Self {
            hotel_id,
            db_path: path,
            conn: Arc::new(Mutex::new(conn)),
            upstream_url,
            auth_token,
        }
    }

    /// Record push operations from a peer reception PC into the LAN Hub store.
    pub fn record_push(&self, req: SyncPushRequest) -> Result<SyncPushResponse, String> {
        if req.hotel_id != self.hotel_id {
            return Err("push hotel_id does not match this LAN hub".into());
        }
        if req.operations.iter().any(|op| op.hotel_id != self.hotel_id) {
            return Err("operation hotel_id does not match this LAN hub".into());
        }
        let conn = self.conn.lock().unwrap();
        let mut results = Vec::new();

        conn.execute_batch("BEGIN TRANSACTION;")
            .map_err(|e| e.to_string())?;

        for op in &req.operations {
            let op_id_str = op.operation_id.to_string();
            let entity_type_str = serde_json::to_string(&op.entity_type)
                .unwrap_or_default()
                .replace('"', "");
            let op_type_str = serde_json::to_string(&op.operation_type)
                .unwrap_or_default()
                .replace('"', "");
            let payload_str = op.payload.to_string();
            let now = Utc::now().to_rfc3339();

            let existing_payload: Option<String> = conn
                .query_row(
                    "SELECT payload FROM lan_outbox WHERE operation_id = ?1",
                    [&op_id_str],
                    |r| r.get(0),
                )
                .ok();
            if let Some(existing) = existing_payload {
                if existing != payload_str {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err("operation_id replayed with a different payload".into());
                }
            }
            let inserted = match conn.execute(
                "INSERT OR IGNORE INTO lan_outbox (operation_id, hotel_id, entity_type, entity_id, op_type, payload, created_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![op_id_str, self.hotel_id, entity_type_str, op.entity_id.to_string(), op_type_str, payload_str, now],
            ) {
                Ok(rows) => rows > 0,
                Err(e) => {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(format!("failed to record op: {e}"));
                }
            };

            if inserted {
                // Only first receipt advances the LAN change log; replays are idempotent.
                let change_res = conn.execute(
                    "INSERT INTO lan_change_log (operation_id, hotel_id, entity_type, entity_id, op_type, payload, created_at)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    params![op_id_str, self.hotel_id, entity_type_str, op.entity_id.to_string(), op_type_str, payload_str, now],
                );
                if let Err(e) = change_res {
                    let _ = conn.execute_batch("ROLLBACK;");
                    return Err(format!("failed to append lan change log: {e}"));
                }
            }

            results.push(SyncOpResult {
                operation_id: op.operation_id,
                entity_type: op.entity_type,
                entity_id: op.entity_id,
                status: if inserted {
                    SyncOpStatus::Applied
                } else {
                    SyncOpStatus::DuplicateIgnored
                },
            });
        }

        conn.execute_batch("COMMIT;").map_err(|e| e.to_string())?;

        let latest_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(max(seq), 0) FROM lan_change_log WHERE hotel_id = ?1",
                [self.hotel_id],
                |r| r.get(0),
            )
            .unwrap_or(0);

        Ok(SyncPushResponse {
            hotel_id: self.hotel_id,
            results,
            server_latest_counter: latest_seq,
        })
    }

    /// Pull local changes from the LAN Hub since a given sequence number.
    pub fn pull_changes(&self, since: i64, limit: usize) -> Result<SyncPullResponse, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT seq, operation_id, hotel_id, entity_type, entity_id, op_type, payload, created_at
                 FROM lan_change_log
                 WHERE hotel_id = ?1 AND seq > ?2
                 ORDER BY seq ASC
                 LIMIT ?3",
            )
            .map_err(|e| e.to_string())?;

        let rows = stmt
            .query_map(params![self.hotel_id, since, limit as i64], |row| {
                let seq: i64 = row.get(0)?;
                let op_id_str: String = row.get(1)?;
                let hotel_id: i64 = row.get(2)?;
                let entity_type_str: String = row.get(3)?;
                let entity_id_str: String = row.get(4)?;
                let op_type_str: String = row.get(5)?;
                let payload_str: String = row.get(6)?;
                let created_at_str: String = row.get(7)?;

                let operation_id = Uuid::parse_str(&op_id_str).unwrap_or_default();
                let entity_id = Uuid::parse_str(&entity_id_str).unwrap_or_default();
                let entity_type: EntityType =
                    serde_json::from_value(serde_json::Value::String(entity_type_str))
                        .unwrap_or(EntityType::Reservation);
                let operation_type: OperationType =
                    serde_json::from_value(serde_json::Value::String(op_type_str))
                        .unwrap_or(OperationType::Insert);
                let payload = serde_json::from_str(&payload_str).unwrap_or(serde_json::Value::Null);
                let server_timestamp = DateTime::parse_from_rfc3339(&created_at_str)
                    .map(|d| d.with_timezone(&Utc))
                    .unwrap_or_else(|_| Utc::now());

                Ok(ServerChangeRecord {
                    change_counter: seq,
                    hotel_id,
                    operation_id,
                    entity_type,
                    entity_id,
                    operation_type,
                    payload,
                    server_timestamp,
                })
            })
            .map_err(|e| e.to_string())?;

        let mut records = Vec::new();
        let mut max_seq = since;
        for r in rows {
            let record = r.map_err(|e| e.to_string())?;
            if record.change_counter > max_seq {
                max_seq = record.change_counter;
            }
            records.push(record);
        }

        let has_more = records.len() == limit;
        Ok(SyncPullResponse {
            hotel_id: self.hotel_id,
            records,
            latest_counter: max_seq,
            has_more,
        })
    }

    /// Drain pending operations (oldest first, at most `limit`) for the upstream
    /// bridge. The caller marks them forwarded after a successful upstream push.
    pub fn take_pending_ops(&self, limit: usize) -> Result<Vec<ChangeOperation>, String> {
        let conn = self.conn.lock().unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT operation_id, entity_type, entity_id, op_type, payload, created_at
                 FROM lan_outbox WHERE forwarded = 0 ORDER BY rowid ASC LIMIT ?1",
            )
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([limit as i64], |row| {
                let op_id: String = row.get(0)?;
                let entity_type: String = row.get(1)?;
                let entity_id: String = row.get(2)?;
                let op_type: String = row.get(3)?;
                let payload: String = row.get(4)?;
                let created_at: String = row.get(5)?;
                Ok(ChangeOperation {
                    operation_id: Uuid::parse_str(&op_id).unwrap_or_default(),
                    hotel_id: self.hotel_id,
                    entity_type: serde_json::from_value(serde_json::Value::String(entity_type))
                        .unwrap_or(EntityType::Reservation),
                    entity_id: Uuid::parse_str(&entity_id).unwrap_or_default(),
                    operation_type: serde_json::from_value(serde_json::Value::String(op_type))
                        .unwrap_or(OperationType::Insert),
                    payload: serde_json::from_str(&payload).unwrap_or(serde_json::Value::Null),
                    client_timestamp: DateTime::parse_from_rfc3339(&created_at)
                        .map(|d| d.with_timezone(&Utc))
                        .unwrap_or_else(|_| Utc::now()),
                })
            })
            .map_err(|e| e.to_string())?;
        let mut ops = Vec::new();
        for r in rows {
            ops.push(r.map_err(|e| e.to_string())?);
        }
        Ok(ops)
    }

    /// Mark operations as successfully forwarded to the cloud server.
    pub fn mark_forwarded(&self, operation_ids: &[Uuid]) -> Result<(), String> {
        let conn = self.conn.lock().unwrap();
        for id in operation_ids {
            conn.execute(
                "UPDATE lan_outbox SET forwarded = 1 WHERE operation_id = ?1",
                params![id.to_string()],
            )
            .map_err(|e| e.to_string())?;
        }
        Ok(())
    }

    /// Get current LAN status overview.
    pub fn status(&self) -> LanStatus {
        let conn = self.conn.lock().unwrap();
        let pending: usize = conn
            .query_row(
                "SELECT count(*) FROM lan_outbox WHERE forwarded = 0",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);
        let latest_seq: i64 = conn
            .query_row(
                "SELECT COALESCE(max(seq), 0) FROM lan_change_log",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        LanStatus {
            hotel_id: self.hotel_id,
            pending_upstream_ops: pending,
            latest_lan_seq: latest_seq,
            upstream_url: self.upstream_url.clone(),
            status: "ready".into(),
        }
    }
}

fn init_db(conn: &Connection) -> rusqlite::Result<()> {
    conn.execute_batch(
        r#"
        CREATE TABLE IF NOT EXISTS lan_outbox (
            operation_id TEXT PRIMARY KEY,
            hotel_id INTEGER NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            op_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL,
            forwarded INTEGER NOT NULL DEFAULT 0
        );

        CREATE TABLE IF NOT EXISTS lan_change_log (
            seq INTEGER PRIMARY KEY AUTOINCREMENT,
            operation_id TEXT NOT NULL,
            hotel_id INTEGER NOT NULL,
            entity_type TEXT NOT NULL,
            entity_id TEXT NOT NULL,
            op_type TEXT NOT NULL,
            payload TEXT NOT NULL,
            created_at TEXT NOT NULL
        );

        CREATE INDEX IF NOT EXISTS idx_lan_change_seq ON lan_change_log (hotel_id, seq);
        CREATE INDEX IF NOT EXISTS idx_lan_outbox_pending ON lan_outbox (forwarded);
        "#,
    )
}

// -----------------------------------------------------------------------------
// Axum HTTP router for LAN peers
// -----------------------------------------------------------------------------

pub fn router(state: LanHubState) -> Router {
    Router::new()
        .route("/api/v1/lan/push", post(handle_push))
        .route("/api/v1/lan/pull", get(handle_pull))
        .route("/api/v1/lan/status", get(handle_status))
        .with_state(state)
}

fn authorize(state: &LanHubState, headers: &HeaderMap) -> Result<(), (StatusCode, String)> {
    match &state.auth_token {
        None => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            "LAN_HUB_TOKEN is required; refusing unauthenticated LAN access".into(),
        )),
        Some(expected) => {
            let actual = headers
                .get(axum::http::header::AUTHORIZATION)
                .and_then(|v| v.to_str().ok())
                .and_then(|v| v.strip_prefix("Bearer "));
            if actual == Some(expected.as_str()) {
                Ok(())
            } else {
                Err((
                    StatusCode::UNAUTHORIZED,
                    "invalid LAN hub bearer token".into(),
                ))
            }
        }
    }
}

async fn handle_push(
    State(state): State<LanHubState>,
    headers: HeaderMap,
    Json(body): Json<SyncPushRequest>,
) -> Result<Json<SyncPushResponse>, (StatusCode, String)> {
    authorize(&state, &headers)?;
    if body.protocol_version != SYNC_PROTOCOL_VERSION {
        return Err((
            StatusCode::UPGRADE_REQUIRED,
            "protocol version mismatch".into(),
        ));
    }
    match state.record_push(body) {
        Ok(resp) => Ok(Json(resp)),
        Err(e)
            if e.contains("does not match") || e.contains("replayed with a different payload") =>
        {
            Err((StatusCode::BAD_REQUEST, e))
        }
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn handle_pull(
    State(state): State<LanHubState>,
    headers: HeaderMap,
    Query(q): Query<LanPullQuery>,
) -> Result<Json<SyncPullResponse>, (StatusCode, String)> {
    authorize(&state, &headers)?;
    let limit = q.limit.unwrap_or(100).min(500);
    match state.pull_changes(q.since, limit) {
        Ok(resp) => Ok(Json(resp)),
        Err(e) => Err((StatusCode::INTERNAL_SERVER_ERROR, e)),
    }
}

async fn handle_status(
    State(state): State<LanHubState>,
    headers: HeaderMap,
) -> Result<Json<LanStatus>, (StatusCode, String)> {
    authorize(&state, &headers)?;
    Ok(Json(state.status()))
}

// -----------------------------------------------------------------------------
// Upstream bridge: forward the LAN outbox to the cloud when the WAN is back
// -----------------------------------------------------------------------------

/// Push all pending LAN outbox operations to the cloud server's
/// `POST {base}/sync/push`, then mark them forwarded. Returns the number of ops
/// forwarded. Safe to call repeatedly; idempotent on the server via
/// `operation_id`.
pub async fn forward_outbox_upstream(
    state: &LanHubState,
    bearer_token: &str,
    client: &reqwest::Client,
) -> Result<usize, String> {
    let base = state
        .upstream_url
        .clone()
        .ok_or_else(|| "no upstream_url configured".to_string())?;
    let ops = state.take_pending_ops(200)?;
    if ops.is_empty() {
        return Ok(0);
    }
    let ids: Vec<Uuid> = ops.iter().map(|o| o.operation_id).collect();
    let req = SyncPushRequest {
        hotel_id: state.hotel_id,
        operations: ops,
        protocol_version: SYNC_PROTOCOL_VERSION,
    };
    let url = format!("{}/sync/push", base.trim_end_matches('/'));
    let resp = client
        .post(&url)
        .bearer_auth(bearer_token)
        .json(&req)
        .send()
        .await
        .map_err(|e| format!("upstream push failed: {e}"))?;
    if !resp.status().is_success() {
        return Err(format!("upstream push HTTP {}", resp.status()));
    }
    // The API wraps DTOs in ApiResponse. Only settle accepted operations;
    // rejected ops remain queued for operator inspection/retry after correction.
    let envelope: serde_json::Value = resp
        .json()
        .await
        .map_err(|e| format!("invalid upstream push response: {e}"))?;
    if envelope.get("success").and_then(|v| v.as_bool()) != Some(true) {
        return Err(envelope
            .get("error")
            .and_then(|v| v.as_str())
            .unwrap_or("upstream rejected push")
            .to_string());
    }
    let data = envelope
        .get("data")
        .ok_or_else(|| "upstream response has no data".to_string())?;
    let results = data
        .get("results")
        .and_then(|v| v.as_array())
        .ok_or_else(|| "upstream response has no results".to_string())?;
    let accepted: Vec<Uuid> = results
        .iter()
        .filter(|r| {
            let status = &r["status"];
            !(status.get("rejected").is_some() || status.as_str() == Some("rejected"))
        })
        .filter_map(|r| {
            r.get("operation_id")
                .and_then(|v| v.as_str())
                .and_then(|s| Uuid::parse_str(s).ok())
        })
        .collect();
    state.mark_forwarded(&accepted)?;
    Ok(accepted.len())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn push_and_pull_between_lan_peers() {
        let dir = tempdir().unwrap();
        let db_path = dir.path().join("lan_hub.sqlite");
        let hub = LanHubState::new(4442, &db_path, None);

        // PC 1 pushes an offline booking
        let op_id = Uuid::now_v7();
        let res_id = Uuid::now_v7();
        let push_req = SyncPushRequest {
            hotel_id: 4442,
            protocol_version: SYNC_PROTOCOL_VERSION,
            operations: vec![ChangeOperation {
                operation_id: op_id,
                hotel_id: 4442,
                entity_type: EntityType::Reservation,
                entity_id: res_id,
                operation_type: OperationType::Insert,
                payload: serde_json::json!({
                    "room_id": Uuid::now_v7(),
                    "checkin_date": "2026-11-10",
                    "checkout_date": "2026-11-12",
                    "status": "confirmed",
                    "total_price_cents": 50000
                }),
                client_timestamp: Utc::now(),
            }],
        };

        let push_res = hub.record_push(push_req).unwrap();
        assert_eq!(push_res.results.len(), 1);
        assert_eq!(push_res.results[0].operation_id, op_id);
        assert_eq!(push_res.server_latest_counter, 1);

        // PC 2 pulls changes from the LAN Hub
        let pull_res = hub.pull_changes(0, 50).unwrap();
        assert_eq!(pull_res.records.len(), 1);
        assert_eq!(pull_res.records[0].entity_type, EntityType::Reservation);
        assert_eq!(pull_res.records[0].entity_id, res_id);

        let status = hub.status();
        assert_eq!(status.pending_upstream_ops, 1);
        assert_eq!(status.latest_lan_seq, 1);
    }

    #[test]
    fn tenant_isolation_and_forwarding() {
        let dir = tempdir().unwrap();
        let hub_a = LanHubState::new(4442, dir.path().join("a.sqlite"), None);
        let hub_b = LanHubState::new(4443, dir.path().join("b.sqlite"), None);

        let op = ChangeOperation {
            operation_id: Uuid::now_v7(),
            hotel_id: 4442,
            entity_type: EntityType::RoomStatus,
            entity_id: Uuid::now_v7(),
            operation_type: OperationType::Update,
            payload: serde_json::json!({ "cleanliness": "clean" }),
            client_timestamp: Utc::now(),
        };
        hub_a
            .record_push(SyncPushRequest {
                hotel_id: 4442,
                protocol_version: SYNC_PROTOCOL_VERSION,
                operations: vec![op.clone()],
            })
            .unwrap();

        // hotel B's hub sees nothing from hotel A
        assert_eq!(hub_b.pull_changes(0, 50).unwrap().records.len(), 0);

        // pending op is drained for upstream forwarding, then marked
        let pending = hub_a.take_pending_ops(10).unwrap();
        assert_eq!(pending.len(), 1);
        assert_eq!(pending[0].operation_id, op.operation_id);
        hub_a.mark_forwarded(&[op.operation_id]).unwrap();
        assert_eq!(hub_a.status().pending_upstream_ops, 0);

        // the LAN change log is retained for peers even after forwarding
        let peer_view = hub_a.pull_changes(0, 50).unwrap();
        assert_eq!(peer_view.records.len(), 1);
        assert_eq!(peer_view.records[0].operation_type, OperationType::Update);
    }
}
