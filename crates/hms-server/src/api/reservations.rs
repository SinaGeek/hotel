//! Reservations: list/create/detail/edit/check-in/check-out/cancel,
//! availability, board, payments and folio (README Ã‚Â§9, Ã‚Â§12).
use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{routing::{get, post}, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(by_id).patch(edit))
        .route("/{id}/check-in", post(check_in))
        .route("/{id}/check-out", post(check_out))
        .route("/{id}/cancel", post(cancel))
}

pub fn board_router() -> Router<AppState> {
    Router::new().route("/", get(board))
}

pub fn payments() -> Router<AppState> {
    Router::new().route("/{id}/payments", post(record_payment).get(payments_of))
}

pub fn payment_reversals() -> Router<AppState> {
    Router::new().route("/{id}/reverse", post(reverse_payment))
}

// ------------------------------------------------------------------------- DTOs

#[derive(Serialize, sqlx::FromRow)]
pub struct ReservationDto {
    pub id: Uuid,
    pub hotel_id: i64,
    pub room_id: Uuid,
    pub channel_id: Uuid,
    pub guest_id: Option<Uuid>,
    pub checkin_date: chrono::NaiveDate,
    pub checkout_date: chrono::NaiveDate,
    pub status: String,
    pub daily_rates: serde_json::Value,
    pub total_price_cents: i64,
    pub currency: String,
    pub adults: i16,
    pub children: i16,
    pub notes: Option<String>,
    pub voucher_number: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Deserialize, Default)]
pub struct ListQuery {
    pub from: Option<chrono::NaiveDate>,
    pub to: Option<chrono::NaiveDate>,
    pub status: Option<String>,
    pub limit: Option<i64>,
    /// keyset pagination (never OFFSET): id of the last row from the previous page
    pub after: Option<String>,
}

// ------------------------------------------------------------------------- helpers

/// Compute effective daily price for a room type on a date: latest matching
/// price rule (overlaps rule range), else room type base price.
async fn price_for_date(
    conn: &mut sqlx::PgConnection,
    hotel_id: i64,
    room_type_id: Uuid,
    date: chrono::NaiveDate,
) -> Result<i64, ApiError> {
    let price: Option<Option<i64>> = sqlx::query_scalar(
        r#"SELECT price_cents FROM price_rules
           WHERE hotel_id = $1 AND room_type_id = $2 AND deleted_at IS NULL
             AND date_from <= $3 AND date_to >= $3
           ORDER BY date_from DESC LIMIT 1"#,
    )
    .bind(hotel_id)
    .bind(room_type_id)
    .bind(date)
    .fetch_optional(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    Ok(price.flatten().unwrap_or(0))
}

/// Effective price for a room type on a date: latest matching price rule,
/// else the room type base price.
async fn effective_price_for_date(
    conn: &mut sqlx::PgConnection,
    hotel_id: i64,
    room_type_id: Uuid,
    date: chrono::NaiveDate,
) -> Result<i64, ApiError> {
    let rule = match price_for_date(conn, hotel_id, room_type_id, date).await {
        Ok(rule) => rule,
        Err(e) => {
            tracing::warn!("effective_price: rules lookup failed: 0={}", e.1);
            return Err(e);
        }
    };
    if rule > 0 {
        return Ok(rule);
    }
    let base: i64 = match sqlx::query_scalar("SELECT base_price_cents FROM room_types WHERE id = $1")
        .bind(room_type_id)
        .fetch_one(&mut *conn)
        .await
    {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!("effective_price: base lookup failed: {e}");
            return Err(ApiError::from(e));
        }
    };
    Ok(base)
}


