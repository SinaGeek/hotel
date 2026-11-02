//! Local-first writes (todo A1): every desktop write (reservation
//! create/edit/check-in/out/cancel, payment, folio extra, guest create/edit,
//! cleanliness) is staged in **one SQLite transaction** that (a) mirrors the
//! optimistic row into the local database and (b) enqueues the change
//! operation into the outbox for the sync engine.
//!
//! When the app is online the caller immediately drives one sync cycle after
//! staging: the engine pushes the queued op (idempotent) and then pulls the
//! server's canonical rows, which overwrite the optimistic values in the
//! mirror (`apply_remote`). The observable behaviour matches the TODO —
//! online the server's answer lands in SQLite, offline (or on any network
//! failure) the write stays queued and nothing is lost. The server stays the
//! authority: conflict resolution happens server-side by arrival order
//! (README §11.4), never client-side.

use crate::local_store::{entity_name, operation_name, LocalStore};
use chrono::Utc;
use hms_core::sync::{ChangeOperation, EntityType, OperationType};
use rusqlite::params;
use uuid::Uuid;
/// What a `local_write` invocation returns to the webview (a4 note: the
/// optimistic mirror may later resolve to `conflict` server-side; the UI is
/// warned, never blocked — README §2.3).
#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum WriteOutcome {
    /// The staged op was pushed and the server's authoritative state has been
    /// pulled into the mirror.
    Confirmed {
        operation_id: String,
        entity_id: String,
        /// A4: local availability warning (offline "looks taken" estimate)
        #[serde(skip_serializing_if = "Option::is_none")]
        warning: Option<String>,
    },
    /// The write lives in SQLite + outbox and will reach the server with the
    /// next successful sync cycle (never lost, never recreated client-side).
    Queued {
        operation_id: String,
        entity_id: String,
        /// A4: local availability warning (offline "looks taken" estimate)
        #[serde(skip_serializing_if = "Option::is_none")]
        warning: Option<String>,
        reason: String,
    },
}

fn err<T>(msg: impl Into<String>) -> Result<T, String> {
    Err(msg.into())
}

/// Map the webview's string names onto the sync vocabulary.
pub fn entity_of(name: &str) -> Result<EntityType, String> {
    Ok(match name {
        "reservation" => EntityType::Reservation,
        "guest" => EntityType::Guest,
        "payment" => EntityType::Payment,
        "folio" => EntityType::FolioTransaction,
        "cleanliness" => EntityType::RoomStatus,
        "room" => EntityType::Room,
        "room_type" => EntityType::RoomType,
        "price_rule" => EntityType::PriceRule,
        "channel" => EntityType::Channel,
        other => return err(format!("unsupported entity '{other}'")),
    })
}

pub fn operation_of(name: &str) -> Result<OperationType, String> {
    Ok(match name {
        "insert" | "create" => OperationType::Insert,
        "update" | "edit" => OperationType::Update,
        "delete" | "cancel" => OperationType::Delete,
        other => return err(format!("unsupported operation '{other}'")),
    })
}

fn p_str<'a>(p: &'a serde_json::Value, key: &str) -> Result<&'a str, String> {
    p.get(key)
        .and_then(|v| v.as_str())
        .ok_or_else(|| format!("payload field '{key}' must be a string"))
}

fn p_uuid(p: &serde_json::Value, key: &str) -> Result<Uuid, String> {
    p_uuid_opt(p, key)?.ok_or_else(|| format!("payload field '{key}' must be a UUID"))
}

fn p_uuid_opt(p: &serde_json::Value, key: &str) -> Result<Option<Uuid>, String> {
    match p.get(key).and_then(|v| v.as_str()) {
        Some(s) if s.is_empty() || s == "null" => Ok(None),
        Some(s) => s
            .parse::<Uuid>()
            .map(Some)
            .map_err(|_| format!("payload field '{key}' is not a UUID")),
        None => Ok(None),
    }
}

fn p_date(p: &serde_json::Value, key: &str) -> Result<chrono::NaiveDate, String> {
    let s = p_str(p, key)?;
    chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .map_err(|_| format!("payload field '{key}' is not YYYY-MM-DD"))
}

