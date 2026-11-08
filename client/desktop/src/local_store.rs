//! SQLite-backed implementation of [`hms_sync::store::SyncStore`] for the
//! desktop local mirror (README §11): outbox, pull cursor, remote applier and
//! status counters all live in the embedded database. `apply_remote` runs in
//! the same connection but keeps its integrity via the `applied_ops` dedupe
//! table so replays of the same record are no-ops.

use crate::dpapi;
use chrono::Utc;
use hms_core::sync::{
    ChangeOperation, EntityType, OfflineLoginCache, OperationType, ServerChangeRecord,
};
use hms_sync::store::{QueuedOperation, RemoteOutcome, Settle, StoreError, SyncStore};
use rusqlite::{params, Connection, OptionalExtension};
use std::path::Path;
use uuid::Uuid;

pub struct LocalStore {
    /// Direct handle for the offline report reads (`reports.rs`); sync engine
    /// access stays behind the `SyncStore` trait methods.
    pub(crate) conn: Connection,
}

fn se(e: rusqlite::Error) -> StoreError {
    StoreError::Io(e.to_string())
}

impl LocalStore {
    /// Open (creating/tracking the schema as needed). The database is
    /// SQLCipher-encrypted (todo C2/C3): the key is DPAPI-protected beside it
    /// and an existing plaintext install is migrated on first open. On Windows
    /// the session token + offline cache are stored DPAPI-protected inside `kv`.
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let key = crate::sqlcipher::load_or_create_key(path).map_err(StoreError::Io)?;
        let conn = crate::sqlcipher::open_encrypted(path, &key).map_err(StoreError::Io)?;
        conn.pragma_update(None, "journal_mode", "WAL")
            .map_err(se)?;
        conn.pragma_update(None, "synchronous", "NORMAL")
            .map_err(se)?;
        Self::init_connection(conn)
    }

    fn init_connection(conn: Connection) -> Result<Self, StoreError> {
        conn.execute_batch(include_str!("../desktop-migrations/0001_init.sql"))
            .map_err(se)?;
        // A3: per-night rate column for existing mirrors (guarded, idempotent)
        let user_version: i64 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .map_err(se)?;
        if user_version < 2 {
            conn.execute_batch(include_str!("../desktop-migrations/0002_rate_columns.sql"))
                .map_err(se)?;
            conn.pragma_update(None, "user_version", 2).map_err(se)?;
        }
        if user_version < 3 {
            conn.execute_batch(include_str!("../desktop-migrations/0003_local_users.sql"))
                .map_err(se)?;
            conn.pragma_update(None, "user_version", 3).map_err(se)?;
        }
        if user_version < 4 {
            conn.execute_batch(include_str!("../desktop-migrations/0004_desktop_workflows.sql"))
                .map_err(se)?;
            conn.pragma_update(None, "user_version", 4).map_err(se)?;
        }
        if user_version < 5 {
            conn.execute_batch(include_str!("../desktop-migrations/0005_desktop_domain_records.sql"))
                .map_err(se)?;
            // Reservation group is additive; only add it if not present.
            let has_group: bool = conn.query_row(
                "SELECT EXISTS(SELECT 1 FROM pragma_table_info('reservations') WHERE name = 'reservation_group_id')",
                [], |r| r.get(0),
            ).map_err(se)?;
            if !has_group {
                conn.execute("ALTER TABLE reservations ADD COLUMN reservation_group_id TEXT", []).map_err(se)?;
            }
            conn.pragma_update(None, "user_version", 5).map_err(se)?;
        }
        if user_version < 6 {
            for (table, column) in [
                ("guests", "hotel_id"), ("room_types", "hotel_id"),
                ("rooms", "hotel_id"), ("channels", "hotel_id"),
                ("reservations", "hotel_id"),
            ] {
                let has_column: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
                    params![table, column], |r| r.get(0),
                ).map_err(se)?;
                if !has_column {
                    conn.execute(&format!("ALTER TABLE {table} ADD COLUMN {column} INTEGER NOT NULL DEFAULT 4442"), []).map_err(se)?;
                }
            }
            conn.pragma_update(None, "user_version", 6).map_err(se)?;
        }
        if user_version < 7 {
            for (table, column) in [
                ("room_types", "weekend_price_cents"), ("rooms", "name"),
                ("price_rules", "hotel_id"),
            ] {
                let has_column: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info(?1) WHERE name = ?2)",
                    params![table, column], |r| r.get(0),
                ).map_err(se)?;
                if !has_column {
                    let sql = match column {
                        "hotel_id" => format!("ALTER TABLE {table} ADD COLUMN {column} INTEGER NOT NULL DEFAULT 4442"),
                        _ => format!("ALTER TABLE {table} ADD COLUMN {column}"),
                    };
                    conn.execute(&sql, []).map_err(se)?;
                }
            }
            conn.pragma_update(None, "user_version", 7).map_err(se)?;
        }
        if user_version < 8 {
            // Booking detail header fields (registration date / voucher / notes)
            // — nullable, so existing mirrors stay valid without backfill.
            for column in ["voucher_number", "created_at", "notes"] {
                let has_column: bool = conn.query_row(
                    "SELECT EXISTS(SELECT 1 FROM pragma_table_info('reservations') WHERE name = ?1)",
                    params![column], |r| r.get(0),
                ).map_err(se)?;
                if !has_column {
                    conn.execute(&format!("ALTER TABLE reservations ADD COLUMN {column}"), []).map_err(se)?;
                }
            }
            conn.pragma_update(None, "user_version", 8).map_err(se)?;
        }
        let mut store = Self { conn };
        // Provision default users on first run (hotel 4442 + admin user)
        store.provision_default_users()?;
        Ok(store)
    }

    // -- read surface used by the tests and the upcoming local-first UI wiring
    // (per-page reads land with the web/desktop integration step)

    #[cfg(test)]
    pub fn with_in_memory() -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory().map_err(se)?;
        let _ = conn.pragma_update(None, "cipher_memory_security", "ON");
        Self::init_connection(conn)
    }

    /// Test-only: drop every mirror row so a test starts from a known state
    /// on an on-disk (encrypted) database.
    #[cfg(test)]
    pub fn kill_all_tables_for_test(&mut self) {
        for t in [
            "applied_ops",
            "outbox",
            "failures",
            "guests",
            "room_types",
            "rooms",
            "channels",
            "price_rules",
            "reservations",
            "payments",
            "folio_transactions",
        ] {
            let _ = self.conn.execute(&format!("DELETE FROM {t}"), []);
        }
    }

    // ---- offline login cache (kv-backed, DPAPI-protected sensitive fields)

    pub fn save_offline_cache(&mut self, cache: &OfflineLoginCache) -> Result<(), StoreError> {
        let value = serde_json::to_string(cache).map_err(|e| StoreError::Io(e.to_string()))?;
        let protected = dpapi::protect(value.as_bytes()).map_err(StoreError::Io)?;
        let hex = hex_encode(&protected);
        self.kv_set("offline_login_cache", &hex)
    }

    pub fn load_offline_cache(&mut self) -> Result<Option<OfflineLoginCache>, StoreError> {
        let Some(hex) = self
            .conn
            .query_row(
                "SELECT value FROM kv WHERE key = 'offline_login_cache'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(se)?
        else {
            return Ok(None);
        };
        let bytes = hex_decode(&hex).map_err(StoreError::Io)?;
        let plain = dpapi::unprotect(&bytes).map_err(StoreError::Io)?;
        let s = String::from_utf8(plain).map_err(|e| StoreError::Io(e.to_string()))?;
        serde_json::from_str(&s)
            .map(Some)
            .map_err(|e| StoreError::Io(e.to_string()))
    }

    pub fn drop_offline_cache(&mut self) -> Result<(), StoreError> {
        self.conn
            .execute("DELETE FROM kv WHERE key = 'offline_login_cache'", [])
            .map_err(se)?;
        Ok(())
    }

    // ---- local users (Settings ▸ bulk import ▸ users): always available,
    // offline-first logins that do not depend on a prior cloud login and do
    // not expire like the sync-cached offline login does.

    pub fn list_domain_records(&self, entity: &str, hotel_id: i64) -> Result<Vec<serde_json::Value>, StoreError> {
        let sql = match entity {
            "rooms" => "SELECT r.id, r.hotel_id, r.room_type_id, r.room_number, r.name, r.floor, r.cleanliness, r.deleted_at FROM rooms r LEFT JOIN room_types t ON t.id = r.room_type_id WHERE r.hotel_id=?1 ORDER BY t.name, r.room_number",
            "room_types" => "SELECT id, hotel_id, name, capacity, base_price_cents, weekend_price_cents FROM room_types WHERE hotel_id=?1 AND deleted_at IS NULL ORDER BY name",
            "guests" => "SELECT id, hotel_id, full_name, nationality, license_plate, birth_date FROM guests WHERE hotel_id=?1 AND deleted_at IS NULL ORDER BY full_name",
            "reservations" => "SELECT id, hotel_id, room_id, channel_id, guest_id, checkin_date, checkout_date, status, total_price_cents, currency, adults, children, daily_rates, external_ref, reservation_group_id FROM reservations WHERE hotel_id=?1 AND deleted_at IS NULL ORDER BY checkin_date",
            "channels" => "SELECT id, hotel_id, kind, name, is_active FROM channels WHERE hotel_id=?1 ORDER BY name",
            "restaurants" => "SELECT id, hotel_id, name, capacity, is_active, configuration_json FROM restaurants WHERE hotel_id=?1 ORDER BY name",
            "price_rules" => "SELECT id, hotel_id, room_type_id, date_from, date_to, price_cents, currency FROM price_rules WHERE hotel_id=?1 ORDER BY room_type_id, date_from",
            // payments/folio have no hotel_id/deleted_at; scope through the hotel's reservations
            "payments" => "SELECT p.id, p.reservation_id, p.amount_cents, p.currency, p.payment_type, p.is_reversal, p.created_at FROM payments p JOIN reservations r ON r.id = p.reservation_id WHERE r.hotel_id=?1 ORDER BY p.created_at",
            "folio" => "SELECT f.id, f.reservation_id, f.category, f.description, f.unit_price_cents, f.quantity, f.total_cents, f.currency FROM folio_transactions f JOIN reservations r ON r.id = f.reservation_id WHERE r.hotel_id=?1 ORDER BY id",
            _ => return Err(StoreError::Io("unsupported local entity".into())),
        };
        let mut stmt = self.conn.prepare(sql).map_err(se)?;
        let rows = stmt.query_map(params![hotel_id], |row| {
            let count = row.as_ref().column_count();
            let mut value = serde_json::Map::new();
            for idx in 0..count {
                let name = row.as_ref().column_name(idx).unwrap_or("field").to_string();
                let field: rusqlite::types::Value = row.get(idx)?;
                let json = match field {
                    rusqlite::types::Value::Null => serde_json::Value::Null,
                    rusqlite::types::Value::Integer(v) => serde_json::json!(v),
                    rusqlite::types::Value::Real(v) => serde_json::json!(v),
                    rusqlite::types::Value::Text(v) => serde_json::json!(v),
                    rusqlite::types::Value::Blob(_) => serde_json::Value::Null,
                };
                value.insert(name, json);
            }
            Ok(serde_json::Value::Object(value))
        }).map_err(se)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(se)
    }

    pub fn save_domain_record(&self, entity: &str, hotel_id: i64, id: &str, value: &serde_json::Value) -> Result<(), StoreError> {
        let required = |key: &str| value.get(key).and_then(|v| v.as_str()).ok_or_else(|| StoreError::Io(format!("missing {key}")));
        let now = Utc::now().timestamp_millis();
        match entity {
            "room_types" => {
                let name = required("name")?;
                let capacity = value.get("capacity").and_then(|v| v.as_i64()).unwrap_or(2);
                let base = value.get("base_price_cents").and_then(|v| v.as_i64()).unwrap_or(0);
                let weekend = value.get("weekend_price_cents").and_then(|v| v.as_i64());
                self.conn.execute("INSERT INTO room_types(id, hotel_id, name, capacity, base_price_cents, weekend_price_cents) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET name=excluded.name,capacity=excluded.capacity,base_price_cents=excluded.base_price_cents,weekend_price_cents=excluded.weekend_price_cents", params![id,hotel_id,name,capacity,base,weekend]).map_err(se)?;
            }
            "rooms" => {
                let number = required("room_number")?;
                let room_type = required("room_type_id")?;
                let floor = value.get("floor").and_then(|v| v.as_i64()).unwrap_or(0);
                let clean = value.get("cleanliness").and_then(|v| v.as_str()).unwrap_or("clean");
                let name = value.get("name").and_then(|v| v.as_str());
                self.conn.execute("INSERT INTO rooms(id, hotel_id, room_type_id, room_number, floor, cleanliness, name) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET room_type_id=excluded.room_type_id,room_number=excluded.room_number,floor=excluded.floor,cleanliness=excluded.cleanliness,name=excluded.name", params![id,hotel_id,room_type,number,floor,clean,name]).map_err(se)?;
            }
            "guests" => {
                let name = required("full_name")?;
                self.conn.execute("INSERT INTO guests(id, hotel_id, full_name, nationality, license_plate, birth_date) VALUES(?1,?2,?3,?4,?5,?6) ON CONFLICT(id) DO UPDATE SET full_name=excluded.full_name,nationality=excluded.nationality,license_plate=excluded.license_plate,birth_date=excluded.birth_date", params![id,hotel_id,name,value.get("nationality").and_then(|v| v.as_str()),value.get("license_plate").and_then(|v| v.as_str()),value.get("birth_date").and_then(|v| v.as_str())]).map_err(se)?;
            }
            "channels" => {
                let name = required("name")?;
                let kind = value.get("kind").and_then(|v| v.as_str()).unwrap_or("other");
                let active = value.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true);
                self.conn.execute("INSERT INTO channels(id, hotel_id, kind, name, is_active) VALUES(?1,?2,?3,?4,?5) ON CONFLICT(id) DO UPDATE SET kind=excluded.kind,name=excluded.name,is_active=excluded.is_active", params![id,hotel_id,kind,name,active as i64]).map_err(se)?;
            }
            "restaurants" => {
                let name = required("name")?;
                let capacity = value.get("capacity").and_then(|v| v.as_i64()).unwrap_or(0);
                let active = value.get("is_active").and_then(|v| v.as_bool()).unwrap_or(true);
                let config = serde_json::to_string(value.get("specifications").unwrap_or(&serde_json::json!({}))).map_err(|e| StoreError::Io(e.to_string()))?;
                self.conn.execute("INSERT INTO restaurants(id, hotel_id, name, capacity, is_active, configuration_json, created_ms, updated_ms) VALUES(?1,?2,?3,?4,?5,?6,?7,?7) ON CONFLICT(id) DO UPDATE SET name=excluded.name,capacity=excluded.capacity,is_active=excluded.is_active,configuration_json=excluded.configuration_json,updated_ms=excluded.updated_ms", params![id,hotel_id,name,capacity,active as i64,config,now]).map_err(se)?;
            }
            "price_rules" => {
                // Calendar override for a room type. date_from == date_to sets a
                // single-night price; a range acts as a batch change.
                let room_type = required("room_type_id")?;
                let from = required("date_from")?;
                let to = required("date_to")?;
                let cents = value.get("price_cents").and_then(|v| v.as_i64()).ok_or_else(|| StoreError::Io("missing price_cents".into()))?;
                let currency = value.get("currency").and_then(|v| v.as_str()).unwrap_or("TRY");
                self.conn.execute("INSERT INTO price_rules(id, hotel_id, room_type_id, date_from, date_to, price_cents, currency) VALUES(?1,?2,?3,?4,?5,?6,?7) ON CONFLICT(id) DO UPDATE SET room_type_id=excluded.room_type_id,date_from=excluded.date_from,date_to=excluded.date_to,price_cents=excluded.price_cents,currency=excluded.currency", params![id,hotel_id,room_type,from,to,cents,currency]).map_err(se)?;
            }
            "reservations" => {
                let room = required("room_id")?;
                let checkin = required("checkin_date")?;
                let checkout = required("checkout_date")?;
                let status = value.get("status").and_then(|v| v.as_str()).unwrap_or("confirmed");
                let guest_id = value.get("guest_id").and_then(|v| v.as_str());
                let tx = self.conn.unchecked_transaction().map_err(se)?;
                tx.execute("INSERT INTO reservations(id,hotel_id,room_id,guest_id,checkin_date,checkout_date,status,reservation_group_id,voucher_number,notes) VALUES(?1,?2,?3,?4,?5,?6,?7,?8,?9,?10) ON CONFLICT(id) DO UPDATE SET room_id=excluded.room_id,guest_id=excluded.guest_id,checkin_date=excluded.checkin_date,checkout_date=excluded.checkout_date,status=excluded.status,reservation_group_id=excluded.reservation_group_id,voucher_number=COALESCE(excluded.voucher_number,reservations.voucher_number),notes=COALESCE(excluded.notes,reservations.notes)", params![id,hotel_id,room,guest_id,checkin,checkout,status,value.get("reservation_group_id").and_then(|v| v.as_str()),value.get("voucher_number").and_then(|v| v.as_str()),value.get("notes").and_then(|v| v.as_str())]).map_err(se)?;
                if let Some(guest_ids) = value.get("guest_ids").and_then(|v| v.as_array()) {
                    tx.execute("DELETE FROM guest_reservation_links WHERE reservation_id=?1", params![id]).map_err(se)?;
                    for guest_id in guest_ids.iter().filter_map(|v| v.as_str()) {
                        tx.execute("INSERT OR IGNORE INTO guest_reservation_links(reservation_id,guest_id,link_type) VALUES(?1,?2,'guest')", params![id,guest_id]).map_err(se)?;
                    }
                }
                tx.commit().map_err(se)?;
            }
            _ => return Err(StoreError::Io("unsupported local entity".into())),
        }
        Ok(())
    }

    pub fn delete_domain_record(&self, entity: &str, hotel_id: i64, id: &str) -> Result<(), StoreError> {
        let table = match entity { "rooms" => "rooms", "room_types" => "room_types", "guests" => "guests", "reservations" => "reservations", "channels" => "channels", "restaurants" => "restaurants", _ => return Err(StoreError::Io("unsupported local entity".into())) };
        if entity == "price_rules" {
            // Calendar overrides have no soft-delete column: remove the row.
            self.conn.execute("DELETE FROM price_rules WHERE hotel_id=?1 AND id=?2", params![hotel_id, id]).map_err(se)?;
            return Ok(());
        }
        if entity == "restaurants" {
            self.conn.execute("UPDATE restaurants SET is_active=0, updated_ms=?3 WHERE hotel_id=?1 AND id=?2", params![hotel_id, id, Utc::now().timestamp_millis()]).map_err(se)?;
        } else {
            self.conn.execute(&format!("UPDATE {table} SET deleted_at=?3 WHERE hotel_id=?1 AND id=?2"), params![hotel_id, id, Utc::now().timestamp_millis()]).map_err(se)?;
        }
        Ok(())
    }

    pub fn restore_domain_record(&self, entity: &str, hotel_id: i64, id: &str) -> Result<(), StoreError> {
        // Soft-deleted rows come back live; price_rules are hard-deleted (no-op).
        match entity {
            "restaurants" => {
                self.conn.execute(
                    "UPDATE restaurants SET is_active=1, updated_ms=?3 WHERE hotel_id=?1 AND id=?2",
                    params![hotel_id, id, Utc::now().timestamp_millis()],
                ).map_err(se)?;
            }
            "rooms" | "room_types" | "guests" | "reservations" | "channels" => {
                self.conn.execute(
                    &format!("UPDATE {entity} SET deleted_at=NULL WHERE hotel_id=?1 AND id=?2"),
                    params![hotel_id, id],
                ).map_err(se)?;
            }
            _ => {}
        }
        Ok(())
    }

    /// Create/refresh a local user. The argon2 PHC string is DPAPI-protected
    /// like every other credential.
    pub fn upsert_local_user(
        &mut self,
        username: &str,
        full_name: &str,
        role: &str,
        hotel_id: i64,
        password: &str,
    ) -> Result<(), StoreError> {
        use argon2::PasswordHasher;
        use rand_core::OsRng;
        let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
        let phc = argon2::Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| StoreError::Io(e.to_string()))?
            .to_string();
        let protected = dpapi::protect(phc.as_bytes()).map_err(StoreError::Io)?;
        let hex = hex_encode(&protected);
        let existing: Option<String> = self
            .conn
            .query_row(
                "SELECT user_id FROM local_users WHERE username = ?1",
                params![username],
                |r| r.get(0),
            )
            .optional()
            .map_err(se)?;
        let user_id = existing.unwrap_or_else(|| Uuid::now_v7().to_string());
        self.conn
            .execute(
                "INSERT INTO local_users (user_id, username, full_name, role, hotel_id, password_phc, created_ms)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)
                 ON CONFLICT(username) DO UPDATE SET
                    full_name = excluded.full_name,
                    role = excluded.role,
                    hotel_id = excluded.hotel_id,
                    password_phc = excluded.password_phc,
                    user_id = excluded.user_id",
                params![
                    user_id,
                    username,
                    if full_name.is_empty() { username } else { full_name },
                    role,
                    hotel_id,
                    hex,
                    chrono::Utc::now().timestamp_millis(),
                ],
            )
            .map_err(se)?;
        Ok(())
    }

    /// Change the password for a locally provisioned user, re-hashing and
    /// re-protecting it for the current Windows user.
    pub fn update_local_user_password(&mut self, user_id: &str, password: &str) -> Result<(), StoreError> {
        use argon2::PasswordHasher;
        use rand_core::OsRng;
        let salt = argon2::password_hash::SaltString::generate(&mut OsRng);
        let phc = argon2::Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| StoreError::Io(e.to_string()))?
            .to_string();
        let protected = dpapi::protect(phc.as_bytes()).map_err(StoreError::Io)?;
        let changed = self.conn.execute(
            "UPDATE local_users SET password_phc = ?2 WHERE user_id = ?1",
            params![user_id, hex_encode(&protected)],
        ).map_err(se)?;
        if changed != 1 {
            return Err(StoreError::Io("local user was not found".into()));
        }
        Ok(())
    }

    /// Local users available on this device (username, hotel, role).
    pub fn list_local_hotels(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT id, code, name, default_currency, time_zone, is_active, address, contact_phone, contact_email FROM hotels ORDER BY updated_ms DESC LIMIT 1000",
        ).map_err(se)?;
        let rows = stmt.query_map([], |r| Ok(serde_json::json!({
            "id": r.get::<_, i64>(0)?, "code": r.get::<_, String>(1)?,
            "name": r.get::<_, String>(2)?, "default_currency": r.get::<_, String>(3)?,
            "time_zone": r.get::<_, String>(4)?, "is_active": r.get::<_, i64>(5)? != 0,
            "address": r.get::<_, Option<String>>(6)?, "contact_phone": r.get::<_, Option<String>>(7)?,
            "contact_email": r.get::<_, Option<String>>(8)?,
        }))).map_err(se)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(se)
    }

    pub fn save_local_hotel(&self, old_id: i64, id: i64, code: &str, name: &str, profile: &serde_json::Value) -> Result<(), StoreError> {
        let now = Utc::now().timestamp_millis();
        let tx = self.conn.unchecked_transaction().map_err(se)?;
        tx.execute(
            "INSERT INTO hotels(id, code, name, default_currency, time_zone, is_active, address, contact_phone, contact_email, created_ms, updated_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, 1, ?6, ?7, ?8, ?9, ?9)
             ON CONFLICT(id) DO UPDATE SET code=excluded.code, name=excluded.name,
               default_currency=excluded.default_currency, time_zone=excluded.time_zone,
               address=excluded.address, contact_phone=excluded.contact_phone,
               contact_email=excluded.contact_email, updated_ms=excluded.updated_ms",
            params![id, code, name,
                profile.get("default_currency").and_then(|v| v.as_str()).unwrap_or("TRY"),
                profile.get("time_zone").and_then(|v| v.as_str()).unwrap_or("Europe/Istanbul"),
                profile.get("address").and_then(|v| v.as_str()),
                profile.get("contactPhone").and_then(|v| v.as_str()),
                profile.get("contactEmail").and_then(|v| v.as_str()), now],
        ).map_err(se)?;
        if old_id != id {
            tx.execute("UPDATE local_users SET hotel_id=?2 WHERE hotel_id=?1", params![old_id, id]).map_err(se)?;
            tx.execute("UPDATE rooms SET hotel_id=?2 WHERE hotel_id=?1", params![old_id, id]).map_err(se)?;
            tx.execute("UPDATE room_types SET hotel_id=?2 WHERE hotel_id=?1", params![old_id, id]).map_err(se)?;
            tx.execute("UPDATE reservations SET hotel_id=?2 WHERE hotel_id=?1", params![old_id, id]).map_err(se)?;
            tx.execute("UPDATE guests SET hotel_id=?2 WHERE hotel_id=?1", params![old_id, id]).map_err(se)?;
            tx.execute("UPDATE channels SET hotel_id=?2 WHERE hotel_id=?1", params![old_id, id]).map_err(se)?;
        }
        let encoded = serde_json::to_string(profile).map_err(|e| StoreError::Io(e.to_string()))?;
        tx.execute(
            "INSERT INTO desktop_kv(key, value, updated_ms) VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value=excluded.value, updated_ms=excluded.updated_ms",
            params![format!("hotel_profile:{id}"), encoded, now],
        ).map_err(se)?;
        if old_id != id {
            tx.execute("DELETE FROM desktop_kv WHERE key=?1", params![format!("hotel_profile:{old_id}")]).map_err(se)?;
            tx.execute("DELETE FROM hotels WHERE id=?1", params![old_id]).map_err(se)?;
        }
        tx.commit().map_err(se)?;
        Ok(())
    }

    /// Persist hotel profile metadata in the encrypted desktop database.
    pub fn save_hotel_profile(&self, hotel_id: i64, value: &serde_json::Value) -> Result<(), StoreError> {
        let encoded = serde_json::to_string(value).map_err(|e| StoreError::Io(e.to_string()))?;
        self.conn.execute(
            "INSERT INTO desktop_kv(key, value, updated_ms) VALUES(?1, ?2, ?3)
             ON CONFLICT(key) DO UPDATE SET value = excluded.value, updated_ms = excluded.updated_ms",
            params![format!("hotel_profile:{hotel_id}"), encoded, Utc::now().timestamp_millis()],
        ).map_err(se)?;
        Ok(())
    }

    pub fn get_hotel_profile(&self, hotel_id: i64) -> Result<Option<serde_json::Value>, StoreError> {
        let encoded: Option<String> = self.conn.query_row(
            "SELECT value FROM desktop_kv WHERE key = ?1",
            params![format!("hotel_profile:{hotel_id}")],
            |r| r.get(0),
        ).optional().map_err(se)?;
        encoded.map(|s| serde_json::from_str(&s).map_err(|e| StoreError::Io(e.to_string()))).transpose()
    }

    pub fn create_shift_transfer(
        &self, id: &str, hotel_id: i64, from_user: &str, to_user: &str,
        amount_cents: i64, currency: &str, notes: Option<&str>,
    ) -> Result<(), StoreError> {
        self.conn.execute(
            "INSERT INTO shift_transfers(id, hotel_id, from_user, to_user, amount_cents, currency, notes, status, created_ms)
             VALUES(?1, ?2, ?3, ?4, ?5, ?6, ?7, 'pending', ?8)",
            params![id, hotel_id, from_user, to_user, amount_cents, currency, notes, Utc::now().timestamp_millis()],
        ).map_err(se)?;
        Ok(())
    }

    pub fn pending_shift_transfer(&self, hotel_id: i64, to_user: &str) -> Result<Option<serde_json::Value>, StoreError> {
        let row: Option<(String, String, String, i64, String, Option<String>, i64)> = self.conn.query_row(
            "SELECT id, from_user, to_user, amount_cents, currency, notes, created_ms
             FROM shift_transfers WHERE hotel_id = ?1 AND to_user = ?2 AND status = 'pending'
             ORDER BY created_ms DESC LIMIT 1",
            params![hotel_id, to_user],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?)),
        ).optional().map_err(se)?;
        Ok(row.map(|(id, from_user, to_user, amount_cents, currency, notes, created_ms)| serde_json::json!({
            "id": id, "from_user": from_user, "to_user": to_user,
            "amount_cents": amount_cents, "currency": currency, "notes": notes, "created_ms": created_ms,
        })))
    }

    pub fn decide_shift_transfer(&self, id: &str, decided_by: &str, approved: bool) -> Result<(), StoreError> {
        let tx = self.conn.unchecked_transaction().map_err(se)?;
        let changed = tx.execute(
            "UPDATE shift_transfers SET status = ?2, decided_ms = ?3, decided_by = ?4
             WHERE id = ?1 AND status = 'pending' AND to_user = ?4",
            params![id, if approved { "approved" } else { "rejected" }, Utc::now().timestamp_millis(), decided_by],
        ).map_err(se)?;
        if changed != 1 {
            return Err(StoreError::Io("shift transfer is no longer pending or recipient does not match".into()));
        }
        tx.commit().map_err(se)?;
        Ok(())
    }

    pub fn list_local_users(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT user_id, username, full_name, role, hotel_id FROM local_users ORDER BY hotel_id, username")
            .map_err(se)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(serde_json::json!({
                    "user_id": r.get::<_, String>(0)?,
                    "username": r.get::<_, String>(1)?,
                    "full_name": r.get::<_, Option<String>>(2)?,
                    "role": r.get::<_, String>(3)?,
                    "hotel_id": r.get::<_, i64>(4)?,
                }))
            })
            .map_err(se)?;
        rows.collect::<Result<Vec<_>, _>>().map_err(se)
    }

    /// Verify a password against the provisioned local users ("always
    /// available": no expiry). Returns the verified identity when matched.
    pub fn local_user_login(
        &self,
        hotel_id: i64,
        username: &str,
        password: &str,
    ) -> Result<Option<(String, String, i64)>, StoreError> {
        use argon2::{Argon2, PasswordHash, PasswordVerifier};
        let Some((user_id, role_hex, matched_hotel, role)): Option<(String, String, i64, String)> = self
            .conn
            .query_row(
                "SELECT user_id, password_phc, hotel_id, role FROM local_users
                 WHERE username = ?1 AND (hotel_id = ?2 OR ?2 = 0)",
                params![username, hotel_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
            )
            .optional()
            .map_err(se)?
        else {
            return Ok(None);
        };
        let bytes = hex_decode(&role_hex).map_err(StoreError::Io)?;
        let plain = dpapi::unprotect(&bytes).map_err(StoreError::Io)?;
        let phc = String::from_utf8(plain).map_err(|e| StoreError::Io(e.to_string()))?;
        let parsed = PasswordHash::new(&phc).map_err(|e| StoreError::Io(e.to_string()))?;
        if Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
        {
            Ok(Some((user_id, role, matched_hotel)))
        } else {
            Ok(None)
        }
    }

    /// Provision default users on first run (idempotent): creates the HMS admin
    /// and hotel 4442 staff from userpass.md when the database is empty.
    fn provision_default_users(&mut self) -> Result<(), StoreError> {
        let count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM local_users", [], |r| r.get(0))
            .map_err(se)?;
        if count > 0 {
            return Ok(()); // users already exist
        }

        // Default users from userpass.md (hotel 4442 kuzen + admin)
        let defaults = [
            (0, "admin", "1234", "admin"),
            (4442, "reception", "123", "reception"),
            (4442, "sina", "123", "reception"),
            (4442, "manager", "4321", "manager"),
            (4442, "hk", "321", "housekeeping"),
            (4442, "ac", "432", "accountant"),
        ];

        for (hotel, user, pass, role) in defaults {
            self.upsert_local_user(user, user, role, hotel, pass)?;
        }
        Ok(())
    }

    // ---- token storage for the background sync worker (DPAPI too)

    pub fn save_token(&mut self, token: &str) -> Result<(), StoreError> {
        let protected = dpapi::protect(token.as_bytes()).map_err(StoreError::Io)?;
        self.kv_set("auth_token", &hex_encode(&protected))
    }

    pub fn load_token(&mut self) -> Result<Option<String>, StoreError> {
        let Some(hex) = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'auth_token'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(se)?
        else {
            return Ok(None);
        };
        let bytes = hex_decode(&hex).map_err(StoreError::Io)?;
        let plain = dpapi::unprotect(&bytes).map_err(StoreError::Io)?;
        Ok(Some(String::from_utf8_lossy(&plain).into_owned()))
    }

    // ---- First-run and connection preferences

    pub fn finish_first_run(
        &mut self,
        mode: &str,
        api_base: Option<&str>,
        storage_dir: &str,
        theme: &str,
    ) -> Result<(), StoreError> {
        self.kv_set("setup_mode", mode)?;
        self.kv_set("setup_complete", "true")?;
        self.kv_set("storage_dir", storage_dir)?;
        self.kv_set("theme", theme)?;
        if let Some(address) = api_base {
            self.kv_set("api_base", address)?;
        }
        self.set_offline_mode(mode == "local")
    }

    pub fn app_preferences(&self) -> Result<serde_json::Value, StoreError> {
        let get = |key: &str| -> Result<Option<String>, StoreError> {
            self.conn
                .query_row("SELECT value FROM kv WHERE key=?1", params![key], |r| {
                    r.get(0)
                })
                .optional()
                .map_err(se)
        };
        Ok(serde_json::json!({
            "first_run": get("setup_complete")?.as_deref() != Some("true"),
            "mode": get("setup_mode")?.unwrap_or_else(|| "cloud".into()),
            "api_base": get("api_base")?,
            "storage_dir": get("storage_dir")?,
            "theme": get("theme")?.unwrap_or_else(|| "light".into()),
            "sync_mode": get("sync_mode")?.unwrap_or_else(|| "auto".into()),
        }))
    }

    /// Connection-preference writes for the Settings page (server address and
    /// sync-mode changes made after first run).
    pub fn set_api_base(&mut self, api_base: &str) -> Result<(), StoreError> {
        self.kv_set("api_base", api_base)
    }

    // ---- Sync mode (manual offline mode bypasses network transport)

    pub fn offline_mode(&self) -> Result<bool, StoreError> {
        let value: Option<String> = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'offline_mode'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(se)?;
        Ok(value.as_deref() == Some("true"))
    }

    pub fn set_offline_mode(&mut self, enabled: bool) -> Result<(), StoreError> {
        self.kv_set("offline_mode", if enabled { "true" } else { "false" })
    }

    /// Sync schedule preference: "auto" (background worker runs) or "manual"
    /// (only explicit "Sync now" triggers a cycle). Default is auto.
    pub fn sync_mode(&self) -> Result<String, StoreError> {
        let value: Option<String> = self
            .conn
            .query_row("SELECT value FROM kv WHERE key='sync_mode'", [], |r| {
                r.get(0)
            })
            .optional()
            .map_err(se)?;
        let mode = value.unwrap_or_else(|| "auto".into());
        if mode == "manual" {
            Ok("manual".into())
        } else {
            Ok("auto".into())
        }
    }

    pub fn set_sync_mode(&mut self, mode: &str) -> Result<(), StoreError> {
        if !["auto", "manual"].contains(&mode) {
            return Err(StoreError::Io("sync mode must be auto or manual".into()));
        }
        self.kv_set("sync_mode", mode)
    }

    /// Local cached login identity for offline authentication.
    pub fn cached_login_identity(&mut self) -> Result<Option<(i64, String, String)>, StoreError> {
        let Some(cache) = self.load_offline_cache()? else {
            return Ok(None);
        };
        Ok(Some((
            cache.hotel_id,
            cache.username.clone(),
            cache.password_hash.clone(),
        )))
    }

    // ---- LAN hub configuration (G4: optional peer sync)

    pub fn save_hub_config(
        &mut self,
        hub_url: Option<String>,
        hub_token: Option<String>,
    ) -> Result<(), StoreError> {
        if let Some(url) = hub_url {
            self.kv_set("hub_url", &url)?;
        } else {
            self.conn
                .execute("DELETE FROM kv WHERE key = 'hub_url'", [])
                .map_err(se)?;
        }
        if let Some(token) = hub_token {
            let protected = dpapi::protect(token.as_bytes()).map_err(StoreError::Io)?;
            self.kv_set("hub_token", &hex_encode(&protected))?;
        } else {
            self.conn
                .execute("DELETE FROM kv WHERE key = 'hub_token'", [])
                .map_err(se)?;
        }
        Ok(())
    }

    pub fn load_hub_config(&mut self) -> Result<(Option<String>, Option<String>), StoreError> {
        let url = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'hub_url'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(se)?;

        let token = if let Some(hex) = self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'hub_token'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(se)?
        {
            let bytes = hex_decode(&hex).map_err(StoreError::Io)?;
            let plain = dpapi::unprotect(&bytes).map_err(StoreError::Io)?;
            Some(String::from_utf8_lossy(&plain).into_owned())
        } else {
            None
        };

        Ok((url, token))
    }

    fn kv_set(&mut self, key: &str, value: &str) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT INTO kv (key, value) VALUES (?1, ?2)
                 ON CONFLICT (key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(se)?;
        Ok(())
    }

    /// The hotel this local DB mirrors (0 = not logged in yet). Written by
    /// `set_auth_token` after the first online login.
    pub fn hotel_id(&mut self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'hotel_id'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(se)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    /// Switch the local mirror to another hotel. The schema is deliberately
    /// single-tenant (no hotel_id on mirror rows), so changing scope must
    /// atomically purge the previous hotel's data and sync bookkeeping first.
    pub fn report_currency(&self) -> Result<String, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM kv WHERE key = 'report_currency'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(se)?
            .unwrap_or_else(|| "TRY".to_owned()))
    }

    pub fn set_report_currency(&mut self, currency: &str) -> Result<(), StoreError> {
        self.kv_set("report_currency", currency)
    }

    pub fn set_hotel_id(&mut self, hotel_id: i64) -> Result<(), StoreError> {
        let current = self.hotel_id()?;
        if current != 0 && current != hotel_id {
            let tx = self.conn.transaction().map_err(se)?;
            tx.execute_batch(
                "DELETE FROM applied_ops;
                 DELETE FROM outbox;
                 DELETE FROM failures;
                 DELETE FROM guests;
                 DELETE FROM room_types;
                 DELETE FROM rooms;
                 DELETE FROM channels;
                 DELETE FROM price_rules;
                 DELETE FROM reservations;
                 DELETE FROM payments;
                 DELETE FROM folio_transactions;
                 DELETE FROM kv WHERE key IN ('pull_cursor','last_synced_at','offline_login_cache','report_currency');",
            )
            .map_err(se)?;
            tx.commit().map_err(se)?;
        }
        self.kv_set("hotel_id", &hotel_id.to_string())
    }

    /// Reservations parked as `conflict` by the sync applier (or locally when
    /// the pulled later-booking echo carries the flag).
    #[allow(dead_code)] // status surface hidden until the UI wiring step
    pub fn tentative_conflicts(&self) -> Result<usize, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM reservations WHERE status = 'conflict' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(se)?)
    }

    /// Local reads used by tests and the first UI integration step: reservations
    /// of this hotel newest-first.
    #[allow(dead_code)] // local read API for the UI wiring step
    pub fn list_reservation_ids(&self) -> Result<Vec<String>, StoreError> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM reservations ORDER BY checkin_date DESC")
            .map_err(se)?;
        let rows = stmt
            .query_map([], |r| r.get::<_, String>(0))
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        Ok(rows)
    }

    pub fn list_reservations_json(&self) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT r.id,r.room_id,r.channel_id,r.guest_id,r.checkin_date,r.checkout_date,r.status,
                    r.total_price_cents,r.currency,r.adults,r.children,g.full_name,
                    r.voucher_number,r.created_at,r.notes
             FROM reservations r LEFT JOIN guests g ON g.id=r.guest_id
             WHERE r.deleted_at IS NULL ORDER BY r.checkin_date DESC LIMIT 2000",
        ).map_err(se)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(serde_json::json!({
                    "id": row.get::<_,String>(0)?,
                    "room_id": row.get::<_,String>(1)?,
                    "channel_id": row.get::<_,Option<String>>(2)?,
                    "guest_id": row.get::<_,Option<String>>(3)?,
                    "checkin_date": row.get::<_,String>(4)?,
                    "checkout_date": row.get::<_,String>(5)?,
                    "status": row.get::<_,String>(6)?,
                    "total_price_cents": row.get::<_,i64>(7)?,
                    "currency": row.get::<_,String>(8)?,
                    "adults": row.get::<_,i64>(9)?,
                    "children": row.get::<_,i64>(10)?,
                    "guest_name": row.get::<_,Option<String>>(11)?,
                    "voucher_number": row.get::<_,Option<String>>(12)?,
                    "created_at": row.get::<_,Option<String>>(13)?,
                    "notes": row.get::<_,Option<String>>(14)?,
                }))
            })
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        Ok(rows)
    }

    /// Guests of one reservation: the primary guest plus every linked guest
    /// (guest_reservation_links). Powers the Booking detail "Guests" tab.
    pub fn reservation_guests(
        &self,
        _hotel_id: i64,
        reservation_id: &str,
    ) -> Result<Vec<serde_json::Value>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT g.id, g.full_name, g.nationality, g.birth_date, g.license_plate, l.link_type
             FROM guest_reservation_links l
             JOIN guests g ON g.id = l.guest_id
             WHERE l.reservation_id = ?1 AND l.link_type = 'guest'
               AND g.deleted_at IS NULL
             ORDER BY g.full_name",
        ).map_err(se)?;
        let mut out: Vec<serde_json::Value> = stmt
            .query_map(params![reservation_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, String>(0)?,
                    "full_name": r.get::<_, String>(1)?,
                    "nationality": r.get::<_, Option<String>>(2)?,
                    "birth_date": r.get::<_, Option<String>>(3)?,
                    "license_plate": r.get::<_, Option<String>>(4)?,
                    "link_type": r.get::<_, String>(5)?,
                }))
            })
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        // the primary guest (reservations.guest_id) — added to the link list by
        // save_reservations, but older rows may miss the link row
        let mut stmt2 = self.conn.prepare(
            "SELECT g.id, g.full_name, g.nationality, g.birth_date, g.license_plate
             FROM reservations r JOIN guests g ON g.id = r.guest_id
             WHERE r.id = ?1 AND g.deleted_at IS NULL",
        ).map_err(se)?;
        let primary = stmt2
            .query_row(params![reservation_id], |r| {
                Ok(serde_json::json!({
                    "id": r.get::<_, String>(0)?,
                    "full_name": r.get::<_, String>(1)?,
                    "nationality": r.get::<_, Option<String>>(2)?,
                    "birth_date": r.get::<_, Option<String>>(3)?,
                    "license_plate": r.get::<_, Option<String>>(4)?,
                }))
            })
            .optional()
            .map_err(se)?;
        if let Some(p) = primary {
            let pid = p.get("id").and_then(|v| v.as_str()).map(String::from);
            if let Some(pid) = pid {
                if !out.iter().any(|g| g.get("id").and_then(|v| v.as_str()) == Some(pid.as_str())) {
                    out.insert(0, p);
                }
            }
        }
        Ok(out)
    }

    /// Guest ids linked to a reservation (primary included) — used to append
    /// newly added guests through the reservation save path.
    pub fn reservation_guest_ids(&self, reservation_id: &str) -> Result<Vec<String>, StoreError> {
        let mut stmt = self.conn.prepare(
            "SELECT guest_id FROM guest_reservation_links WHERE reservation_id=?1 AND link_type='guest'",
        ).map_err(se)?;
        let mut ids: Vec<String> = stmt
            .query_map(params![reservation_id], |r| r.get::<_, String>(0))
            .map_err(se)?.filter_map(|x| x.ok()).collect();
        drop(stmt);
        let mut stmt = self.conn.prepare("SELECT guest_id FROM reservations WHERE id=?1").map_err(se)?;
        if let Some(pid) = stmt.query_row(params![reservation_id], |r| r.get::<_, Option<String>>(0)).optional().map_err(se)?.flatten() {
            if !ids.contains(&pid) { ids.insert(0, pid); }
        }
        Ok(ids)
    }
}

