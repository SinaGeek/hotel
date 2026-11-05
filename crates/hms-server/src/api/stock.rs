//! G6: stock items + movements, and hotel expenses.
//!
//! Stock movements are append-only (like payments): on-hand is derived with
//! `SUM(quantity)`, never stored, so concurrent consumption cannot corrupt the
//! count. Expenses are soft-deleted.
use super::{AResult, ApiError};
use crate::auth::AuthUser;
use crate::db;
use crate::AppState;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{
    routing::{get, patch},
    Json, Router,
};
use chrono::NaiveDate;
use hms_core::roles::Permission;
use hms_core::site2::{
    validate_movement_sign, Expense, StockItem, StockMovement, StockMovementType,
};
use hms_core::ApiResponse;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_items).post(create_item))
        .route("/{id}", patch(update_item))
        .route("/{id}/movements", get(list_movements).post(record_movement))
        .route("/{id}/on-hand", get(on_hand))
}

pub fn expenses_router() -> Router<AppState> {
    Router::new()
        .route("/", get(list_expenses).post(create_expense))
        .route("/summary", get(expense_summary))
        .route("/{id}", patch(update_expense))
}

#[derive(Serialize, sqlx::FromRow)]
struct StockItemWithQty {
    id: Uuid,
    sku: Option<String>,
    name: String,
    unit: String,
    unit_cost_cents: i64,
    min_level: i32,
    currency: String,
    is_active: bool,
    on_hand: i64,
    needs_restock: bool,
}

async fn list_items(
    State(state): State<AppState>,
    auth: AuthUser,
) -> AResult<Json<ApiResponse<Vec<StockItemWithQty>>>> {
    if !has_stock_permission(&state, &auth).await? {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "stock_manage required".into(),
        ));
    }
    let rows: Vec<StockItemWithQty> = sqlx::query_as(
        r#"SELECT i.id, i.sku, i.name, i.unit, i.unit_cost_cents, i.min_level, i.currency, i.is_active,
                  COALESCE(SUM(m.quantity), 0)::bigint AS on_hand,
                  (COALESCE(SUM(m.quantity), 0) <= i.min_level) AS needs_restock
           FROM stock_items i
           LEFT JOIN stock_movements m ON m.stock_item_id = i.id AND m.hotel_id = i.hotel_id
           WHERE i.hotel_id = $1 AND i.deleted_at IS NULL
           GROUP BY i.id
           ORDER BY i.name"#,
    )
    .bind(auth.hotel_id)
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}

async fn has_stock_permission(state: &AppState, auth: &AuthUser) -> Result<bool, ApiError> {
    let achieved = crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::StockManage,
    )
    .await?;
    Ok(achieved)
}

#[derive(Deserialize)]
struct CreateItemBody {
    name: String,
    sku: Option<String>,
    unit: Option<String>,
    unit_cost_cents: Option<i64>,
    min_level: Option<i32>,
    currency: Option<String>,
}

async fn create_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateItemBody>,
) -> AResult<Json<ApiResponse<StockItem>>> {
    if !has_stock_permission(&state, &auth).await? {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "stock_manage required".into(),
        ));
    }
    if body.name.trim().is_empty() {
        return Err(ApiError(StatusCode::BAD_REQUEST, "name is required".into()));
    }
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        r#"INSERT INTO stock_items (id, hotel_id, sku, name, unit, unit_cost_cents, min_level, currency)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8)"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.sku)
    .bind(&body.name)
    .bind(body.unit.clone().unwrap_or_else(|| "adet".into()))
    .bind(body.unit_cost_cents.unwrap_or(0))
    .bind(body.min_level.unwrap_or(0))
    .bind(body.currency.clone().unwrap_or_else(|| "TRY".into()))
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create stock item: {e}")))?;
    let payload =
        serde_json::json!({ "name": body.name, "unit": body.unit, "min_level": body.min_level });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "stock_item",
        id,
        "insert",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "stock_item",
        id,
        "created",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: StockItem = sqlx::query_as(
        "SELECT id, hotel_id, sku, name, unit, unit_cost_cents, min_level, currency, is_active, created_at, updated_at, deleted_at
         FROM stock_items WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize)]
