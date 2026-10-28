//! SQLite-backed implementation of [`hms_sync::store::SyncStore`] for the
//! desktop local mirror (README §11): outbox, pull cursor, remote applier and
//! status counters all live in the embedded database. `apply_remote` runs in
//! the same connection but keeps its integrity via the `applied_ops` dedupe
//! table so replays of the same record are no-ops.

use crate::dpapi;
use chrono::Utc;
use hms_core::sync::{
    ChangeOperation, EntityType, OperationType, OfflineLoginCache, ServerChangeRecord,
};
use hms_sync::store::{QueuedOperation, RemoteOutcome, Settle, StoreError, SyncStore};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

pub struct LocalStore {
    conn: Connection,
}

fn se(e: rusqlite::Error) -> StoreError {
    StoreError::Io(e.to_string())
}

impl LocalStore {
    /// Open (creating/tracking the schema as needed). On Windows the session
    /// token + offline cache are stored DPAPI-protected inside `kv`.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open(path).map_err(se)?;
        conn.pragma_update(None, "journal_mode", "WAL").map_err(se)?;
        conn.pragma_update(None, "synchronous", "NORMAL").map_err(se)?;
        conn.execute_batch(include_str!(
            "../desktop-migrations/0001_init.sql"
        ))
        .map_err(se)?;
        Ok(Self { conn })
    }

    // -- read surface used by the tests and the upcoming local-first UI wiring
    // (per-page reads land with the web/desktop integration step)

    #[allow(dead_code)] // consumed by #[cfg(test)] and the next integration step
    pub fn with_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory().map_err(se)?;
        conn.execute_batch(include_str!(
            "../desktop-migrations/0001_init.sql"
        ))
        .map_err(se)?;
        Ok(Self { conn })
    }

    // ---- offline login cache (kv-backed, DPAPI-protected sensitive fields)

    pub fn save_offline_cache(&mut self, cache: &OfflineLoginCache) -> Result<(), StoreError> {
        let value = serde_json::to_string(cache)
            .map_err(|e| StoreError::Io(e.to_string()))?;
        let protected = dpapi::protect(value.as_bytes())
            .map_err(StoreError::Io)?;
        let hex = hex_encode(&protected);
        self.kv_set("offline_login_cache", &hex)
    }

    pub fn load_offline_cache(&mut self) -> Result<Option<OfflineLoginCache>, StoreError> {
        let Some(hex) = self.conn
            .query_row("SELECT value FROM kv WHERE key = 'offline_login_cache'", [], |r| r.get::<_, String>(0))
            .optional()
            .map_err(se)?
        else {
            return Ok(None);
        };
        let bytes = hex_decode(&hex).map_err(StoreError::Io)?;
        let plain = dpapi::unprotect(&bytes).map_err(StoreError::Io)?;
        let s = String::from_utf8(plain).map_err(|e| StoreError::Io(e.to_string()))?;
        serde_json::from_str(&s).map(Some).map_err(|e| StoreError::Io(e.to_string()))
    }

    pub fn drop_offline_cache(&mut self) -> Result<(), StoreError> {
        self.conn.execute("DELETE FROM kv WHERE key = 'offline_login_cache'", [])
            .map_err(se)?;
        Ok(())
    }

    // ---- token storage for the background sync worker (DPAPI too)

    pub fn save_token(&mut self, token: &str) -> Result<(), StoreError> {
        let protected = dpapi::protect(token.as_bytes()).map_err(StoreError::Io)?;
        self.kv_set("auth_token", &hex_encode(&protected))
    }

    pub fn load_token(&mut self) -> Result<Option<String>, StoreError> {
        let Some(hex) = self.conn
            .query_row("SELECT value FROM kv WHERE key = 'auth_token'", [], |r| r.get::<_, String>(0))
            .optional()
            .map_err(se)?
        else {
            return Ok(None);
        };
        let bytes = hex_decode(&hex).map_err(StoreError::Io)?;
        let plain = dpapi::unprotect(&bytes).map_err(StoreError::Io)?;
        Ok(Some(String::from_utf8_lossy(&plain).into_owned()))
    }

    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(se)?;
        Ok(())
    }

    /// The hotel this local DB mirrors (0 = not logged in yet). Written by
    /// `set_auth_token` after the first online login.
    pub fn hotel_id(&mut self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'hotel_id'", [], |r| r.get::<_, String>(0))
            .optional()
            .map_err(se)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    pub fn set_hotel_id(&mut self, hotel_id: i64) -> Result<(), StoreError> {
        self.kv_set("hotel_id", &hotel_id.to_string())
    }

    /// Reservations parked as `conflict` by the sync applier (or locally when
    /// the pulled later-booking echo carries the flag).
    #[allow(dead_code)] // status surface hidden until the UI wiring step
    pub fn tentative_conflicts(&self) -> Result<usize, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM reservations WHERE status = 'conflict' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(se)?)
    }

    /// Local reads used by tests and the first UI integration step: reservations
    /// of this hotel newest-first.
    #[allow(dead_code)] // local read API for the UI wiring step
    pub fn list_reservation_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare("SELECT id FROM reservations ORDER BY checkin_date DESC").map_err(se)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        Ok(rows)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

