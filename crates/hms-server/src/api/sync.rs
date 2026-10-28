//! Sync (README §11): server side of the offline protocol.
//!
//! `POST /sync/push` — applies a batch of offline operations idempotently
//! (deduplicated by operation id in `sync_ops`). Every applied op re-runs the
//! same-path logic a normal API call would (payments stay append-only,
//! cleanliness is last-write-wins, reservations hit the no-double-booking
//! `EXCLUDE` constraint so a later offline booking lands as status `conflict`
//! instead of being dropped), then echoes the change into `change_log` with the
//! CLIENT operation id so other clients (and the same client's retry) see it
//! exactly once. `GET /sync/pull?since=<seq>` streams that log back.
//!
//! Conflict rules per README §11.5:
//!   payments / folio: append-only (conflicts impossible),
//!   cleanliness: last-write-wins,
//!   rooms / room types / price rules / channels: server wins on UPDATE (the
//! client op is rejected with a reason, keeping server state authoritative for
//! config); inserts still apply (offline creation has no server copy to defer
//! to),
//!   guests: client data — last write wins (kept simple for reception),
//!   reservations: the DB constraint resolves overlap -> `conflict`.
//!
//! `user` ops are rejected outright: user provisioning is an admin/server-only
//! flow even offline (README §8; offline caches only verify logins).
//!
//! Clock protection (§11.4): client timestamps more than 10 minutes in the
//! future or a year in the past are rejected; conflicts order by server
//! arrival, never by client clocks.
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use chrono::Utc;
use hms_core::sync::{
    ChangeOperation, EntityType, OperationType as OT, ServerChangeRecord, SyncOpResult,
    SyncOpStatus, SyncPullResponse, SyncPushRequest, SyncPushResponse,
};
use hms_core::ApiResponse;
use serde::Deserialize;
use uuid::Uuid;

use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/push", post(push)).route("/pull", get(pull))
}

// ----------------------------------------------------------------------- pull

#[derive(Deserialize)]
pub struct PullQuery {
    pub since: i64,
    /// page size, default 500, max 2000 (clients loop via has_more)
    pub limit: Option<i64>,
}

/// change_log entity strings -> protocol enum ('hotel' rows are filtered in SQL
/// below: bootstrap data travels in the auth handshake, not the change stream).
fn entity_type_of(s: &str) -> Option<EntityType> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

fn operation_type_of(s: &str) -> Option<OT> {
    serde_json::from_value(serde_json::Value::String(s.to_owned())).ok()
}

async fn pull(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<PullQuery>,
) -> AResult<Json<ApiResponse<SyncPullResponse>>> {
    let limit = q.limit.unwrap_or(500).clamp(1, 2000);

    let latest: i64 = sqlx::query_scalar(
        "SELECT COALESCE(max(change_seq), 0) FROM change_log WHERE hotel_id = $1",
    )
    .bind(auth.hotel_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    let rows: Vec<(i64, Uuid, String, Uuid, String, serde_json::Value, chrono::DateTime<Utc>)> =
        sqlx::query_as(
            r#"SELECT change_seq, operation_id, entity_type, entity_id, operation, payload, server_time
               FROM change_log
               WHERE hotel_id = $1 AND change_seq > $2
                 AND entity_type IN ('room_type','room','room_status','reservation','guest',
                                     'payment','folio_transaction','price_rule','user','channel')
               ORDER BY change_seq
               LIMIT $3"#,
        )
        .bind(auth.hotel_id)
        .bind(q.since)
        .bind(limit)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;

    let records: Vec<ServerChangeRecord> = rows
        .into_iter()
        .filter_map(|(seq, op_id, et, eid, oper, payload, ts)| {
            let entity_type = entity_type_of(&et)?;
            let operation_type = operation_type_of(&oper)?;
            Some(ServerChangeRecord {
                change_counter: seq,
                hotel_id: auth.hotel_id,
                operation_id: op_id,
                entity_type,
                entity_id: eid,
                operation_type,
                payload,
                server_timestamp: ts,
            })
        })
        .collect();
    let last_seq = records.last().map(|r| r.change_counter).unwrap_or(q.since);
    let has_more = latest > last_seq;

    Ok(Json(ApiResponse {
        success: true,
        data: SyncPullResponse { hotel_id: auth.hotel_id, records, latest_counter: latest, has_more },
        error: None,
    }))
}

// ----------------------------------------------------------------------- push

async fn push(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(req): Json<SyncPushRequest>,
) -> AResult<Json<ApiResponse<SyncPushResponse>>> {
    if req.hotel_id != auth.hotel_id {
        return Err(ApiError(StatusCode::FORBIDDEN, "operations belong to another hotel".into()));
    }

    let now = Utc::now();
    let mut results = Vec::with_capacity(req.operations.len());
    for op in &req.operations {
        // clock skew (§11.4): client clocks are never trusted
        if op.client_timestamp > now + chrono::Duration::seconds(600)
            || op.client_timestamp < now - chrono::Duration::days(365)
        {
            results.push(reject(op, "clock skew: client timestamp far from server time"));
            continue;
        }
        results.push(match apply(&state, &auth, op).await {
            Ok(status) => SyncOpResult {
                operation_id: op.operation_id,
                entity_type: op.entity_type,
                entity_id: op.entity_id,
                status,
            },
            Err(reason) => reject(op, &reason),
        });
    }

    let latest: i64 = sqlx::query_scalar(
        "SELECT COALESCE(max(change_seq), 0) FROM change_log WHERE hotel_id = $1",
    )
    .bind(auth.hotel_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: SyncPushResponse { hotel_id: req.hotel_id, results, server_latest_counter: latest },
        error: None,
    }))
}