struct UpdateItemBody {
    name: Option<String>,
    unit: Option<String>,
    unit_cost_cents: Option<i64>,
    min_level: Option<i32>,
    is_active: Option<bool>,
}

async fn update_item(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateItemBody>,
) -> AResult<Json<ApiResponse<StockItem>>> {
    if !has_stock_permission(&state, &auth).await? {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "stock_manage required".into(),
        ));
    }
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let affected = sqlx::query(
        r#"UPDATE stock_items SET
             name = COALESCE($3, name),
             unit = COALESCE($4, unit),
             unit_cost_cents = COALESCE($5, unit_cost_cents),
             min_level = COALESCE($6, min_level),
             is_active = COALESCE($7, is_active),
             updated_at = now()
           WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.name)
    .bind(&body.unit)
    .bind(body.unit_cost_cents)
    .bind(body.min_level)
    .bind(body.is_active)
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .rows_affected();
    if affected == 0 {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "stock item not found".into(),
        ));
    }
    let payload = serde_json::json!({ "name": body.name, "min_level": body.min_level, "is_active": body.is_active });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "stock_item",
        id,
        "update",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "stock_item",
        id,
        "updated",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: StockItem = sqlx::query_as(
        "SELECT id, hotel_id, sku, name, unit, unit_cost_cents, min_level, currency, is_active, created_at, updated_at, deleted_at
         FROM stock_items WHERE id = $1",
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize)]
struct MovementQuery {
    limit: Option<i64>,
}

async fn list_movements(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Query(q): Query<MovementQuery>,
) -> AResult<Json<ApiResponse<Vec<StockMovement>>>> {
    if !has_stock_permission(&state, &auth).await? {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "stock_manage required".into(),
        ));
    }
    let rows: Vec<StockMovement> = sqlx::query_as(
        r#"SELECT id, hotel_id, stock_item_id, movement_type::text AS movement_type, quantity,
                  unit_cost_cents, room_id, reservation_id, note, created_by, created_at
           FROM stock_movements
           WHERE hotel_id = $1 AND stock_item_id = $2
           ORDER BY created_at DESC
           LIMIT $3"#,
    )
    .bind(auth.hotel_id)
    .bind(id)
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

#[derive(Deserialize)]
struct CreateMovementBody {
    movement_type: String,
    quantity: i32,
    unit_cost_cents: Option<i64>,
    room_id: Option<Uuid>,
    reservation_id: Option<Uuid>,
    note: Option<String>,
}

async fn record_movement(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<CreateMovementBody>,
) -> AResult<Json<ApiResponse<StockMovement>>> {
    if !has_stock_permission(&state, &auth).await? {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "stock_manage required".into(),
        ));
    }
    let movement_type = StockMovementType::parse(&body.movement_type).ok_or_else(|| {
        ApiError(
            StatusCode::BAD_REQUEST,
            "movement_type must be in|out|adjustment".into(),
        )
    })?;
    validate_movement_sign(movement_type, body.quantity)
        .map_err(|e| ApiError(StatusCode::BAD_REQUEST, e))?;

    let movement_id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let inserted = sqlx::query(
        r#"INSERT INTO stock_movements
             (id, hotel_id, stock_item_id, movement_type, quantity, unit_cost_cents, room_id, reservation_id, note, created_by)
           SELECT $1, $2, i.id, $3, $4, $5, $6, $7, $8, $9
           FROM stock_items i
           WHERE i.id = $10 AND i.hotel_id = $2 AND i.deleted_at IS NULL"#,
    )
    .bind(movement_id)
    .bind(auth.hotel_id)
    .bind(movement_type.as_str())
    .bind(body.quantity)
    .bind(body.unit_cost_cents.unwrap_or(0))
    .bind(body.room_id)
    .bind(body.reservation_id)
    .bind(&body.note)
    .bind(auth.user_id)
    .bind(id)
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .rows_affected();
    if inserted == 0 {
        return Err(ApiError(
            StatusCode::NOT_FOUND,
            "stock item not found".into(),
        ));
    }
    let payload = serde_json::json!({
        "stock_item_id": id, "movement_type": movement_type.as_str(), "quantity": body.quantity
    });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "stock_item",
        id,
        "update",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "stock_movement",
        movement_id,
        "recorded",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: StockMovement = sqlx::query_as(
        r#"SELECT id, hotel_id, stock_item_id, movement_type::text AS movement_type, quantity,
                  unit_cost_cents, room_id, reservation_id, note, created_by, created_at
           FROM stock_movements WHERE id = $1"#,
    )
    .bind(movement_id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

async fn on_hand(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
) -> AResult<Json<ApiResponse<i64>>> {
    if !has_stock_permission(&state, &auth).await? {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "stock_manage required".into(),
        ));
    }
    let qty: i64 = sqlx::query_scalar(
        "SELECT COALESCE(SUM(quantity), 0)::bigint FROM stock_movements WHERE hotel_id = $1 AND stock_item_id = $2",
    )
    .bind(auth.hotel_id)
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: qty,
        error: None,
    }))
}