impl SyncStore for LocalStore {
    fn enqueue(&mut self, op: ChangeOperation, now_ms: i64) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO outbox
                   (operation_id, hotel_id, entity_type, entity_id, operation_type,
                    payload, client_timestamp, enqueued_at_ms, attempts, last_error, next_retry_at_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,NULL,NULL)",
                params![
                    op.operation_id.to_string(),
                    op.hotel_id,
                    entity_name(op.entity_type),
                    op.entity_id.to_string(),
                    operation_name(op.operation_type),
                    serde_json::to_string(&op.payload).map_err(|e| StoreError::Io(e.to_string()))?,
                    op.client_timestamp.to_rfc3339(),
                    now_ms,
                ],
            )
            .map_err(se)?;
        Ok(())
    }

    fn pending(&mut self, limit: usize) -> Result<Vec<QueuedOperation>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT operation_id, hotel_id, entity_type, entity_id, operation_type,
                        payload, client_timestamp, enqueued_at_ms, attempts, last_error, next_retry_at_ms
                 FROM outbox ORDER BY enqueued_at_ms LIMIT ?1",
            )
            .map_err(se)?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(QueuedOperation {
                    op: ChangeOperation {
                        operation_id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                        hotel_id: r.get(1)?,
                        entity_type: entity_of(&r.get::<_, String>(2)?),
                        entity_id: Uuid::parse_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                        operation_type: operation_of(&r.get::<_, String>(4)?),
                        payload: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
                        client_timestamp: chrono::DateTime::parse_from_rfc3339(
                            &r.get::<_, String>(6)?,
                        )
                        .unwrap_or_else(|_| Utc::now().into())
                        .with_timezone(&Utc),
                    },
                    enqueued_at_ms: r.get(7)?,
                    attempts: r.get::<_, i64>(8)?.max(0) as u32,
                    last_error: r.get(9)?,
                    next_retry_at_ms: r.get(10)?,
                })
            })
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        Ok(rows)
    }

    fn record_attempt(
        &mut self,
        operation_id: Uuid,
        attempts: u32,
        last_error: Option<String>,
        next_retry_at_ms: Option<i64>,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "UPDATE outbox SET attempts = ?2, last_error = ?3, next_retry_at_ms = ?4
                 WHERE operation_id = ?1",
                params![operation_id.to_string(), attempts as i64, last_error, next_retry_at_ms],
            )
            .map_err(se)?;
        Ok(())
    }

    fn settle(&mut self, operation_id: Uuid, outcome: Settle) -> Result<(), StoreError> {
        let id = operation_id.to_string();
        if outcome == Settle::FailedPermanent {
            // preserve for the status UI with reason + attempts
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO failures (operation_id, entity_type, entity_id, attempts, last_error, occurred_at)
                     SELECT operation_id, entity_type, entity_id, attempts, last_error, ?2 FROM outbox WHERE operation_id = ?1",
                    params![id, Utc::now().timestamp_millis()],
                )
                .map_err(se)?;
        }
        self.conn
            .execute("DELETE FROM outbox WHERE operation_id = ?1", params![id])
            .map_err(se)?;
        Ok(())
    }

    fn failures(&mut self) -> Result<Vec<QueuedOperation>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT operation_id, entity_type, entity_id, attempts, last_error, occurred_at
                 FROM failures ORDER BY occurred_at",
            )
            .map_err(se)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(QueuedOperation {
                    op: ChangeOperation {
                        operation_id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                        hotel_id: 0,
                        entity_type: entity_of(&r.get::<_, String>(1)?),
                        entity_id: Uuid::parse_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                        operation_type: OperationType::Insert,
                        payload: serde_json::json!({}),
                        client_timestamp: Utc::now(),
                    },
                    enqueued_at_ms: r.get(5)?,
                    attempts: r.get::<_, i64>(3)?.max(0) as u32,
                    last_error: r.get(4)?,
                    next_retry_at_ms: None,
                })
            })
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        Ok(rows)
    }

    fn clear_failure(&mut self, operation_id: Uuid) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM failures WHERE operation_id = ?1", params![operation_id.to_string()])
            .map_err(se)?;
        Ok(())
    }

    fn pending_count(&mut self) -> Result<usize, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .map_err(se)?)
    }

    fn pull_cursor(&mut self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'pull_cursor'", [], |r| r.get::<_, String>(0))
            .optional()
            .map_err(se)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    fn set_pull_cursor(&mut self, seq: i64) -> Result<(), StoreError> {
        self.kv_set("pull_cursor", &seq.to_string())
    }

    fn apply_remote(&mut self, record: &ServerChangeRecord) -> Result<RemoteOutcome, StoreError> {
        // dedupe: the same change_seq may arrive twice (resume mid-pull)
        let id = record.operation_id.to_string();
        let seq = record.change_counter;
        let seen: Option<i64> = self
            .conn
            .query_row(
                "SELECT change_seq FROM applied_ops WHERE operation_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map_err(se)?;
        if seen.is_some() {
            self.bump_cursor(seq)?;
            return Ok(RemoteOutcome::AlreadyKnown);
        }

        let payload = &record.payload;
        let eid = record.entity_id.to_string();
        let now = Utc::now().timestamp_millis();
        match (record.entity_type, record.operation_type) {
            (EntityType::Guest, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO guests (id, full_name, nationality, license_plate, birth_date)
                         VALUES (?1,?2,?3,?4,?5)
                         ON CONFLICT (id) DO UPDATE SET
                           full_name = excluded.full_name, nationality = excluded.nationality,
                           license_plate = excluded.license_plate, birth_date = excluded.birth_date,
                           deleted_at = NULL",
                        params![
                            eid,
                            payload["full_name"].as_str().unwrap_or(""),
                            payload["nationality"].as_str(),
                            payload["license_plate"].as_str(),
                            payload["birth_date"].as_str(),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Guest, OperationType::Delete) => {
                self.conn
                    .execute("UPDATE guests SET deleted_at = ?2 WHERE id = ?1", params![eid, now])
                    .map_err(se)?;
            }
            (EntityType::Reservation, OperationType::Insert | OperationType::Update) => {
                let conflict = payload.get("conflict").is_some() || payload["status"].as_str() == Some("conflict");
                self.conn
                    .execute(
                        "INSERT INTO reservations
                           (id, room_id, channel_id, guest_id, checkin_date, checkout_date,
                            status, total_price_cents, adults, children)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                         ON CONFLICT (id) DO UPDATE SET
                           room_id = excluded.room_id, channel_id = excluded.channel_id,
                           guest_id = excluded.guest_id, checkin_date = excluded.checkin_date,
                           checkout_date = excluded.checkout_date,
                           status = CASE WHEN reservations.status = 'conflict' AND NOT ?11
                                         THEN reservations.status ELSE excluded.status END,
                           total_price_cents = excluded.total_price_cents,
                           adults = excluded.adults, children = excluded.children, deleted_at = NULL",
                        params![
                            eid,
                            payload["room_id"].as_str().unwrap_or(""),
                            payload["channel_id"].as_str(),
                            payload["guest_id"].as_str(),
                            payload["checkin_date"].as_str().unwrap_or(""),
                            payload["checkout_date"].as_str().unwrap_or(""),
                            if conflict { "conflict" } else { payload["status"].as_str().unwrap_or("confirmed") },
                            payload["total_price_cents"].as_i64().unwrap_or(0),
                            payload["adults"].as_i64().unwrap_or(1).max(0),
                            payload["children"].as_i64().unwrap_or(0).max(0),
                            conflict,
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Reservation, OperationType::Delete) => {
                self.conn
                    .execute("UPDATE reservations SET deleted_at = ?2 WHERE id = ?1", params![eid, now])
                    .map_err(se)?;
            }
            (EntityType::Room, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO rooms (id, room_type_id, room_number, floor, cleanliness)
                         VALUES (?1,?2,?3,?4,?5)
                         ON CONFLICT (id) DO UPDATE SET room_type_id = excluded.room_type_id,
                           room_number = excluded.room_number, floor = excluded.floor,
                           cleanliness = excluded.cleanliness, deleted_at = NULL",
                        params![
                            eid,
                            payload["room_type_id"].as_str().unwrap_or(""),
                            payload["room_number"].as_str().unwrap_or(""),
                            payload["floor"].as_i64().unwrap_or(0),
                            payload["cleanliness"].as_str().unwrap_or("clean"),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::RoomStatus, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "UPDATE rooms SET cleanliness = ?2 WHERE id = ?1",
                        params![eid, payload["cleanliness"].as_str().unwrap_or("clean")],
                    )
                    .map_err(se)?;
            }
            (EntityType::RoomType, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO room_types (id, name, capacity, base_price_cents)
                         VALUES (?1,?2,?3,?4)
                         ON CONFLICT (id) DO UPDATE SET name = excluded.name,
                           capacity = excluded.capacity, base_price_cents = excluded.base_price_cents",
                        params![
                            eid,
                            payload["name"].as_str().unwrap_or(""),
                            payload["capacity"].as_i64().unwrap_or(2),
                            payload["base_price_cents"].as_i64().unwrap_or(0),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::PriceRule, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO price_rules (id, room_type_id, date_from, date_to, price_cents, currency)
                         VALUES (?1,?2,?3,?4,?5,?6)
                         ON CONFLICT (id) DO UPDATE SET room_type_id = excluded.room_type_id,
                           date_from = excluded.date_from, date_to = excluded.date_to,
                           price_cents = excluded.price_cents, currency = excluded.currency",
                        params![
                            eid,
                            payload["room_type_id"].as_str().unwrap_or(""),
                            payload["date_from"].as_str().unwrap_or(""),
                            payload["date_to"].as_str().unwrap_or(""),
                            payload["price_cents"].as_i64().unwrap_or(0),
                            payload["currency"].as_str().unwrap_or("TRY"),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Channel, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO channels (id, kind, name, is_active) VALUES (?1,?2,?3,?4)
                         ON CONFLICT (id) DO UPDATE SET kind = excluded.kind,
                           name = excluded.name, is_active = excluded.is_active",
                        params![
                            eid,
                            payload["kind"].as_str().unwrap_or("other"),
                            payload["name"].as_str().unwrap_or(""),
                            payload["is_active"].as_bool().unwrap_or(true) as i64,
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Payment, OperationType::Insert) => {
                self.conn
                    .execute(
                        "INSERT OR IGNORE INTO payments (id, reservation_id, amount_cents, currency,
                              payment_type, reference, is_reversal, created_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                        params![
                            eid,
                            payload["reservation_id"].as_str().unwrap_or(""),
                            payload["amount_cents"].as_i64().unwrap_or(0),
                            payload["currency"].as_str().unwrap_or("TRY"),
                            payload["payment_type"].as_str().unwrap_or("cash"),
                            payload["reference"].as_str(),
                            (payload["amount_cents"].as_i64().unwrap_or(0) < 0) as i64,
                            now,
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Payment, OperationType::Update | OperationType::Delete) => {
                // append-only: a late echo of a rejected op is a no-op here
            }
            (EntityType::FolioTransaction, OperationType::Insert) => {
                self.conn
                    .execute(
                        "INSERT OR IGNORE INTO folio_transactions (id, reservation_id, category,
                              description, unit_price_cents, quantity, total_cents, currency)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                        params![
                            eid,
                            payload["reservation_id"].as_str().unwrap_or(""),
                            payload["category"].as_str().unwrap_or("other"),
                            payload["description"].as_str().unwrap_or(""),
                            payload["unit_price_cents"].as_i64().unwrap_or(0),
                            payload["quantity"].as_i64().unwrap_or(1),
                            payload["total_cents"].as_i64().unwrap_or(0),
                            payload["currency"].as_str().unwrap_or("TRY"),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::FolioTransaction, _) => {}
            (EntityType::User, _) => {
                // user ops never reach the stream from pushes (rejected);
                // server-side user changes we deliberately do not mirror
            }
            (EntityType::Room, OperationType::Delete) => {
                self.conn
                    .execute("UPDATE rooms SET deleted_at = ?2 WHERE id = ?1", params![eid, now])
                    .map_err(se)?;
            }
            (EntityType::RoomType, OperationType::Delete) => {
                self.conn
                    .execute("UPDATE room_types SET deleted_at = ?2 WHERE id = ?1", params![eid, now])
                    .map_err(se)?;
            }
            (EntityType::RoomStatus, OperationType::Delete) => {
                // room removals arrive as Room deletes; no tombstone locally
            }
            (EntityType::PriceRule, OperationType::Delete) => {
                self.conn
                    .execute("DELETE FROM price_rules WHERE id = ?1", params![eid])
                    .map_err(se)?;
            }
            (EntityType::Channel, OperationType::Delete) => {
                self.conn
                    .execute("UPDATE channels SET is_active = 0 WHERE id = ?1", params![eid])
                    .map_err(se)?;
            }
        }

        self.conn
            .execute(
                "INSERT INTO applied_ops (operation_id, change_seq, applied_at) VALUES (?1,?2,?3)",
                params![id, seq, now],
            )
            .map_err(se)?;
        self.bump_cursor(seq)?;
        Ok(RemoteOutcome::Applied)
    }

    fn last_synced_at_ms(&mut self) -> Result<Option<i64>, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'last_synced_at'", [], |r| r.get::<_, String>(0))
            .optional()
            .map_err(se)?
            .and_then(|v| v.parse().ok()))
    }

    fn set_last_synced_at_ms(&mut self, ms: i64) -> Result<(), StoreError> {
        self.kv_set("last_synced_at", &ms.to_string())
    }

    fn conflict_count(&mut self) -> Result<usize, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM reservations WHERE status = 'conflict' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(se)?)
    }
}

impl LocalStore {
    fn bump_cursor(&mut self, seq: i64) -> Result<(), StoreError> {
        // only forward (monotonic per-hotel counter; never a timestamp)
        let cur = self.pull_cursor()?;
        if seq > cur {
            self.set_pull_cursor(seq)?;
        }
        Ok(())
    }
}

fn entity_name(e: EntityType) -> &'static str {
    match e {
        EntityType::RoomType => "room_type",
        EntityType::Room => "room",
        EntityType::RoomStatus => "room_status",
        EntityType::Reservation => "reservation",
        EntityType::Guest => "guest",
        EntityType::Payment => "payment",
        EntityType::FolioTransaction => "folio_transaction",
        EntityType::PriceRule => "price_rule",
        EntityType::User => "user",
        EntityType::Channel => "channel",
    }
}

fn entity_of(s: &str) -> EntityType {
    match s {
        "room_type" => EntityType::RoomType,
        "room" => EntityType::Room,
        "room_status" => EntityType::RoomStatus,
        "payment" => EntityType::Payment,
        "folio_transaction" => EntityType::FolioTransaction,
        "price_rule" => EntityType::PriceRule,
        "user" => EntityType::User,
        "channel" => EntityType::Channel,
        "reservation" => EntityType::Reservation,
        _ => EntityType::Guest,
    }
}

fn operation_name(o: OperationType) -> &'static str {
    match o {
        OperationType::Insert => "insert",
        OperationType::Update => "update",
        OperationType::Delete => "delete",
    }
}

fn operation_of(s: &str) -> OperationType {
    match s {
        "update" => OperationType::Update,
        "delete" => OperationType::Delete,
        _ => OperationType::Insert,
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use hms_core::sync::{
        DEFAULT_OFFLINE_LOGIN_DAYS, SyncOpResult, SyncOpStatus, SyncPullResponse,
        SyncPushRequest, SyncPushResponse,
    };
    use hms_sync::engine::SyncEngine;
    use hms_sync::transport::{SyncTransport, TransportError};
    use std::{collections::VecDeque, sync::Mutex};

    const HOTEL: i64 = 4443;

    fn guest_op(name: &str) -> ChangeOperation {
        ChangeOperation {
            operation_id: Uuid::now_v7(),
            hotel_id: HOTEL,
            entity_type: EntityType::Guest,
            entity_id: Uuid::now_v7(),
            operation_type: OperationType::Insert,
            payload: serde_json::json!({ "full_name": name }),
            client_timestamp: Utc::now(),
        }
    }

    fn server_change(seq: i64, entity_type: EntityType, entity_id: Uuid, payload: serde_json::Value) -> ServerChangeRecord {
        ServerChangeRecord {
            change_counter: seq,
            hotel_id: HOTEL,
            operation_id: Uuid::now_v7(),
            entity_type,
            entity_id,
            operation_type: OperationType::Insert,
            payload,
            server_timestamp: Utc::now(),
        }
    }

    /// Scripted `SyncTransport`: acknowledges every push as Applied and hands
    /// out pull pages in script order (traits needs Send + Sync -> Mutexes).
    struct ScriptedTransport {
        pushes: Mutex<Vec<SyncPushRequest>>,
        pulls: Mutex<VecDeque<Vec<ServerChangeRecord>>>,
    }

    impl ScriptedTransport {
        fn new(pages: Vec<Vec<ServerChangeRecord>>) -> Self {
            Self {
                pushes: Mutex::new(Vec::new()),
                pulls: Mutex::new(VecDeque::from(pages)),
            }
        }
    }

    impl SyncTransport for ScriptedTransport {
        async fn push(&self, req: SyncPushRequest) -> Result<SyncPushResponse, TransportError> {
            self.pushes.lock().expect("pushes lock").push(req.clone());
            let results = req
                .operations
                .iter()
                .map(|op| SyncOpResult {
                    operation_id: op.operation_id,
                    entity_type: op.entity_type,
                    entity_id: op.entity_id,
                    status: SyncOpStatus::Applied,
                })
                .collect();
            Ok(SyncPushResponse {
                hotel_id: req.hotel_id,
                results,
                server_latest_counter: 3,
            })
        }

        async fn pull(
            &self,
            hotel_id: i64,
            _since: i64,
            _limit: usize,
        ) -> Result<SyncPullResponse, TransportError> {
            let mut pages = self.pulls.lock().expect("pulls lock");
            let records = pages
                .pop_front()
                .ok_or_else(|| TransportError::Network("pull script exhausted".into()))?;
            Ok(SyncPullResponse {
                hotel_id,
                records,
                latest_counter: 3,
                has_more: false,
            })
        }
    }

    #[test]
    fn enqueue_pending_attempt_settle_round_trip() {
        let mut s = LocalStore::with_in_memory().unwrap();
        s.set_hotel_id(HOTEL).unwrap();
        assert_eq!(s.hotel_id().unwrap(), HOTEL);

        // two ops enqueued out of order; pending() is oldest-enqueue first
        let op_a = guest_op("Ada");
        s.enqueue(op_a.clone(), 100).unwrap();
        let op_b = ChangeOperation { operation_id: Uuid::now_v7(), ..op_a.clone() };
        s.enqueue(op_b.clone(), 50).unwrap();

        let q = s.pending(10).unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].op.operation_id, op_b.operation_id);
        assert_eq!(q[0].enqueued_at_ms, 50);
        assert_eq!(q[1].op.operation_id, op_a.operation_id);
        assert_eq!(q[1].op.payload["full_name"], "Ada");
        assert_eq!(q[1].attempts, 0);

        s.record_attempt(op_a.operation_id, 1, Some("net down".into()), Some(2_000))
            .unwrap();
        let q = s.pending(10).unwrap();
        assert_eq!(q[1].attempts, 1);
        assert_eq!(q[1].last_error.as_deref(), Some("net down"));
        assert_eq!(q[1].next_retry_at_ms, Some(2_000));

        s.settle(op_a.operation_id, hms_sync::store::Settle::Acknowledged).unwrap();
        assert_eq!(s.pending_count().unwrap(), 1);

        // permanent rejection: out of the outbox, preserved in the failure list
        s.settle(op_b.operation_id, hms_sync::store::Settle::FailedPermanent)
            .unwrap();
        assert_eq!(s.pending_count().unwrap(), 0);
        let fails = s.failures().unwrap();
        assert_eq!(fails.len(), 1);
        assert_eq!(fails[0].op.operation_id, op_b.operation_id);
        assert_eq!(fails[0].attempts, 0);
        s.clear_failure(op_b.operation_id).unwrap();
        assert!(s.failures().unwrap().is_empty());
    }

    #[test]
    fn apply_remote_dedupe_cursor_and_conflict_count() {
        let mut s = LocalStore::with_in_memory().unwrap();
        let guest_id = Uuid::now_v7();

        let rec = server_change(
            7,
            EntityType::Guest,
            guest_id,
            serde_json::json!({ "full_name": "Pulled Guest", "nationality": "DE" }),
        );

        // fresh apply + dedupe on operation id (resume mid-pull)
        assert_eq!(s.apply_remote(&rec).unwrap(), RemoteOutcome::Applied);
        assert_eq!(s.apply_remote(&rec).unwrap(), RemoteOutcome::AlreadyKnown);

        // cursor only moves forward; the older record (a different guest) is
        // still applied last-write, but cursor 7 is not regressed by seq 5.
        // (The engine only feeds ascending counters; a stale seq arriving
        // mid-pull is an echo, hence the forward-only cursor.)
        let older_guest = Uuid::now_v7();
        let older = server_change(
            5,
            EntityType::Guest,
            older_guest,
            serde_json::json!({ "full_name": "Older Neighbor" }),
        );
        assert_eq!(s.apply_remote(&older).unwrap(), RemoteOutcome::Applied);
        assert_eq!(s.pull_cursor().unwrap(), 7);
        let name: String = s
            .conn
            .query_row(
                "SELECT full_name FROM guests WHERE id = ?1",
                params![guest_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Pulled Guest");
        let older_name: String = s
            .conn
            .query_row(
                "SELECT full_name FROM guests WHERE id = ?1",
                params![older_guest.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(older_name, "Older Neighbor");

        // a reservation parked as conflict is counted for the status UI
        let res_rec = server_change(
            8,
            EntityType::Reservation,
            Uuid::now_v7(),
            serde_json::json!({
                "room_id": Uuid::now_v7().to_string(),
                "checkin_date": "2026-11-20",
                "checkout_date": "2026-11-22",
                "status": "confirmed",
                "conflict": true,
            }),
        );
        assert_eq!(s.apply_remote(&res_rec).unwrap(), RemoteOutcome::Applied);
        assert_eq!(s.pull_cursor().unwrap(), 8);
        assert_eq!(s.conflict_count().unwrap(), 1);
    }

    #[test]
    fn offline_cache_round_trip() {
        let mut s = LocalStore::with_in_memory().unwrap();
        let cache = OfflineLoginCache::create(
            Uuid::now_v7(),
            HOTEL,
            "hkdemo".into(),
            hms_core::roles::Role::Housekeeping,
            "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNo".into(),
            Utc::now(),
            DEFAULT_OFFLINE_LOGIN_DAYS,
        );
        s.save_offline_cache(&cache).unwrap();
        let loaded = s.load_offline_cache().unwrap().expect("cache present");
        assert_eq!(loaded.user_id, cache.user_id);
        assert_eq!(loaded.hotel_id, HOTEL);
        assert_eq!(loaded.username, "hkdemo");
        assert_eq!(loaded.role, hms_core::roles::Role::Housekeeping);
        assert_eq!(loaded.password_hash, cache.password_hash);
        assert!(loaded.is_valid(Utc::now()));
        assert!(!loaded.is_valid(Utc::now() + chrono::Duration::days(DEFAULT_OFFLINE_LOGIN_DAYS + 1)));

        s.drop_offline_cache().unwrap();
        assert!(s.load_offline_cache().unwrap().is_none());
    }

    #[test]
    fn token_round_trip_and_hotel_default() {
        let mut s = LocalStore::with_in_memory().unwrap();
        let token = "b2987dd930734f44a35f00dfa8a9e0a2";
        assert_eq!(s.load_token().unwrap(), None);
        s.save_token(token).unwrap();
        assert_eq!(s.load_token().unwrap().as_deref(), Some(token));
        assert_eq!(s.hotel_id().unwrap(), 0, "no login recorded yet");
        s.set_hotel_id(HOTEL).unwrap();
        assert_eq!(s.hotel_id().unwrap(), HOTEL);
        s.set_hotel_id(1).unwrap();
        assert_eq!(s.hotel_id().unwrap(), 1, "kv overwrite wins");
    }

    #[tokio::test]
    async fn engine_end_to_end_over_local_store() {
        let guest_id = Uuid::now_v7();
        let pull_page = vec![server_change(
            3,
            EntityType::Guest,
            guest_id,
            serde_json::json!({ "full_name": "Pulled Guest" }),
        )];

        let path = std::env::temp_dir()
            .join(format!("hms_desktop_test_{}", Uuid::new_v4()))
            .with_extension("sqlite");

        {
            let mut store = LocalStore::open(&path).expect("open file store");
            store.set_hotel_id(HOTEL).unwrap();
            let op = guest_op("Engine Guest");
            store.enqueue(op.clone(), 0).unwrap();

            let transport = ScriptedTransport::new(vec![pull_page]);
            let mut engine = SyncEngine::new(HOTEL, store, transport);
            let out = engine.sync_once().await.expect("sync cycle");
            assert_eq!(out.pushed_ok, 1, "one outbox op acknowledged");
            assert_eq!(out.pulled_applied, 1, "one server change applied");
            assert_eq!(out.pending, 0);
            assert!(out.last_synced_at_ms.is_some());

            let snap = engine.status_snapshot().unwrap();
            assert_eq!(snap.pull_cursor, 3);
            assert_eq!(snap.pending, 0);
            assert_eq!(snap.conflicts, 0);
            assert_eq!(snap.failures, 0);
            // engine (and the SQLite handle it owns) dropped at end of block
        }

        // reopen the same file: the applied change is durable
        let mut store2 = LocalStore::open(&path).expect("reopen store");
        let name: String = store2
            .conn
            .query_row(
                "SELECT full_name FROM guests WHERE id = ?1",
                params![guest_id.to_string()],
                |r| r.get(0),
            )
            .expect("pulled row present");
        assert_eq!(name, "Pulled Guest");
        assert_eq!(store2.pull_cursor().unwrap(), 3);
        assert!(
            store2.last_synced_at_ms().unwrap().is_some(),
            "last synced timestamp persisted"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
    }
}