async fn rates_for_stay(
    conn: &mut sqlx::PgConnection,
    hotel_id: i64,
    room_id: Uuid,
    from: chrono::NaiveDate,
    to: chrono::NaiveDate,
    currency: &str,
) -> Result<(Vec<serde_json::Value>, i64), ApiError> {
    let room_type_id: Uuid = sqlx::query_scalar("SELECT room_type_id FROM rooms WHERE id = $1 AND hotel_id = $2")
        .bind(room_id)
        .bind(hotel_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| ApiError(StatusCode::BAD_REQUEST, "room not found".into()))?;
    let base: i64 = sqlx::query_scalar("SELECT base_price_cents FROM room_types WHERE id = $1")
        .bind(room_type_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(ApiError::from)?;

    let mut rates: Vec<serde_json::Value> = Vec::new();
    let mut total: i64 = 0;
    let mut d = from;
    while d < to {
        let price = price_for_date(&mut *conn, hotel_id, room_type_id, d).await?;
        let price = if price > 0 { price } else { base };
        rates.push(serde_json::json!({ "date": d.to_string(), "price_cents": price }));
        total += price;
        d = d.succ_opt().unwrap_or(d);
    }
    let _ = currency;
    Ok((rates, total))
}

async fn room_type_of(
    conn: &mut sqlx::PgConnection,
    room_id: Uuid,
    hotel_id: i64,
) -> Result<Uuid, (StatusCode, String)> {    sqlx::query_scalar::<_, Uuid>("SELECT room_type_id FROM rooms WHERE id = $1 AND hotel_id = $2")
        .bind(room_id)
        .bind(hotel_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("room lookup: {e}")))
}
type SQL_ERR = (StatusCode, String);

/// Maintain daily_revenue rows for a reservation change (README Ã‚Â§10): same transaction!
async fn upsert_daily_revenue(
    conn: &mut sqlx::PgConnection,
    res: &ReservationUpsertInfo,
) -> Result<(), (StatusCode, String)> {
    let room_type_id: Uuid = room_type_of(conn, res.room_id, res.hotel_id).await
        .expect("reservation references a room of the same tenant");
    // For each stay date: recompute room revenue from reservation status
    let non_revenue = res.status == "cancelled" || res.status == "conflict";
    sqlx::query(
        r#"INSERT INTO daily_revenue (hotel_id, date, room_id, room_type_id, channel_id, room_revenue_cents, occupied, updated_at)
           SELECT $1, d.day, $3, $4, $5, COALESCE(r.price_cents, 0), NOT $6::boolean, now()
           FROM generate_series($2::date, ($2::date + make_interval(days => $7))::date - INTERVAL '1 day', INTERVAL '1 day') AS d(day)
           LEFT JOIN LATERAL (
              SELECT (elem->>'price_cents')::bigint AS price_cents
              FROM jsonb_array_elements($8::jsonb) elem
              WHERE (elem->>'date') = to_char(d.day, 'YYYY-MM-DD')
           ) r ON true
           WHERE (SELECT price_cents FROM daily_revenue WHERE hotel_id = $1 AND date = d.day AND room_id = $3) IS NULL
              OR true
           ON CONFLICT (hotel_id, date, room_id, channel_id) DO UPDATE
             SET room_revenue_cents = EXCLUDED.room_revenue_cents,
                 channel_id = EXCLUDED.channel_id,
                 occupied = EXCLUDED.occupied,
                 room_type_id = EXCLUDED.room_type_id,
                 updated_at = now()"#,
    )
    .bind(res.hotel_id)
    .bind(res.checkin_date)
    .bind(res.room_id)
    .bind(room_type_id)
    .bind(res.channel_id)
    .bind(non_revenue)
    .bind(res.checkout_date.signed_duration_since(res.checkin_date).num_days() as i32)
    .bind(res.daily_rates.clone())
    .execute(&mut *conn)
    .await
    .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("daily_revenue: {e}")))?;

    if non_revenue {
        sqlx::query(
            r#"UPDATE daily_revenue SET room_revenue_cents = 0, occupied = false, updated_at = now()
               WHERE hotel_id = $1 AND room_id = $2 AND date >= $3 AND date < $4"#,
        )
        .bind(res.hotel_id)
        .bind(res.room_id)
        .bind(res.checkin_date)
        .bind(res.checkout_date)
        .execute(&mut *conn)
        .await
        .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, format!("daily_revenue clear: {e}")))?;
    }
    Ok(())
}

pub struct ReservationUpsertInfo {
    pub hotel_id: i64,
    pub room_id: Uuid,
    pub channel_id: Uuid,
    pub checkin_date: chrono::NaiveDate,
    pub checkout_date: chrono::NaiveDate,
    pub status: String,
    pub daily_rates: serde_json::Value,
}

