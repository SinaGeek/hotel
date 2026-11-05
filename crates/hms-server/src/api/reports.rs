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
        .route("/by-day.pdf", get(by_day_pdf))
        .route("/by-room", get(by_room))
        .route("/forecast", get(forecast))
        .route("/inhouse", get(inhouse))
        .route("/maincourante", get(maincourante))
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

/// F5: the by-day report as a server-rendered PDF (browser-print replacement).
/// Same permission, range and query as the JSON `by_day`; the pagination to
/// A4 pages (40 day rows per page, totals in page 1's footer) happens in
/// `crate::pdf::by_day_report_pdf`.
async fn by_day_pdf(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ReportQuery>,
) -> AResult<axum::response::Response> {
    require_report_view(&state, &auth).await?;
    let (from, to) = validate_range(&q)?;

    #[derive(sqlx::FromRow)]
    struct Row {
        date: chrono::NaiveDate,
        room_revenue_cents: i64,
        extra_revenue_cents: i64,
        payments_collected_cents: i64,
        occupied_nights: i64,
    }
    let rows: Vec<Row> = sqlx::query_as(
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

    #[derive(sqlx::FromRow)]
    struct HotelMeta {
        name: String,
        default_currency: String,
    }
    let meta: HotelMeta = sqlx::query_as(
        "SELECT name, default_currency FROM hotels WHERE id = $1",
    )
    .bind(auth.hotel_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;

    let cell_rows: Vec<[String; 5]> = rows
        .iter()
        .map(|r| {
            [
                r.date.format("%F").to_string(),
                crate::pdf::fmt_cents(r.room_revenue_cents),
                crate::pdf::fmt_cents(r.extra_revenue_cents),
                crate::pdf::fmt_cents(r.payments_collected_cents),
                r.occupied_nights.to_string(),
            ]
        })
        .collect();
    let totals: [i64; 4] = [
        rows.iter().map(|r| r.room_revenue_cents).sum(),
        rows.iter().map(|r| r.extra_revenue_cents).sum(),
        rows.iter().map(|r| r.payments_collected_cents).sum(),
        rows.iter().map(|r| r.occupied_nights).sum(),
    ];
    let bytes = crate::pdf::by_day_report_pdf(
        &meta.name,
        &meta.default_currency,
        &from.format("%F").to_string(),
        &to.format("%F").to_string(),
        &cell_rows,
        totals,
        &chrono::Utc::now().to_rfc3339(),
    );
    Ok(crate::pdf::pdf_response(
        bytes,
        &format!("hms-daily-revenue-{from}-{to}.pdf"),
    ))
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

// ------------------------------------------------------------------------- 5b.6 Daily set
// Occupancy Forecast (per-date arrivals/occupied/vacant/guests/%), In-House
// list with balances, Main Courante (per-date business diary). These read raw
// reservations + payments (the daily_revenue aggregate has no per-stay
// granularity), matching the reference /reports/* pages 1:1 where the data
// model allows.

#[derive(Deserialize)]
pub struct ForecastQuery {
    pub start: chrono::NaiveDate,
    /// number of days to show (default 15, max 90)
    pub days: Option<i64>,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct ForecastRow {
    pub date: chrono::NaiveDate,
    /// confirmed + checked-in stays starting that day
    pub arrivals: i64,
    /// stays covering the day: checkin <= date < checkout
    pub occupied: i64,
    /// rentable rooms minus occupied
    pub vacant: i64,
    /// adults + children of the stays covering the day
    pub guests: i64,
    pub occupancy_pct: f64,
}

async fn forecast(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ForecastQuery>,
) -> AResult<Json<ApiResponse<Vec<ForecastRow>>>> {
    require_report_view(&state, &auth).await?;
    let days = q.days.unwrap_or(15);
    if !(1..=90).contains(&days) {
        return Err(ApiError(StatusCode::BAD_REQUEST, "days must be 1..=90".into()));
    }
    let rooms = room_count(&state.db, auth.hotel_id).await.map_err(ApiError::from)?;
    let rows: Vec<ForecastRow> = sqlx::query_as(
        r#"
        WITH days AS (
            SELECT generate_series($2::date, $2::date + ($3::int - 1), '1 day'::interval)::date AS d
        )
        SELECT days.d AS date,
               (SELECT count(*) FROM reservations res
                WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
                  AND res.status IN ('confirmed','checked_in')
                  AND res.checkin_date = days.d) AS arrivals,
               (SELECT count(*) FROM reservations res
                WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
                  AND res.status IN ('confirmed','checked_in')
                  AND res.checkin_date <= days.d AND res.checkout_date > days.d) AS occupied,
               $4::bigint - (SELECT count(*) FROM reservations res
                WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
                  AND res.status IN ('confirmed','checked_in')
                  AND res.checkin_date <= days.d AND res.checkout_date > days.d) AS vacant,
               COALESCE((SELECT sum(res.adults + res.children) FROM reservations res
                WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
                  AND res.status IN ('confirmed','checked_in')
                  AND res.checkin_date <= days.d AND res.checkout_date > days.d), 0)::bigint AS guests,
               CASE WHEN $4::bigint > 0
                    THEN 100.0 * (SELECT count(*) FROM reservations res
                                  WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
                                    AND res.status IN ('confirmed','checked_in')
                                    AND res.checkin_date <= days.d AND res.checkout_date > days.d)::float8 / $4::bigint
                    ELSE 0.0 END AS occupancy_pct
        FROM days
        ORDER BY days.d
        "#,
    )
    .bind(auth.hotel_id)
    .bind(q.start)
    .bind(days)
    .bind(rooms)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse { success: true, data: rows, error: None }))
}

#[derive(Deserialize)]
pub struct StayDateQuery {
    pub date: chrono::NaiveDate,
}

#[derive(Serialize, sqlx::FromRow)]
pub struct InhouseRow {
    pub reservation_id: uuid::Uuid,
    pub guest_name: Option<String>,
    pub room_number: String,
    pub room_type_name: String,
    pub checkin_date: chrono::NaiveDate,
    pub checkout_date: chrono::NaiveDate,
    pub guests: i64,
    /// total price minus payments recorded up to AND on the date
    pub balance_cents: i64,
    /// the stay's per-night rate for the selected date
    pub day_rate_cents: Option<i64>,
    pub currency: String,
}

#[derive(Serialize)]
pub struct InhouseReport {
    pub date: chrono::NaiveDate,
    pub currency: String,
    pub bookings: i64,
    pub adults: i64,
    pub children: i64,
    /// sum of the day rates of the in-house stays for this date
    pub room_rate_cents: i64,
    /// sum of row balances
    pub balance_cents: i64,
    pub rows: Vec<InhouseRow>,
}

async fn inhouse(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<StayDateQuery>,
) -> AResult<Json<ApiResponse<InhouseReport>>> {
    require_report_view(&state, &auth).await?;
    let rows: Vec<InhouseRow> = sqlx::query_as(
        r#"
        SELECT res.id AS reservation_id,
               g.full_name AS guest_name,
               r.room_number,
               rt.name AS room_type_name,
               res.checkin_date,
               res.checkout_date,
               (res.adults + res.children)::bigint AS guests,
               (res.total_price_cents - COALESCE(
                   (SELECT sum(p.amount_cents)::bigint FROM payments p
                    WHERE p.reservation_id = res.id AND p.created_at::date <= $2), 0)) AS balance_cents,
               COALESCE(
                   (SELECT (e->>'price_cents')::bigint
                    FROM jsonb_array_elements(res.daily_rates) e
                    WHERE e->>'date' = $2::text
                    LIMIT 1),
                   (res.total_price_cents::numeric / GREATEST((res.checkout_date - res.checkin_date), 1)))::bigint AS day_rate_cents,
               res.currency
        FROM reservations res
        JOIN rooms r ON r.id = res.room_id
        JOIN room_types rt ON rt.id = r.room_type_id
        LEFT JOIN guests g ON g.id = res.guest_id
        WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
          AND res.status IN ('confirmed','checked_in')
          AND res.checkin_date <= $2 AND res.checkout_date > $2
        ORDER BY r.room_number
        "#,
    )
    .bind(auth.hotel_id)
    .bind(q.date)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;

    let adults: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(adults),0)::bigint FROM reservations res
         WHERE res.hotel_id = $1 AND res.deleted_at IS NULL AND res.status IN ('confirmed','checked_in')
           AND res.checkin_date <= $2 AND res.checkout_date > $2",
    )
    .bind(auth.hotel_id)
    .bind(q.date)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    let children: i64 = sqlx::query_scalar(
        "SELECT COALESCE(sum(children),0)::bigint FROM reservations res
         WHERE res.hotel_id = $1 AND res.deleted_at IS NULL AND res.status IN ('confirmed','checked_in')
           AND res.checkin_date <= $2 AND res.checkout_date > $2",
    )
    .bind(auth.hotel_id)
    .bind(q.date)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    let currency: String = sqlx::query_scalar("SELECT default_currency FROM hotels WHERE id = $1")
        .bind(auth.hotel_id)
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;

    let room_rate_cents: i64 = rows.iter().map(|r| r.day_rate_cents.unwrap_or(0)).sum();
    let balance_cents: i64 = rows.iter().map(|r| r.balance_cents).sum();
    let bookings = rows.len() as i64;
    Ok(Json(ApiResponse {
        success: true,
        data: InhouseReport { date: q.date, currency, bookings, adults, children, room_rate_cents, balance_cents, rows },
        error: None,
    }))
}