fn reject(op: &ChangeOperation, reason: &str) -> SyncOpResult {
    SyncOpResult {
        operation_id: op.operation_id,
        entity_type: op.entity_type,
        entity_id: op.entity_id,
        status: SyncOpStatus::Rejected { reason: reason.to_owned() },
    }
}

// ------------------------------------------------------------------ payload

fn p_str<'a>(v: &'a serde_json::Value, k: &str) -> Result<&'a str, String> {
    v.get(k)
        .and_then(|x| x.as_str())
        .ok_or_else(|| format!("payload missing string '{k}'"))
}

fn p_opt_str(v: &serde_json::Value, k: &str) -> Result<Option<String>, String> {
    match v.get(k) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => Ok(Some(s.clone())),
        Some(_) => Err(format!("payload '{k}' must be a string or null")),
    }
}

fn p_uuid(v: &serde_json::Value, k: &str) -> Result<Uuid, String> {
    p_str(v, k)?.parse::<Uuid>().map_err(|_| format!("payload '{k}' is not a uuid"))
}

fn p_opt_uuid(v: &serde_json::Value, k: &str) -> Result<Option<Uuid>, String> {
    match v.get(k) {
        None | Some(serde_json::Value::Null) => Ok(None),
        Some(serde_json::Value::String(s)) => {
            s.parse::<Uuid>().map(Some).map_err(|_| format!("payload '{k}' is not a uuid"))
        }
        Some(_) => Err(format!("payload '{k}' must be a string or null")),
    }
}

fn p_date(v: &serde_json::Value, k: &str) -> Result<chrono::NaiveDate, String> {
    p_str(v, k)?.parse::<chrono::NaiveDate>().map_err(|_| format!("payload '{k}' is not a date"))
}

fn p_opt_date(v: &serde_json::Value, k: &str) -> Result<Option<chrono::NaiveDate>, String> {
    match p_opt_str(v, k)? {
        None => Ok(None),
        Some(s) => s
            .parse::<chrono::NaiveDate>()
            .map(Some)
            .map_err(|_| format!("payload '{k}' is not a date")),
    }
}

// ------------------------------------------------------------------ helpers

const OP_APPLIED: &str = "applied";
const OP_DUP: &str = "duplicate_ignored";
const OP_RESOLVED: &str = "conflict_resolved";

fn op_status_db(s: &SyncOpStatus) -> Option<(&'static str, Option<String>)> {
    match s {
        SyncOpStatus::Applied => Some((OP_APPLIED, None)),
        SyncOpStatus::DuplicateIgnored => Some((OP_DUP, None)),
        SyncOpStatus::ConflictResolved { message } => Some((OP_RESOLVED, Some(message.clone()))),
        SyncOpStatus::Rejected { .. } => None, // determinate failures are NOT memorized (see module comment)
    }
}

/// Replays an already-processed op status from the sync_ops row.
fn replay_status(s: &str) -> SyncOpStatus {
    match s {
        OP_RESOLVED => SyncOpStatus::ConflictResolved { message: "replayed: ".into() },
        OP_DUP => SyncOpStatus::DuplicateIgnored,
        _ => SyncOpStatus::Applied,
    }
}

fn entity_sql_name(e: EntityType) -> &'static str {
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

fn operation_sql_name(o: OT) -> &'static str {
    match o {
        OT::Insert => "insert",
        OT::Update => "update",
        OT::Delete => "delete",
    }
}

fn is_exclusion_violation(e: &sqlx::Error) -> bool {
    e.as_database_error().and_then(|dbe| dbe.code()).map(|c| c == "23P01").unwrap_or(false)
}

fn is_fk_violation(e: &sqlx::Error) -> bool {
    e.as_database_error().and_then(|dbe| dbe.code()).map(|c| c == "23503").unwrap_or(false)
}

