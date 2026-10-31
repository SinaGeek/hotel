//! Guests CRUD (README Â§4: guest records + history)
use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{routing::{get, patch}, Json, Router};
use chrono::Utc;
use hms_core::ApiResponse;
use hms_core::guest_identity::{decrypt_option_guest_field, encrypt_option_guest_field};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", patch(patrol).delete(delete))
        .route("/{id}/export", get(export))
}

async fn patrol() -> Json<ApiResponse<()>> {
    Json(ApiResponse { success: false, data: (), error: Some("PATCH /guests/{id} arrives in the next batch".into()) })
}

/// Full guest record including decrypted identity fields (GDPR export).
#[derive(Serialize)]
pub struct GuestExportDto {
    pub id: Uuid,
    pub hotel_id: i64,
    pub full_name: String,
    pub id_number: Option<String>,
    pub nationality: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub license_plate: Option<String>,
    pub birth_date: Option<chrono::NaiveDate>,
    pub is_primary: bool,
    pub kbs_reported: bool,
    pub created_at: Option<chrono::DateTime<Utc>>,
    pub updated_at: Option<chrono::DateTime<Utc>>,
    pub deleted_at: Option<chrono::DateTime<Utc>>,
}

/// Guest list never exposes encrypted identity fields as plaintext.
#[derive(Serialize, sqlx::FromRow)]
pub struct GuestDto {
    pub id: Uuid,
    pub full_name: String,
    pub nationality: Option<String>,
    pub license_plate: Option<String>,
    pub birth_date: Option<chrono::NaiveDate>,
    pub is_primary: bool,
    pub kbs_reported: bool,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    axum::extract::Query(q): axum::extract::Query<ListQuery>,
) -> AResult<Json<ApiResponse<Vec<GuestDto>>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::ReservationView).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "reservation_view required".into()));
    }
    let search = format!("%{}%", q.search.unwrap_or_default().to_lowercase());
    let rows: Vec<GuestDto> = sqlx::query_as(
        r#"SELECT id, full_name, nationality, license_plate, birth_date, is_primary, kbs_reported
           FROM guests
           WHERE hotel_id = $1 AND deleted_at IS NULL AND lower(full_name) LIKE $2
           ORDER BY created_at DESC
           LIMIT 100"#,
    )
    .bind(auth.hotel_id)
    .bind(search)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

#[derive(Deserialize, Default)]
struct ListQuery {
    search: Option<String>,
}

#[derive(Deserialize, Serialize)]
struct CreateGuestBody {
    full_name: String,
    nationality: Option<String>,
    license_plate: Option<String>,
    birth_date: Option<chrono::NaiveDate>,
    id_number: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    is_primary: Option<bool>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateGuestBody>,
) -> AResult<Json<ApiResponse<GuestDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::ReservationCreate).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "reservation_create required".into()));
    }
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let id_number_encrypted =
        encrypt_option_guest_field(body.id_number.as_deref())
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("id number encryption failed: {e}")))?;
    let phone_encrypted =
        encrypt_option_guest_field(body.phone.as_deref())
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("phone encryption failed: {e}")))?;
    let email_encrypted =
        encrypt_option_guest_field(body.email.as_deref())
            .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("email encryption failed: {e}")))?;
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        r#"INSERT INTO guests
             (id, hotel_id, full_name, id_number_encrypted, phone_encrypted, email_encrypted,
              nationality, license_plate, birth_date, is_primary)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.full_name)
    .bind(&id_number_encrypted)
    .bind(&phone_encrypted)
    .bind(&email_encrypted)
    .bind(&body.nationality)
    .bind(&body.license_plate)
    .bind(body.birth_date)
    .bind(body.is_primary.unwrap_or(true))
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create guest: {e}")))?;
    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "guest", id, "insert", serde_json::json!({"full_name": body.full_name})).await?;
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "guest", id, "created", None, Some(serde_json::json!({"full_name": body.full_name}))).await?;
    conn.commit().await.map_err(ApiError::from)?;

    let guest: GuestDto = sqlx::query_as("SELECT id, full_name, nationality, license_plate, birth_date, is_primary, kbs_reported FROM guests WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: guest, error: None }))
}

/// GDPR export: return the guest's full record including decrypted identity fields.
async fn export(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<GuestExportDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::GuestExport).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "guest_export required".into()));
    }
    let row = sqlx::query_as::<_, (Uuid, i64, String, Option<String>, Option<String>, Option<String>, Option<String>, Option<String>, Option<chrono::NaiveDate>, bool, bool, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>, Option<chrono::DateTime<Utc>>)>(
        r#"SELECT id, hotel_id, full_name, id_number_encrypted, phone_encrypted, email_encrypted,
                  nationality, license_plate, birth_date, is_primary, kbs_reported,
                  created_at, updated_at, deleted_at
             FROM guests
            WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "guest not found".into()))?;

    let (
        id, hotel_id, full_name, id_number_encrypted, phone_encrypted, email_encrypted,
        nationality, license_plate, birth_date, is_primary, kbs_reported,
        created_at, updated_at, deleted_at,
    ) = row;

    Ok(Json(ApiResponse {
        success: true,
        data: GuestExportDto {
            id,
            hotel_id,
            full_name,
            id_number: decrypt_option_guest_field(id_number_encrypted.as_deref())
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("id number decryption failed: {e}")))?,
            nationality,
            phone: decrypt_option_guest_field(phone_encrypted.as_deref())
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("phone decryption failed: {e}")))?,
            email: decrypt_option_guest_field(email_encrypted.as_deref())
                .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("email decryption failed: {e}")))?,
            license_plate,
            birth_date,
            is_primary,
            kbs_reported,
            created_at,
            updated_at,
            deleted_at,
        },
        error: None,
    }))
}

/// GDPR deletion: soft-delete the guest and anonymize personal data.
/// Reservations referencing this guest keep the (now anonymized) row.
async fn delete(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<()>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::GuestDelete).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "guest_delete required".into()));
    }
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let deleted_at = Utc::now();
    let result = sqlx::query(
        r#"UPDATE guests
              SET deleted_at = $1, full_name = NULL,
                  id_number_encrypted = NULL, phone_encrypted = NULL, email_encrypted = NULL
            WHERE id = $2 AND hotel_id = $3 AND deleted_at IS NULL"#,
    )
    .bind(deleted_at)
    .bind(id)
    .bind(auth.hotel_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot delete guest: {e}")))?;
    if result.rows_affected() == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "guest not found".into()));
    }
    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "guest", id, "delete", serde_json::json!({})).await?;
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "guest", id, "deleted", None, Some(serde_json::json!({}))).await?;
    conn.commit().await.map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: (), error: None }))
}
