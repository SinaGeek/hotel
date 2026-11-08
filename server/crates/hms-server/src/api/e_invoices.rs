//! G6: e-invoice (GİB / UBL-TR) envelope.
//!
//! This module owns the *document lifecycle* — draft → queued → sent →
//! accepted/rejected, plus cancellation. It does NOT talk to GİB: submission is
//! performed by a licensed entegratör (external service) which polls `queued`
//! rows, posts the UBL-TR payload, and writes the result back through
//! `PATCH /e-invoices/{id}/status`. That boundary keeps credentials and network
//! dependencies out of this repository while still giving reception a single
//! place to issue, track and re-print invoices.
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{
    routing::{get, patch, post},
    Json, Router,
};
use hms_core::roles::Permission;
use hms_core::site2::{EInvoice, EInvoiceStatus};
use hms_core::ApiResponse;
use serde::Deserialize;
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list).post(create))
        .route("/{id}", get(get_one))
        .route("/{id}/status", patch(change_status))
        .route("/{id}/from-folio", post(create_from_folio))
}

/// Numbered invoices are issued per hotel per year: `INV-<year>-<6-digit>`.
async fn next_invoice_number(state: &AppState, hotel_id: i64) -> Result<String, ApiError> {
    let year: i32 = sqlx::query_scalar("SELECT EXTRACT(year FROM now())::int")
        .fetch_one(&state.db)
        .await
        .map_err(ApiError::from)?;
    let seq: i64 = sqlx::query_scalar(
        "SELECT count(*)::bigint + 1 FROM e_invoices WHERE hotel_id = $1 AND invoice_number LIKE $2",
    )
    .bind(hotel_id)
    .bind(format!("INV-{year}-%"))
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(format!("INV-{year}-{seq:06}"))
}

#[derive(Deserialize)]
struct ListQuery {
    status: Option<String>,
    limit: Option<i64>,
}

async fn list(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ListQuery>,
) -> AResult<Json<ApiResponse<Vec<EInvoice>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::InvoiceManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "invoice_manage required".into(),
        ));
    }
    let rows: Vec<EInvoice> = sqlx::query_as(
        r#"SELECT id, hotel_id, reservation_id, agency_id, invoice_number, scenario, invoice_type,
                  uuid_gib, tax_number, status::text AS status, net_cents, vat_cents, total_cents,
                  currency, NULL::text AS ubl_xml, error_message, issued_at, created_by, created_at, updated_at
           FROM e_invoices
           WHERE hotel_id = $1 AND ($2::text IS NULL OR status::text = $2)
           ORDER BY created_at DESC
           LIMIT $3"#,
    )
    .bind(auth.hotel_id)
    .bind(q.status)
    .bind(q.limit.unwrap_or(100).clamp(1, 500))
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
) -> AResult<Json<ApiResponse<EInvoice>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::InvoiceManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "invoice_manage required".into(),
        ));
    }
    let row: EInvoice = sqlx::query_as(
        r#"SELECT id, hotel_id, reservation_id, agency_id, invoice_number, scenario, invoice_type,
                  uuid_gib, tax_number, status::text AS status, net_cents, vat_cents, total_cents,
                  currency, ubl_xml, error_message, issued_at, created_by, created_at, updated_at
           FROM e_invoices WHERE id = $1 AND hotel_id = $2"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "invoice not found".into()))?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize)]
struct CreateInvoiceBody {
    reservation_id: Option<Uuid>,
    agency_id: Option<Uuid>,
    scenario: Option<String>,
    invoice_type: Option<String>,
    tax_number: Option<String>,
    net_cents: i64,
    vat_cents: i64,
    currency: Option<String>,
    ubl_xml: Option<String>,
    /// Queue immediately for the entegratör instead of leaving a draft.
    submit: Option<bool>,
}

async fn create(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateInvoiceBody>,
) -> AResult<Json<ApiResponse<EInvoice>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::InvoiceManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "invoice_manage required".into(),
        ));
    }
    if body.net_cents < 0 || body.vat_cents < 0 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "net_cents and vat_cents must be >= 0".into(),
        ));
    }
    let total_cents = body.net_cents.checked_add(body.vat_cents).ok_or_else(|| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "invoice total exceeds supported range".into(),
        )
    })?;
    let invoice_type = body.invoice_type.clone().unwrap_or_else(|| "SATIS".into());
    if !["SATIS", "IADE", "TEVKIFAT", "ISTISNA"].contains(&invoice_type.as_str()) {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "invalid invoice_type".into(),
        ));
    }
    let status = if body.submit.unwrap_or(false) {
        "queued"
    } else {
        "draft"
    };
    let invoice_number = next_invoice_number(&state, auth.hotel_id).await?;
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();

    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        r#"INSERT INTO e_invoices
             (id, hotel_id, reservation_id, agency_id, invoice_number, scenario, invoice_type,
              tax_number, status, net_cents, vat_cents, total_cents, currency, ubl_xml, created_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(body.reservation_id)
    .bind(body.agency_id)
    .bind(&invoice_number)
    .bind(
        body.scenario
            .clone()
            .unwrap_or_else(|| "TEMELFATURA".into()),
    )
    .bind(&invoice_type)
    .bind(&body.tax_number)
    .bind(status)
    .bind(body.net_cents)
    .bind(body.vat_cents)
    .bind(total_cents)
    .bind(body.currency.clone().unwrap_or_else(|| "TRY".into()))
    .bind(&body.ubl_xml)
    .bind(auth.user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| {
        ApiError(
            StatusCode::BAD_REQUEST,
            format!("cannot create invoice: {e}"),
        )
    })?;
    let payload = serde_json::json!({
        "invoice_number": invoice_number, "status": status,
        "net_cents": body.net_cents, "vat_cents": body.vat_cents, "total_cents": total_cents,
    });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "e_invoice",
        id,
        "insert",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "e_invoice",
        id,
        "created",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    get_one(State(state), auth, Path(id)).await
}