fn hex_encode(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02X}")).collect()
}

fn hex_decode(s: &str) -> Result<Vec<u8>, String> {
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).map_err(|e| e.to_string()))
        .collect()
}

impl SyncStore for LocalStore {
    fn enqueue(&mut self, op: ChangeOperation, now_ms: i64) -> Result<(), StoreError> {
        self.conn
            .execute(
                "INSERT OR REPLACE INTO outbox
                   (operation_id, hotel_id, entity_type, entity_id, operation_type,
                    payload, client_timestamp, enqueued_at_ms, attempts, last_error, next_retry_at_ms)
                 VALUES (?1,?2,?3,?4,?5,?6,?7,?8,0,NULL,NULL)",
                params![
                    op.operation_id.to_string(),
                    op.hotel_id,
                    entity_name(op.entity_type),
                    op.entity_id.to_string(),
                    operation_name(op.operation_type),
                    serde_json::to_string(&op.payload).map_err(|e| StoreError::Io(e.to_string()))?,
                    op.client_timestamp.to_rfc3339(),
                    now_ms,
                ],
            )
            .map_err(se)?;
        Ok(())
    }

    fn pending(&mut self, limit: usize) -> Result<Vec<QueuedOperation>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT operation_id, hotel_id, entity_type, entity_id, operation_type,
                        payload, client_timestamp, enqueued_at_ms, attempts, last_error, next_retry_at_ms
                 FROM outbox ORDER BY enqueued_at_ms LIMIT ?1",
            )
            .map_err(se)?;
        let rows = stmt
            .query_map(params![limit as i64], |r| {
                Ok(QueuedOperation {
                    op: ChangeOperation {
                        operation_id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                        hotel_id: r.get(1)?,
                        entity_type: entity_of(&r.get::<_, String>(2)?),
                        entity_id: Uuid::parse_str(&r.get::<_, String>(3)?).unwrap_or_default(),
                        operation_type: operation_of(&r.get::<_, String>(4)?),
                        payload: serde_json::from_str(&r.get::<_, String>(5)?).unwrap_or_default(),
                        client_timestamp: chrono::DateTime::parse_from_rfc3339(
                            &r.get::<_, String>(6)?,
                        )
                        .unwrap_or_else(|_| Utc::now().into())
                        .with_timezone(&Utc),
                    },
                    enqueued_at_ms: r.get(7)?,
                    attempts: r.get::<_, i64>(8)?.max(0) as u32,
                    last_error: r.get(9)?,
                    next_retry_at_ms: r.get(10)?,
                })
            })
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        Ok(rows)
    }

    fn record_attempt(
        &mut self,
        operation_id: Uuid,
        attempts: u32,
        last_error: Option<String>,
        next_retry_at_ms: Option<i64>,
    ) -> Result<(), StoreError> {
        self.conn
            .execute(
                "UPDATE outbox SET attempts = ?2, last_error = ?3, next_retry_at_ms = ?4
                 WHERE operation_id = ?1",
                params![
                    operation_id.to_string(),
                    attempts as i64,
                    last_error,
                    next_retry_at_ms
                ],
            )
            .map_err(se)?;
        Ok(())
    }

    fn settle(&mut self, operation_id: Uuid, outcome: Settle) -> Result<(), StoreError> {
        let id = operation_id.to_string();
        if outcome == Settle::FailedPermanent {
            // preserve for the status UI with reason + attempts
            self.conn
                .execute(
                    "INSERT OR IGNORE INTO failures (operation_id, entity_type, entity_id, attempts, last_error, occurred_at)
                     SELECT operation_id, entity_type, entity_id, attempts, last_error, ?2 FROM outbox WHERE operation_id = ?1",
                    params![id, Utc::now().timestamp_millis()],
                )
                .map_err(se)?;
        }
        self.conn
            .execute("DELETE FROM outbox WHERE operation_id = ?1", params![id])
            .map_err(se)?;
        Ok(())
    }

    fn failures(&mut self) -> Result<Vec<QueuedOperation>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT operation_id, entity_type, entity_id, attempts, last_error, occurred_at
                 FROM failures ORDER BY occurred_at",
            )
            .map_err(se)?;
        let rows = stmt
            .query_map([], |r| {
                Ok(QueuedOperation {
                    op: ChangeOperation {
                        operation_id: Uuid::parse_str(&r.get::<_, String>(0)?).unwrap_or_default(),
                        hotel_id: 0,
                        entity_type: entity_of(&r.get::<_, String>(1)?),
                        entity_id: Uuid::parse_str(&r.get::<_, String>(2)?).unwrap_or_default(),
                        operation_type: OperationType::Insert,
                        payload: serde_json::json!({}),
                        client_timestamp: Utc::now(),
                    },
                    enqueued_at_ms: r.get(5)?,
                    attempts: r.get::<_, i64>(3)?.max(0) as u32,
                    last_error: r.get(4)?,
                    next_retry_at_ms: None,
                })
            })
            .map_err(se)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(se)?;
        Ok(rows)
    }

    fn clear_failure(&mut self, operation_id: Uuid) -> Result<(), StoreError> {
        self.conn
            .execute(
                "DELETE FROM failures WHERE operation_id = ?1",
                params![operation_id.to_string()],
            )
            .map_err(se)?;
        Ok(())
    }

    fn pending_count(&mut self) -> Result<usize, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM outbox", [], |r| r.get::<_, i64>(0))
            .map(|n| n as usize)
            .map_err(se)?)
    }

    fn pull_cursor(&mut self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT value FROM kv WHERE key = 'pull_cursor'", [], |r| {
                r.get::<_, String>(0)
            })
            .optional()
            .map_err(se)?
            .and_then(|v| v.parse().ok())
            .unwrap_or(0))
    }

    fn set_pull_cursor(&mut self, seq: i64) -> Result<(), StoreError> {
        self.kv_set("pull_cursor", &seq.to_string())
    }

    fn apply_remote(&mut self, record: &ServerChangeRecord) -> Result<RemoteOutcome, StoreError> {
        // dedupe: the same change_seq may arrive twice (resume mid-pull)
        let id = record.operation_id.to_string();
        let seq = record.change_counter;
        let seen: Option<i64> = self
            .conn
            .query_row(
                "SELECT change_seq FROM applied_ops WHERE operation_id = ?1",
                params![id],
                |r| r.get(0),
            )
            .optional()
            .map_err(se)?;
        if seen.is_some() {
            self.bump_cursor(seq)?;
            return Ok(RemoteOutcome::AlreadyKnown);
        }

        let payload = &record.payload;
        let eid = record.entity_id.to_string();
        let now = Utc::now().timestamp_millis();
        match (record.entity_type, record.operation_type) {
            (EntityType::Guest, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO guests (id, full_name, nationality, license_plate, birth_date)
                         VALUES (?1,?2,?3,?4,?5)
                         ON CONFLICT (id) DO UPDATE SET
                           full_name = excluded.full_name, nationality = excluded.nationality,
                           license_plate = excluded.license_plate, birth_date = excluded.birth_date,
                           deleted_at = NULL",
                        params![
                            eid,
                            payload["full_name"].as_str().unwrap_or(""),
                            payload["nationality"].as_str(),
                            payload["license_plate"].as_str(),
                            payload["birth_date"].as_str(),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Guest, OperationType::Delete) => {
                self.conn
                    .execute(
                        "UPDATE guests SET deleted_at = ?2 WHERE id = ?1",
                        params![eid, now],
                    )
                    .map_err(se)?;
            }
            (EntityType::Reservation, OperationType::Insert | OperationType::Update) => {
                let conflict = payload.get("conflict").is_some()
                    || payload["status"].as_str() == Some("conflict");
                self.conn
                    .execute(
                        "INSERT INTO reservations
                           (id, room_id, channel_id, guest_id, checkin_date, checkout_date,
                            status, total_price_cents, adults, children, daily_rates)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8,?9,?10,?11)
                         ON CONFLICT (id) DO UPDATE SET
                           room_id = excluded.room_id, channel_id = excluded.channel_id,
                           guest_id = excluded.guest_id, checkin_date = excluded.checkin_date,
                           checkout_date = excluded.checkout_date,
                           status = excluded.status,
                           total_price_cents = excluded.total_price_cents,
                           adults = excluded.adults, children = excluded.children,
                           daily_rates = excluded.daily_rates, deleted_at = NULL",
                        params![
                            eid,
                            payload["room_id"].as_str().unwrap_or(""),
                            payload["channel_id"].as_str(),
                            payload["guest_id"].as_str(),
                            payload["checkin_date"].as_str().unwrap_or(""),
                            payload["checkout_date"].as_str().unwrap_or(""),
                            if conflict {
                                "conflict"
                            } else {
                                payload["status"].as_str().unwrap_or("confirmed")
                            },
                            payload["total_price_cents"].as_i64().unwrap_or(0),
                            payload["adults"].as_i64().unwrap_or(1).max(0),
                            payload["children"].as_i64().unwrap_or(0).max(0),
                            match payload.get("daily_rates") {
                                Some(v) if !v.is_null() => serde_json::to_string(v).ok(),
                                _ => None,
                            },
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Reservation, OperationType::Delete) => {
                self.conn
                    .execute(
                        "UPDATE reservations SET deleted_at = ?2 WHERE id = ?1",
                        params![eid, now],
                    )
                    .map_err(se)?;
            }
            (EntityType::Room, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO rooms (id, room_type_id, room_number, floor, cleanliness)
                         VALUES (?1,?2,?3,?4,?5)
                         ON CONFLICT (id) DO UPDATE SET room_type_id = excluded.room_type_id,
                           room_number = excluded.room_number, floor = excluded.floor,
                           cleanliness = excluded.cleanliness, deleted_at = NULL",
                        params![
                            eid,
                            payload["room_type_id"].as_str().unwrap_or(""),
                            payload["room_number"].as_str().unwrap_or(""),
                            payload["floor"].as_i64().unwrap_or(0),
                            payload["cleanliness"].as_str().unwrap_or("clean"),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::RoomStatus, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "UPDATE rooms SET cleanliness = ?2 WHERE id = ?1",
                        params![eid, payload["cleanliness"].as_str().unwrap_or("clean")],
                    )
                    .map_err(se)?;
            }
            (EntityType::RoomType, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO room_types (id, name, capacity, base_price_cents)
                         VALUES (?1,?2,?3,?4)
                         ON CONFLICT (id) DO UPDATE SET name = excluded.name,
                           capacity = excluded.capacity, base_price_cents = excluded.base_price_cents",
                        params![
                            eid,
                            payload["name"].as_str().unwrap_or(""),
                            payload["capacity"].as_i64().unwrap_or(2),
                            payload["base_price_cents"].as_i64().unwrap_or(0),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::PriceRule, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO price_rules (id, room_type_id, date_from, date_to, price_cents, currency)
                         VALUES (?1,?2,?3,?4,?5,?6)
                         ON CONFLICT (id) DO UPDATE SET room_type_id = excluded.room_type_id,
                           date_from = excluded.date_from, date_to = excluded.date_to,
                           price_cents = excluded.price_cents, currency = excluded.currency",
                        params![
                            eid,
                            payload["room_type_id"].as_str().unwrap_or(""),
                            payload["date_from"].as_str().unwrap_or(""),
                            payload["date_to"].as_str().unwrap_or(""),
                            payload["price_cents"].as_i64().unwrap_or(0),
                            payload["currency"].as_str().unwrap_or("TRY"),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Channel, OperationType::Insert | OperationType::Update) => {
                self.conn
                    .execute(
                        "INSERT INTO channels (id, kind, name, is_active) VALUES (?1,?2,?3,?4)
                         ON CONFLICT (id) DO UPDATE SET kind = excluded.kind,
                           name = excluded.name, is_active = excluded.is_active",
                        params![
                            eid,
                            payload["kind"].as_str().unwrap_or("other"),
                            payload["name"].as_str().unwrap_or(""),
                            payload["is_active"].as_bool().unwrap_or(true) as i64,
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Payment, OperationType::Insert) => {
                self.conn
                    .execute(
                        "INSERT OR IGNORE INTO payments (id, reservation_id, amount_cents, currency,
                              payment_type, reference, is_reversal, created_at)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                        params![
                            eid,
                            payload["reservation_id"].as_str().unwrap_or(""),
                            payload["amount_cents"].as_i64().unwrap_or(0),
                            payload["currency"].as_str().unwrap_or("TRY"),
                            payload["payment_type"].as_str().unwrap_or("cash"),
                            payload["reference"].as_str(),
                            (payload["amount_cents"].as_i64().unwrap_or(0) < 0) as i64,
                            // A3: real payment day = the record's server timestamp,
                            // never the sync-apply moment
                            record.server_timestamp.timestamp_millis(),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::Payment, OperationType::Update | OperationType::Delete) => {
                // append-only: a late echo of a rejected op is a no-op here
            }
            (EntityType::FolioTransaction, OperationType::Insert) => {
                self.conn
                    .execute(
                        "INSERT OR IGNORE INTO folio_transactions (id, reservation_id, category,
                              description, unit_price_cents, quantity, total_cents, currency)
                         VALUES (?1,?2,?3,?4,?5,?6,?7,?8)",
                        params![
                            eid,
                            payload["reservation_id"].as_str().unwrap_or(""),
                            payload["category"].as_str().unwrap_or("other"),
                            payload["description"].as_str().unwrap_or(""),
                            payload["unit_price_cents"].as_i64().unwrap_or(0),
                            payload["quantity"].as_i64().unwrap_or(1),
                            payload["total_cents"].as_i64().unwrap_or(0),
                            payload["currency"].as_str().unwrap_or("TRY"),
                        ],
                    )
                    .map_err(se)?;
            }
            (EntityType::FolioTransaction, _) => {}
            (EntityType::User, _) => {
                // user ops never reach the stream from pushes (rejected);
                // server-side user changes we deliberately do not mirror
            }
            (EntityType::Room, OperationType::Delete) => {
                self.conn
                    .execute(
                        "UPDATE rooms SET deleted_at = ?2 WHERE id = ?1",
                        params![eid, now],
                    )
                    .map_err(se)?;
            }
            (EntityType::RoomType, OperationType::Delete) => {
                self.conn
                    .execute(
                        "UPDATE room_types SET deleted_at = ?2 WHERE id = ?1",
                        params![eid, now],
                    )
                    .map_err(se)?;
            }
            (EntityType::RoomStatus, OperationType::Delete) => {
                // room removals arrive as Room deletes; no tombstone locally
            }
            (EntityType::PriceRule, OperationType::Delete) => {
                self.conn
                    .execute("DELETE FROM price_rules WHERE id = ?1", params![eid])
                    .map_err(se)?;
            }
            (EntityType::Channel, OperationType::Delete) => {
                self.conn
                    .execute(
                        "UPDATE channels SET is_active = 0 WHERE id = ?1",
                        params![eid],
                    )
                    .map_err(se)?;
            }
        }

        self.conn
            .execute(
                "INSERT INTO applied_ops (operation_id, change_seq, applied_at) VALUES (?1,?2,?3)",
                params![id, seq, now],
            )
            .map_err(se)?;
        self.bump_cursor(seq)?;
        Ok(RemoteOutcome::Applied)
    }

    fn last_synced_at_ms(&mut self) -> Result<Option<i64>, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT value FROM kv WHERE key = 'last_synced_at'",
                [],
                |r| r.get::<_, String>(0),
            )
            .optional()
            .map_err(se)?
            .and_then(|v| v.parse().ok()))
    }

    fn set_last_synced_at_ms(&mut self, ms: i64) -> Result<(), StoreError> {
        self.kv_set("last_synced_at", &ms.to_string())
    }

    fn conflict_count(&mut self) -> Result<usize, StoreError> {
        Ok(self
            .conn
            .query_row(
                "SELECT COUNT(*) FROM reservations WHERE status = 'conflict' AND deleted_at IS NULL",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|n| n as usize)
            .map_err(se)?)
    }
}

impl LocalStore {
    /// Direct access to the mirror for local-first staging (same crate only).
    pub(crate) fn conn_for_local_writes(&mut self) -> &mut Connection {
        &mut self.conn
    }

    /// Mirror lookup of a payment so the webview can build a reversal row
    /// offline (reversals are negative payment inserts — README §7).
    pub fn payment_mirror_lookup(
        &mut self,
        payment_id: &str,
    ) -> Result<Option<(String, i64, String)>, StoreError> {
        self.conn
            .query_row(
                "SELECT reservation_id, amount_cents, currency FROM payments WHERE id = ?1",
                params![payment_id],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()
            .map_err(se)
    }

    fn bump_cursor(&mut self, seq: i64) -> Result<(), StoreError> {
        // only forward (monotonic per-hotel counter; never a timestamp)
        let cur = self.pull_cursor()?;
        if seq > cur {
            self.set_pull_cursor(seq)?;
        }
        Ok(())
    }
}

pub(crate) fn entity_name(e: EntityType) -> &'static str {
    match e {
        EntityType::RoomType => "room_type",
        EntityType::Room => "room",
        EntityType::RoomStatus => "room_status",
        EntityType::Reservation => "reservation",
        EntityType::Guest => "guest",
        EntityType::Payment => "payment",
        EntityType::FolioTransaction => "folio_transaction",
        EntityType::PriceRule => "price_rule",
        EntityType::User => "user",
        EntityType::Channel => "channel",
    }
}

fn entity_of(s: &str) -> EntityType {
    match s {
        "room_type" => EntityType::RoomType,
        "room" => EntityType::Room,
        "room_status" => EntityType::RoomStatus,
        "payment" => EntityType::Payment,
        "folio_transaction" => EntityType::FolioTransaction,
        "price_rule" => EntityType::PriceRule,
        "user" => EntityType::User,
        "channel" => EntityType::Channel,
        "reservation" => EntityType::Reservation,
        _ => EntityType::Guest,
    }
}

pub(crate) fn operation_name(o: OperationType) -> &'static str {
    match o {
        OperationType::Insert => "insert",
        OperationType::Update => "update",
        OperationType::Delete => "delete",
    }
}

fn operation_of(s: &str) -> OperationType {
    match s {
        "update" => OperationType::Update,
        "delete" => OperationType::Delete,
        _ => OperationType::Insert,
    }
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use hms_core::sync::{
        SyncOpResult, SyncOpStatus, SyncPullResponse, SyncPushRequest, SyncPushResponse,
        DEFAULT_OFFLINE_LOGIN_DAYS,
    };
    use hms_sync::engine::SyncEngine;
    use hms_sync::transport::{SyncTransport, TransportError};
    use std::{collections::VecDeque, sync::Mutex};

    const HOTEL: i64 = 4443;

    fn guest_op(name: &str) -> ChangeOperation {
        ChangeOperation {
            operation_id: Uuid::now_v7(),
            hotel_id: HOTEL,
            entity_type: EntityType::Guest,
            entity_id: Uuid::now_v7(),
            operation_type: OperationType::Insert,
            payload: serde_json::json!({ "full_name": name }),
            client_timestamp: Utc::now(),
        }
    }

    fn server_change(
        seq: i64,
        entity_type: EntityType,
        entity_id: Uuid,
        payload: serde_json::Value,
    ) -> ServerChangeRecord {
        ServerChangeRecord {
            change_counter: seq,
            hotel_id: HOTEL,
            operation_id: Uuid::now_v7(),
            entity_type,
            entity_id,
            operation_type: OperationType::Insert,
            payload,
            server_timestamp: Utc::now(),
        }
    }

    /// Scripted `SyncTransport`: acknowledges every push as Applied and hands
    /// out pull pages in script order (traits needs Send + Sync -> Mutexes).
    struct ScriptedTransport {
        pushes: Mutex<Vec<SyncPushRequest>>,
        pulls: Mutex<VecDeque<Vec<ServerChangeRecord>>>,
    }

    impl ScriptedTransport {
        fn new(pages: Vec<Vec<ServerChangeRecord>>) -> Self {
            Self {
                pushes: Mutex::new(Vec::new()),
                pulls: Mutex::new(VecDeque::from(pages)),
            }
        }
    }

    impl SyncTransport for ScriptedTransport {
        async fn push(&self, req: SyncPushRequest) -> Result<SyncPushResponse, TransportError> {
            self.pushes.lock().expect("pushes lock").push(req.clone());
            let results = req
                .operations
                .iter()
                .map(|op| SyncOpResult {
                    operation_id: op.operation_id,
                    entity_type: op.entity_type,
                    entity_id: op.entity_id,
                    status: SyncOpStatus::Applied,
                })
                .collect();
            Ok(SyncPushResponse {
                hotel_id: req.hotel_id,
                results,
                server_latest_counter: 3,
            })
        }

        async fn pull(
            &self,
            hotel_id: i64,
            _since: i64,
            _limit: usize,
        ) -> Result<SyncPullResponse, TransportError> {
            let mut pages = self.pulls.lock().expect("pulls lock");
            let records = pages
                .pop_front()
                .ok_or_else(|| TransportError::Network("pull script exhausted".into()))?;
            Ok(SyncPullResponse {
                hotel_id,
                records,
                latest_counter: 3,
                has_more: false,
            })
        }
    }

    #[test]
    fn enqueue_pending_attempt_settle_round_trip() {
        let mut s = LocalStore::with_in_memory().unwrap();
        s.set_hotel_id(HOTEL).unwrap();
        assert_eq!(s.hotel_id().unwrap(), HOTEL);

        // two ops enqueued out of order; pending() is oldest-enqueue first
        let op_a = guest_op("Ada");
        s.enqueue(op_a.clone(), 100).unwrap();
        let op_b = ChangeOperation {
            operation_id: Uuid::now_v7(),
            ..op_a.clone()
        };
        s.enqueue(op_b.clone(), 50).unwrap();

        let q = s.pending(10).unwrap();
        assert_eq!(q.len(), 2);
        assert_eq!(q[0].op.operation_id, op_b.operation_id);
        assert_eq!(q[0].enqueued_at_ms, 50);
        assert_eq!(q[1].op.operation_id, op_a.operation_id);
        assert_eq!(q[1].op.payload["full_name"], "Ada");
        assert_eq!(q[1].attempts, 0);

        s.record_attempt(op_a.operation_id, 1, Some("net down".into()), Some(2_000))
            .unwrap();
        let q = s.pending(10).unwrap();
        assert_eq!(q[1].attempts, 1);
        assert_eq!(q[1].last_error.as_deref(), Some("net down"));
        assert_eq!(q[1].next_retry_at_ms, Some(2_000));

        s.settle(op_a.operation_id, hms_sync::store::Settle::Acknowledged)
            .unwrap();
        assert_eq!(s.pending_count().unwrap(), 1);

        // permanent rejection: out of the outbox, preserved in the failure list
        s.settle(op_b.operation_id, hms_sync::store::Settle::FailedPermanent)
            .unwrap();
        assert_eq!(s.pending_count().unwrap(), 0);
        let fails = s.failures().unwrap();
        assert_eq!(fails.len(), 1);
        assert_eq!(fails[0].op.operation_id, op_b.operation_id);
        assert_eq!(fails[0].attempts, 0);
        s.clear_failure(op_b.operation_id).unwrap();
        assert!(s.failures().unwrap().is_empty());
    }

    #[test]
    fn apply_remote_dedupe_cursor_and_conflict_count() {
        let mut s = LocalStore::with_in_memory().unwrap();
        let guest_id = Uuid::now_v7();

        let rec = server_change(
            7,
            EntityType::Guest,
            guest_id,
            serde_json::json!({ "full_name": "Pulled Guest", "nationality": "DE" }),
        );

        // fresh apply + dedupe on operation id (resume mid-pull)
        assert_eq!(s.apply_remote(&rec).unwrap(), RemoteOutcome::Applied);
        assert_eq!(s.apply_remote(&rec).unwrap(), RemoteOutcome::AlreadyKnown);

        // cursor only moves forward; the older record (a different guest) is
        // still applied last-write, but cursor 7 is not regressed by seq 5.
        // (The engine only feeds ascending counters; a stale seq arriving
        // mid-pull is an echo, hence the forward-only cursor.)
        let older_guest = Uuid::now_v7();
        let older = server_change(
            5,
            EntityType::Guest,
            older_guest,
            serde_json::json!({ "full_name": "Older Neighbor" }),
        );
        assert_eq!(s.apply_remote(&older).unwrap(), RemoteOutcome::Applied);
        assert_eq!(s.pull_cursor().unwrap(), 7);
        let name: String = s
            .conn
            .query_row(
                "SELECT full_name FROM guests WHERE id = ?1",
                params![guest_id.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(name, "Pulled Guest");
        let older_name: String = s
            .conn
            .query_row(
                "SELECT full_name FROM guests WHERE id = ?1",
                params![older_guest.to_string()],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(older_name, "Older Neighbor");

        // a reservation parked as conflict is counted for the status UI
        let res_rec = server_change(
            8,
            EntityType::Reservation,
            Uuid::now_v7(),
            serde_json::json!({
                "room_id": Uuid::now_v7().to_string(),
                "checkin_date": "2026-11-20",
                "checkout_date": "2026-11-22",
                "status": "confirmed",
                "conflict": true,
            }),
        );
        assert_eq!(s.apply_remote(&res_rec).unwrap(), RemoteOutcome::Applied);
        assert_eq!(s.pull_cursor().unwrap(), 8);
        assert_eq!(s.conflict_count().unwrap(), 1);
    }

    #[test]
    fn offline_mode_persists_and_defaults_to_online() {
        let mut store = LocalStore::with_in_memory().expect("store");
        assert!(!store.offline_mode().unwrap());
        store.set_offline_mode(true).unwrap();
        assert!(store.offline_mode().unwrap());
        store.set_offline_mode(false).unwrap();
        assert!(!store.offline_mode().unwrap());
    }

    #[test]
    fn offline_cache_round_trip() {
        let mut s = LocalStore::with_in_memory().unwrap();
        let cache = OfflineLoginCache::create(
            Uuid::now_v7(),
            HOTEL,
            "hkdemo".into(),
            hms_core::roles::Role::Housekeeping,
            "$argon2id$v=19$m=19456,t=2,p=1$c2FsdHNhbHRzYWx0$aGFzaGhhc2hoYXNo".into(),
            Utc::now(),
            DEFAULT_OFFLINE_LOGIN_DAYS,
        );
        s.save_offline_cache(&cache).unwrap();
        let loaded = s.load_offline_cache().unwrap().expect("cache present");
        assert_eq!(loaded.user_id, cache.user_id);
        assert_eq!(loaded.hotel_id, HOTEL);
        assert_eq!(loaded.username, "hkdemo");
        assert_eq!(loaded.role, hms_core::roles::Role::Housekeeping);
        assert_eq!(loaded.password_hash, cache.password_hash);
        assert!(loaded.is_valid(Utc::now()));
        assert!(
            !loaded.is_valid(Utc::now() + chrono::Duration::days(DEFAULT_OFFLINE_LOGIN_DAYS + 1))
        );

        s.drop_offline_cache().unwrap();
        assert!(s.load_offline_cache().unwrap().is_none());
    }

    #[test]
    fn token_round_trip_and_hotel_switch_clears_previous_tenant() {
        let mut s = LocalStore::with_in_memory().unwrap();
        let token = "b2987dd930734f44a35f00dfa8a9e0a2";
        assert_eq!(s.load_token().unwrap(), None);
        s.save_token(token).unwrap();
        assert_eq!(s.load_token().unwrap().as_deref(), Some(token));
        assert_eq!(s.hotel_id().unwrap(), 0, "no login recorded yet");
        s.set_hotel_id(HOTEL).unwrap();
        assert_eq!(s.hotel_id().unwrap(), HOTEL);
        s.conn
            .execute(
                "INSERT INTO guests (id, full_name) VALUES ('old-guest','Old Tenant Guest')",
                [],
            )
            .unwrap();
        s.set_pull_cursor(42).unwrap();
        s.set_hotel_id(1).unwrap();
        assert_eq!(s.hotel_id().unwrap(), 1);
        assert_eq!(
            s.pull_cursor().unwrap(),
            0,
            "tenant switch resets the sync cursor"
        );
        let guests: i64 = s
            .conn
            .query_row("SELECT COUNT(*) FROM guests", [], |r| r.get(0))
            .unwrap();
        assert_eq!(
            guests, 0,
            "tenant switch must not expose prior tenant mirror rows"
        );
    }

    #[tokio::test]
    async fn engine_end_to_end_over_local_store() {
        let guest_id = Uuid::now_v7();
        let pull_page = vec![server_change(
            3,
            EntityType::Guest,
            guest_id,
            serde_json::json!({ "full_name": "Pulled Guest" }),
        )];

        let path = std::env::temp_dir()
            .join(format!("hms_desktop_test_{}", Uuid::new_v4()))
            .with_extension("sqlite");

        {
            let mut store = LocalStore::open(&path).expect("open file store");
            store.set_hotel_id(HOTEL).unwrap();
            let op = guest_op("Engine Guest");
            store.enqueue(op.clone(), 0).unwrap();

            let transport = ScriptedTransport::new(vec![pull_page]);
            let mut engine = SyncEngine::new(HOTEL, store, transport);
            let out = engine.sync_once().await.expect("sync cycle");
            assert_eq!(out.pushed_ok, 1, "one outbox op acknowledged");
            assert_eq!(out.pulled_applied, 1, "one server change applied");
            assert_eq!(out.pending, 0);
            assert!(out.last_synced_at_ms.is_some());

            let snap = engine.status_snapshot().unwrap();
            assert_eq!(snap.pull_cursor, 3);
            assert_eq!(snap.pending, 0);
            assert_eq!(snap.conflicts, 0);
            assert_eq!(snap.failures, 0);
            // engine (and the SQLite handle it owns) dropped at end of block
        }

        // reopen the same file: the applied change is durable
        let mut store2 = LocalStore::open(&path).expect("reopen store");
        let name: String = store2
            .conn
            .query_row(
                "SELECT full_name FROM guests WHERE id = ?1",
                params![guest_id.to_string()],
                |r| r.get(0),
            )
            .expect("pulled row present");
        assert_eq!(name, "Pulled Guest");
        assert_eq!(store2.pull_cursor().unwrap(), 3);
        assert!(
            store2.last_synced_at_ms().unwrap().is_some(),
            "last synced timestamp persisted"
        );

        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_file(path.with_extension("sqlite-wal"));
        let _ = std::fs::remove_file(path.with_extension("sqlite-shm"));
    }
}