/// Per-type permission gate mirroring the REST endpoints (README §6 matrix);
/// the permission to require for (entity, operation) kind.
fn required_perm(e: EntityType, o: OT) -> Result<hms_core::roles::Permission, ()> {
    Ok(match (e, o) {
        (EntityType::User, _) => return Err(()),
        (EntityType::Guest, _) => hms_core::roles::Permission::ReservationCreate,
        (EntityType::Reservation, OT::Insert) => hms_core::roles::Permission::ReservationCreate,
        (EntityType::Reservation, _) => hms_core::roles::Permission::ReservationEdit,
        (EntityType::RoomStatus, _) => hms_core::roles::Permission::HousekeepingUpdateStatus,
        (EntityType::RoomType, _) => hms_core::roles::Permission::RoomTypeManage,
        (EntityType::Room, _) => hms_core::roles::Permission::RoomManage,
        (EntityType::Payment, _) => hms_core::roles::Permission::PaymentRecord,
        (EntityType::FolioTransaction, _) => hms_core::roles::Permission::FolioManage,
        (EntityType::PriceRule, _) => hms_core::roles::Permission::PriceRuleManage,
        (EntityType::Channel, _) => hms_core::roles::Permission::RoomManage,
    })
}

async fn apply(state: &AppState, auth: &AuthUser, op: &ChangeOperation) -> Result<SyncOpStatus, String> {
    let perm = required_perm(op.entity_type, op.operation_type)
        .map_err(|_| "user provisioning requires the online admin path (server wins, README §11)".to_owned())?;
    let ok = crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, perm)
        .await
        .map_err(|e| e.to_string())?;
    if !ok {
        return Err(format!("{} required to sync this entity", perm.as_str()));
    }

    let mut conn = state.db.begin().await.map_err(|e| format!("tx: {e}"))?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await.map_err(|e| e.1)?;

    // -------- idempotency: an op already processed replays its recorded status
    let seen: Option<String> = sqlx::query_scalar(
        "SELECT status FROM sync_ops WHERE hotel_id = $1 AND operation_id = $2",
    )
    .bind(auth.hotel_id)
    .bind(op.operation_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;
    if let Some(s) = seen {
        let st = replay_status(&s);
        let _ = conn.rollback().await; // nothing written
        return Ok(st);
    }

    let status = match op.entity_type {
        EntityType::Reservation => apply_reservation(&mut conn, auth, op).await,
        EntityType::Guest => apply_guest(&mut conn, auth, op).await,
        EntityType::RoomStatus => apply_cleanliness(&mut conn, auth, op).await,
        EntityType::Payment => apply_payment(&mut conn, auth, op).await,
        EntityType::FolioTransaction => apply_folio(&mut conn, auth, op).await,
        EntityType::RoomType => config_insert_or_server_wins(&mut conn, auth, op, "room_type").await,
        EntityType::Room => config_insert_or_server_wins(&mut conn, auth, op, "room").await,
        EntityType::PriceRule => config_insert_or_server_wins(&mut conn, auth, op, "price_rule").await,
        EntityType::Channel => config_insert_or_server_wins(&mut conn, auth, op, "channel").await,
        EntityType::User => Err("unreachable (handled by required_perm)".into()),
    }?; // Err -> Rejected; the open transaction is rolled back on drop

    conn.commit().await.map_err(|e| format!("commit: {e}"))?;
    Ok(status)
}

async fn persist(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
    status: &SyncOpStatus,
    echo: serde_json::Value,
) -> Result<(), String> {
    if let Some((db_status, detail)) = op_status_db(status) {
        sqlx::query(
            "INSERT INTO sync_ops (hotel_id, operation_id, status, detail)
             VALUES ($1,$2,$3,$4)
             ON CONFLICT (hotel_id, operation_id) DO NOTHING",
        )
        .bind(auth.hotel_id)
        .bind(op.operation_id)
        .bind(db_status)
        .bind(detail)
        .execute(&mut *conn)
        .await
        .map_err(|e| format!("sync_ops: {e}"))?;
    }
    db::record_change(
        conn,
        auth.hotel_id,
        Some(auth.user_id),
        op.operation_id,
        entity_sql_name(op.entity_type),
        op.entity_id,
        operation_sql_name(op.operation_type),
        echo,
    )
    .await
    .map(|_| ())
    .map_err(|e| e.1)
}

// ------------------------------------------------------------------ guests