// ------------------------------------------------------------------------- endpoints

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> AResult<Json<ApiResponse<Vec<ReservationDto>>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::ReservationView).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "reservation_view required".into()));
    }
    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let limit = q.limit.unwrap_or(50).min(200);
    // Keyset pagination on created_at + id (never OFFSET Ã¢â‚¬â€ README Ã‚Â§12).
    let after_uuid = q.after.as_ref().and_then(|s| Uuid::parse_str(s).ok());
    let rows: Vec<ReservationDto> = sqlx::query_as(
        r#"SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status,
                  daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at
           FROM reservations
           WHERE hotel_id = $1
             AND (COALESCE($3::text, status) = status OR $3::text IS NULL)
             AND ($4::date IS NULL OR checkout_date > $4)
             AND ($5::date IS NULL OR checkin_date < $5)
             AND ($6::uuid IS NULL OR (created_at, id) <
                   (SELECT created_at, id FROM reservations WHERE id = $6))
           ORDER BY created_at DESC, id DESC
           LIMIT $2"#,
    )
    .bind(auth.hotel_id)
    .bind(limit)
    .bind(q.status.clone())
    .bind(q.from)
    .bind(q.to)
    .bind(after_uuid)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

async fn by_id(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<ReservationDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::ReservationView).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "reservation_view required".into()));
    }
    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let row: ReservationDto = sqlx::query_as(
        r#"SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status,
                  daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at
           FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| ApiError(StatusCode::NOT_FOUND, "reservation not found".into()))?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}

#[derive(Deserialize, Serialize)]
pub struct CreateReservationBody {
    pub room_id: Uuid,
    pub channel_id: Uuid,
    pub guest_id: Option<Uuid>,
    pub checkin_date: chrono::NaiveDate,
    pub checkout_date: chrono::NaiveDate,
    pub adults: i16,
    pub children: i16,
    pub currency: Option<String>,
    pub notes: Option<String>,
    pub voucher_number: Option<String>,
}

pub async fn validate_reservation(
    body: &CreateReservationBody,
) -> Result<(), hms_core::validation::ValidationError> {
    if body.checkout_date <= body.checkin_date {
        return Err(hms_core::validation::ValidationError::InvalidDateRange);
    }
    if body.adults < 1 {
        return Err(hms_core::validation::ValidationError::InvalidPax);
    }
    Ok(())
}

