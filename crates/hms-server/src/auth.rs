//! Authentication: login (hotel id + username + password), JWT sessions,
//! change-password, logout. Permissions REQUIRED for tenant RLS scope.
use crate::AppState;
use argon2::password_hash::PasswordVerifier;
use argon2::{Argon2, PasswordHasher};
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::{extract::{State}, http::{HeaderMap, StatusCode}, routing::post};
use axum::{Json, Router};
use hms_core::ApiResponse;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

pub const SESSION_DAYS: i64 = 2; // short-lived sessions (README Â§8)

#[derive(Debug, Serialize, Deserialize)]
pub struct Claims {
    pub sub: Uuid,             // user id
    pub hotel_id: i64,
    pub role: String,
    /// forced password change pending
    pub fpc: bool,
    pub exp: i64,
}

pub fn encode_token(state: &AppState, claims: &Claims) -> Result<String, (StatusCode, Json<ApiResponse<()>>)> {
    jsonwebtoken::encode(
        &Header::default(),
        claims,
        &EncodingKey::from_secret(state.session_secret.as_bytes()),
    )
    .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "token encode failed"))
}

/// Extracted, authenticated caller. Rejects unauthenticated requests.
pub struct AuthUser {
    pub user_id: Uuid,
    pub hotel_id: i64,
    pub role: String,
    pub force_password_change: bool,
}

impl FromRequestParts<AppState> for AuthUser {
    type Rejection = (StatusCode, Json<ApiResponse<()>>);

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let header = parts.headers.get("authorization")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "missing bearer token"))?;
        let token = header.strip_prefix("Bearer ")
            .ok_or_else(|| err(StatusCode::UNAUTHORIZED, "invalid authorization header"))?;
        let claims = jsonwebtoken::decode::<Claims>(
            token,
            &DecodingKey::from_secret(state.session_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|_| err(StatusCode::UNAUTHORIZED, "invalid or expired session"))?;

        // Forced password change: only auth endpoints are reachable. The nested
        // router strips "/api/v1/auth", so accept both full and stripped paths.
        let path = parts.uri.path();
        let is_auth_ep = path.contains("/auth") || path.ends_with("/change-password") || path.ends_with("/logout");
        if claims.claims.fpc && !is_auth_ep {
            return Err(err(StatusCode::FORBIDDEN, "password change required before any other action"));
        }

        Ok(AuthUser {
            user_id: claims.claims.sub,
            hotel_id: claims.claims.hotel_id,
            role: claims.claims.role,
            force_password_change: claims.claims.fpc,
        })
    }
}

pub(crate) fn err(status: StatusCode, message: &str) -> (StatusCode, Json<ApiResponse<()>>) {
    (status, Json(ApiResponse { success: false, data: (), error: Some(message.to_string()) }))
}

/// Permission check against the DB matrix (role_permissions + per-hotel overrides).
/// The matrix in `hms-core::roles` is only the fallback.
pub async fn has_permission(pool: &PgPool, role: &str, hotel_id: i64, perm: hms_core::roles::Permission) -> sqlx::Result<bool> {
    let has_platform = hms_core::roles::Role::from_str(role)
        .map(|r| r.is_platform_admin())
        .unwrap_or(false);
    if has_platform {
        return Ok(true);
    }
    let value: Option<String> = sqlx::query_scalar(
        r#"
        SELECT allowed::text FROM (
          SELECT allowed FROM role_permissions WHERE role = $1 AND permission = $2
          UNION ALL
          SELECT allowed FROM hotel_permission_overrides WHERE hotel_id = $3 AND role = $1 AND permission = $2
          ORDER BY allowed DESC
        ) combined LIMIT 1
        "#,
    )
    .bind(role.to_string())
    .bind(perm.as_str().to_string())
    .bind(hotel_id)
    .fetch_optional(pool)
    .await?;
    Ok(value.map(|v| v == "true").unwrap_or(false))
}

// in ApiError

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", post(login))
        .route("/refresh", post(refresh))
        .route("/logout", post(logout))
        .route("/change-password", post(change_password))
}

#[derive(serde::Deserialize)]
struct LoginBody {
    hotel_id: i64,
    username: String,
    password: String,
    /// Set by the desktop webview (README Â§11.7): the response then carries
    /// `offline_verifier` so the native shell can cache credentials for
    /// offline verification. Plain browsers never receive the hash.
    #[serde(default)]
    offline: bool,
}

#[derive(serde::Serialize)]
struct LoginResponse {
    token: String,
    user: UserDto,
    permissions: Vec<String>,
    /// The argon2 PHC hash of the just-verified password, ONLY when the
    /// request stated `offline: true` (desktop). Offline login re-verifies
    /// the typed password against it once the server is unreachable; the DPAPI
    /// wrapper on the desktop protects the persisted copy.
    #[serde(skip_serializing_if = "Option::is_none")]
    offline_verifier: Option<String>,
}

#[derive(serde::Serialize)]
struct UserDto {
    user_id: Uuid,
    hotel_id: i64,
    username: String,
    full_name: String,
    role: String,
    force_password_change: bool,
}