/// Guests are client data: upsert, last-write-wins (tombstones respected).
async fn apply_guest(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
) -> Result<SyncOpStatus, String> {
    match op.operation_type {
        OT::Delete => {
            sqlx::query(
                "UPDATE guests SET deleted_at = now(), updated_at = now()
                 WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
            )
            .bind(op.entity_id)
            .bind(auth.hotel_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
            persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"tombstone": true})).await?;
            Ok(SyncOpStatus::Applied)
        }
        OT::Insert | OT::Update => {
            let full_name = p_str(&op.payload, "full_name")?.to_owned();
            let n = sqlx::query(
                r#"INSERT INTO guests
                     (id, hotel_id, full_name, id_number_encrypted, phone_encrypted, email_encrypted,
                      nationality, license_plate, birth_date, is_primary)
                   VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
                   ON CONFLICT (id) DO UPDATE SET
                     full_name = EXCLUDED.full_name,
                     id_number_encrypted = EXCLUDED.id_number_encrypted,
                     phone_encrypted = EXCLUDED.phone_encrypted,
                     email_encrypted = EXCLUDED.email_encrypted,
                     nationality = EXCLUDED.nationality,
                     license_plate = EXCLUDED.license_plate,
                     birth_date = EXCLUDED.birth_date,
                     is_primary = EXCLUDED.is_primary,
                     updated_at = now()
                   WHERE guests.deleted_at IS NULL"#,
            )
            .bind(op.entity_id)
            .bind(auth.hotel_id)
            .bind(&full_name)
            .bind(p_opt_str(&op.payload, "id_number")?)
            .bind(p_opt_str(&op.payload, "phone")?)
            .bind(p_opt_str(&op.payload, "email")?)
            .bind(p_opt_str(&op.payload, "nationality")?)
            .bind(p_opt_str(&op.payload, "license_plate")?)
            .bind(p_opt_date(&op.payload, "birth_date")?)
            .bind(op.payload.get("is_primary").and_then(|b| b.as_bool()).unwrap_or(true))
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?
            .rows_affected();
            if n == 0 {
                // tombstoned guest stays deleted — replaying is a no-op
                persist(conn, auth, op, &SyncOpStatus::DuplicateIgnored,
                    serde_json::json!({"note": "tombstoned guest stays deleted"})).await?;
                return Ok(SyncOpStatus::DuplicateIgnored);
            }
            persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"full_name": full_name})).await?;
            Ok(SyncOpStatus::Applied)
        }
    }
}

// ------------------------------------------------------------- reservations

/// Offline reservation insert: identical column semantics to the online create
/// path, but the no-double-booking EXCLUDE resolves to status `conflict`
/// instead of an error (later booking is accepted — README §11.5).
async fn apply_reservation(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
) -> Result<SyncOpStatus, String> {
    match op.operation_type {
        OT::Insert => insert_reservation(conn, auth, op).await,
        OT::Update => update_reservation(conn, auth, op).await,
        OT::Delete => {
            // tombstone a reservation (nothing is ever hard-deleted, §7)
            sqlx::query(
                "UPDATE reservations SET deleted_at = now()
                 WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
            )
            .bind(op.entity_id)
            .bind(auth.hotel_id)
            .execute(&mut *conn)
            .await
            .map_err(|e| e.to_string())?;
            persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"tombstone": true})).await?;
            Ok(SyncOpStatus::Applied)
        }
    }
}

async fn insert_reservation(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
) -> Result<SyncOpStatus, String> {
    let room_id = p_uuid(&op.payload, "room_id")?;
    let channel_id = p_uuid(&op.payload, "channel_id")?;
    let guest_id = p_opt_uuid(&op.payload, "guest_id")?;
    let checkin = p_date(&op.payload, "checkin_date")?;
    let checkout = p_date(&op.payload, "checkout_date")?;
    if checkout <= checkin {
        return Err("check-out must be after check-in".into());
    }
    let adults = op.payload.get("adults").and_then(|a| a.as_i64()).unwrap_or(1) as i16;
    let children = op.payload.get("children").and_then(|a| a.as_i64()).unwrap_or(0) as i16;
    let currency = p_opt_str(&op.payload, "currency")?.unwrap_or_else(|| "TRY".into());
    let notes = p_opt_str(&op.payload, "notes")?;
    let voucher = p_opt_str(&op.payload, "voucher_number")?;

    // same pricing path as the online create (price rules, then room-type base)
    let (rates, total) =
        super::reservations::rates_for_stay(&mut *conn, auth.hotel_id, room_id, checkin, checkout, &currency)
            .await
            .map_err(|e| e.1.to_string())?;
    let rates_json = serde_json::to_value(&rates).unwrap_or_default();

    let fields = ResInsert {
        id: op.entity_id,
        hotel_id: auth.hotel_id,
        room_id,
        channel_id,
        guest_id,
        checkin,
        checkout,
        rates_json: &rates_json,
        total,
        currency: &currency,
        adults,
        children,
        notes: &notes,
        voucher: &voucher,
        created_by: auth.user_id,
    };

    let final_status = {
        // The no-double-booking EXCLUDE aborts the whole tx on violation, so the
        // first attempt runs inside a SAVEPOINT: on overlap we roll back to it
        // and re-insert with status `conflict` on a clean transaction state
        // (README §11.5 — the later booking is accepted, never dropped).
        let mut sp = sqlx::Transaction::<'_, sqlx::Postgres>::begin(&mut *conn, None)
            .await
            .map_err(|e| format!("savepoint: {e}"))?;
        match run_res_insert(&mut sp, &fields, "confirmed").await {
            Err(e) if is_exclusion_violation(&e) => {
                let _ = sp.rollback().await;
                run_res_insert(&mut *conn, &fields, "conflict")
                    .await
                    .map_err(|e| format!("conflict insert: {e}"))?;
                super::reservations::seed_revenue_rows(&mut *conn, auth.hotel_id, room_id, channel_id, op.entity_id).await;
                let reason = "overlapping stay -> status conflict (reassign in UI)".to_owned();
                persist(conn, auth, op,
                    &SyncOpStatus::ConflictResolved { message: reason.clone() },
                    serde_json::json!({"checkin_date": checkin, "checkout_date": checkout})).await?;
                SyncOpStatus::ConflictResolved { message: reason }
            }
            Err(e) if is_fk_violation(&e) => return Err("references an unknown room/channel/guest for this hotel".into()),
            Err(e) => return Err(format!("reservation insert: {e}")),
            Ok(_) => {
                let _ = sp.commit().await;
                super::reservations::seed_revenue_rows(&mut *conn, auth.hotel_id, room_id, channel_id, op.entity_id).await;
                super::reservations::upsert_daily_revenue(&mut *conn, &super::reservations::ReservationUpsertInfo {
                    hotel_id: auth.hotel_id,
                    room_id,
                    channel_id,
                    checkin_date: checkin,
                    checkout_date: checkout,
                    status: "confirmed".into(),
                    daily_rates: rates_json.clone(),
                })
                .await
                .map_err(|e| e.1.to_string())?;
                persist(conn, auth, op, &SyncOpStatus::Applied,
                    serde_json::json!({"checkin_date": checkin, "checkout_date": checkout})).await?;
                SyncOpStatus::Applied
            }
        }
    };
    Ok(final_status)
}

