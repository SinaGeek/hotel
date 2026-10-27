//! Guests CRUD (README Â§4: guest records + history)
use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{routing::{get, patch}, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new().route("/", get(list).post(create)).route("/{id}", patch(patrol))
}

async fn patrol() -> Json<ApiResponse<()>> {
    Json(ApiResponse { success: false, data: (), error: Some("PATCH /guests/{id} arrives in the next batch".into()) })
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
    .bind(body.id_number.clone())
    .bind(body.phone.clone())
    .bind(body.email.clone())
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
