//! Reports (README §10): every query reads `daily_revenue` — never raw
//! reservations. All tenant-scoped by auth.hotel_id, gated by the
//! `report_view_tenant` permission (manager default; configurable in the matrix).
use super::{ApiError, AResult};
use crate::auth::AuthUser;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/summary", get(summary))
        .route("/by-channel", get(by_channel))
        .route("/by-day", get(by_day))
        .route("/by-room", get(by_room))
}

#[derive(Deserialize)]
pub struct ReportQuery {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
}

/// from inclusive, to exclusive (consistent with the board). Bounds: 400 days
/// max, keep the aggregate cheap.
fn validate_range(q: &ReportQuery) -> Result<(chrono::NaiveDate, chrono::NaiveDate), ApiError> {
    if q.to <= q.from {
        return Err(ApiError(StatusCode::BAD_REQUEST, "to must be after from".into()));
    }
    let days = (q.to - q.from).num_days();
    if days > 400 {
        return Err(ApiError(StatusCode::BAD_REQUEST, "range too large (max 400 days)".into()));
    }
    Ok((q.from, q.to))
}

/// Number of rentable rooms (live rooms only) — denominator for available
/// room-nights / RevPAR.
async fn room_count(pool: &sqlx::PgPool, hotel_id: i64) -> sqlx::Result<i64> {
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms WHERE hotel_id = $1 AND deleted_at IS NULL")
        .bind(hotel_id)
        .fetch_one(pool)
        .await?;
    Ok(n)
}

#[derive(Serialize)]
pub struct SummaryDto {
    pub from: chrono::NaiveDate,
    pub to: chrono::NaiveDate,
    pub currency: String,
    pub room_revenue_cents: i64,
    pub extra_revenue_cents: i64,
    pub payments_collected_cents: i64,
    pub occupied_nights: i64,
    pub available_room_nights: i64,
    pub occupancy_pct: f64,
    /// room revenue / occupied nights (None when 0 stays)
    pub adr_cents: Option<f64>,
    /// room revenue / available room-nights (None when no rentable rooms)
    pub revpar_cents: Option<f64>,
}

async fn require_report_view(state: &AppState, auth: &AuthUser) -> AResult<()> {
    if !crate::auth::has_permission(&state.db, &auth.role, auth.hotel_id, hms_core::roles::Permission::ReportViewTenant).await? {
        return Err(ApiError(StatusCode::FORBIDDEN, "report_view_tenant required".into()));
    }
    Ok(())
}

