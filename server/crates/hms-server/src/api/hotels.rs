//! Hotels CRUD (platform admin only, README Â§12)
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::State;
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(by_id).patch(update))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct HotelDto {
    pub id: i64,
    pub name: String,
    pub default_currency: String,
    pub time_zone: String,
    pub is_active: bool,
    pub next_change_seq: i64,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AResult<Json<ApiResponse<Vec<HotelDto>>>> {
    if !auth.role.contains("admin") {
        // tenant users see only their own hotel
        let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
        db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
        let rows: Vec<HotelDto> = sqlx::query_as("SELECT id, name, default_currency, time_zone, is_active,
       COALESCE((SELECT max(change_seq) FROM change_log WHERE hotel_id = hotels.id), 0) AS next_change_seq
       FROM hotels WHERE deleted_at IS NULL").fetch_all(&mut *conn).await.map_err(ApiError::from)?;
        return Ok(Json(ApiResponse {
            success: true,
            data: rows,
            error: None,
        }));
    }
    let rows: Vec<HotelDto> = sqlx::query_as("SELECT id, name, default_currency, time_zone, is_active,
       COALESCE((SELECT max(change_seq) FROM change_log WHERE hotel_id = hotels.id), 0) AS next_change_seq
       FROM hotels WHERE deleted_at IS NULL ORDER BY id")
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}

async fn by_id(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<i64>,
) -> AResult<Json<ApiResponse<HotelDto>>> {
    if !auth.role.contains("admin") && auth.hotel_id != id {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "cannot view another hotel".into(),
        ));
    }
    if auth.role.contains("admin") {
        let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
        db::set_platform_scope(&mut conn).await?;
        let row: HotelDto = sqlx::query_as("SELECT id, name, default_currency, time_zone, is_active,
       COALESCE((SELECT max(change_seq) FROM change_log WHERE hotel_id = hotels.id), 0) AS next_change_seq
       FROM hotels WHERE id = $1 AND deleted_at IS NULL")
            .bind(id)
            .fetch_one(&mut *conn)
            .await
            .map_err(|_| ApiError(StatusCode::NOT_FOUND, "hotel not found".into()))?;
        return Ok(Json(ApiResponse {
            success: true,
            data: row,
            error: None,
        }));
    }
    let row: HotelDto = sqlx::query_as("SELECT id, name, default_currency, time_zone, is_active,
       COALESCE((SELECT max(change_seq) FROM change_log WHERE hotel_id = hotels.id), 0) AS next_change_seq
       FROM hotels WHERE id = $1 AND deleted_at IS NULL")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(|_| ApiError(StatusCode::NOT_FOUND, "hotel not found".into()))?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize, Serialize)]
struct CreateHotelBody {
    id: i64,
    name: String,
    default_currency: Option<String>,
    time_zone: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateHotelBody>,
) -> AResult<Json<ApiResponse<HotelDto>>> {
    if !auth.role.contains("admin") {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "only platform admin can create hotels".into(),
        ));
    }
    let op_id = uuid::Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_platform_scope(&mut conn).await?;
    sqlx::query("INSERT INTO hotels (id, name, default_currency, time_zone) VALUES ($1, $2, COALESCE($3,'TRY'), COALESCE($4,'Europe/Istanbul'))")
        .bind(body.id)
        .bind(&body.name)
        .bind(body.default_currency.clone())
        .bind(body.time_zone.clone())
        .execute(&mut *conn)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create hotel: {e}")))?;
    db::record_change(
        &mut conn,
        body.id,
        Some(auth.user_id),
        op_id,
        "hotel",
        uuid::Uuid::now_v7(),
        "insert",
        serde_json::json!({"name": body.name}),
    )
    .await?;
    sqlx::query("INSERT INTO audit_log (hotel_id, user_id, entity_type, entity_id, action, new_value) VALUES ($1, $2, 'hotel', '00000000-0000-0000-0000-000000000000'::uuid, 'created', $3)")
        .bind(body.id)
        .bind(auth.user_id)
        .bind(serde_json::json!({"name": body.name}))
        .execute(&mut *conn)
        .await
        .map_err(ApiError::from)?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: HotelDto = sqlx::query_as("SELECT id, name, default_currency, time_zone, is_active,
       COALESCE((SELECT max(change_seq) FROM change_log WHERE hotel_id = hotels.id), 0) AS next_change_seq
       FROM hotels WHERE id = $1")
        .bind(body.id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize, Serialize)]
struct UpdateHotelBody {
    name: Option<String>,
    default_currency: Option<String>,
    time_zone: Option<String>,
    is_active: Option<bool>,
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<i64>,
    Json(body): Json<UpdateHotelBody>,
) -> AResult<Json<ApiResponse<HotelDto>>> {
    if !auth.role.contains("admin") {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "only platform admin can edit hotels".into(),
        ));
    }
    let op_id = uuid::Uuid::now_v7();
    let old: Option<HotelDto> = sqlx::query_as("SELECT id, name, default_currency, time_zone, is_active,
       COALESCE((SELECT max(change_seq) FROM change_log WHERE hotel_id = hotels.id), 0) AS next_change_seq
       FROM hotels WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?;
    let old = old.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "hotel not found".into()))?;

    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_platform_scope(&mut conn).await?;
    sqlx::query(
        r#"UPDATE hotels SET
             name = COALESCE($2, name),
             default_currency = COALESCE($3, default_currency),
             time_zone = COALESCE($4, time_zone),
             is_active = COALESCE($5, is_active),
             updated_at = now()
           WHERE id = $1"#,
    )
    .bind(id)
    .bind(body.name.clone())
    .bind(body.default_currency.clone())
    .bind(body.time_zone.clone())
    .bind(body.is_active)
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    db::record_change(
        &mut conn,
        id,
        Some(auth.user_id),
        op_id,
        "hotel",
        uuid::Uuid::now_v7(),
        "update",
        serde_json::json!({"patch": body}),
    )
    .await?;
    db::write_audit(
        &mut conn,
        id,
        Some(auth.user_id),
        "hotel",
        uuid::Uuid::now_v7(),
        "updated",
        Some(serde_json::to_value(&old).unwrap_or_default()),
        Some(serde_json::to_value(&body).unwrap_or_default()),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;

    let row: HotelDto = sqlx::query_as("SELECT id, name, default_currency, time_zone, is_active,
       COALESCE((SELECT max(change_seq) FROM change_log WHERE hotel_id = hotels.id), 0) AS next_change_seq
       FROM hotels WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}
