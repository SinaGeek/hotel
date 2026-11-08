//! G6: agencies (site-2 "CARI" accounts) — travel agencies and corporate
//! accounts with a running receivable balance and credit limit.
//!
//! Every mutation is recorded in `change_log` (so it reaches the desktop mirror
//! and the LAN hub through the existing sync protocol) and audited.
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{routing::get, Json, Router};
use hms_core::roles::Permission;
use hms_core::{site2::Agency, ApiResponse};
use serde::Deserialize;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one).patch(update))
        .route("/{id}/payments", axum::routing::post(record_payment))
}

#[derive(Deserialize)]
struct ListQuery {
    search: Option<String>,
    include_inactive: Option<bool>,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> AResult<Json<ApiResponse<Vec<Agency>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::AgencyManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "agency_manage required".into(),
        ));
    }
    let rows: Vec<Agency> = sqlx::query_as(
        r#"SELECT id, hotel_id, name, tax_number, tax_office, contact_name, phone, email, address,
                  balance_cents, currency, credit_limit_cents, is_active, notes,
                  created_at, updated_at, deleted_at
           FROM agencies
           WHERE hotel_id = $1 AND deleted_at IS NULL
             AND ($2::boolean OR is_active)
             AND ($3::text IS NULL OR name ILIKE '%' || $3 || '%' OR tax_number = $3)
           ORDER BY name"#,
    )
    .bind(auth.hotel_id)
    .bind(q.include_inactive.unwrap_or(false))
    .bind(q.search)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}

async fn get_one(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<Agency>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::AgencyManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "agency_manage required".into(),
        ));
    }
    let row: Agency = sqlx::query_as(
        r#"SELECT id, hotel_id, name, tax_number, tax_office, contact_name, phone, email, address,
                  balance_cents, currency, credit_limit_cents, is_active, notes,
                  created_at, updated_at, deleted_at
           FROM agencies WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "agency not found".into()))?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize)]
struct CreateAgencyBody {
    name: String,
    tax_number: Option<String>,
    tax_office: Option<String>,
    contact_name: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    address: Option<String>,
    currency: Option<String>,
    credit_limit_cents: Option<i64>,
    notes: Option<String>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateAgencyBody>,
) -> AResult<Json<ApiResponse<Agency>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::AgencyManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "agency_manage required".into(),
        ));
    }
    if body.name.trim().is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "name is required".into()));
    }
    let currency = body.currency.clone().unwrap_or_else(|| "TRY".into());
    if currency.len() != 3 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "currency must be a 3-letter code".into(),
        ));
    }
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        r#"INSERT INTO agencies (id, hotel_id, name, tax_number, tax_office, contact_name, phone, email,
                                 address, currency, credit_limit_cents, notes)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.name)
    .bind(&body.tax_number)
    .bind(&body.tax_office)
    .bind(&body.contact_name)
    .bind(&body.phone)
    .bind(&body.email)
    .bind(&body.address)
    .bind(&currency)
    .bind(body.credit_limit_cents.unwrap_or(0))
    .bind(&body.notes)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create agency: {e}")))?;
    let payload = serde_json::json!({
        "name": body.name, "tax_number": body.tax_number, "currency": currency,
        "credit_limit_cents": body.credit_limit_cents.unwrap_or(0), "is_active": true,
    });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "agency",
        id,
        "insert",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "agency",
        id,
        "created",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    get_one(State(state), auth, Path(id)).await
}

#[derive(Deserialize)]
struct UpdateAgencyBody {
    name: Option<String>,
    tax_number: Option<String>,
    tax_office: Option<String>,
    contact_name: Option<String>,
    phone: Option<String>,
    email: Option<String>,
    address: Option<String>,
    credit_limit_cents: Option<i64>,
    is_active: Option<bool>,
    notes: Option<String>,
}

async fn update(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateAgencyBody>,
) -> AResult<Json<ApiResponse<Agency>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::AgencyManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "agency_manage required".into(),
        ));
    }
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let affected = sqlx::query(
        r#"UPDATE agencies SET
             name = COALESCE($3, name),
             tax_number = COALESCE($4, tax_number),
             tax_office = COALESCE($5, tax_office),
             contact_name = COALESCE($6, contact_name),
             phone = COALESCE($7, phone),
             email = COALESCE($8, email),
             address = COALESCE($9, address),
             credit_limit_cents = COALESCE($10, credit_limit_cents),
             is_active = COALESCE($11, is_active),
             notes = COALESCE($12, notes),
             updated_at = now()
           WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.name)
    .bind(&body.tax_number)
    .bind(&body.tax_office)
    .bind(&body.contact_name)
    .bind(&body.phone)
    .bind(&body.email)
    .bind(&body.address)
    .bind(body.credit_limit_cents)
    .bind(body.is_active)
    .bind(&body.notes)
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .rows_affected();
    if affected == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "agency not found".into()));
    }
    let payload = serde_json::json!({ "name": body.name, "is_active": body.is_active });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "agency",
        id,
        "update",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "agency",
        id,
        "updated",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    get_one(State(state), auth, Path(id)).await
}

#[derive(Deserialize)]
struct AgencyPaymentBody {
    /// Positive = the agency paid us (reduces the receivable).
    amount_cents: i64,
    reference: Option<String>,
}

/// Settle part of an agency's balance. A negative amount re-opens the balance
/// (correction), mirroring the payments table's reversal convention.
async fn record_payment(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<AgencyPaymentBody>,
) -> AResult<Json<ApiResponse<Agency>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::AgencyManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "agency_manage required".into(),
        ));
    }
    if body.amount_cents == 0 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "amount_cents must not be zero".into(),
        ));
    }
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let new_balance: Option<i64> = sqlx::query_scalar(
        r#"UPDATE agencies SET balance_cents = balance_cents - $3, updated_at = now()
           WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL
           RETURNING balance_cents"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(body.amount_cents)
    .fetch_optional(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    let new_balance =
        new_balance.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "agency not found".into()))?;
    let payload = serde_json::json!({
        "amount_cents": body.amount_cents, "reference": body.reference, "balance_cents": new_balance
    });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "agency",
        id,
        "update",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "agency",
        id,
        "payment_recorded",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    get_one(State(state), auth, Path(id)).await
}

/// Agency list is read by the e-invoice dialog too (shared permission check).
pub(crate) async fn agency_exists(
    db: &sqlx::PgPool,
    hotel_id: i64,
    agency_id: Uuid,
) -> Result<bool, ApiError> {
    let found: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM agencies WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL",
    )
    .bind(agency_id)
    .bind(hotel_id)
    .fetch_optional(db)
    .await
    .map_err(ApiError::from)?;
    Ok(found.is_some())
}