/// Borrow bundle for the reservation insert (called twice for the
/// confirmed -> conflict fallback; can't be a closure because both calls hold
/// the same &mut connection across awaits).
struct ResInsert<'a> {
    id: Uuid,
    hotel_id: i64,
    room_id: Uuid,
    channel_id: Uuid,
    guest_id: Option<Uuid>,
    checkin: chrono::NaiveDate,
    checkout: chrono::NaiveDate,
    rates_json: &'a serde_json::Value,
    total: i64,
    currency: &'a str,
    adults: i16,
    children: i16,
    notes: &'a Option<String>,
    voucher: &'a Option<String>,
    created_by: Uuid,
}

async fn run_res_insert(
    conn: &mut sqlx::PgConnection,
    f: &ResInsert<'_>,
    status: &str,
) -> Result<sqlx::postgres::PgQueryResult, sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO reservations
             (id, hotel_id, room_id, channel_id, guest_id, stay, checkin_date, checkout_date,
              status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_by)
           VALUES ($1,$2,$3,$4,$5, daterange($6,$7,'[)'), $6, $7,
              $8, $9, $10, $11, $12, $13, $14, $15, $16)"#,
    )
    .bind(f.id)
    .bind(f.hotel_id)
    .bind(f.room_id)
    .bind(f.channel_id)
    .bind(f.guest_id)
    .bind(f.checkin)
    .bind(f.checkout)
    .bind(status.to_owned())
    .bind(f.rates_json)
    .bind(f.total)
    .bind(f.currency)
    .bind(f.adults)
    .bind(f.children)
    .bind(f.notes)
    .bind(f.voucher)
    .bind(f.created_by)
    .execute(&mut *conn)
    .await
}