fn p_i64(p: &serde_json::Value, key: &str, default: i64) -> Result<i64, String> {
    match p.get(key) {
        None => Ok(default),
        Some(v) => v
            .as_i64()
            .ok_or_else(|| format!("payload field '{key}' must be an integer")),
    }
}

/// Server-applier-compatible validation + defaults. Payment and folio edits
/// are rejected: both are append-only server-side (README §7).
fn normalize(
    entity: EntityType,
    operation: OperationType,
    mut payload: serde_json::Value,
) -> Result<serde_json::Value, String> {
    use OperationType as OT;
    match entity {
        EntityType::Reservation => match operation {
            OT::Insert => {
                if p_date(&payload, "checkout_date")? <= p_date(&payload, "checkin_date")? {
                    return err("check-out must be after check-in");
                }
                p_uuid(&payload, "room_id")?;
                p_uuid(&payload, "channel_id")?;
                p_uuid_opt(&payload, "guest_id")?;
                let adults = p_i64(&payload, "adults", 1)?;
                if adults < 1 {
                    return err("adults must be at least 1");
                }
                if adults > 32 || p_i64(&payload, "children", 0)? < 0 {
                    return err("invalid occupancy");
                }
                if !payload.get("status").and_then(|s| s.as_str()).is_some() {
                    payload["status"] = serde_json::json!("confirmed");
                } else {
                    return err("status is decided by the server; omit it on create");
                }
            }
            OT::Update => {
                if let Some(s) = payload
                    .get("status")
                    .and_then(|v| v.as_str())
                    .map(str::to_owned)
                {
                    if !["confirmed", "checked_in", "checked_out", "cancelled"]
                        .contains(&s.as_str())
                    {
                        return err(format!("invalid status '{s}'"));
                    }
                }
                let i = payload.get("checkin_date").is_some();
                let o = payload.get("checkout_date").is_some();
                if i || o {
                    let ci = match payload.get("checkin_date").and_then(|v| v.as_str()) {
                        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                            .map_err(|_| "checkin_date is not YYYY-MM-DD".to_owned())?,
                        None => return err("edit with dates needs checkin_date"),
                    };
                    let co = match payload.get("checkout_date").and_then(|v| v.as_str()) {
                        Some(s) => chrono::NaiveDate::parse_from_str(s, "%Y-%m-%d")
                            .map_err(|_| "checkout_date is not YYYY-MM-DD".to_owned())?,
                        None => return err("edit with dates needs checkout_date"),
                    };
                    if co <= ci {
                        return err("check-out must be after check-in");
                    }
                }
                if let Some(r) = p_uuid_opt(&payload, "room_id")? {
                    let _ = r;
                }
            }
            OT::Delete => {}
        },
        EntityType::Guest => match operation {
            OT::Insert | OT::Update => {
                let full_name = p_str(&payload, "full_name")?.trim();
                if full_name.is_empty() {
                    return err("guest full_name is required");
                }
                payload["full_name"] = serde_json::json!(full_name);
                if let Some(bd) = payload.get("birth_date").and_then(|v| v.as_str()) {
                    if !bd.is_empty() {
                        chrono::NaiveDate::parse_from_str(bd, "%Y-%m-%d")
                            .map_err(|_| "birth_date is not YYYY-MM-DD".to_owned())?;
                    } else {
                        payload["birth_date"] = serde_json::Value::Null;
                    }
                }
                if !payload.get("is_primary").is_some() {
                    payload["is_primary"] = serde_json::json!(true);
                }
            }
            OT::Delete => {}
        },
        EntityType::Payment => match operation {
            OT::Insert => {
                p_uuid(&payload, "reservation_id")?;
                let amount = p_i64(&payload, "amount_cents", 0)?;
                if amount == 0 {
                    return err("amount_cents must not be zero");
                }
                payload["is_reversal"] = serde_json::json!(amount < 0);
                if p_str(&payload, "currency")?.len() != 3 {
                    return err("currency must be a 3-letter code");
                }
            }
            _ => return err("payments are append-only"),
        },
        EntityType::FolioTransaction => match operation {
            OT::Insert => {
                p_uuid(&payload, "reservation_id")?;
                let qty = p_i64(&payload, "quantity", 1)?;
                if !(1..=1000).contains(&qty) {
                    return err("invalid quantity");
                }
                let unit = p_i64(&payload, "unit_price_cents", 0)?;
                if unit < 0 {
                    return err("unit_price_cents must not be negative");
                }
                if p_str(&payload, "currency")?.len() != 3 {
                    return err("currency must be a 3-letter code");
                }
                if payload.get("category").is_none() {
                    payload["category"] = serde_json::json!("other");
                }
                payload["total_cents"] = serde_json::json!(unit * qty);
            }
            _ => return err("folio transactions are append-only"),
        },
        EntityType::RoomStatus => {
            let c = p_str(&payload, "cleanliness")?;
            if !["clean", "cleaning", "dirty", "maintenance"].contains(&c) {
                return err(format!("invalid cleanliness '{c}'"));
            }
        }
        EntityType::Room => {
            p_uuid(&payload, "room_type_id")?;
            if p_str(&payload, "room_number")?.is_empty() {
                return err("room_number is required");
            }
        }
        EntityType::RoomType => {
            if p_str(&payload, "name")?.is_empty() {
                return err("room type name is required");
            }
            let cap = p_i64(&payload, "capacity", 2)?;
            if !(1..=64).contains(&cap) || p_i64(&payload, "base_price_cents", 0)? < 0 {
                return err("invalid room type");
            }
        }
        EntityType::PriceRule => {
            p_uuid(&payload, "room_type_id")?;
            if p_date(&payload, "date_to")? <= p_date(&payload, "date_from")? {
                return err("price rule end must be after start");
            }
            if p_i64(&payload, "price_cents", -1)? < 0 {
                return err("price_cents must not be negative");
            }
        }
        EntityType::Channel => {
            if p_str(&payload, "kind")?.is_empty() {
                return err("channel kind is required");
            }
        }
        EntityType::User => unreachable!("user writes use the online admin path"),
    }
    Ok(payload)
}

