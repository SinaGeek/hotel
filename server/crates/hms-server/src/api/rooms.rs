//! Room types & rooms + cleanliness/housekeeping endpoints (README Â§12)
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{
    routing::{get, patch},
    Json, Router,
};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn room_types() -> Router<AppState> {
    Router::new()
        .route("/", get(list_types).post(create_type))
        .route("/{id}", axum::routing::patch(patch_type))
}

pub fn rooms() -> Router<AppState> {
    Router::new()
        .route("/", get(list_rooms).post(create_room))
        .route("/{id}/cleanliness", patch(cleanliness))
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomTypeDto {
    pub id: Uuid,
    pub name: String,
    pub capacity: i32,
    pub base_price_cents: i64,
    pub amenities: Vec<String>,
}

async fn list_types(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AResult<Json<ApiResponse<Vec<RoomTypeDto>>>> {
    if let Some(cached) =
        state
            .cache
            .get(auth.hotel_id, crate::cache::CacheDomain::RoomTypes, "all")
    {
        if let Ok(data) = serde_json::from_value::<Vec<RoomTypeDto>>(cached) {
            return Ok(Json(ApiResponse {
                success: true,
                data,
                error: None,
            }));
        }
    }
    let rows: Vec<RoomTypeDto> = sqlx::query_as(
        "SELECT id, name, capacity, base_price_cents, amenities FROM room_types WHERE hotel_id = $1 AND deleted_at IS NULL ORDER BY name",
    )
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    if let Ok(val) = serde_json::to_value(&rows) {
        state.cache.set(
            auth.hotel_id,
            crate::cache::CacheDomain::RoomTypes,
            "all",
            val,
            std::time::Duration::from_secs(60),
        );
    }
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}

#[derive(Deserialize, Serialize)]
struct CreateRoomTypeBody {
    name: String,
    capacity: i32,
    base_price_cents: i64,
    amenities: Option<Vec<String>>,
}

async fn create_type(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateRoomTypeBody>,
) -> AResult<Json<ApiResponse<RoomTypeDto>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::RoomTypeManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "room_type_manage required".into(),
        ));
    }
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query("INSERT INTO room_types (id, hotel_id, name, capacity, base_price_cents, amenities) VALUES ($1,$2,$3,$4,$5,$6)")
        .bind(id)
        .bind(auth.hotel_id)
        .bind(&body.name)
        .bind(body.capacity)
        .bind(body.base_price_cents)
        .bind(body.amenities.clone().unwrap_or_default())
        .execute(&mut *conn)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create room type: {e}")))?;
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "room_type",
        id,
        "insert",
        serde_json::json!(&body),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "room_type",
        id,
        "created",
        None,
        Some(serde_json::json!(&body)),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: RoomTypeDto = sqlx::query_as(
        "SELECT id, name, capacity, base_price_cents, amenities FROM room_types WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::RoomTypes);
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::Board);
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize, Serialize)]
struct PatchRoomTypeBody {
    name: Option<String>,
    capacity: Option<i32>,
    base_price_cents: Option<i64>,
    amenities: Option<Vec<String>>,
}

async fn patch_type(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<PatchRoomTypeBody>,
) -> AResult<Json<ApiResponse<RoomTypeDto>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::RoomTypeManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "room_type_manage required".into(),
        ));
    }
    let old: Option<RoomTypeDto> = sqlx::query_as("SELECT id, name, capacity, base_price_cents, amenities FROM room_types WHERE id = $1 AND hotel_id = $2")
        .bind(id).bind(auth.hotel_id)
        .fetch_optional(&state.db).await.map_err(ApiError::from)?;
    let old = old.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "room type not found".into()))?;

    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        r#"UPDATE room_types SET
             name = COALESCE($3, name),
             capacity = COALESCE($4, capacity),
             base_price_cents = COALESCE($5, base_price_cents),
             amenities = COALESCE($6, amenities),
             updated_at = now()
           WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(body.name.clone())
    .bind(body.capacity)
    .bind(body.base_price_cents)
    .bind(body.amenities.clone())
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "room_type",
        id,
        "update",
        serde_json::json!(&body),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "room_type",
        id,
        "updated",
        Some(serde_json::to_value(&old).unwrap_or_default()),
        Some(serde_json::json!(&body)),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: RoomTypeDto = sqlx::query_as(
        "SELECT id, name, capacity, base_price_cents, amenities FROM room_types WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::RoomTypes);
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::Board);
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Serialize, Deserialize, sqlx::FromRow)]
pub struct RoomDto {
    pub id: Uuid,
    pub room_type_id: Uuid,
    pub room_number: String,
    pub floor: i32,
    pub cleanliness: String,
}