/// Offline edit: same optional-field semantics as the online edit endpoint
/// (§9 — price recalculated server-side, stay rebuilt, DB constraint guarded).
async fn update_reservation(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
) -> Result<SyncOpStatus, String> {
    #[derive(sqlx::FromRow)]
    struct Row {
        room_id: Uuid,
        channel_id: Uuid,
        guest_id: Option<Uuid>,
        checkin: chrono::NaiveDate,
        checkout: chrono::NaiveDate,
        status: String,
        currency: String,
    }
    let row: Row = sqlx::query_as(
        r#"SELECT room_id, channel_id, guest_id, checkin_date AS checkin, checkout_date AS checkout,
                  status, currency
           FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(op.entity_id)
    .bind(auth.hotel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "reservation not found or deleted".to_owned())?;

    let room_id = p_opt_uuid(&op.payload, "room_id")?.unwrap_or(row.room_id);
    let channel_id = p_opt_uuid(&op.payload, "channel_id")?.unwrap_or(row.channel_id);
    let guest_id = p_opt_uuid(&op.payload, "guest_id")?.or(row.guest_id);
    let checkin = p_opt_date(&op.payload, "checkin_date")?.unwrap_or(row.checkin);
    let checkout = p_opt_date(&op.payload, "checkout_date")?.unwrap_or(row.checkout);
    if checkout <= checkin {
        return Err("check-out must be after check-in".into());
    }
    // status transition allowed offline: check-in / check-out / cancel
    let status = match p_opt_str(&op.payload, "status")? {
        None => row.status,
        Some(s) if ["confirmed", "checked_in", "checked_out", "cancelled"].contains(&s.as_str()) => s,
        Some(_) => return Err("invalid status".into()),
    };

    let (rates, total) =
        super::reservations::rates_for_stay(&mut *conn, auth.hotel_id, room_id, checkin, checkout, &row.currency)
            .await
            .map_err(|e| e.1.to_string())?;
    let rates_json = serde_json::to_value(&rates).unwrap_or_default();

    let res = sqlx::query(
        r#"UPDATE reservations SET
             room_id = $3, channel_id = $4, guest_id = $5,
             stay = daterange($6,$7,'[)'), checkin_date = $6, checkout_date = $7,
             status = $8, daily_rates = $9, total_price_cents = $10,
             adults = COALESCE($11, reservations.adults),
             children = COALESCE($12, reservations.children),
             notes = COALESCE($13, reservations.notes),
             voucher_number = COALESCE($14, reservations.voucher_number),
             deleted_at = NULL,
             updated_at = now()
           WHERE id = $1 AND hotel_id = $2"#,
    )
    .bind(op.entity_id)
    .bind(auth.hotel_id)
    .bind(room_id)
    .bind(channel_id)
    .bind(guest_id)
    .bind(checkin)
    .bind(checkout)
    .bind(&status)
    .bind(&rates_json)
    .bind(total)
    .bind(op.payload.get("adults").and_then(|a| a.as_i64()).map(|a| a as i16))
    .bind(op.payload.get("children").and_then(|a| a.as_i64()).map(|a| a as i16))
    .bind(p_opt_str(&op.payload, "notes")?)
    .bind(p_opt_str(&op.payload, "voucher_number")?)
    .execute(&mut *conn)
    .await;

    let final_status = match res {
        Err(e) if is_exclusion_violation(&e) => {
            // an offline room move/dates change may collide — Park it as conflict
            sqlx::query("UPDATE reservations SET status = 'conflict', updated_at = now() WHERE id = $1 AND hotel_id = $2")
                .bind(op.entity_id)
                .bind(auth.hotel_id)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("conflict park: {e}"))?;
            SyncOpStatus::ConflictResolved {
                message: "edit would double-book -> status conflict (reassign in UI)".into(),
            }
        }
        Err(e) => return Err(format!("reservation update: {e}")),
        Ok(_) if status != "conflict" => SyncOpStatus::Applied,
        Ok(_) => SyncOpStatus::Applied,
    };

    super::reservations::upsert_daily_revenue(&mut *conn, &super::reservations::ReservationUpsertInfo {
        hotel_id: auth.hotel_id,
        room_id,
        channel_id,
        checkin_date: checkin,
        checkout_date: checkout,
        status: status.clone(),
        daily_rates: rates_json,
    })
    .await
    .map_err(|e| e.1.to_string())?;
    persist(conn, auth, op, &final_status, serde_json::json!({"status": status})).await?;
    Ok(final_status)
}

// -------------------------------------------------------------- housekeeping

async fn apply_cleanliness(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
) -> Result<SyncOpStatus, String> {
    let cleanliness = p_str(&op.payload, "cleanliness")?.to_owned();
    let n = sqlx::query(
        "UPDATE rooms SET cleanliness = $3, updated_at = now()
         WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
    )
    .bind(op.entity_id)
    .bind(auth.hotel_id)
    .bind(&cleanliness)
    .execute(&mut *conn)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();
    if n == 0 {
        return Err("room not found or deleted".into());
    }
    // last-write-wins (README §11.5) — ordering is server arrival
    persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"cleanliness": cleanliness})).await?;
    Ok(SyncOpStatus::Applied)
}

// ------------------------------------------------------------------ payments

async fn apply_payment(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
) -> Result<SyncOpStatus, String> {
    if op.operation_type != OT::Insert {
        // append-only by design (README §7/§11.5): corrections are NEW reversal rows
        return Err("payments are append-only; record a reversal payment instead".into());
    }
    let reservation_id = p_uuid(&op.payload, "reservation_id")?;
    let amount = op
        .payload
        .get("amount_cents")
        .and_then(|a| a.as_i64())
        .ok_or_else(|| "payload missing integer 'amount_cents'".to_owned())?;
    let currency = p_opt_str(&op.payload, "currency")?.unwrap_or_else(|| "TRY".into());
    let payment_type = p_opt_str(&op.payload, "payment_type")?.unwrap_or_else(|| "cash".into());
    let reference = p_opt_str(&op.payload, "reference")?;

    let res: (Uuid, Uuid) = sqlx::query_as(
        "SELECT room_id, channel_id FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
    )
    .bind(reservation_id)
    .bind(auth.hotel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "reservation not found or deleted".to_owned())?;

    sqlx::query(
        "INSERT INTO payments (id, hotel_id, reservation_id, amount_cents, currency, payment_type, reference, created_by)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
    )
    .bind(op.entity_id)
    .bind(auth.hotel_id)
    .bind(reservation_id)
    .bind(amount)
    .bind(&currency)
    .bind(&payment_type)
    .bind(&reference)
    .bind(auth.user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("payment insert: {e}"))?;

    // daily_revenue: collected money tracked on the payment day (README §10)
    // — same shape as the online record-payment path
    let day = chrono::Utc::now().date_naive();
    sqlx::query(
        r#"INSERT INTO daily_revenue (hotel_id, date, room_id, room_type_id, channel_id, payment_collected_cents, occupied)
           VALUES ($1, $2, $3, (SELECT room_type_id FROM rooms WHERE id = $3), $4,
                   CASE WHEN $5 < 0 THEN 0 ELSE $5 END, false)
           ON CONFLICT (hotel_id, date, room_id, channel_id) DO UPDATE
             SET payment_collected_cents = daily_revenue.payment_collected_cents + $5"#,
    )
    .bind(auth.hotel_id)
    .bind(day)
    .bind(res.0)
    .bind(res.1)
    .bind(amount)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("daily_revenue payment: {e}"))?;

    persist(conn, auth, op, &SyncOpStatus::Applied,
        serde_json::json!({"amount_cents": amount, "payment_type": payment_type})).await?;
    Ok(SyncOpStatus::Applied)
}

