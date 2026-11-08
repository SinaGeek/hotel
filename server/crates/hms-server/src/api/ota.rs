// OTA Channel Manager Integration API
// Manages Booking.com and Expedia connectivity, credentials, room mappings, and sync operations

use crate::auth::AuthUser;
use crate::db;
use crate::{AResult, ApiError, AppState};
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

pub fn ota_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_configs).post(create_config))
        .route(
            "/:id",
            get(get_config).put(update_config).delete(delete_config),
        )
        .route("/:id/test", post(test_connection))
        .route("/:id/sync", post(manual_sync))
        .route(
            "/:id/room-mappings",
            get(list_room_mappings).put(save_room_mappings),
        )
        .route("/:id/sync-log", get(sync_log))
}

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct OTAConfigDto {
    pub id: Uuid,
    pub hotel_id: i64,
    pub provider: String,
    pub enabled: bool,
    pub hotel_code: Option<String>,
    pub base_url: Option<String>,
    pub last_sync_at: Option<chrono::DateTime<chrono::Utc>>,
    pub last_success_at: Option<chrono::DateTime<chrono::Utc>>,
    pub status: String,
    pub error_message: Option<String>,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub updated_at: chrono::DateTime<chrono::Utc>,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct CreateOTAConfigBody {
    pub provider: String, // 'booking_com' | 'expedia'
    pub enabled: bool,
    pub credentials: OTACredentials,
    pub hotel_code: String,
}

#[derive(Debug, Deserialize, Serialize)]
pub struct UpdateOTAConfigBody {
    pub enabled: Option<bool>,
    pub credentials: Option<OTACredentials>,
    pub hotel_code: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "type")]
pub enum OTACredentials {
    #[serde(rename = "booking_com")]
    BookingCom { username: String, password: String },
    #[serde(rename = "expedia")]
    Expedia { api_key: String, api_secret: String },
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OTARoomMappingDto {
    pub id: Uuid,
    pub ota_config_id: Uuid,
    pub local_room_type_id: Uuid,
    pub ota_room_id: String,
    pub ota_rate_plan_id: Option<String>,
    pub is_active: bool,
}

#[derive(Debug, Deserialize)]
pub struct SaveRoomMappingsBody {
    pub mappings: Vec<RoomMappingInput>,
}

#[derive(Debug, Deserialize)]
pub struct RoomMappingInput {
    pub local_room_type_id: Uuid,
    pub ota_room_id: String,
    pub ota_rate_plan_id: Option<String>,
}

#[derive(Debug, Serialize, sqlx::FromRow)]
pub struct OTASyncLogDto {
    pub id: Uuid,
    pub direction: String,
    pub entity_type: String,
    pub operation: String,
    pub external_id: Option<String>,
    pub local_id: Option<Uuid>,
    pub status: String,
    pub error_message: Option<String>,
    pub synced_at: chrono::DateTime<chrono::Utc>,
}

// List all OTA configs for the current hotel
async fn list_configs(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AResult<Json<ApiResponse<Vec<OTAConfigDto>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_view required".into(),
        ));
    }

    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let configs: Vec<OTAConfigDto> = sqlx::query_as(
        "SELECT id, hotel_id, provider, enabled, hotel_code, base_url, 
                last_sync_at, last_success_at, status, error_message, created_at, updated_at
         FROM ota_configs
         WHERE hotel_id = $1
         ORDER BY provider",
    )
    .bind(auth.hotel_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: configs,
        error: None,
    }))
}

