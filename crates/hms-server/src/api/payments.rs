//! Folio transactions per reservation (Room / Minibar / Spa / Restaurant / Extra â€” README Â§7)
use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn folio_router() -> Router<AppState> {
    Router::new().route("/{id}/folio", get(list_folio).post(add_folio))
}

#[derive(Serialize, sqlx::FromRow)]
struct FolioDto {
    id: Uuid,
    reservation_id: Uuid,
    category: String,
    description: String,
    unit_price_cents: i64,
    quantity: i32,
    total_cents: i64,
    currency: String,
    created_at: chrono::DateTime<chrono::Utc>,
}

async fn list_folio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<Vec<FolioDto>>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::FolioManage).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "folio_manage required".into()));
    }
    let rows: Vec<FolioDto> = sqlx::query_as(
        r#"SELECT id, reservation_id, category, description, unit_price_cents, quantity, total_cents, currency, created_at
           FROM folio_transactions WHERE reservation_id = $1 AND hotel_id = $2 AND deleted_at IS NULL ORDER BY created_at"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

#[derive(Deserialize, Serialize)]
struct AddFolioBody {
    category: String,
    description: String,
    unit_price_cents: i64,
    quantity: i32,
    currency: Option<String>,
}

async fn add_folio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AddFolioBody>,
) -> AResult<Json<ApiResponse<FolioDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::FolioManage).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "folio_manage required".into()));
    }
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let res_room: (Uuid, Uuid) = sqlx::query_as("SELECT room_id, channel_id FROM reservations WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL")
        .bind(id)
        .bind(auth.hotel_id)
        .fetch_one(&mut *conn)
        .await
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "reservation not found".into()))?;
    let total = body.unit_price_cents * (body.quantity as i64).max(1);

    let fid = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let currency = body.currency.clone().unwrap_or_else(|| "TRY".into());
    sqlx::query(
        r#"INSERT INTO folio_transactions (id, hotel_id, reservation_id, category, description, unit_price_cents, quantity, total_cents, currency)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)"#,
    )
    .bind(fid)
    .bind(auth.hotel_id)
    .bind(id)
    .bind(&body.category)
    .bind(&body.description)
    .bind(body.unit_price_cents)
    .bind(body.quantity)
    .bind(total)
    .bind(&currency)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("folio insert: {e}")))?;

    // extra revenue joins the stay dates' daily_revenue rows (README Â§10)
    sqlx::query(
        r#"UPDATE daily_revenue dr
           SET extra_revenue_cents = dr.extra_revenue_cents + $3::bigint,
               updated_at = now()
           FROM reservations res
           WHERE dr.hotel_id = $1 AND dr.room_id = res.room_id AND res.id = $2
             AND dr.date >= res.checkin_date AND dr.date < res.checkout_date"#,
    )
    .bind(auth.hotel_id)
    .bind(id)
    .bind(total)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, format!("folio revenue: {e}")))?;
    // ensure the aggregation rows exist when none did yet
    crate::api::reservations::seed_revenue_rows(&mut conn, auth.hotel_id, res_room.0, res_room.1, id).await;

    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "folio_transaction", fid, "insert",
        serde_json::json!(&body)).await?;
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "folio_transaction", fid, "created", None, serde_json::json!(&body).into()).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: FolioDto = sqlx::query_as(
        "SELECT id, reservation_id, category, description, unit_price_cents, quantity, total_cents, currency, created_at FROM folio_transactions WHERE id = $1",
    )
    .bind(fid)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}