pub async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateReservationBody>,
) -> AResult<Json<ApiResponse<ReservationDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::ReservationCreate).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "reservation_create required".into()));
    }
    validate_reservation(&body).await.map_err(|e| ApiError(StatusCode::from_u16(e.status_code()).unwrap(), e.to_string()))?;

    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let currency = body.currency.clone().unwrap_or_else(|| "TRY".into());

    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let (rates, total) = rates_for_stay(&mut conn, auth.hotel_id, body.room_id, body.checkin_date, body.checkout_date, &currency).await?;

    sqlx::query(
        r#"INSERT INTO reservations
             (id, hotel_id, room_id, channel_id, guest_id, stay, checkin_date, checkout_date,
              status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_by)
           VALUES ($1,$2,$3,$4,$5, daterange($6,$7,'[)'), $6, $7,
              'confirmed', $8, $9, $10, $11, $12, $13, $14, $15)"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(body.room_id)
    .bind(body.channel_id)
    .bind(body.guest_id)
    .bind(body.checkin_date)
    .bind(body.checkout_date)
    .bind(serde_json::to_value(&rates).unwrap_or_default())
    .bind(total)
    .bind(&currency)
    .bind(body.adults)
    .bind(body.children)
    .bind(&body.notes)
    .bind(&body.voucher_number)
    .bind(auth.user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::CONFLICT, format!("cannot book (no double booking): {e}")))?;

    // audit + pre-aggregation in the SAME transaction (README §10/§11).
    // record_change runs LAST: it row-locks the hotels row (change-seq bump)
    // until commit, so keeping it last keeps the critical section (~0 queries
    // + commit) short enough for concurrent saves to hit the p95 target.
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "reservation", id, "created", None,
        serde_json::to_value(&body).ok()).await?;
    upsert_daily_revenue(&mut conn, &ReservationUpsertInfo {
        hotel_id: auth.hotel_id,
        room_id: body.room_id,
        channel_id: body.channel_id,
        checkin_date: body.checkin_date,
        checkout_date: body.checkout_date,
        status: "confirmed".into(),
        daily_rates: serde_json::to_value(&rates).unwrap_or_default(),
    }).await?;
    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "reservation", id, "insert",
        serde_json::json!({"reservation": id})).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: ReservationDto = sqlx::query_as(
        "SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at FROM reservations WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}

#[derive(Deserialize, Serialize)]
struct EditReservationBody {
    room_id: Option<Uuid>,
    channel_id: Option<Uuid>,
    guest_id: Option<Uuid>,
    checkin_date: Option<chrono::NaiveDate>,
    checkout_date: Option<chrono::NaiveDate>,
    adults: Option<i16>,
    children: Option<i16>,
    notes: Option<String>,
    voucher_number: Option<String>,
}

pub async fn edit(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<EditReservationBody>,
) -> AResult<Json<ApiResponse<ReservationDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::ReservationEdit).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "reservation_edit required".into()));
    }
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let existing: ReservationDto = sqlx::query_as(
        "SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|_| ApiError(StatusCode::NOT_FOUND, "reservation not found".into()))?;
    if existing.status == "checked_out" || existing.status == "cancelled" {
        return Err(ApiError(StatusCode::CONFLICT, "cannot edit a checked-out or cancelled reservation".into()));
    }

    let room_id = body.room_id.unwrap_or(existing.room_id);
    let channel_id = body.channel_id.unwrap_or(existing.channel_id);
    let guest_id = body.guest_id.or(existing.guest_id);
    let checkin = body.checkin_date.unwrap_or(existing.checkin_date);
    let checkout = body.checkout_date.unwrap_or(existing.checkout_date);
    if checkout <= checkin {
        return Err(ApiError(StatusCode::BAD_REQUEST, "check-out must be after check-in".into()));
    }

    let (rates, total) = rates_for_stay(&mut conn, auth.hotel_id, room_id, checkin, checkout, &existing.currency).await?;

    sqlx::query(
        r#"UPDATE reservations SET
             room_id = $2, channel_id = $3, guest_id = $4,
             stay = daterange($5,$6,'[)'), checkin_date = $5, checkout_date = $6,
             daily_rates = $7, total_price_cents = $8,
             adults = COALESCE($9, adults), children = COALESCE($10, children),
             notes = COALESCE($11, notes), voucher_number = COALESCE($12, voucher_number),
             updated_at = now()
           WHERE id = $1 AND hotel_id = $13"#,
    )
    .bind(id)
    .bind(room_id)
    .bind(channel_id)
    .bind(guest_id)
    .bind(checkin)
    .bind(checkout)
    .bind(serde_json::to_value(&rates).unwrap_or_default())
    .bind(total)
    .bind(body.adults)
    .bind(body.children)
    .bind(body.notes.clone())
    .bind(body.voucher_number.clone())
    .bind(auth.hotel_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::CONFLICT, format!("update not allowed (no double booking): {e}")))?;

    let op_id = Uuid::now_v7();
    // record_change last (row-locks the hotels row until commit — see create)
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "reservation", id, "updated",
        Some(serde_json::to_value(&existing).unwrap_or_default()), Some(serde_json::json!(&body))).await?;
    upsert_daily_revenue(&mut conn, &ReservationUpsertInfo {
        hotel_id: auth.hotel_id, room_id, channel_id, checkin_date: checkin, checkout_date: checkout,
        status: existing.status.clone(), daily_rates: serde_json::to_value(&rates).unwrap_or_default(),
    }).await?;
    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "reservation", id, "update", serde_json::json!(&body)).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: ReservationDto = sqlx::query_as(
        "SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at FROM reservations WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}

pub async fn status_change(
    state: &AppState,
    auth: &AuthUser,
    id: Uuid,
    new_status: &str,
) -> AResult<Json<ApiResponse<ReservationDto>>> {
    use crate::auth::has_permission;
    let perm = match new_status {
        "checked_in" => hms_core::roles::Permission::ReservationCheckIn,
        "checked_out" => hms_core::roles::Permission::ReservationCheckOut,
        "cancelled" => hms_core::roles::Permission::ReservationCancel,
        _ => unreachable!(),
    };
    if !has_permission(&state.db, &auth.role, auth.hotel_id, perm).await.unwrap_or(false) {
        return Err(ApiError(StatusCode::FORBIDDEN, format!("{} required", perm.as_str())));
    }
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let existing: ReservationDto = sqlx::query_as(
        "SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
    )
    .bind(id).bind(auth.hotel_id)
    .fetch_one(&mut *conn).await
    .map_err(|_| ApiError(StatusCode::NOT_FOUND, "reservation not found".to_string()))?;
    sqlx::query("UPDATE reservations SET status = $2, updated_at = now() WHERE id = $1 AND hotel_id = $3")
        .bind(id).bind(new_status).bind(auth.hotel_id)
        .execute(&mut *conn).await.map_err(ApiError::from)?;
    let op_id = Uuid::now_v7();
    // record_change last (row-locks the hotels row until commit — see create)
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "reservation", id, new_status,
        Some(serde_json::json!({"status": existing.status})), Some(serde_json::json!({"status": new_status}))).await?;
    if new_status == "cancelled" {
        upsert_daily_revenue(&mut conn, &ReservationUpsertInfo {
            hotel_id: auth.hotel_id, room_id: existing.room_id, channel_id: existing.channel_id,
            checkin_date: existing.checkin_date, checkout_date: existing.checkout_date,
            status: new_status.to_string(), daily_rates: existing.daily_rates.clone(),
        }).await?;
    }
    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "reservation", id, "update",
        serde_json::json!({"status": new_status})).await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: ReservationDto = sqlx::query_as(
        "SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at FROM reservations WHERE id = $1",
    )
    .bind(id).fetch_one(&state.db).await.map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}

