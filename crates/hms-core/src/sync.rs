use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationType {
    Insert,
    Update,
    Delete, // Tombstone
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityType {
    RoomType,
    Room,
    RoomStatus,
    Reservation,
    Guest,
    Payment,
    FolioTransaction,
    PriceRule,
    User,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangeOperation {
    /// Client-generated UUIDv7 for deduplication / idempotency
    pub operation_id: Uuid,
    pub hotel_id: i64,
    pub entity_type: EntityType,
    pub entity_id: Uuid,
    pub operation_type: OperationType,
    pub payload: serde_json::Value,
    pub client_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncOpStatus {
    Applied,
    DuplicateIgnored,
    ConflictResolved { message: String },
    Rejected { reason: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncOpResult {
    pub operation_id: Uuid,
    pub entity_type: EntityType,
    pub entity_id: Uuid,
    pub status: SyncOpStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPushRequest {
    pub hotel_id: i64,
    pub operations: Vec<ChangeOperation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPushResponse {
    pub hotel_id: i64,
    pub results: Vec<SyncOpResult>,
    pub server_latest_counter: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullRequest {
    pub hotel_id: i64,
    pub since_counter: i64,
    pub limit: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerChangeRecord {
    pub change_counter: i64,
    pub hotel_id: i64,
    pub operation_id: Uuid,
    pub entity_type: EntityType,
    pub entity_id: Uuid,
    pub operation_type: OperationType,
    pub payload: serde_json::Value,
    pub server_timestamp: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncPullResponse {
    pub hotel_id: i64,
    pub records: Vec<ServerChangeRecord>,
    pub latest_counter: i64,
    pub has_more: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OfflineLoginCache {
    pub user_id: Uuid,
    pub hotel_id: i64,
    pub username: String,
    pub role: crate::roles::Role,
    pub password_hash: String,
    pub cached_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>, // 14 days default
}

impl OfflineLoginCache {
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        now <= self.expires_at
    }
}
