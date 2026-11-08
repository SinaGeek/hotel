//! Roles + permission matrix (admin only: view matrix; edit via PUT per role)
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list))
        .route("/{role}/permissions", get(matrix).put(set_permissions))
}

/// Full matrix (allowed or not) for every permission of a given set of roles.
#[derive(serde::Serialize)]
pub struct RoleMatrix {
    pub role: String,
    pub permissions: Vec<PermissionState>,
}

#[derive(serde::Serialize)]
pub struct PermissionState {
    pub permission: String,
    pub default_allowed: bool,
    hotel_override: Option<bool>,
    pub effective: bool,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AResult<Json<ApiResponse<Vec<RoleMatrix>>>> {
    let mut matrices: Vec<RoleMatrix> = Vec::new();
    if auth.role.contains("admin") {
        for role in ["admin", "manager", "reception", "housekeeping"] {
            matrices.push(matrix_for(&state, role, auth.hotel_id).await?);
        }
    } else {
        matrices.push(matrix_for(&state, &auth.role, auth.hotel_id).await?);
    }
    Ok(Json(ApiResponse {
        success: true,
        data: matrices,
        error: None,
    }))
}

async fn matrix(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(role): Path<String>,
) -> AResult<Json<ApiResponse<RoleMatrix>>> {
    Ok(Json(ApiResponse {
        success: true,
        data: matrix_for(&state, &role, auth.hotel_id).await?,
        error: None,
    }))
}

async fn matrix_for(state: &AppState, role: &str, hotel_id: i64) -> AResult<RoleMatrix> {
    let base: Vec<(String, bool)> =
        sqlx::query_as("SELECT permission, allowed FROM role_permissions WHERE role = $1")
            .bind(role)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?;
    let overrides: Vec<(String, bool)> =
        sqlx::query_as("SELECT permission, allowed FROM hotel_permission_overrides WHERE hotel_id = $1 AND role = $2")
            .bind(hotel_id)
            .bind(role)
            .fetch_all(&state.db)
            .await
            .map_err(ApiError::from)?;

    let permissions = hms_core::roles::PERMISSIONS
        .iter()
        .map(|p| {
            let key = p.as_str().to_string();
            let default_allowed = base.iter().any(|(k, v)| *k == key && *v);
            let hotel_override = overrides.iter().find(|(k, _)| *k == key).map(|(_, v)| *v);
            let effective = hotel_override.unwrap_or(default_allowed);
            PermissionState {
                permission: key,
                default_allowed,
                hotel_override,
                effective,
            }
        })
        .collect();
    Ok(RoleMatrix {
        role: role.to_string(),
        permissions,
    })
}

#[derive(Deserialize, Serialize)]
struct PutPermissionsBody {
    /// which hotel's overrides to write (admin can be hotel 0 in his session)
    hotel_id: i64,
    /// permission -> effective allowed (writes an override row per hotel)
    values: Vec<PermissionSetValue>,
}

#[derive(Deserialize, Serialize)]
struct PermissionSetValue {
    permission: String,
    allowed: bool,
}

async fn set_permissions(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(role): Path<String>,
    Json(body): Json<PutPermissionsBody>,
) -> AResult<Json<ApiResponse<()>>> {
    if !auth.role.contains("admin") {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "only platform admin defines role abilities".into(),
        ));
    }
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    crate::db::set_platform_scope(&mut conn).await?;
    for v in &body.values {
        if v.permission == "report_view_tenant" {
            // per-hotel configurable report access (README Â§6)
            sqlx::query(
                r#"INSERT INTO hotel_permission_overrides (hotel_id, role, permission, allowed)
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT (hotel_id, role, permission) DO UPDATE SET allowed = EXCLUDED.allowed"#,
            )
            .bind(body.hotel_id)
            .bind(&role)
            .bind(&v.permission)
            .bind(v.allowed)
            .execute(&mut *conn)
            .await
            .map_err(ApiError::from)?;
        } else {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "only per-hotel configurable permissions (report_view_tenant today) can be edited"
                    .into(),
            ));
        }
    }
    conn.commit().await.map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: (),
        error: None,
    }))
}