async fn check_in(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<ReservationDto>>> {
    status_change(&state, &auth, id, "checked_in").await
}

async fn check_out(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<ReservationDto>>> {
    status_change(&state, &auth, id, "checked_out").await
}

async fn cancel(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<ReservationDto>>> {
    status_change(&state, &auth, id, "cancelled").await
}

// ------------------------------------------------------------------------- availability & board

#[derive(Deserialize, Serialize)]
pub struct BoardQuery {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
}

#[derive(Serialize)]
pub struct BoardResponse {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub rooms: Vec<BoardRoom>,
    pub occupancy: Vec<BoardDay>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct BoardRoom {
    pub id: Uuid,
    pub room_number: String,
    pub room_type_id: Uuid,
    pub room_type_name: String,
    pub cleanliness: String,
    pub stays: Vec<BoardStay>,
    pub seasonal_price_cents: i64,
}

#[derive(Serialize)]
pub struct BoardStay {
    pub reservation_id: Uuid,
    pub checkin_date: chrono::NaiveDate,
    pub checkout_date: chrono::NaiveDate,
    pub status: String,
    pub guest_name: Option<String>,
    pub total_price_cents: i64,
    pub currency: String,
}

#[derive(Serialize)]
pub struct BoardDay {
    pub date: chrono::NaiveDate,
    pub available: i64,
    pub occupied: i64,
    /// cheapest price across room types that day (for the calendar header)
    pub from_price_cents: Option<i64>,
}

async fn board(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<BoardQuery>,
) -> AResult<Json<ApiResponse<BoardResponse>>> {
    if q.to < q.from {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid range".into()));
    }
    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let room_ids: Vec<(Uuid, String, Uuid, String, String)> = sqlx::query_as(
        r#"SELECT r.id, r.room_number, r.room_type_id, rt.name AS room_type_name, r.cleanliness
           FROM rooms r JOIN room_types rt ON rt.id = r.room_type_id
           WHERE r.hotel_id = $1 AND r.deleted_at IS NULL AND rt.deleted_at IS NULL
           ORDER BY r.room_number"#,
    )
    .bind(auth.hotel_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    let stays: Vec<(Uuid, Uuid, chrono::NaiveDate, chrono::NaiveDate, String, Option<String>, i64, String)> = sqlx::query_as(
        r#"SELECT res.id, res.room_id, res.checkin_date, res.checkout_date, res.status,
                  g.full_name, res.total_price_cents, res.currency
           FROM reservations res
           LEFT JOIN guests g ON g.id = res.guest_id
           WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
             AND (res.checkin_date, res.checkout_date) OVERLAPS ($2::date, $3::date)"#,
    )
    .bind(auth.hotel_id)
    .bind(q.from)
    .bind(q.to)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    // housekeeping role must never see guest names/prices (README §6)
    let see_financials = hms_core::roles::Role::from_str(&auth.role)
        .map(|r| r.can_see_financials_and_guests())
        .unwrap_or(true);

    // Batched per-room-type pricing at the board start date: ONE query for all
    // room types (the effective price the per-room loop used to look up
    // individually — O(rooms) queries per request).
    let rt_prices: Vec<(Uuid, i64)> = sqlx::query_as(
        r#"SELECT rt.id,
                  COALESCE(NULLIF(
                    (SELECT pr.price_cents FROM price_rules pr
                     WHERE pr.hotel_id = $1 AND pr.room_type_id = rt.id AND pr.deleted_at IS NULL
                       AND pr.date_from <= $2 AND pr.date_to >= $2
                     ORDER BY pr.date_from DESC LIMIT 1), 0),
                    rt.base_price_cents)
           FROM room_types rt
           WHERE rt.hotel_id = $1 AND rt.deleted_at IS NULL"#,
    )
    .bind(auth.hotel_id)
    .bind(q.from)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    let price_by_rt: std::collections::HashMap<Uuid, i64> = rt_prices.into_iter().collect();

    let stay_map: std::collections::HashMap<Uuid, Vec<&(Uuid, Uuid, chrono::NaiveDate, chrono::NaiveDate, String, Option<String>, i64, String)>> =
        stays.iter().fold(std::collections::HashMap::new(), |mut m, s| {
            m.entry(s.1).or_default().push(s);
            m
        });

    let mut rooms = Vec::new();
    for (id, number, rt_id, rt_name, cleanliness) in &room_ids {
        let seasonal_price_cents = price_by_rt.get(rt_id).copied().unwrap_or(0);
        let stay_rows = stay_map.get(id).cloned().unwrap_or_default();
        let mut stay_list = Vec::new();
        for s in stay_rows {
            stay_list.push(BoardStay {
                reservation_id: s.0,
                checkin_date: s.2,
                checkout_date: s.3,
                status: s.4.clone(),
                guest_name: if see_financials { s.5.clone() } else { None },
                total_price_cents: if see_financials { s.6 } else { 0 },
                currency: s.7.clone(),
            });
        }
        rooms.push(BoardRoom {
            id: *id,
            room_number: number.clone(),
            room_type_id: *rt_id,
            room_type_name: rt_name.clone(),
            cleanliness: cleanliness.clone(),
            stays: stay_list,
            seasonal_price_cents,
        });
    }

    // day occupancy + cheapest effective price — ONE query for the whole range
    // (the previous code ran one query per day × room type)
    let by_day: Vec<(chrono::NaiveDate, i64, Option<i64>)> = sqlx::query_as(
        r#"SELECT d.t::date,
                  (SELECT count(*) FROM reservations res
                   WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
                     AND res.status IN ('confirmed','checked_in')
                     AND res.checkin_date <= d.t::date AND res.checkout_date > d.t::date) AS occupied,
                  (SELECT min(NULLIF(COALESCE(
                     (SELECT pr.price_cents FROM price_rules pr
                      WHERE pr.hotel_id = $1 AND pr.room_type_id = rt.id AND pr.deleted_at IS NULL
                        AND pr.date_from <= d.t::date AND pr.date_to >= d.t::date
                      ORDER BY pr.date_from DESC LIMIT 1),
                     rt.base_price_cents), 0))
                   FROM room_types rt
                   WHERE rt.hotel_id = $1 AND rt.deleted_at IS NULL) AS cheapest
           FROM generate_series($2::date, ($3::date - INTERVAL '1 day'), INTERVAL '1 day') AS d(t)
           ORDER BY 1"#,
    )
    .bind(auth.hotel_id)
    .bind(q.from)
    .bind(q.to)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    let total_rooms = room_ids.len() as i64;
    let mut occupancy = Vec::new();
    for (date, occ, cheapest) in by_day {
        occupancy.push(BoardDay { date, available: total_rooms - occ, occupied: occ, from_price_cents: cheapest });
    }

    Ok(Json(ApiResponse {
        success: true,
        data: BoardResponse { from: q.from, to: q.to, rooms, occupancy },
        error: None,
    }))
}

// ------------------------------------------------------------------------- payments

#[derive(Deserialize, Serialize)]
struct PaymentBody {
    amount_cents: i64,
    payment_type: String,
    currency: Option<String>,
    reference: Option<String>,
}

#[derive(Serialize, sqlx::FromRow)]
struct PaymentDto {
    pub id: Uuid,
    pub reservation_id: Uuid,
    pub amount_cents: i64,
    pub currency: String,
    pub payment_type: String,
    pub reference: Option<String>,
    pub is_reversal: bool,
    pub created_by: Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
}

async fn payments_of(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<Vec<PaymentDto>>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::FinancialView).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "financial_view required".into()));
    }
    let rows: Vec<PaymentDto> = sqlx::query_as(
        r#"SELECT id, reservation_id, amount_cents, currency, payment_type, reference, is_reversal, created_by, created_at
           FROM payments WHERE reservation_id = $1 AND hotel_id = $2 ORDER BY created_at ASC"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

async fn record_payment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PaymentBody>,
) -> AResult<Json<ApiResponse<PaymentDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::PaymentRecord).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "payment_record required".into()));
    }
    if !["cash", "credit_card", "bank_transfer", "agency", "refund"].contains(&body.payment_type.as_str()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid payment type".into()));
    }
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let res: ReservationDto = sqlx::query_as(
        "SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status, daily_rates, total_price_cents, currency, adults, children, notes, voucher_number, created_at FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
    )
    .bind(id).bind(auth.hotel_id)
    .fetch_one(&mut *conn).await
    .map_err(|_| ApiError(StatusCode::NOT_FOUND, "reservation not found".into()))?;

    let pid = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let currency = body.currency.clone().unwrap_or(res.currency.clone());
    let day = chrono::Utc::now().date_naive();
    sqlx::query(
        r#"INSERT INTO payments (id, hotel_id, reservation_id, amount_cents, currency, payment_type, reference, created_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
    )
    .bind(pid)
    .bind(auth.hotel_id)
    .bind(id)
    .bind(body.amount_cents)
    .bind(&currency)
    .bind(&body.payment_type)
    .bind(&body.reference)
    .bind(auth.user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("payment insert: {e}")))?;

    // daily_revenue: collected money tracked on the payment day (README Ã‚Â§10)
    sqlx::query(
        r#"INSERT INTO daily_revenue (hotel_id, date, room_id, room_type_id, channel_id, payment_collected_cents, occupied)
           VALUES ($1, $2, $3, (SELECT room_type_id FROM rooms WHERE id = $3), $4,
                   CASE WHEN $5 < 0 THEN 0 ELSE $5 END, COALESCE((SELECT occupied FROM daily_revenue WHERE hotel_id=$1 AND date=$2 AND room_id=$3), false))
           ON CONFLICT (hotel_id, date, room_id, channel_id) DO UPDATE
             SET payment_collected_cents = daily_revenue.payment_collected_cents + $5
           "#,
    )
    .bind(auth.hotel_id)
    .bind(day)
    .bind(res.room_id)
    .bind(res.channel_id)
    .bind(body.amount_cents)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("daily_revenue payment: {e}")))?;

    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "payment", pid, "insert",
        serde_json::json!({"amount_cents": body.amount_cents, "payment_type": body.payment_type})).await?;
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "payment", pid, "created", None,
        serde_json::to_value(&body).ok()).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: PaymentDto = sqlx::query_as(
        "SELECT id, reservation_id, amount_cents, currency, payment_type, reference, is_reversal, created_by, created_at FROM payments WHERE id = $1",
    )
    .bind(pid).fetch_one(&state.db).await.map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}