#[derive(Serialize, sqlx::FromRow)]
pub struct MainCouranteRow {
    pub reservation_id: uuid::Uuid,
    pub guest_name: Option<String>,
    pub room_number: String,
    pub checkin_date: chrono::NaiveDate,
    pub checkout_date: chrono::NaiveDate,
    /// the stay's per-night rate for the selected date
    pub daily_rate_cents: Option<i64>,
    /// room-type list rate for the date (price rule in effect, else base)
    pub room_rate_cents: i64,
    /// total price minus payments recorded before the date
    pub previous_balance_cents: i64,
    /// total price minus payments recorded up to AND on the date
    pub balance_cents: i64,
    pub currency: String,
    pub status: String,
}

#[derive(Serialize)]
pub struct MainCouranteReport {
    pub date: chrono::NaiveDate,
    pub count: i64,
    pub rows: Vec<MainCouranteRow>,
}

async fn maincourante(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<StayDateQuery>,
) -> AResult<Json<ApiResponse<MainCouranteReport>>> {
    require_report_view(&state, &auth).await?;
    let rows: Vec<MainCouranteRow> = sqlx::query_as(
        r#"
        SELECT res.id AS reservation_id,
               g.full_name AS guest_name,
               r.room_number,
               res.checkin_date,
               res.checkout_date,
               COALESCE(
                   (SELECT (e->>'price_cents')::bigint
                    FROM jsonb_array_elements(res.daily_rates) e
                    WHERE e->>'date' = $2::text
                    LIMIT 1),
                   (res.total_price_cents::numeric / GREATEST((res.checkout_date - res.checkin_date), 1)))::bigint AS daily_rate_cents,
               COALESCE(NULLIF(
                   (SELECT pr.price_cents FROM price_rules pr
                    WHERE pr.hotel_id = res.hotel_id AND pr.room_type_id = rt.id AND pr.deleted_at IS NULL
                      AND pr.date_from <= $2 AND pr.date_to >= $2
                    ORDER BY pr.date_from DESC LIMIT 1), 0),
                   rt.base_price_cents) AS room_rate_cents,
               (res.total_price_cents - COALESCE(
                   (SELECT sum(p.amount_cents)::bigint FROM payments p
                    WHERE p.reservation_id = res.id AND p.created_at::date < $2), 0)) AS previous_balance_cents,
               (res.total_price_cents - COALESCE(
                   (SELECT sum(p.amount_cents)::bigint FROM payments p
                    WHERE p.reservation_id = res.id AND p.created_at::date <= $2), 0)) AS balance_cents,
               res.currency,
               res.status
        FROM reservations res
        JOIN rooms r ON r.id = res.room_id
        JOIN room_types rt ON rt.id = r.room_type_id
        LEFT JOIN guests g ON g.id = res.guest_id
        WHERE res.hotel_id = $1 AND res.deleted_at IS NULL
          AND res.status IN ('confirmed','checked_in')
          AND (  (res.checkin_date <= $2 AND res.checkout_date > $2)   -- covering the night
              OR (res.checkin_date = $2)                              -- arriving today
              OR (res.checkout_date = $2) )                           -- leaving today
        ORDER BY r.room_number
        "#,
    )
    .bind(auth.hotel_id)
    .bind(q.date)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    let count = rows.len() as i64;
    Ok(Json(ApiResponse {
        success: true,
        data: MainCouranteReport { date: q.date, count, rows },
        error: None,
    }))
}
