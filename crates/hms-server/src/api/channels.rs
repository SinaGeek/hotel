//! Channels: per-hotel configurable list (README §4/§7). Seeded with the
//! standard kinds on first use; hotels can add "other" channels.
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
pub struct ChannelDto {
    pub id: Uuid,
    pub kind: String,
    pub name: String,
    pub is_active: bool,
}

async fn list(State(state): State<AppState>, auth: AuthUser) -> AResult<Json<ApiResponse<Vec<ChannelDto>>>> {
    // every authenticated hotel user can list channels (needed to book)
    let rows: Vec<ChannelDto> = sqlx::query_as(
        "SELECT id, kind, name, is_active FROM channels WHERE hotel_id = $1 AND deleted_at IS NULL ORDER BY name",
    )
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    // First-touch seeding: add any missing standard channels for this hotel
    let existing_kinds: Vec<String> = rows.iter().map(|r| r.kind.clone()).collect();
    let standard_channels = [
        ("direct", "Direct"),
        ("walk_in", "Walk-in"),
        ("booking_com", "Booking.com"),
        ("expedia", "Expedia"),
        ("agency", "Agency"),
        ("other", "Other"),
    ];
    let missing: Vec<_> = standard_channels
        .iter()
        .filter(|(kind, _)| !existing_kinds.contains(&kind.to_string()))
        .collect();

    if !missing.is_empty() {
        let op_id = Uuid::now_v7();
        let mut conn = state.db.begin().await.map_err(ApiError::from)?;
        db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
        for (kind, name) in missing {
            let id = Uuid::now_v7();
            sqlx::query("INSERT INTO channels (id, hotel_id, kind, name) VALUES ($1,$2,$3,$4)")
                .bind(id)
                .bind(auth.hotel_id)
                .bind(kind)
                .bind(name)
                .execute(&mut *conn)
                .await
                .map_err(ApiError::from)?;
            db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "channel", id, "insert", serde_json::json!({"kind": kind, "name": name, "is_active": true})).await?;
        }
        conn.commit().await.map_err(ApiError::from)?;
        let rows: Vec<ChannelDto> = sqlx::query_as(
            "SELECT id, kind, name, is_active FROM channels WHERE hotel_id = $1 AND deleted_at IS NULL ORDER BY name",
        )
        .bind(auth.hotel_id)
        .fetch_all(&state.db)
        .await
        .map_err(ApiError::from)?;
        return Ok(Json(ApiResponse { success: true, data: rows, error: None }));
    }

    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

#[derive(Deserialize, Serialize)]
struct CreateChannelBody {
    kind: String,
    name: String,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateChannelBody>,
) -> AResult<Json<ApiResponse<ChannelDto>>> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::RoomManage).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "room_manage (manager-level) required".into()));
    }
    if !["direct", "walk_in", "booking_com", "expedia", "agency", "other"].contains(&body.kind.as_str()) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "invalid channel kind".into()));
    }
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query("INSERT INTO channels (id, hotel_id, kind, name) VALUES ($1,$2,$3,$4)")
        .bind(id)
        .bind(auth.hotel_id)
        .bind(&body.kind)
        .bind(&body.name)
        .execute(&mut *conn)
        .await
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create channel: {e}")))?;
    db::record_change(&mut conn, auth.hotel_id, Some(auth.user_id), op_id, "channel", id, "insert", serde_json::json!(&body)).await?;
    db::write_audit(&mut conn, auth.hotel_id, Some(auth.user_id), "channel", id, "created", None, Some(serde_json::json!(&body))).await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: ChannelDto = sqlx::query_as("SELECT id, kind, name, is_active FROM channels WHERE id = $1")
        .bind(id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: row, error: None }))
}