/// A4 (README §2.3): offline availability check — WARN, never block.
/// Overlapping stays on the same room in the local mirror count as "looks
/// taken locally"; the booking is always allowed (overbooking is accepted;
/// the server's EXCLUDE resolves it to status `conflict`).
fn local_overlap_warning(
    store: &mut LocalStore,
    room_id: Uuid,
    checkin: &str,
    checkout: &str,
    exclude: Option<Uuid>,
) -> Option<String> {
    let conn = store.conn_for_local_writes();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM reservations
              WHERE room_id = ?1 AND id <> ?2 AND deleted_at IS NULL
                AND status NOT IN ('cancelled','conflict')
                AND checkin_date < ?3 AND checkout_date > ?4",
            params![
                room_id.to_string(),
                exclude.map(|e| e.to_string()).unwrap_or_default(),
                checkout,
                checkin,
            ],
            |r| r.get(0),
        )
        .unwrap_or(0);
    (n > 0).then(|| {
        format!(
            "room {room_id} looks taken locally for {checkin}..{checkout} (offline estimate; \
             the booking was queued and the server will decide)"
        )
    })
}

/// Validate and stage one local write: the optimistic mirror row and the
/// outbox entry are committed in the SAME SQLite transaction (todo A1). The
/// webview may pass `null` `entity_id`; new entities get a client-generated
/// UUIDv7 which the server adopts verbatim on push. Returns the staged op and
/// an A4 availability warning for reservation writes (None = no local clash).
pub fn stage_local_write(
    store: &mut LocalStore,
    hotel_id: i64,
    entity: EntityType,
    operation: OperationType,
    entity_id: Option<Uuid>,
    payload: serde_json::Value,
) -> Result<(ChangeOperation, Option<String>), String> {
    if hotel_id == 0 {
        return err("no hotel logged in on this device");
    }
    let payload = normalize(entity, operation, payload)?;
    let entity_id = match (entity_id, operation) {
        (Some(id), _) => id,
        (None, OperationType::Insert) => Uuid::now_v7(),
        (None, _) => return err("entity_id is required for edits"),
    };
    // A4: pure warning, computed before staging so even a full-ranged edit
    // can warn; reads on a fresh mirror are cheap
    let warning = if entity == EntityType::Reservation {
        match operation {
            OperationType::Insert | OperationType::Update => {
                let room_id = match payload.get("room_id").and_then(|v| v.as_str()) {
                    Some(s) => match s.parse::<Uuid>() {
                        Ok(u) => Some(u),
                        Err(_) => return err("payload field 'room_id' must be a UUID"),
                    },
                    // edit keeping the current room: no local availability
                    // warning (the mirror row's room is unchanged)
                    None => None,
                };
                match room_id {
                    Some(u) => {
                        let checkin = payload["checkin_date"].as_str().unwrap_or("").to_owned();
                        let checkout = payload["checkout_date"].as_str().unwrap_or("").to_owned();
                        if checkin.is_empty() || checkout.is_empty() {
                            return err("checkin_date/checkout_date are required");
                        }
                        local_overlap_warning(store, u, &checkin, &checkout, Some(entity_id))
                    }
                    None => None,
                }
            }
            OperationType::Delete => None,
        }
    } else {
        None
    };
    let op = ChangeOperation {
        operation_id: Uuid::now_v7(),
        hotel_id,
        entity_type: entity,
        entity_id,
        operation_type: operation,
        payload,
        client_timestamp: Utc::now(),
    };
    stage(store, &op)?;
    Ok((op, warning))
}

