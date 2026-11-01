//! Users CRUD (README Ã‚Â§6: manager can create accounts below manager level)
use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use argon2::password_hash::{rand_core::OsRng, SaltString};
use argon2::{Argon2, PasswordHasher};
use axum::extract::State;
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list).post(create)).route("/{id}", axum::routing::patch(patch))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct UserDto {
    pub id: Uuid,
    pub hotel_id: i64,
    pub username: String,
    pub full_name: String,
    pub email: Option<String>,
    pub role: String,
    pub is_active: bool,
    pub force_password_change: bool,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AResult<Json<ApiResponse<Vec<UserDto>>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::UserView).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "user_view required".into()));
    }
    // Platform admin sees every hotel (the admin panel needs this); tenant
    // callers are scoped to their own hotel.
    if auth.role.contains("admin") {
        let mut tx = state.db.begin().await.map_err(ApiError::from)?;
        db::set_platform_scope(&mut tx).await?;
        let users: Vec<UserDto> = sqlx::query_as(
            "SELECT id, hotel_id, username, full_name, email, role, is_active, force_password_change
             FROM users WHERE deleted_at IS NULL ORDER BY hotel_id, username",
        )
        .fetch_all(&mut *tx)
        .await
        .map_err(ApiError::from)?;
        tx.commit().await.map_err(ApiError::from)?;
        return Ok(Json(ApiResponse { success: true, data: users, error: None }));
    }
    let users: Vec<UserDto> = sqlx::query_as(
        "SELECT id, hotel_id, username, full_name, email, role, is_active, force_password_change
         FROM users WHERE hotel_id = $1 AND deleted_at IS NULL ORDER BY username",
    )
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: users, error: None }))
}

static ROLE_RANK: &[(&str, u8)] = &[("housekeeping", 1), ("reception", 2), ("manager", 3)];

fn rank(role: &str) -> u8 {
    ROLE_RANK.iter().find(|(r, _)| *r == role).map(|(_, v)| *v).unwrap_or(0)
}

#[derive(Deserialize, Serialize)]
struct CreateUserBody {
    /// only the platform admin may create users for another hotel
    hotel_id: Option<i64>,
    username: String,
    full_name: String,
    email: Option<String>,
    role: String,
    password: String,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateUserBody>,
) -> AResult<Json<ApiResponse<UserDto>>> {
    // permission: admin/manager via user_manage
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::UserManage).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "user_manage required".into()));
    }
    // managers can only create strictly below manager level
    if !auth.role.contains("admin") && rank(&body.role) >= rank(&auth.role) {
        return Err(ApiError(StatusCode::FORBIDDEN, "managers can only create accounts below manager level".into()));
    }
    // Initial passwords are forced to change on first login, so a short
    // hotel-provisioned password (e.g. `1234`) is allowed; real strength comes
    // from the forced rotation, not from the bootstrap value.
    if body.password.len() < 3 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "password too short (min 3)".into()));
    }
    let salt = SaltString::generate(&mut OsRng);
    let hash = Argon2::default().hash_password(body.password.as_bytes(), &salt)
        .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "hash failed".into()))?
        .to_string();

    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    // platform admin may create users for any hotel; tenant callers are pinned to their own
    let target_hotel = if auth.role.contains("admin") { body.hotel_id.unwrap_or(0) } else { auth.hotel_id };
    if !auth.role.contains("admin") && body.hotel_id.is_some() && body.hotel_id != Some(auth.hotel_id) {
        return Err(ApiError(StatusCode::FORBIDDEN, "cannot create users for another hotel".into()));
    }
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, target_hotel).await?;
    sqlx::query(
        r#"INSERT INTO users (id, hotel_id, username, full_name, email, role, password_hash, force_password_change)
           VALUES ($1, $2, $3, $4, $5, $6, $7, TRUE)"#,
    )
    .bind(id)
    .bind(target_hotel)
    .bind(&body.username)
    .bind(&body.full_name)
    .bind(&body.email)
    .bind(&body.role)
    .bind(&hash)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create user: {e}")))?;
    db::record_change(&mut conn, target_hotel, Some(auth.user_id), op_id, "user", id, "insert", serde_json::json!({"username": body.username, "role": body.role})).await?;
    db::write_audit(&mut conn, target_hotel, Some(auth.user_id), "user", id, "created", None, Some(serde_json::json!({"username": body.username, "role": body.role}))).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let user: UserDto = sqlx::query_as("SELECT id, hotel_id, username, full_name, email, role, is_active, force_password_change FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: user, error: None }))
}

#[derive(Deserialize, Serialize)]
struct PatchUserBody {
    full_name: Option<String>,
    email: Option<String>,
    role: Option<String>,
    is_active: Option<bool>,
    password: Option<String>,
}

async fn patch(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Path(id): axum::extract::Path<Uuid>,
    Json(body): Json<PatchUserBody>,
) -> AResult<Json<ApiResponse<UserDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::UserManage).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "user_manage required".into()));
    }
    let old: Option<UserDto> = sqlx::query_as("SELECT id, hotel_id, username, full_name, email, role, is_active, force_password_change FROM users WHERE id = $1")
        .bind(id)
        .fetch_optional(&state.db)
        .await
        .map_err(ApiError::from)?;
    let old = old.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "user not found".into()))?;
    // Tenant callers may only touch their own hotel; the platform admin can
    // edit a user in any hotel from the admin panel.
    if !auth.role.contains("admin") && old.hotel_id != auth.hotel_id {
        return Err(ApiError(StatusCode::FORBIDDEN, "cannot edit another hotel's user".into()));
    }
    if let Some(r) = &body.role {
        if !auth.role.contains("admin") && rank(r) >= rank(&auth.role) {
            return Err(ApiError(StatusCode::FORBIDDEN, "cannot assign a role above your own".into()));
        }
    }

    let mut new_hash: Option<String> = None;
    if let Some(pw) = &body.password {
        if pw.len() < 3 {
            return Err(ApiError(StatusCode::BAD_REQUEST, "password too short (min 3)".into()));
        }
        let salt = SaltString::generate(&mut OsRng);
        new_hash = Some(Argon2::default().hash_password(pw.as_bytes(), &salt)
            .map_err(|_| ApiError(StatusCode::INTERNAL_SERVER_ERROR, "hash failed".into()))?.to_string());
    }

    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    if auth.role.contains("admin") {
        db::set_platform_scope(&mut conn).await?;
    } else {
        db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    }
    let n = sqlx::query(
        r#"UPDATE users SET
             full_name = COALESCE($2, full_name),
             email = COALESCE($3, email),
             role = COALESCE($4, role),
             is_active = COALESCE($5, is_active),
             password_hash = COALESCE($6, password_hash),
             force_password_change = CASE WHEN $6 IS NOT NULL THEN TRUE ELSE force_password_change END,
             updated_at = now()
           WHERE id = $1 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(body.full_name.clone())
    .bind(body.email.clone())
    .bind(body.role.clone())
    .bind(body.is_active)
    .bind(new_hash.clone())
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .rows_affected();
    if n == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "user not found".into()));
    }
    db::record_change(&mut conn, old.hotel_id, Some(auth.user_id), op_id, "user", id, "update", serde_json::json!({"patch": body})).await?;
    db::write_audit(&mut conn, old.hotel_id, Some(auth.user_id), "user", id, "updated",
        Some(serde_json::to_value(&old).unwrap_or_default()),
        Some(serde_json::to_value(&body).unwrap_or_default())).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let user: UserDto = sqlx::query_as("SELECT id, username, full_name, email, role, is_active, force_password_change FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: user, error: None }))
}
