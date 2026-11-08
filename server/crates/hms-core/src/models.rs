use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RoomCleanliness {
    Clean,
    Dirty,
    Cleaning,
    Inspected,
    OutOfOrder,
}

impl Default for RoomCleanliness {
    fn default() -> Self {
        RoomCleanliness::Clean
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RoomType {
    pub id: Uuid,
    pub hotel_id: i64,
    pub name: String,
    pub capacity: u32,
    pub base_price_cents: i64,
    pub amenities: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Room {
    pub id: Uuid,
    pub hotel_id: i64,
    pub room_type_id: Uuid,
    pub room_number: String,
    pub floor: i32,
    pub cleanliness: RoomCleanliness,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ReservationStatus {
    Confirmed,
    CheckedIn,
    CheckedOut,
    Cancelled,
    /// DB value is 'conflict' (README §9 wording); wire value kept as `conflict`.
    #[serde(rename = "conflict")]
    ConflictNeedsReassignment,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Channel {
    Direct,
    WalkIn,
    BookingCom,
    Expedia,
    Agency,
    Other,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyRate {
    pub date: NaiveDate,
    pub price_cents: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Reservation {
    pub id: Uuid,
    pub hotel_id: i64,
    pub room_id: Uuid,
    pub channel: Channel,
    pub checkin_date: NaiveDate,
    pub checkout_date: NaiveDate,
    pub status: ReservationStatus,
    pub daily_rates: Vec<DailyRate>,
    pub total_price_cents: i64,
    pub currency: String,
    pub adults: u16,
    pub children: u16,
    pub notes: Option<String>,
    pub voucher_number: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Guest {
    pub id: Uuid,
    pub hotel_id: i64,
    pub reservation_id: Option<Uuid>,
    pub full_name: String,
    pub id_number_encrypted: Option<String>,
    pub nationality: Option<String>,
    pub phone_encrypted: Option<String>,
    pub email_encrypted: Option<String>,
    pub license_plate: Option<String>,
    pub birth_date: Option<NaiveDate>,
    pub is_primary: bool,
    pub kbs_reported: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaymentType {
    Cash,
    CreditCard,
    BankTransfer,
    Agency,
    Refund,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Payment {
    pub id: Uuid,
    pub hotel_id: i64,
    pub reservation_id: Uuid,
    pub amount_cents: i64, // positive for collections, negative for reversals/refunds
    pub currency: String,
    pub payment_type: PaymentType,
    pub reference: Option<String>,
    pub created_by: Uuid,
    pub is_reversal: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FolioTransaction {
    pub id: Uuid,
    pub hotel_id: i64,
    pub reservation_id: Uuid,
    pub category: String, // Room, Minibar, Spa, Restaurant, Extra
    pub description: String,
    pub unit_price_cents: i64,
    pub quantity: u32,
    pub total_cents: i64,
    pub currency: String,
    pub created_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DailyRevenueAggregate {
    pub hotel_id: i64,
    pub date: NaiveDate,
    pub room_id: Uuid,
    pub room_type_id: Uuid,
    pub channel: Channel,
    pub room_revenue_cents: i64,
    pub extra_revenue_cents: i64,
    pub payment_collected_cents: i64,
    pub occupied: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuditLogEntry {
    pub id: Uuid,
    pub hotel_id: i64,
    pub user_id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub old_value: Option<serde_json::Value>,
    pub new_value: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserRecord {
    pub id: Uuid,
    pub hotel_id: i64,
    pub username: String,
    pub full_name: String,
    pub email: String,
    pub role: crate::roles::Role,
    pub password_hash: String,
    pub is_active: bool,
    pub force_password_change: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}