async fn list_rooms(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AResult<Json<ApiResponse<Vec<RoomDto>>>> {
    if let Some(cached) = state
        .cache
        .get(auth.hotel_id, crate::cache::CacheDomain::Rooms, "all")
    {
        if let Ok(data) = serde_json::from_value::<Vec<RoomDto>>(cached) {
            return Ok(Json(ApiResponse {
                success: true,
                data,
                error: None,
            }));
        }
    }
    let rows: Vec<RoomDto> = sqlx::query_as(
        "SELECT id, room_type_id, room_number, floor, cleanliness FROM rooms WHERE hotel_id = $1 AND deleted_at IS NULL ORDER BY room_number",
    )
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    if let Ok(val) = serde_json::to_value(&rows) {
        state.cache.set(
            auth.hotel_id,
            crate::cache::CacheDomain::Rooms,
            "all",
            val,
            std::time::Duration::from_secs(60),
        );
    }
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}

#[derive(Deserialize, Serialize)]
struct CreateRoomBody {
    room_type_id: Uuid,
    room_number: String,
    floor: Option<i32>,
}

async fn create_room(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateRoomBody>,
) -> AResult<Json<ApiResponse<RoomDto>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::RoomManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "room_manage required".into(),
        ));
    }
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query("INSERT INTO rooms (id, hotel_id, room_type_id, room_number, floor) VALUES ($1,$2,$3,$4,$5)")
        .bind(id)
        .bind(auth.hotel_id)
        .bind(body.room_type_id)
        .bind(&body.room_number)
        .bind(body.floor.unwrap_or(0))
        .execute(&mut *conn)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create room: {e}")))?;
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "room",
        id,
        "insert",
        serde_json::json!(&body),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "room",
        id,
        "created",
        None,
        Some(serde_json::json!(&body)),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: RoomDto = sqlx::query_as(
        "SELECT id, room_type_id, room_number, floor, cleanliness FROM rooms WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::Rooms);
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::Board);
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize, Serialize)]
struct CleanlinessBody {
    cleanliness: String,
    notes: Option<String>,
}

/// Housekeeping update-model: last write wins (README Â§11)
async fn cleanliness(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CleanlinessBody>,
) -> AResult<Json<ApiResponse<RoomDto>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::HousekeepingUpdateStatus,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "housekeeping_update_status required".into(),
        ));
    }
    if !["dirty", "cleaning", "clean", "inspected", "out_of_order"]
        .contains(&body.cleanliness.as_str())
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "invalid cleanliness value".into(),
        ));
    }
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let old: Option<RoomDto> = sqlx::query_as("SELECT id, room_type_id, room_number, floor, cleanliness FROM rooms WHERE id = $1 AND hotel_id = $2")
        .bind(id).bind(auth.hotel_id).fetch_optional(&mut *conn).await.map_err(ApiError::from)?;
    let old = old.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "room not found".into()))?;
    sqlx::query(
        "UPDATE rooms SET cleanliness = $3, updated_at = now() WHERE id = $1 AND hotel_id = $2",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.cleanliness)
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "room_status",
        id,
        "update",
        serde_json::json!({"cleanliness": body.cleanliness}),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "room",
        id,
        "cleanliness_updated",
        Some(serde_json::json!({"cleanliness": old.cleanliness})),
        Some(serde_json::json!({"cleanliness": body.cleanliness, "notes": body.notes})),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: RoomDto = sqlx::query_as(
        "SELECT id, room_type_id, room_number, floor, cleanliness FROM rooms WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::Rooms);
    state
        .cache
        .invalidate(auth.hotel_id, crate::cache::CacheDomain::Board);
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}