/// Commit mirror + outbox atomically.
fn stage(store: &mut LocalStore, op: &ChangeOperation) -> Result<(), String> {
    let conn = store.conn_for_local_writes();
    let tx = conn.unchecked_transaction().map_err(|e| e.to_string())?;
    mirror_write(&tx, op)?;
    outbox_insert(&tx, op)?;
    tx.commit().map_err(|e| format!("commit local write: {e}"))
}

/// True while the op is still waiting in the outbox (used by the Tauri
/// command to tell `confirmed` from `queued` after a sync cycle).
pub fn op_is_still_pending(store: &mut LocalStore, operation_id: Uuid) -> Result<bool, String> {
    let conn = store.conn_for_local_writes();
    let n: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM outbox WHERE operation_id = ?1",
            params![operation_id.to_string()],
            |r| r.get(0),
        )
        .map_err(|e| e.to_string())?;
    Ok(n > 0)
}

fn outbox_insert(
    tx: &rusqlite::Transaction<'_>,
    op: &ChangeOperation,
) -> Result<(), String> {
    tx.execute(
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
            serde_json::to_string(&op.payload).map_err(|e| e.to_string())?,
            op.client_timestamp.to_rfc3339(),
            Utc::now().timestamp_millis(),
        ],
    )
    .map_err(|e| format!("outbox: {e}"))?;
    Ok(())
}