async fn apply_folio(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
) -> Result<SyncOpStatus, String> {
    if op.operation_type != OT::Insert {
        return Err("folio items are append-only (README §7); corrections are reversals".into());
    }
    let reservation_id = p_uuid(&op.payload, "reservation_id")?;
    let category = p_str(&op.payload, "category")?.to_owned();
    let description = p_str(&op.payload, "description")?.to_owned();
    let unit = op
        .payload
        .get("unit_price_cents")
        .and_then(|a| a.as_i64())
        .ok_or_else(|| "payload missing integer 'unit_price_cents'".to_owned())?;
    let qty = op.payload.get("quantity").and_then(|a| a.as_i64()).unwrap_or(1) as i32;
    let total = op
        .payload
        .get("total_cents")
        .and_then(|a| a.as_i64())
        .unwrap_or(unit * qty as i64);
    let currency = p_opt_str(&op.payload, "currency")?.unwrap_or_else(|| "TRY".into());

    let res: (Uuid, Uuid) = sqlx::query_as(
        "SELECT room_id, channel_id FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
    )
    .bind(reservation_id)
    .bind(auth.hotel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?
    .ok_or_else(|| "reservation not found or deleted".to_owned())?;

    sqlx::query(
        "INSERT INTO folio_transactions (id, hotel_id, reservation_id, category, description, unit_price_cents, quantity, total_cents, currency)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)",
    )
    .bind(op.entity_id)
    .bind(auth.hotel_id)
    .bind(reservation_id)
    .bind(&category)
    .bind(&description)
    .bind(unit)
    .bind(qty)
    .bind(total)
    .bind(&currency)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("folio insert: {e}"))?;

    // extra revenue joins the stay dates' daily_revenue rows (README §10)
    sqlx::query(
        r#"UPDATE daily_revenue dr
           SET extra_revenue_cents = dr.extra_revenue_cents + $3::bigint,
               updated_at = now()
           FROM reservations r
           WHERE dr.hotel_id = $1 AND dr.room_id = r.room_id AND r.id = $2
             AND dr.date >= r.checkin_date AND dr.date < r.checkout_date"#,
    )
    .bind(auth.hotel_id)
    .bind(reservation_id)
    .bind(total)
    .execute(&mut *conn)
    .await
    .map_err(|e| format!("daily_revenue extra: {e}"))?;
    super::reservations::seed_revenue_rows(&mut *conn, auth.hotel_id, res.0, res.1, reservation_id).await;

    persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"category": category})).await?;
    Ok(SyncOpStatus::Applied)
}

// --------------------------------------------------------- config entities