async fn reverse_payment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<PaymentDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::PaymentRefund).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "payment_refund required".into()));
    }
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let original: PaymentDto = sqlx::query_as(
        "SELECT id, reservation_id, amount_cents, currency, payment_type, reference, is_reversal, created_by, created_at FROM payments WHERE id = $1 AND hotel_id = $2",
    )
    .bind(id).bind(auth.hotel_id)
    .fetch_one(&mut *conn).await
    .map_err(|_| ApiError(StatusCode::NOT_FOUND, "payment not found".into()))?;
    if original.is_reversal {
        return Err(ApiError(StatusCode::BAD_REQUEST, "cannot reverse a reversal".into()));
    }

    // Corrections = reversal entry + never edits (README Ã‚Â§7)
    let rid = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let day = chrono::Utc::now().date_naive();
    sqlx::query(
        r#"INSERT INTO payments (id, hotel_id, reservation_id, amount_cents, currency, payment_type, reference, is_reversal, created_by)
           VALUES ($1,$2,$3, -$4, $5, 'refund', $6, TRUE, $7)"#,
    )
    .bind(rid)
    .bind(auth.hotel_id)
    .bind(original.reservation_id)
    .bind(original.amount_cents)
    .bind(&original.currency)
    .bind(original.reference.clone())
    .bind(auth.user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("reversal insert: {e}")))?;
    sqlx::query(
        r#"INSERT INTO daily_revenue (hotel_id, date, room_id, room_type_id, channel_id, payment_collected_cents)
           VALUES ($1, $2, $3, (SELECT room_type_id FROM rooms WHERE id = $3), $4, 0)
           ON CONFLICT (hotel_id, date, room_id, channel_id) DO UPDATE
             SET payment_collected_cents = daily_revenue.payment_collected_cents - $5"#,
    )
    .bind(auth.hotel_id)
    .bind(day)
    .bind(reservation_room(&mut *conn, original.reservation_id).await)
    .bind(reservation_channel(&mut *conn, original.reservation_id).await)
    .bind(original.amount_cents)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("daily_revenue reversal: {e}")))?;

    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "payment", rid, "insert",
        serde_json::json!({"reversal_of": id})).await?;
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "payment", rid, "reversed", None,
        Some(serde_json::json!({"reversal_of": id}))).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: PaymentDto = sqlx::query_as(
        "SELECT id, reservation_id, amount_cents, currency, payment_type, reference, is_reversal, created_by, created_at FROM payments WHERE id = $1",
    )
    .bind(rid).fetch_one(&state.db).await.map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}

