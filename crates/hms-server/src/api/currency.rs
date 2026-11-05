//! G6: currency exchange rates. The hotel's `default_currency` is the base;
//! every rate says "1 unit of `currency` = rate_micro / 1e6 base units" for a
//! given date. Reads answer with the most recent rate on or before `on`, so a
//! hotel only has to enter a rate when it changes.
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use chrono::NaiveDate;
use hms_core::roles::Permission;
use hms_core::site2::{convert_to_base_cents, CurrencyRate};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(upsert))
        .route("/convert", get(convert))
}

#[derive(Deserialize)]
struct ListQuery {
    on: Option<NaiveDate>,
}

/// Latest rate per currency on or before `on` (default: today).
async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> AResult<Json<ApiResponse<Vec<CurrencyRate>>>> {
    // financial data: same gate as reports/payments
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::FinancialView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "financial_view required".into(),
        ));
    }
    let on = q.on.unwrap_or_else(|| chrono::Utc::now().date_naive());
    let rows: Vec<CurrencyRate> = sqlx::query_as(
        r#"SELECT DISTINCT ON (currency)
                  id, hotel_id, currency, rate_micro, valid_on, created_by, created_at
           FROM currency_rates
           WHERE hotel_id = $1 AND valid_on <= $2
           ORDER BY currency, valid_on DESC"#,
    )
    .bind(auth.hotel_id)
    .bind(on)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}

#[derive(Deserialize)]
struct UpsertBody {
    currency: String,
    rate_micro: i64,
    valid_on: Option<NaiveDate>,
}

/// Insert-or-update today's (or a given day's) rate for a currency. Upsert keeps
/// the endpoint idempotent for the desktop mirror and the LAN hub replays.
async fn upsert(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<UpsertBody>,
) -> AResult<Json<ApiResponse<CurrencyRate>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::FinancialView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "financial_view required".into(),
        ));
    }
    if body.currency.len() != 3 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "currency must be a 3-letter code".into(),
        ));
    }
    if body.rate_micro <= 0 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "rate_micro must be positive".into(),
        ));
    }
    let valid_on = body
        .valid_on
        .unwrap_or_else(|| chrono::Utc::now().date_naive());
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let row: CurrencyRate = sqlx::query_as(
        r#"INSERT INTO currency_rates (id, hotel_id, currency, rate_micro, valid_on, created_by)
           VALUES ($1,$2,$3,$4,$5,$6)
           ON CONFLICT (hotel_id, currency, valid_on)
           DO UPDATE SET rate_micro = EXCLUDED.rate_micro, created_by = EXCLUDED.created_by
           RETURNING id, hotel_id, currency, rate_micro, valid_on, created_by, created_at"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(body.currency.to_uppercase())
    .bind(body.rate_micro)
    .bind(valid_on)
    .bind(auth.user_id)
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot store rate: {e}")))?;
    let payload = serde_json::json!({
        "currency": row.currency, "rate_micro": row.rate_micro, "valid_on": row.valid_on
    });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "currency_rate",
        row.id,
        "insert",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "currency_rate",
        row.id,
        "upserted",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize)]
struct ConvertQuery {
    from: String,
    amount_cents: i64,
    on: Option<NaiveDate>,
}

#[derive(Serialize)]
struct ConvertResponse {
    from: String,
    rate_micro: i64,
    amount_cents: i64,
    base_amount_cents: i64,
    valid_on: NaiveDate,
}

/// Convert an amount into the hotel's default currency using the latest rate.
async fn convert(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ConvertQuery>,
) -> AResult<Json<ApiResponse<ConvertResponse>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::FinancialView,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "financial_view required".into(),
        ));
    }
    let on = q.on.unwrap_or_else(|| chrono::Utc::now().date_naive());
    let row: Option<(i64, NaiveDate)> = sqlx::query_as(
        "SELECT rate_micro, valid_on FROM currency_rates
         WHERE hotel_id = $1 AND currency = $2 AND valid_on <= $3
         ORDER BY valid_on DESC LIMIT 1",
    )
    .bind(auth.hotel_id)
    .bind(q.from.to_uppercase())
    .bind(on)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;
    let (rate_micro, valid_on) =
        row.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "no rate for that currency".into()))?;
    let base_amount_cents = convert_to_base_cents(q.amount_cents, rate_micro)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;
    Ok(Json(ApiResponse {
        success: true,
        data: ConvertResponse {
            from: q.from.to_uppercase(),
            rate_micro,
            amount_cents: q.amount_cents,
            base_amount_cents,
            valid_on,
        },
        error: None,
    }))
}
