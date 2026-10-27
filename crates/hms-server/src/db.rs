//! Shared DB helpers: per-transaction tenant scope for RLS, change numbering,
//! change_log rows and audit_log writes.

use axum::http::StatusCode;
use sqlx::PgConnection;

pub type ApiResult<T> = Result<T, (StatusCode, String)>;

pub fn api_err(status: StatusCode, message: impl Into<String>) -> (StatusCode, String) {
    (status, message.into())
}

/// Scope the connection to one tenant for the remainder of the transaction
/// (README §8). Uses set_config(..., is_local=TRUE).
pub async fn set_tenant_scope(conn: &mut PgConnection, hotel_id: i64) -> Result<(), (StatusCode, String)> {
    sqlx::query("SELECT set_config('app.hotel_id', $1, true)")
        .bind(hotel_id.to_string())
        .execute(conn)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("scope: {e}")))?;
    Ok(())
}

/// Platform-admin scope (hotel 0 bootstrap / cross-hotel admin work).
pub async fn set_platform_scope(conn: &mut PgConnection) -> Result<(), (StatusCode, String)> {
    sqlx::query("SELECT set_config('app.is_platform_admin', 'true', true)")
        .execute(conn)
        .await
        .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("scope: {e}")))?;
    Ok(())
}

/// Allocate the next change_seq for the change_log sync stream (README §11):
/// a dedicated PG sequence — monotonically increasing and never row-locked,
/// so concurrent writes don't serialize on a hotels counter row (see 0008).
pub async fn record_change(
    conn: &mut PgConnection,
    hotel_id: i64,
    actor: Option<uuid::Uuid>,
    operation_id: uuid::Uuid,
    entity_type: &str,
    entity_id: uuid::Uuid,
    operation: &str,
    payload: serde_json::Value,
) -> Result<i64, (StatusCode, String)> {
    let seq: i64 = sqlx::query_scalar("SELECT nextval('change_log_seq')")
    .fetch_one(&mut *conn)
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("change_seq: {e}")))?;

    sqlx::query(
        r#"INSERT INTO change_log (hotel_id, change_seq, operation_id, entity_type, entity_id, operation, payload, actor_user_id)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8)"#,
    )
    .bind(hotel_id)
    .bind(seq)
    .bind(operation_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(operation)
    .bind(payload)
    .bind(actor)
    .execute(&mut *conn)
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("change_log: {e}")))?;

    Ok(seq)
}

/// Audit trail entry (who/what/when/old/new — README §7).
pub async fn write_audit(
    conn: &mut PgConnection,
    hotel_id: i64,
    user_id: Option<uuid::Uuid>,
    entity_type: &str,
    entity_id: uuid::Uuid,
    action: &str,
    old_value: Option<serde_json::Value>,
    new_value: Option<serde_json::Value>,
) -> Result<(), (StatusCode, String)> {
    sqlx::query(
        "INSERT INTO audit_log (hotel_id, user_id, entity_type, entity_id, action, old_value, new_value) VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(hotel_id)
    .bind(user_id)
    .bind(entity_type)
    .bind(entity_id)
    .bind(action)
    .bind(old_value)
    .bind(new_value)
    .execute(&mut *conn)
    .await
    .map_err(|e| api_err(StatusCode::INTERNAL_SERVER_ERROR, format!("audit: {e}")))?;
    Ok(())
}
