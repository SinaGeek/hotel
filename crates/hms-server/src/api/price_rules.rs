//! Price rules: seasonal/special pricing per room type (README Â§4, Â§7)
use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list).post(create))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct PriceRuleDto {
    pub id: Uuid,
    pub room_type_id: Uuid,
    pub date_from: chrono::NaiveDate,
    pub date_to: chrono::NaiveDate,
    pub price_cents: i64,
    pub currency: String,
}

async fn list(State(state): State<AppState>, auth: AuthUser) -> AResult<Json<ApiResponse<Vec<PriceRuleDto>>>> {
    let rows: Vec<PriceRuleDto> = sqlx::query_as(
        "SELECT id, room_type_id, date_from, date_to, price_cents, currency FROM price_rules WHERE hotel_id = $1 AND deleted_at IS NULL ORDER BY date_from",
    )
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

#[derive(Deserialize, Serialize)]
struct CreateBody {
    room_type_id: Uuid,
    date_from: chrono::NaiveDate,
    date_to: chrono::NaiveDate,
    price_cents: i64,
    currency: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateBody>,
) -> AResult<Json<ApiResponse<PriceRuleDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::PriceRuleManage).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "price_rule_manage required".into()));
    }
    hms_core::validation::validate_price_rule(body.date_from, body.date_to, body.price_cents)
        .map_err(ApiError::from)?;
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        "INSERT INTO price_rules (id, hotel_id, room_type_id, date_from, date_to, price_cents, currency, created_by) VALUES ($1,$2,$3,$4,$5,$6,COALESCE($7,'TRY'),$8)",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(body.room_type_id)
    .bind(body.date_from)
    .bind(body.date_to)
    .bind(body.price_cents)
    .bind(body.currency.clone())
    .bind(auth.user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create price rule: {e}")))?;
    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "price_rule", id, "insert", serde_json::json!(&body)).await?;
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "price_rule", id, "created", None, Some(serde_json::json!(&body))).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: PriceRuleDto = sqlx::query_as("SELECT id, room_type_id, date_from, date_to, price_cents, currency FROM price_rules WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}