// Create new OTA config
async fn create_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateOTAConfigBody>,
) -> AResult<Json<ApiResponse<OTAConfigDto>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_manage required".into(),
        ));
    }

    if !["booking_com", "expedia"].contains(&body.provider.as_str()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid provider".into()));
    }

    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    // Encrypt credentials (simple approach: we'll use pgcrypto in production)
    let credentials_json = serde_json::to_string(&body.credentials)
        .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
    let credentials_encrypted = credentials_json.as_bytes(); // TODO: proper encryption

    let base_url = match body.provider.as_str() {
        "booking_com" => Some("https://supply-xml.booking.com".to_string()),
        "expedia" => Some("https://api.expediagroup.com/supply/lodging".to_string()),
        _ => None,
    };

    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO ota_configs (id, hotel_id, provider, enabled, credentials_encrypted, hotel_code, base_url)
         VALUES ($1, $2, $3, $4, $5, $6, $7)"
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.provider)
    .bind(body.enabled)
    .bind(credentials_encrypted)
    .bind(&body.hotel_code)
    .bind(base_url)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        if e.to_string().contains("unique") {
            ApiError(StatusCode::CONFLICT, format!("{} already configured", body.provider))
        } else {
            ApiError::from(e)
        }
    })?;

    conn.commit().await.map_err(ApiError::from)?;

    let config: OTAConfigDto = sqlx::query_as(
        "SELECT id, hotel_id, provider, enabled, hotel_code, base_url,
                last_sync_at, last_success_at, status, error_message, created_at, updated_at
         FROM ota_configs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: config,
        error: None,
    }))
}

// Get single config
async fn get_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<OTAConfigDto>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_view required".into(),
        ));
    }

    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let config: OTAConfigDto = sqlx::query_as(
        "SELECT id, hotel_id, provider, enabled, hotel_code, base_url,
                last_sync_at, last_success_at, status, error_message, created_at, updated_at
         FROM ota_configs WHERE id = $1 AND hotel_id = $2",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "config not found".into()))?;

    Ok(Json(ApiResponse {
        success: true,
        data: config,
        error: None,
    }))
}

// Update config
async fn update_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateOTAConfigBody>,
) -> AResult<Json<ApiResponse<OTAConfigDto>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_manage required".into(),
        ));
    }

    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    // Verify ownership
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ota_configs WHERE id = $1 AND hotel_id = $2)",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    if !exists {
        return Err(ApiError(StatusCode::NOT_FOUND, "config not found".into()));
    }

    if let Some(enabled) = body.enabled {
        sqlx::query("UPDATE ota_configs SET enabled = $1, updated_at = now() WHERE id = $2")
            .bind(enabled)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(ApiError::from)?;
    }

    if let Some(credentials) = body.credentials {
        let credentials_json = serde_json::to_string(&credentials)
            .map_err(|e| ApiError(StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        let credentials_encrypted = credentials_json.as_bytes();

        sqlx::query(
            "UPDATE ota_configs SET credentials_encrypted = $1, updated_at = now() WHERE id = $2",
        )
        .bind(credentials_encrypted)
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(ApiError::from)?;
    }

    if let Some(hotel_code) = body.hotel_code {
        sqlx::query("UPDATE ota_configs SET hotel_code = $1, updated_at = now() WHERE id = $2")
            .bind(&hotel_code)
            .bind(id)
            .execute(&mut *conn)
            .await
            .map_err(ApiError::from)?;
    }

    conn.commit().await.map_err(ApiError::from)?;

    let config: OTAConfigDto = sqlx::query_as(
        "SELECT id, hotel_id, provider, enabled, hotel_code, base_url,
                last_sync_at, last_success_at, status, error_message, created_at, updated_at
         FROM ota_configs WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: config,
        error: None,
    }))
}

// Delete config
async fn delete_config(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<()>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_manage required".into(),
        ));
    }

    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let result = sqlx::query("DELETE FROM ota_configs WHERE id = $1 AND hotel_id = $2")
        .bind(id)
        .bind(auth.hotel_id)
        .execute(&mut *conn)
        .await
        .map_err(ApiError::from)?;

    if result.rows_affected() == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "config not found".into()));
    }

    conn.commit().await.map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: (),
        error: None,
    }))
}

// Test connection (placeholder - real implementation calls OTA APIs)
async fn test_connection(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<String>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_manage required".into(),
        ));
    }

    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let config: OTAConfigDto = sqlx::query_as(
        "SELECT id, hotel_id, provider, enabled, hotel_code, base_url,
                last_sync_at, last_success_at, status, error_message, created_at, updated_at
         FROM ota_configs WHERE id = $1 AND hotel_id = $2",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "config not found".into()))?;

    // TODO: Real connection test
    // For now, just update status to 'connected'
    sqlx::query("UPDATE ota_configs SET status = 'connected', error_message = NULL, updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: format!("{} connection test successful (demo)", config.provider),
        error: None,
    }))
}

