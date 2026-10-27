pub mod channels;
pub mod guests;
pub mod hotels;
pub mod payments;
pub mod price_rules;
pub mod reports;
pub mod reservations;
pub mod roles;
pub mod rooms;
pub mod users;

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use hms_core::ApiResponse;

/// Uniform API error: status + human message
pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.0, Json(ApiResponse::<()> { success: false, data: (), error: Some(self.1) })).into_response()
    }
}

impl From<(StatusCode, String)> for ApiError {
    fn from(v: (StatusCode, String)) -> Self {
        ApiError(v.0, v.1)
    }
}

pub type AResult<T> = Result<T, ApiError>;

impl From<hms_core::validation::ValidationError> for ApiError {
    fn from(e: hms_core::validation::ValidationError) -> Self {
        ApiError(
            StatusCode::from_u16(e.status_code()).unwrap_or(StatusCode::BAD_REQUEST),
            e.to_string(),
        )
    }
}

pub fn router() -> axum::Router<crate::AppState> {
    axum::Router::new()
        .nest("/hotels", hotels::router())
        .nest("/users", users::router())
        .nest("/roles", roles::router())
        .nest("/room-types", rooms::room_types())
        .nest("/rooms", rooms::rooms())
        .nest("/guests", guests::router())
        .nest("/reservations", reservations::router())
        .nest("/reservations", reservations::payments())
        .nest("/reservations", payments::folio_router())
        .nest("/payments", reservations::payment_reversals())
        .nest("/price-rules", price_rules::router())
        .nest("/channels", channels::router())
        .nest("/board", reservations::board_router())
        .nest("/reports", reports::router())
}
