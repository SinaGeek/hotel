use serde::{Serialize, Deserialize};

/// Standard API envelope: every endpoint answers with `{ success, data, error }`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiResponse<T> {
    pub success: bool,
    pub data: T,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub message: String,
    pub code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest {
    pub username: String,
    pub password: String,
    pub hotel_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest {
    pub username: String,
    pub full_name: String,
    pub email: String,
    pub role: String,
    pub password: String,
    pub hotel_id: i64,
    pub force_password_change: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateRoomStatusRequest {
    pub room_id: String,
    pub cleanliness: String,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PriceRule {
    pub room_type_id: String,
    pub date_from: String,
    pub date_to: String,
    pub price_cents: i64,
    pub currency: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateReservationRequest {
    pub room_id: String,
    pub checkin_date: String,
    pub checkout_date: String,
    pub channel: String,
    pub currency: String,
    pub total_price_cents: i64,
    pub adults: u16,
    pub children: u16,
    pub guest_ids: Vec<String>,
    pub notes: Option<String>,
}