// Manual sync trigger
async fn manual_sync(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<String>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_manage required".into(),
        ));
    }

    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let config: OTAConfigDto = sqlx::query_as(
        "SELECT id, hotel_id, provider, enabled, hotel_code, base_url,
                last_sync_at, last_success_at, status, error_message, created_at, updated_at
         FROM ota_configs WHERE id = $1 AND hotel_id = $2",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_optional(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "config not found".into()))?;

    if !config.enabled {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "config not enabled".into(),
        ));
    }

    // TODO: Actual sync implementation (pull reservations, push rates/availability)
    // For now, update timestamps
    sqlx::query("UPDATE ota_configs SET last_sync_at = now(), last_success_at = now(), status = 'connected', updated_at = now() WHERE id = $1")
        .bind(id)
        .execute(&state.db)
        .await
        .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: format!("{} manual sync completed (demo)", config.provider),
        error: None,
    }))
}

// List room mappings
async fn list_room_mappings(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<Vec<OTARoomMappingDto>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_view required".into(),
        ));
    }

    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let mappings: Vec<OTARoomMappingDto> = sqlx::query_as(
        "SELECT id, ota_config_id, local_room_type_id, ota_room_id, ota_rate_plan_id, is_active
         FROM ota_room_mappings
         WHERE ota_config_id = $1
         ORDER BY local_room_type_id",
    )
    .bind(id)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: mappings,
        error: None,
    }))
}

// Save room mappings (replaces all)
async fn save_room_mappings(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<SaveRoomMappingsBody>,
) -> AResult<Json<ApiResponse<Vec<OTARoomMappingDto>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_manage required".into(),
        ));
    }

    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    // Verify config ownership
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM ota_configs WHERE id = $1 AND hotel_id = $2)",
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    if !exists {
        return Err(ApiError(StatusCode::NOT_FOUND, "config not found".into()));
    }

    // Delete existing mappings
    sqlx::query("DELETE FROM ota_room_mappings WHERE ota_config_id = $1")
        .bind(id)
        .execute(&mut *conn)
        .await
        .map_err(ApiError::from)?;

    // Insert new mappings
    for mapping in &body.mappings {
        sqlx::query(
            "INSERT INTO ota_room_mappings (id, ota_config_id, local_room_type_id, ota_room_id, ota_rate_plan_id)
             VALUES ($1, $2, $3, $4, $5)"
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(mapping.local_room_type_id)
        .bind(&mapping.ota_room_id)
        .bind(&mapping.ota_rate_plan_id)
        .execute(&mut *conn)
        .await
        .map_err(ApiError::from)?;
    }

    conn.commit().await.map_err(ApiError::from)?;

    // Return updated mappings
    let mappings: Vec<OTARoomMappingDto> = sqlx::query_as(
        "SELECT id, ota_config_id, local_room_type_id, ota_room_id, ota_rate_plan_id, is_active
         FROM ota_room_mappings
         WHERE ota_config_id = $1
         ORDER BY local_room_type_id",
    )
    .bind(id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: mappings,
        error: None,
    }))
}

// Get sync log
async fn sync_log(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<Vec<OTASyncLogDto>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::ChannelView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "channel_view required".into(),
        ));
    }

    let mut conn = state.db.acquire().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;

    let logs: Vec<OTASyncLogDto> = sqlx::query_as(
        "SELECT id, direction, entity_type, operation, external_id, local_id, status, error_message, synced_at
         FROM ota_sync_log
         WHERE ota_config_id = $1 AND hotel_id = $2
         ORDER BY synced_at DESC
         LIMIT 100"
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_all(&mut *conn)
    .await
    .map_err(ApiError::from)?;

    Ok(Json(ApiResponse {
        success: true,
        data: logs,
        error: None,
    }))
}