// ------------------------------------------------------------------ expenses

#[derive(Deserialize)]
struct ExpenseQuery {
    from: Option<NaiveDate>,
    to: Option<NaiveDate>,
    category: Option<String>,
    limit: Option<i64>,
}

async fn list_expenses(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ExpenseQuery>,
) -> AResult<Json<ApiResponse<Vec<Expense>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::ExpenseManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "expense_manage required".into(),
        ));
    }
    let rows: Vec<Expense> = sqlx::query_as(
        r#"SELECT id, hotel_id, category, description, amount_cents, currency, spent_on, agency_id,
                  payment_type, invoice_number, created_by, created_at, updated_at, deleted_at
           FROM expenses
           WHERE hotel_id = $1 AND deleted_at IS NULL
             AND ($2::date IS NULL OR spent_on >= $2)
             AND ($3::date IS NULL OR spent_on <= $3)
             AND ($4::text IS NULL OR category = $4)
           ORDER BY spent_on DESC, created_at DESC
           LIMIT $5"#,
    )
    .bind(auth.hotel_id)
    .bind(q.from)
    .bind(q.to)
    .bind(q.category)
    .bind(q.limit.unwrap_or(200).clamp(1, 1000))
    .fetch_all(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: rows,
        error: None,
    }))
}

#[derive(Serialize, sqlx::FromRow)]
struct ExpenseSummaryRow {
    category: String,
    total_cents: i64,
    count: i64,
}

async fn expense_summary(
    State(state): State<AppState>,
    auth: AuthUser,
    Query(q): Query<ExpenseQuery>,
) -> AResult<Json<ApiResponse<Vec<ExpenseSummaryRow>>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::ExpenseManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "expense_manage required".into(),
        ));
    }
    let rows: Vec<ExpenseSummaryRow> = sqlx::query_as(
        r#"SELECT category, SUM(amount_cents)::bigint AS total_cents, count(*)::bigint AS count
           FROM expenses
           WHERE hotel_id = $1 AND deleted_at IS NULL
             AND ($2::date IS NULL OR spent_on >= $2)
             AND ($3::date IS NULL OR spent_on <= $3)
           GROUP BY category ORDER BY total_cents DESC"#,
    )
    .bind(auth.hotel_id)
    .bind(q.from)
    .bind(q.to)
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
struct CreateExpenseBody {
    category: String,
    description: String,
    amount_cents: i64,
    currency: Option<String>,
    spent_on: Option<NaiveDate>,
    agency_id: Option<Uuid>,
    payment_type: Option<String>,
    invoice_number: Option<String>,
}

async fn create_expense(
    State(state): State<AppState>,
    auth: AuthUser,
    Json(body): Json<CreateExpenseBody>,
) -> AResult<Json<ApiResponse<Expense>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::ExpenseManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "expense_manage required".into(),
        ));
    }
    if body.amount_cents <= 0 {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "amount_cents must be positive".into(),
        ));
    }
    if let Some(a) = body.agency_id {
        if !super::agencies::agency_exists(&state.db, auth.hotel_id, a).await? {
            return Err(ApiError(StatusCode::BAD_REQUEST, "unknown agency".into()));
        }
    }
    let id = Uuid::now_v7();
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    sqlx::query(
        r#"INSERT INTO expenses (id, hotel_id, category, description, amount_cents, currency, spent_on,
                                 agency_id, payment_type, invoice_number, created_by)
           VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.category)
    .bind(&body.description)
    .bind(body.amount_cents)
    .bind(body.currency.clone().unwrap_or_else(|| "TRY".into()))
    .bind(body.spent_on.unwrap_or_else(|| chrono::Utc::now().date_naive()))
    .bind(body.agency_id)
    .bind(&body.payment_type)
    .bind(&body.invoice_number)
    .bind(auth.user_id)
    .execute(&mut *conn)
    .await
    .map_err(|e| ApiError(StatusCode::BAD_REQUEST, format!("cannot create expense: {e}")))?;
    let payload = serde_json::json!({
        "category": body.category, "description": body.description, "amount_cents": body.amount_cents
    });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "expense",
        id,
        "insert",
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "expense",
        id,
        "created",
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: Expense = sqlx::query_as(
        r#"SELECT id, hotel_id, category, description, amount_cents, currency, spent_on, agency_id,
                  payment_type, invoice_number, created_by, created_at, updated_at, deleted_at
           FROM expenses WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}