/// Inserts apply (offline creation has no server copy to defer to); updates of
/// config entities are resolved SERVER-WINS (README §11.5): the op is recorded
/// as conflict_resolved with a reason so the client learns the server copy is
/// authoritative, and the change_log echo lets OTHER clients converge.
async fn config_insert_or_server_wins(
    conn: &mut sqlx::PgConnection,
    auth: &AuthUser,
    op: &ChangeOperation,
    table: &str,
) -> Result<SyncOpStatus, String> {
    // entity name -> physical table name (change_log entity_type uses 'room')
    let physical = match table {
        "room" => "rooms",
        "room_type" => "room_types",
        _ => table,
    };
    let exists: Option<Uuid> = sqlx::query_scalar(&format!(
        "SELECT id FROM {physical} WHERE id = $1 AND hotel_id = $2"
    ))
    .bind(op.entity_id)
    .bind(auth.hotel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(|e| e.to_string())?;

    if exists.is_none() && op.operation_type == OT::Delete {
        // deleting something never seen here: already the desired end state
        persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"note": "already absent"})).await?;
        return Ok(SyncOpStatus::Applied);
    }
    if exists.is_some() && op.operation_type == OT::Insert {
        // same id + insert: treat as idempotent duplicate of a server-side create
        persist(conn, auth, op, &SyncOpStatus::DuplicateIgnored, serde_json::json!({"note": "already present"})).await?;
        return Ok(SyncOpStatus::DuplicateIgnored);
    }

    match op.operation_type {
        OT::Insert => match table {
            "room_type" => {
                let name = p_str(&op.payload, "name")?.to_owned();
                let capacity = op.payload.get("capacity").and_then(|c| c.as_i64()).unwrap_or(2) as i32;
                let base = op.payload.get("base_price_cents").and_then(|c| c.as_i64()).unwrap_or(0);
                let amenities: Vec<String> =
                    serde_json::from_value(op.payload.get("amenities").cloned().unwrap_or(serde_json::json!([])))
                        .unwrap_or_default();
                sqlx::query(
                    "INSERT INTO room_types (id, hotel_id, name, capacity, base_price_cents, amenities)
                     VALUES ($1,$2,$3,$4,$5,$6)",
                )
                .bind(op.entity_id)
                .bind(auth.hotel_id)
                .bind(name)
                .bind(capacity)
                .bind(base)
                .bind(&amenities)
                .execute(&mut *conn)
                .await
                .map_err(|e| format!("room_type insert: {e}"))?;
                persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"table": table})).await?;
                Ok(SyncOpStatus::Applied)
            }
            "room" => {
                let room_number = p_str(&op.payload, "room_number")?.to_owned();
                let room_type_id = p_uuid(&op.payload, "room_type_id")?;
                let floor = op.payload.get("floor").and_then(|f| f.as_i64()).unwrap_or(0) as i32;
                let cleanliness = p_opt_str(&op.payload, "cleanliness")?.unwrap_or_else(|| "clean".into());
                sqlx::query(
                    "INSERT INTO rooms (id, hotel_id, room_type_id, room_number, floor, cleanliness)
                     VALUES ($1,$2,$3,$4,$5,$6)",
                )
                .bind(op.entity_id)
                .bind(auth.hotel_id)
                .bind(room_type_id)
                .bind(&room_number)
                .bind(floor)
                .bind(&cleanliness)
                .execute(&mut *conn)
                .await
                .map_err(|e| if is_fk_violation(&e) { "unknown room_type for this hotel".into() } else { format!("room insert: {e}") })?;
                persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"table": table})).await?;
                Ok(SyncOpStatus::Applied)
            }
            "price_rule" => {
                let room_type_id = p_uuid(&op.payload, "room_type_id")?;
                let from = p_date(&op.payload, "date_from")?;
                let to = p_date(&op.payload, "date_to")?;
                let price = op
                    .payload
                    .get("price_cents")
                    .and_then(|c| c.as_i64())
                    .ok_or_else(|| "payload missing integer 'price_cents'".to_owned())?;
                let currency = p_opt_str(&op.payload, "currency")?.unwrap_or_else(|| "TRY".into());
                sqlx::query(
                    "INSERT INTO price_rules (id, hotel_id, room_type_id, date_from, date_to, price_cents, currency, created_by)
                     VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
                )
                .bind(op.entity_id)
                .bind(auth.hotel_id)
                .bind(room_type_id)
                .bind(from)
                .bind(to)
                .bind(price)
                .bind(&currency)
                .bind(auth.user_id)
                .execute(&mut *conn)
                .await
                .map_err(|e| if is_fk_violation(&e) { "unknown room_type for this hotel".into() } else { format!("price_rule insert: {e}") })?;
                persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"table": table})).await?;
                Ok(SyncOpStatus::Applied)
            }
            "channel" => {
                let name = p_str(&op.payload, "name")?.to_owned();
                let kind = p_str(&op.payload, "kind")?.to_owned();
                let active = op.payload.get("is_active").and_then(|b| b.as_bool()).unwrap_or(true);
                sqlx::query("INSERT INTO channels (id, hotel_id, kind, name, is_active) VALUES ($1,$2,$3,$4,$5)")
                    .bind(op.entity_id)
                    .bind(auth.hotel_id)
                    .bind(&kind)
                    .bind(&name)
                    .bind(active)
                    .execute(&mut *conn)
                    .await
                    .map_err(|e| format!("channel insert: {e}"))?;
                persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"table": table})).await?;
                Ok(SyncOpStatus::Applied)
            }
            _ => Err(format!("unsupported table '{table}'")),
        },
        OT::Update | OT::Delete => {
            if exists.is_some() {
                // server wins: keep the server row, tell the client why
                let detail = format!("server wins per README §11 ({table})");
                persist(conn, auth, op,
                    &SyncOpStatus::ConflictResolved { message: detail.clone() },
                    serde_json::json!({"server_wins": true, "table": table})).await?;
                return Ok(SyncOpStatus::ConflictResolved { message: detail });
            }
            // row absent (e.g. an offline-created row that failed earlier):
            // treat repeated updates/deletes as applied no-ops so the outbox drains
            persist(conn, auth, op, &SyncOpStatus::Applied, serde_json::json!({"table": table})).await?;
            Ok(SyncOpStatus::Applied)
        }
    }
}
