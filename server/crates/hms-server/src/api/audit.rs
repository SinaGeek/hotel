//! Audit log read endpoint
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use chrono::{DateTime, Utc};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list))
}

#[derive(Deserialize)]
struct AuditQuery {
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
struct AuditLogDto {
    id: Uuid,
    hotel_id: i64,
    user_id: Option<Uuid>,
    entity_type: String,
    entity_id: Uuid,
    action: String,
    old_value: Option<serde_json::Value>,
    new_value: Option<serde_json::Value>,
    timestamp: DateTime<Utc>,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<AuditQuery>,
) -> AResult<Json<ApiResponse<Vec<AuditLogDto>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        hms_core::roles::Permission::AuditLogView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "audit_log_view required".into(),
        ));
    }
    let limit = q.limit.unwrap_or(100).clamp(1, 500);
    let rows: Vec<AuditLogDto> = sqlx::query_as(
        r#"SELECT id, hotel_id, user_id, entity_type, entity_id, action, old_value, new_value, timestamp
           FROM audit_log
           WHERE hotel_id = $1
           ORDER BY timestamp DESC
           LIMIT $2"#,
    )
    .bind(auth.hotel_id)
    .bind(limit)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}