#[derive(Deserialize)]
struct UpdateExpenseBody {
    category: Option<String>,
    description: Option<String>,
    amount_cents: Option<i64>,
    spent_on: Option<NaiveDate>,
    payment_type: Option<String>,
    invoice_number: Option<String>,
    /// soft delete
    deleted: Option<bool>,
}

async fn update_expense(
    State(state): State<AppState>,
    auth: AuthUser,
    Path(id): Path<Uuid>,
    Json(body): Json<UpdateExpenseBody>,
) -> AResult<Json<ApiResponse<Expense>>> {
    if !crate::auth::has_permission(
        &state.db,
        &auth.role,
        auth.hotel_id,
        Permission::ExpenseManage,
    )
    .await?
    {
        return Err(ApiError(
            StatusCode::FORBIDDEN,
            "expense_manage required".into(),
        ));
    }
    if let Some(a) = body.amount_cents {
        if a <= 0 {
            return Err(ApiError(
                StatusCode::BAD_REQUEST,
                "amount_cents must be positive".into(),
            ));
        }
    }
    let op_id = Uuid::now_v7();
    let mut conn = state.db.begin().await.map_err(ApiError::from)?;
    db::set_tenant_scope(&mut conn, auth.hotel_id).await?;
    let affected = sqlx::query(
        r#"UPDATE expenses SET
             category = COALESCE($3, category),
             description = COALESCE($4, description),
             amount_cents = COALESCE($5, amount_cents),
             spent_on = COALESCE($6, spent_on),
             payment_type = COALESCE($7, payment_type),
             invoice_number = COALESCE($8, invoice_number),
             deleted_at = CASE WHEN $9::boolean THEN now() ELSE deleted_at END,
             updated_at = now()
           WHERE id = $1 AND hotel_id = $2 AND deleted_at IS NULL"#,
    )
    .bind(id)
    .bind(auth.hotel_id)
    .bind(&body.category)
    .bind(&body.description)
    .bind(body.amount_cents)
    .bind(body.spent_on)
    .bind(&body.payment_type)
    .bind(&body.invoice_number)
    .bind(body.deleted.unwrap_or(false))
    .execute(&mut *conn)
    .await
    .map_err(ApiError::from)?
    .rows_affected();
    if affected == 0 {
        return Err(ApiError(StatusCode::NOT_FOUND, "expense not found".into()));
    }
    let op = if body.deleted.unwrap_or(false) {
        "delete"
    } else {
        "update"
    };
    let payload =
        serde_json::json!({ "amount_cents": body.amount_cents, "category": body.category });
    db::record_change(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        op_id,
        "expense",
        id,
        op,
        payload.clone(),
    )
    .await?;
    db::write_audit(
        &mut conn,
        auth.hotel_id,
        Some(auth.user_id),
        "expense",
        id,
        op,
        None,
        Some(payload),
    )
    .await?;
    conn.commit().await.map_err(ApiError::from)?;
    let row: Expense = sqlx::query_as(
        r#"SELECT id, hotel_id, category, description, amount_cents, currency, spent_on, agency_id,
                  payment_type, invoice_number, created_by, created_at, updated_at, deleted_at
           FROM expenses WHERE id = $1"#,
    )
    .bind(id)
    .fetch_one(&state.db)
    .await
    .map_err(ApiError::from)?;
    Ok(Json(ApiResponse {
        success: true,
        data: row,
        error: None,
    }))
}