/// Build a draft invoice from a reservation's folio totals. The folio endpoint
/// is the source of truth for the amount, so reception never retypes money.
async fn create_from_folio(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<EInvoice>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::InvoiceManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "invoice_manage required".into(),
        ));
    }
    let totals: Option<(i64, String)> = sqlx::query_as(
        r#"SELECT
             (COALESCE((SELECT SUM(total_cents) FROM folio_transactions WHERE reservation_id = r.id AND hotel_id = r.hotel_id), 0)
              + COALESCE(r.total_price_cents, 0))::bigint AS total_cents,
             r.currency
           FROM reservations r
           WHERE r.id = $1 AND r.hotel_id = $2 AND r.deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .fetch_optional(&state.db)
    .await
    .map_err(ApiError::from)?;
    let (total_cents, currency) =
        totals.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "reservation not found".into()))?;
    // Do not infer Turkish VAT from a generic folio total: rates, exemptions,
    // withholding and invoice scenarios depend on fiscal/product decisions.
    // Start at zero VAT and let the authorized operator/entegratör populate
    // the legally correct tax breakdown before submission.
    let vat_cents = 0;
    let net_cents = total_cents;
    create(
        State(state),
        auth,
        Json(CreateInvoiceBody {
            reservation_id: Some(id),
            agency_id: None,
            scenario: Some("TEMELFATURA".into()),
            invoice_type: Some("SATIS".into()),
            tax_number: None,
            net_cents,
            vat_cents,
            currency: Some(currency),
            ubl_xml: None,
            submit: Some(false),
        }),
    )
    .await
}

#[derive(Deserialize)]
struct StatusBody {
    status: String,
    uuid_gib: Option<String>,
    error_message: Option<String>,
}

/// Advance the lifecycle. Illegal transitions (e.g. draft → accepted, or any
/// move out of a terminal state) are rejected with 409.
async fn change_status(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<StatusBody>,
) -> AResult<Json<ApiResponse<EInvoice>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::InvoiceManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "invoice_manage required".into(),
        ));
    }
    let next = EInvoiceStatus::parse(&body.status)
        .ok_or_else(|| ApiError(StatusCode::BAD_REQUEST, "invalid status".into()))?;
    let current: Option<(String,)> =
        sqlx::query_as("SELECT status::text FROM e_invoices WHERE id = $1 AND hotel_id = $2")
            .bind(id)
            .bind(auth.hotel_id)
            .fetch_optional(&state.db)
            .await
            .map_err(ApiError::from)?;
    let (current_str,) =
        current.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "invoice not found".into()))?;
    let current = EInvoiceStatus::parse(&current_str).ok_or_else(|| {
        ApiError(
            StatusCode::INTERNAL_SERVER_ERROR,
            "stored status invalid".into(),
        )
    })?;
    if !current.can_transition_to(next) {
        return Err(ApiError(
            StatusCode::CONFLICT,
            format!(
                "illegal transition {} -> {}",
                current.as_str(),
                next.as_str()
            ),
        ));
    }

    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        r#"UPDATE e_invoices SET
             status = $3,
             uuid_gib = COALESCE($4, uuid_gib),
             error_message = COALESCE($5, error_message),
             issued_at = CASE WHEN $3 IN ('sent','accepted') THEN COALESCE(issued_at, now()) ELSE issued_at END,
             updated_at = now()
           WHERE id = $1 AND hotel_id = $2"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(next.as_str())
    .bind(&body.uuid_gib)
    .bind(&body.error_message)
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?;
    let payload = serde_json::json!({
        "status": next.as_str(), "uuid_gib": body.uuid_gib, "error_message": body.error_message
    });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "e_invoice",
        id,
        "update",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "e_invoice",
        id,
        "status_changed",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    get_one(State(state), auth, Path(id)).await
}