async fn reservation_room(conn: &mut sqlx::PgConnection, res_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT room_id FROM reservations WHERE id = $1").bind(res_id).fetch_one(conn).await.unwrap_or_default()
}
async fn reservation_channel(conn: &mut sqlx::PgConnection, res_id: Uuid) -> Uuid {
    sqlx::query_scalar("SELECT channel_id FROM reservations WHERE id = $1").bind(res_id).fetch_one(conn).await.unwrap_or_default()
}

/// Ensure daily_revenue rows exist for a reservation (used when folio arrives before revenue rows).
pub async fn seed_revenue_rows(
    conn: &mut sqlx::PgConnection,
    hotel_id: i64,
    room_id: Uuid,
    channel_id: Uuid,
    reservation_id: Uuid,
) {
    let _ = sqlx::query(
        r#"INSERT INTO daily_revenue (hotel_id, date, room_id, room_type_id, channel_id, occupied)
           SELECT $1, d.day, $2, (SELECT room_type_id FROM rooms WHERE id = $2), $3, false
           FROM reservations res,
                generate_series(res.checkin_date, res.checkout_date - INTERVAL '1 day', INTERVAL '1 day') AS d(day)
           WHERE res.id = $4 AND res.hotel_id = $1
           ON CONFLICT DO NOTHING"#,
    )
    .bind(hotel_id).bind(room_id).bind(channel_id).bind(reservation_id)
    .execute(&mut *conn)
    .await;
}

impl From<sqlx::Error> for ApiError {
    fn from(e: sqlx::Error) -> Self {
        ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("db: {e}"))
    }
}