async fn summary(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ReportQuery>,
) -> AResult<Json<ApiResponse<SummaryDto>>> {
    require_report_view(&state, &auth).await?;
    let (from, to) = validate_range(&q)?;
    let days = (to - from).num_days() as i64;

    #[derive(sqlx::FromRow)]
    struct Agg {
        room_revenue_cents: i64,
        extra_revenue_cents: i64,
        payments_collected_cents: i64,
        occupied_nights: i64,
    }
    let agg: Option<Agg> = sqlx::query_as(
        r#"
        SELECT COALESCE(sum(room_revenue_cents),0)::bigint       AS room_revenue_cents,
               COALESCE(sum(extra_revenue_cents),0)::bigint      AS extra_revenue_cents,
               COALESCE(sum(payment_collected_cents),0)::bigint  AS payments_collected_cents,
               count(*) FILTER (WHERE occupied)::bigint          AS occupied_nights
        FROM daily_revenue
        WHERE hotel_id = $1 AND date >= $2 AND date < $3
        "#,
    )
    .bind(auth.hotel_id)
    .bind(from)
    .bind(to)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;
    let agg = agg.unwrap_or(Agg { room_revenue_cents: 0, extra_revenue_cents: 0, payments_collected_cents: 0, occupied_nights: 0 });

    let currency: String = sqlx::query_scalar("SELECT default_currency FROM hotels WHERE id = $1")
        .bind(auth.hotel_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    let rooms = room_count(&state.db, auth.hotel_id).await.map_err(ApiError::from)?;
    let available = rooms * days;

    let occupancy_pct = if available > 0 { agg.occupied_nights as f64 * 100.0 / available as f64 } else { 0.0 };
    // Occupancy for hotels with more rooms than exist is nonsense; cap sanity.
    let adr = if agg.occupied_nights > 0 { Some(agg.room_revenue_cents as f64 / agg.occupied_nights as f64) } else { None };
    let revpar = if available > 0 { Some(agg.room_revenue_cents as f64 / available as f64) } else { None };

    Ok(Json(ApiResponse {
        success: true,
        data: SummaryDto {
            from,
            to,
            currency,
            room_revenue_cents: agg.room_revenue_cents,
            extra_revenue_cents: agg.extra_revenue_cents,
            payments_collected_cents: agg.payments_collected_cents,
            occupied_nights: agg.occupied_nights,
            available_room_nights: available,
            occupancy_pct,
            adr_cents: adr,
            revpar_cents: revpar,
        },
        error: None,
    }))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ChannelRevenueDto {
    pub channel_id: uuid::Uuid,
    pub channel_name: String,
    pub kind: String,
    pub room_revenue_cents: i64,
    pub extra_revenue_cents: i64,
    pub payments_collected_cents: i64,
    pub occupied_nights: i64,
}

async fn by_channel(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ReportQuery>,
) -> AResult<Json<ApiResponse<Vec<ChannelRevenueDto>>>> {
    require_report_view(&state, &auth).await?;
    let (from, to) = validate_range(&q)?;
    let rows: Vec<ChannelRevenueDto> = sqlx::query_as(
        r#"
        SELECT c.id AS channel_id, c.name AS channel_name, c.kind,
               COALESCE(sum(dr.room_revenue_cents),0)::bigint      AS room_revenue_cents,
               COALESCE(sum(dr.extra_revenue_cents),0)::bigint     AS extra_revenue_cents,
               COALESCE(sum(dr.payment_collected_cents),0)::bigint AS payments_collected_cents,
               count(*) FILTER (WHERE dr.occupied)::bigint         AS occupied_nights
        FROM channels c
        LEFT JOIN daily_revenue dr
               ON dr.channel_id = c.id AND dr.hotel_id = c.hotel_id
              AND dr.date >= $2 AND dr.date < $3
        WHERE c.hotel_id = $1 AND c.deleted_at IS NULL
        GROUP BY c.id, c.name, c.kind
        ORDER BY (COALESCE(sum(dr.room_revenue_cents),0) + COALESCE(sum(dr.extra_revenue_cents),0)) DESC, c.name
        "#,
    )
    .bind(auth.hotel_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct DayReportDto {
    pub date: chrono::NaiveDate,
    pub room_revenue_cents: i64,
    pub extra_revenue_cents: i64,
    pub payments_collected_cents: i64,
    pub occupied_nights: i64,
}

async fn by_day(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ReportQuery>,
) -> AResult<Json<ApiResponse<Vec<DayReportDto>>>> {
    require_report_view(&state, &auth).await?;
    let (from, to) = validate_range(&q)?;
    let rows: Vec<DayReportDto> = sqlx::query_as(
        r#"
        SELECT date,
               COALESCE(sum(room_revenue_cents),0)::bigint      AS room_revenue_cents,
               COALESCE(sum(extra_revenue_cents),0)::bigint     AS extra_revenue_cents,
               COALESCE(sum(payment_collected_cents),0)::bigint AS payments_collected_cents,
               count(*) FILTER (WHERE occupied)::bigint         AS occupied_nights
        FROM daily_revenue
        WHERE hotel_id = $1 AND date >= $2 AND date < $3
        GROUP BY date
        ORDER BY date
        "#,
    )
    .bind(auth.hotel_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct RoomRevenueDto {
    pub room_id: uuid::Uuid,
    pub room_number: String,
    pub room_type_name: String,
    pub room_revenue_cents: i64,
    pub extra_revenue_cents: i64,
    pub occupied_nights: i64,
}

async fn by_room(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ReportQuery>,
) -> AResult<Json<ApiResponse<Vec<RoomRevenueDto>>>> {
    require_report_view(&state, &auth).await?;
    let (from, to) = validate_range(&q)?;
    let rows: Vec<RoomRevenueDto> = sqlx::query_as(
        r#"
        SELECT r.id AS room_id, r.room_number, rt.name AS room_type_name,
               COALESCE(sum(dr.room_revenue_cents),0)::bigint  AS room_revenue_cents,
               COALESCE(sum(dr.extra_revenue_cents),0)::bigint AS extra_revenue_cents,
               count(*) FILTER (WHERE dr.occupied)::bigint     AS occupied_nights
        FROM rooms r
        JOIN room_types rt ON rt.id = r.room_type_id
        LEFT JOIN daily_revenue dr
               ON dr.room_id = r.id AND dr.hotel_id = r.hotel_id
              AND dr.date >= $2 AND dr.date < $3
        WHERE r.hotel_id = $1 AND r.deleted_at IS NULL
        GROUP BY r.id, r.room_number, rt.name
        ORDER BY r.room_number
        "#,
    )
    .bind(auth.hotel_id)
    .bind(from)
    .bind(to)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}
