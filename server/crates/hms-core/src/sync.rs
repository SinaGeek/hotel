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
    Channel,
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
    /// Sync protocol version of the sending client (README §11.6). Servers
    /// compare against SYNC_PROTOCOL_MIN_VERSION and reject older clients.
    #[serde(default = "default_protocol_version")]
    pub protocol_version: u32,
}

/// Current client sync protocol version (README §11.6). Bump on a breaking
/// protocol change; the server's SYNC_PROTOCOL_MIN_VERSION decides which old
/// desktop builds in the field still sync.
pub const SYNC_PROTOCOL_VERSION: u32 = 1;

fn default_protocol_version() -> u32 {
    SYNC_PROTOCOL_VERSION
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

/// Default expiry for cached desktop credentials (README §15 OFFLINE_LOGIN_DAYS).
pub const DEFAULT_OFFLINE_LOGIN_DAYS: i64 = 14;

impl OfflineLoginCache {
    pub fn is_valid(&self, now: DateTime<Utc>) -> bool {
        now <= self.expires_at
    }

    /// Build a cache after a SUCCESSFUL online login: the desktop stores the
    /// argon2 password hash (the verifier) plus role/permissions so the app can
    /// verify logins with no server reachable. Expire after OFFLINE_LOGIN_DAYS
    /// (default 14).
    pub fn create(
        user_id: Uuid,
        hotel_id: i64,
        username: String,
        role: crate::roles::Role,
        password_hash: String,
        now: DateTime<Utc>,
        days: i64,
    ) -> Self {
        let days = if days > 0 {
            days
        } else {
            DEFAULT_OFFLINE_LOGIN_DAYS
        };
        Self {
            user_id,
            hotel_id,
            username,
            role,
            password_hash,
            cached_at: now,
            expires_at: now + chrono::Duration::days(days),
        }
    }

    /// Refresh the window after another successful online login (same user):
    /// keeps the original cached_at for audit but pushes the expiry out again.
    pub fn refresh(mut self, now: DateTime<Utc>, days: i64) -> Self {
        let days = if days > 0 {
            days
        } else {
            DEFAULT_OFFLINE_LOGIN_DAYS
        };
        self.expires_at = now + chrono::Duration::days(days);
        self
    }

    /// Lockout path (README §11.7): a user deactivated on the server is locked
    /// out at the NEXT SYNC, not while offline. The engine calls this when a
    /// sync reports the account as inactive (401/403) so the caller can wipe
    /// the persisted cache and force an online login.
    pub fn invalidate(&mut self) {
        self.expires_at = chrono::DateTime::<chrono::Utc>::MIN_UTC;
    }
}