async fn login(
    State(state): State<AppState>,
    headers: HeaderMap,
    Json(body): Json<LoginBody>,
) -> Result<Json<ApiResponse<LoginResponse>>, (StatusCode, Json<ApiResponse<()>>)> {
    // Rate limiting per hotel:username (and IP) â€” README Â§8
    let ip = headers.get("x-forwarded-for")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("local")
        .to_string();
    let key = format!("{}|{}|{}", body.hotel_id, body.username, ip);
    {
        let mut rl = state.rate_limiter.lock().await;
        if !rl.check(&key) {
            return Err(err(StatusCode::TOO_MANY_REQUESTS, "too many login attempts, try later"));
        }
    }

    let row: Option<(Uuid, String, String, String, String, bool, bool)> = sqlx::query_as(
        r#"SELECT id, username, full_name, role, password_hash, is_active, force_password_change
           FROM users
           WHERE hotel_id = $1 AND username = $2 AND deleted_at IS NULL"#,
    )
    .bind(body.hotel_id)
    .bind(body.username.clone())
    .fetch_optional(&state.db)
    .await
    .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;

    let (user_id, username, full_name, role, hash, is_active, fpc) =
        row.ok_or_else(|| err(StatusCode::UNAUTHORIZED, "invalid credentials"))?;

    if !is_active {
        return Err(err(StatusCode::FORBIDDEN, "account is disabled"));
    }

    let ok = argon2::PasswordHash::new(&hash)
        .map(|parsed| Argon2::default().verify_password(body.password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false);
    if !ok {
        return Err(err(StatusCode::UNAUTHORIZED, "invalid credentials"));
    }

    let permissions = load_permissions(&state.db, &role, body.hotel_id)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;

    let claims = Claims {
        sub: user_id,
        hotel_id: body.hotel_id,
        role: role.clone(),
        fpc,
        exp: (chrono::Utc::now() + chrono::Duration::hours(24 * SESSION_DAYS)).timestamp(),
    };
    let token = encode_token(&state, &claims)?;

    Ok(Json(ApiResponse {
        success: true,
        data: LoginResponse {
            token,
            user: UserDto {
                user_id,
                hotel_id: body.hotel_id,
                username,
                full_name,
                role,
                force_password_change: fpc,
            },
            permissions,
            offline_verifier: body.offline.then(|| hash.clone()),
        },
        error: None,
    }))
}

/// Effective permission strings for a role+hotel (base matrix merged with overrides).
pub async fn load_permissions(pool: &PgPool, role: &str, hotel_id: i64) -> sqlx::Result<Vec<String>> {
    if hms_core::roles::Role::from_str(role).map(|r| r.is_platform_admin()).unwrap_or(false) {
        return Ok(hms_core::roles::PERMISSIONS.iter().map(|p| p.as_str().to_string()).collect());
    }
    let base: Vec<String> = sqlx::query_scalar(
        r#"SELECT p.permission FROM role_permissions p WHERE p.role = $1 AND p.allowed"#,
    )
    .bind(role.to_string())
    .fetch_all(pool)
    .await?;
    let overrides: Vec<(String, bool)> = sqlx::query_as(
        r#"SELECT permission, allowed FROM hotel_permission_overrides WHERE hotel_id = $1 AND role = $2"#,
    )
    .bind(hotel_id)
    .bind(role.to_string())
    .fetch_all(pool)
    .await?;
    let mut set: Vec<String> = base;
    for (perm, allowed) in overrides {
        if allowed {
            if !set.contains(&perm) {
                set.push(perm);
            }
        } else {
            set.retain(|p| p != &perm);
        }
    }
    Ok(set)
}

async fn logout() -> Json<ApiResponse<()>> {
    // JWTs are stateless; clients drop the token. (Refresh-token blocklist in Phase 6 sync work.)
    Json(ApiResponse { success: true, data: (), error: None })
}

async fn refresh(
    State(state): State<AppState>,
    auth: AuthUser,
) -> Result<Json<ApiResponse<String>>, (StatusCode, Json<ApiResponse<()>>)> {
    let claims = Claims {
        sub: auth.user_id,
        hotel_id: auth.hotel_id,
        role: auth.role.clone(),
        // keep the flag unchanged: refresh never clears a pending forced change
        fpc: auth.force_password_change,
        exp: (chrono::Utc::now() + chrono::Duration::hours(24 * SESSION_DAYS)).timestamp(),
    };
    let token = encode_token(&state, &claims)?;
    Ok(Json(ApiResponse { success: true, data: token, error: None }))
}

#[derive(serde::Deserialize)]
struct ChangePasswordBody {
    old_password: String,
    new_password: String,
}

async fn change_password(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<ChangePasswordBody>,
) -> Result<Json<ApiResponse<()>>, (StatusCode, Json<ApiResponse<()>>)> {
    // Hotels provision short initial passwords (e.g. `1234`); the forced
    // change flow lets each user choose their own. Keep the same minimum as
    // user creation so provisioning remains consistent.
    if body.new_password.len() < 3 {
        return Err(err(StatusCode::BAD_REQUEST, "password too short (min 3)"));
    }
    let row: Option<String> = sqlx::query_scalar("SELECT password_hash FROM users WHERE id = $1")
        .bind(auth.user_id)
        .fetch_optional(&state.db)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    let hash = row.ok_or_else(|| err(StatusCode::UNAUTHORIZED, "user not found"))?;
    let ok = argon2::PasswordHash::new(&hash)
        .map(|parsed| Argon2::default().verify_password(body.old_password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false);
    if !ok {
        return Err(err(StatusCode::UNAUTHORIZED, "old password incorrect"));
    }
    let salt = argon2::password_hash::SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let new_hash = Argon2::default()
        .hash_password(body.new_password.as_bytes(), &salt)
        .map_err(|_| err(StatusCode::INTERNAL_SERVER_ERROR, "hash failed"))?
        .to_string();
    sqlx::query("UPDATE users SET password_hash = $2, force_password_change = FALSE, updated_at = now() WHERE id = $1")
        .bind(auth.user_id)
        .bind(new_hash)
        .execute(&state.db)
        .await
        .map_err(|e| err(StatusCode::INTERNAL_SERVER_ERROR, &format!("db: {e}")))?;
    Ok(Json(ApiResponse { success: true, data: (), error: None }))
}