/// Optimistic mirror write with the SAME column semantics as the server's
/// sync applier and `apply_remote`, so a later pull overwrites these rows
/// (todo A2's "server decides"). Nothing here estimates prices: totals stay
/// whatever the payload carries (0 for unspriced creates) until the server's
/// canonical values land; per-night rates are task A3.
fn mirror_write(tx: &rusqlite::Transaction<'_>, op: &ChangeOperation) -> Result<(), String> {
    let eid = op.entity_id.to_string();
    let now = Utc::now().timestamp_millis();
    let p = &op.payload;
    match (op.entity_type, op.operation_type) {
        (EntityType::Reservation, OperationType::Insert | OperationType::Update) => {
            tx.execute(
                "INSERT INTO reservations
                   (id, room_id, channel_id, guest_id, checkin_date, checkout_date,
                    status, total_price_cents, adults, children)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10)
                 ON CONFLICT (id) DO UPDATE SET
                   room_id = excluded.room_id, channel_id = excluded.channel_id,
                   guest_id = excluded.guest_id, checkin_date = excluded.checkin_date,
                   checkout_date = excluded.checkout_date, status = excluded.status,
                   total_price_cents = excluded.total_price_cents,
                   adults = excluded.adults, children = excluded.children,
                   deleted_at = NULL",
                params![
                    eid,
                    p["room_id"].as_str().unwrap_or(""),
                    p["channel_id"].as_str().unwrap_or(""),
                    p["guest_id"].as_str(),
                    p["checkin_date"].as_str().unwrap_or(""),
                    p["checkout_date"].as_str().unwrap_or(""),
                    p["status"].as_str().unwrap_or("confirmed"),
                    p["total_price_cents"].as_i64().unwrap_or(0),
                    p["adults"].as_i64().unwrap_or(1).max(0),
                    p["children"].as_i64().unwrap_or(0).max(0),
                ],
            )
            .map_err(|e| format!("mirror reservation: {e}"))?;
        }
        (EntityType::Reservation, OperationType::Delete) => {
            tx.execute(
                "UPDATE reservations SET deleted_at = ?2 WHERE id = ?1",
                params![eid, now],
            )
            .map_err(|e| format!("mirror reservation tombstone: {e}"))?;
        }
        (EntityType::Guest, OperationType::Insert | OperationType::Update) => {
            tx.execute(
                "INSERT INTO guests (id, full_name, nationality, license_plate, birth_date)
                 VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT (id) DO UPDATE SET
                   full_name = excluded.full_name, nationality = excluded.nationality,
                   license_plate = excluded.license_plate, birth_date = excluded.birth_date,
                   deleted_at = NULL",
                params![
                    eid,
                    p["full_name"].as_str().unwrap_or(""),
                    p["nationality"].as_str(),
                    p["license_plate"].as_str(),
                    p["birth_date"].as_str(),
                ],
            )
            .map_err(|e| format!("mirror guest: {e}"))?;
        }
        (EntityType::Guest, OperationType::Delete) => {
            tx.execute(
                "UPDATE guests SET deleted_at = ?2 WHERE id = ?1",
                params![eid, now],
            )
            .map_err(|e| format!("mirror guest tombstone: {e}"))?;
        }
        (EntityType::Payment, OperationType::Insert) => {
            tx.execute(
                "INSERT OR IGNORE INTO payments (id, reservation_id, amount_cents, currency,
                       payment_type, reference, is_reversal, created_at)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    eid,
                    p["reservation_id"].as_str().unwrap_or(""),
                    p["amount_cents"].as_i64().unwrap_or(0),
                    p["currency"].as_str().unwrap_or("TRY"),
                    p["payment_type"].as_str().unwrap_or("cash"),
                    p["reference"].as_str(),
                    (p["amount_cents"].as_i64().unwrap_or(0) < 0) as i64,
                    now,
                ],
            )
            .map_err(|e| format!("mirror payment: {e}"))?;
        }
        (EntityType::Payment, _) | (EntityType::FolioTransaction, OperationType::Update) => {
            // append-only; normalize() rejects these before staging
        }
        (EntityType::FolioTransaction, OperationType::Delete) => {
            // append-only
        }
        (EntityType::FolioTransaction, OperationType::Insert) => {
            tx.execute(
                "INSERT OR IGNORE INTO folio_transactions (id, reservation_id, category,
                       description, unit_price_cents, quantity, total_cents, currency)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                params![
                    eid,
                    p["reservation_id"].as_str().unwrap_or(""),
                    p["category"].as_str().unwrap_or("other"),
                    p["description"].as_str().unwrap_or(""),
                    p["unit_price_cents"].as_i64().unwrap_or(0),
                    p["quantity"].as_i64().unwrap_or(1),
                    p["total_cents"].as_i64().unwrap_or(0),
                    p["currency"].as_str().unwrap_or("TRY"),
                ],
            )
            .map_err(|e| format!("mirror folio: {e}"))?;
        }
        (EntityType::RoomStatus, _) => {
            tx.execute(
                "UPDATE rooms SET cleanliness = ?2 WHERE id = ?1",
                params![eid, p["cleanliness"].as_str().unwrap_or("clean")],
            )
            .map_err(|e| format!("mirror cleanliness: {e}"))?;
        }
        (EntityType::Room, OperationType::Insert | OperationType::Update) => {
            tx.execute(
                "INSERT INTO rooms (id, room_type_id, room_number, floor, cleanliness)
                 VALUES (?1,?2,?3,?4,?5)
                 ON CONFLICT (id) DO UPDATE SET room_type_id = excluded.room_type_id,
                   room_number = excluded.room_number, floor = excluded.floor,
                   cleanliness = excluded.cleanliness, deleted_at = NULL",
                params![
                    eid,
                    p["room_type_id"].as_str().unwrap_or(""),
                    p["room_number"].as_str().unwrap_or(""),
                    p["floor"].as_i64().unwrap_or(0),
                    p["cleanliness"].as_str().unwrap_or("clean"),
                ],
            )
            .map_err(|e| format!("mirror room: {e}"))?;
        }
        (EntityType::Room, OperationType::Delete) => {
            tx.execute(
                "UPDATE rooms SET deleted_at = ?2 WHERE id = ?1",
                params![eid, now],
            )
            .map_err(|e| format!("mirror room tombstone: {e}"))?;
        }
        (EntityType::RoomType, OperationType::Insert | OperationType::Update) => {
            tx.execute(
                "INSERT INTO room_types (id, name, capacity, base_price_cents)
                 VALUES (?1,?2,?3,?4)
                 ON CONFLICT (id) DO UPDATE SET name = excluded.name,
                   capacity = excluded.capacity, base_price_cents = excluded.base_price_cents",
                params![
                    eid,
                    p["name"].as_str().unwrap_or(""),
                    p["capacity"].as_i64().unwrap_or(2),
                    p["base_price_cents"].as_i64().unwrap_or(0),
                ],
            )
            .map_err(|e| format!("mirror room type: {e}"))?;
        }
        (EntityType::RoomType, OperationType::Delete) => {
            tx.execute(
                "UPDATE room_types SET deleted_at = ?2 WHERE id = ?1",
                params![eid, now],
            )
            .map_err(|e| format!("mirror room type tombstone: {e}"))?;
        }
        (EntityType::PriceRule, OperationType::Insert | OperationType::Update) => {
            tx.execute(
                "INSERT INTO price_rules (id, room_type_id, date_from, date_to, price_cents, currency)
                 VALUES (?1,?2,?3,?4,?5,?6)
                 ON CONFLICT (id) DO UPDATE SET room_type_id = excluded.room_type_id,
                   date_from = excluded.date_from, date_to = excluded.date_to,
                   price_cents = excluded.price_cents, currency = excluded.currency",
                params![
                    eid,
                    p["room_type_id"].as_str().unwrap_or(""),
                    p["date_from"].as_str().unwrap_or(""),
                    p["date_to"].as_str().unwrap_or(""),
                    p["price_cents"].as_i64().unwrap_or(0),
                    p["currency"].as_str().unwrap_or("TRY"),
                ],
            )
            .map_err(|e| format!("mirror price rule: {e}"))?;
        }
        (EntityType::PriceRule, OperationType::Delete) => {
            tx.execute(
                "DELETE FROM price_rules WHERE id = ?1",
                params![eid],
            )
            .map_err(|e| format!("mirror price rule delete: {e}"))?;
        }
        (EntityType::Channel, OperationType::Insert | OperationType::Update) => {
            tx.execute(
                "INSERT INTO channels (id, kind, name, is_active) VALUES (?1,?2,?3,?4)
                 ON CONFLICT (id) DO UPDATE SET kind = excluded.kind,
                   name = excluded.name, is_active = excluded.is_active",
                params![
                    eid,
                    p["kind"].as_str().unwrap_or("other"),
                    p["name"].as_str().unwrap_or(""),
                    (p["is_active"].as_bool().unwrap_or(true)) as i64,
                ],
            )
            .map_err(|e| format!("mirror channel: {e}"))?;
        }
        (EntityType::Channel, OperationType::Delete) => {
            tx.execute(
                "UPDATE channels SET is_active = 0 WHERE id = ?1",
                params![eid],
            )
            .map_err(|e| format!("mirror channel disable: {e}"))?;
        }
        (EntityType::User, _) => unreachable!("user writes use the online admin path"),
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_store::LocalStore;
    use hms_core::sync::ServerChangeRecord;
    use hms_sync::store::{RemoteOutcome, SyncStore};

    fn store() -> LocalStore {
        LocalStore::with_in_memory().expect("store")
    }

    fn res_payload(room: &str, ch: &str) -> serde_json::Value {
        serde_json::json!({
            "room_id": room, "channel_id": ch, "guest_id": null,
            "checkin_date": "2026-03-01", "checkout_date": "2026-03-03",
            "adults": 2, "children": 0, "currency": "TRY"
        })
    }

    fn server_record(
        seq: i64,
        entity: EntityType,
        eid: Uuid,
        o: OperationType,
        payload: serde_json::Value,
    ) -> ServerChangeRecord {
        ServerChangeRecord {
            change_counter: seq,
            hotel_id: 4442,
            operation_id: Uuid::now_v7(),
            entity_type: entity,
            entity_id: eid,
            operation_type: o,
            payload,
            server_timestamp: Utc::now(),
        }
    }

    #[test]
    fn a1_stage_commits_mirror_and_outbox_atomically() {
        let mut s = store();
        let room = Uuid::now_v7();
        let ch = Uuid::now_v7();
        let (op, _a4) = stage_local_write(
            &mut s,
            4442,
            EntityType::Reservation,
            OperationType::Insert,
            None,
            res_payload(&room.to_string(), &ch.to_string()),
        )
        .expect("staged");

        // outbox holds exactly one pending op with the same id
        assert!(op_is_still_pending(&mut s, op.operation_id).unwrap());
        assert_eq!(s.pending_count().unwrap(), 1);

        // mirror row exists with the optimistic payload values
        let total: i64 = s
            .conn_for_local_writes()
            .query_row(
                "SELECT total_price_cents FROM reservations WHERE id = ?1",
                params![op.entity_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(total, 0, "unspriced optimistic create carries 0 until server pull");
        assert_eq!(s.list_reservation_ids().unwrap().len(), 1);
    }

    #[test]
    fn a2_server_pull_overwrites_local_values() {
        let mut s = store();
        let room = Uuid::now_v7();
        let ch = Uuid::now_v7();
        let (op, _a4) = stage_local_write(
            &mut s,
            4442,
            EntityType::Reservation,
            OperationType::Insert,
            None,
            res_payload(&room.to_string(), &ch.to_string()),
        )
        .expect("staged");

        // server answer: same reservation, but the server priced it and
        // flipped the status to conflict (double-booked)
        let mut payload = res_payload(&room.to_string(), &ch.to_string());
        payload["total_price_cents"] = serde_json::json!(12_345);
        payload["status"] = serde_json::json!("conflict");
        let outcome = s
            .apply_remote(&server_record(
                7,
                EntityType::Reservation,
                op.entity_id,
                OperationType::Insert,
                payload,
            ))
            .expect("applied");
        assert!(matches!(outcome, RemoteOutcome::Applied));

        let (total, status): (i64, String) = s
            .conn_for_local_writes()
            .query_row(
                "SELECT total_price_cents, status FROM reservations WHERE id = ?1",
                params![op.entity_id.to_string()],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .unwrap();
        assert_eq!(total, 12_345, "server price wins");
        assert_eq!(status, "conflict", "server status wins");
    }

    #[test]
    fn validation_matches_the_server_applier() {
        let mut s = store();
        let bad = res_payload("not-a-uuid", &Uuid::now_v7().to_string());
        assert!(stage_local_write(
            &mut s, 4442, EntityType::Reservation, OperationType::Insert, None, bad
        )
        .is_err());
        assert!(stage_local_write(
            &mut s, 4442, EntityType::Reservation, OperationType::Insert, None,
            res_payload(&Uuid::now_v7().to_string(), &Uuid::now_v7().to_string())
                .tap_mut(|p| p["checkout_date"] = serde_json::json!("2026-03-01")),
        )
        .is_err(), "checkout must be after checkin");
        assert!(stage_local_write(
            &mut s, 4442, EntityType::Payment, OperationType::Update, Some(Uuid::now_v7()),
            serde_json::json!({"amount_cents": 100})
        )
        .is_err(), "payments are append-only");
        // offline guest edit with empty name rejected, nothing staged
        assert!(stage_local_write(
            &mut s, 4442, EntityType::Guest, OperationType::Insert, None,
            serde_json::json!({"full_name": "  "})
        )
        .is_err());
        assert_eq!(s.pending_count().unwrap(), 0);
    }

    #[test]
    fn a4_offline_overlap_warns_but_does_not_block() {
        let mut s = store();
        let room = Uuid::now_v7();
        let ch = Uuid::now_v7();
        let (first, warning) = stage_local_write(
            &mut s,
            4442,
            EntityType::Reservation,
            OperationType::Insert,
            None,
            res_payload(&room.to_string(), &ch.to_string()),
        )
        .expect("first staging keeps mirror room_id");
        assert!(
            warning.is_none(),
            "first booking on an empty mirror must not warn"
        );

        // a second overlapping booking on the same room: queued, with warning
        let (second, warning) = stage_local_write(
            &mut s,
            4442,
            EntityType::Reservation,
            OperationType::Insert,
            None,
            res_payload(&room.to_string(), &ch.to_string()),
        )
        .expect("overbooking IS allowed offline (README §2.3)");
        assert!(warning.is_some(), "overlap must warn");
        assert!(op_is_still_pending(&mut s, second.operation_id).unwrap());

        // a NON-overlapping booking (different dates, same room): no warning
        let mut p = res_payload(&room.to_string(), &ch.to_string());
        p["checkin_date"] = serde_json::json!("2026-03-10");
        p["checkout_date"] = serde_json::json!("2026-03-12");
        let (_, warning) = stage_local_write(
            &mut s,
            4442,
            EntityType::Reservation,
            OperationType::Insert,
            None,
            p,
        )
        .expect("non overlapping");
        assert!(warning.is_none());

        // A4 detail: the optimistic rows must keep room_id to compute the
        // warning, mirror now holds THREE reservations and the server decides
        assert_eq!(s.list_reservation_ids().unwrap().len(), 3);
        let room_check: i64 = s
            .conn_for_local_writes()
            .query_row(
                "SELECT COUNT(*) FROM reservations WHERE room_id = ?1",
                params![room.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(room_check, 3, "first reservation must not lose its room_id");
        let _ = first;
    }

    #[test]
    fn unknown_hotel_is_rejected_and_cleanliness_routes_to_rooms() {
        let mut s = store();
        assert!(stage_local_write(
            &mut s, 0, EntityType::Reservation, OperationType::Insert, None,
            res_payload(&Uuid::now_v7().to_string(), &Uuid::now_v7().to_string())
        )
        .is_err());

        let room = Uuid::now_v7();
        stage_local_write(
            &mut s,
            4442,
            EntityType::Room,
            OperationType::Insert,
            Some(room),
            serde_json::json!({"room_type_id": Uuid::now_v7().to_string(),
                               "room_number": "101", "floor": 1}),
        )
        .expect("room staged");
        stage_local_write(
            &mut s,
            4442,
            EntityType::RoomStatus,
            OperationType::Update,
            Some(room),
            serde_json::json!({"cleanliness": "dirty"}),
        )
        .expect("cleanliness staged");
        let c: String = s
            .conn_for_local_writes()
            .query_row(
                "SELECT cleanliness FROM rooms WHERE id = ?1",
                params![room.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(c, "dirty");
        assert_eq!(s.pending_count().unwrap(), 2);
    }

    // strum-free tap helper
    trait TapMut {
        fn tap_mut(self, f: impl FnOnce(&mut Self)) -> Self;
    }
    impl<T> TapMut for T {
        fn tap_mut(mut self, f: impl FnOnce(&mut Self)) -> Self {
            f(&mut self);
            self
        }
    }
}
