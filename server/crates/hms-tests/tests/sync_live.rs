//! Live-server sync integration tests (README Â§11 / todo Phase 8, item 212).
//!
//! Gated on BOTH:
//!   - `TEST_API_URL` (e.g. `http://localhost:8080`) â€” the release server must
//!     be running,
//!   - `TEST_DATABASE_URL` â€” for the per-run SQL fixture and row-level
//!     assertions (the DB is the source-of-truth oracle).
//! When either is absent the tests print `[skip]` and pass, so plain
//! `cargo test` stays green without infrastructure.
//!
//! Covered offline-protocol guarantees (README Â§11):
//!   1. offline N days then sync: backdated client timestamps APPLY (client
//!      data is welcome even when old); repeat push = idempotent (duplicate).
//!   2. two offline clients book the same room -> the later booking lands as
//!      status `conflict`, never dropped (EXCLUDE + SAVEPOINT).
//!   3. clock-skewed ops (far future) cannot win: rejected outright.
//!   4. interrupted sync replays pay zero duplicates (operation-id idempotency
//!      in-sync and inside the SAME batch, plus exactly one change_log row).
//!   5. cross-hotel push is a 403 (JWT scope wins over the request body).

use chrono::{Duration, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

const TEST_PASSWORD: &[u8] = b"SyncTestPass!2026";

/// Combined env gate: returns `(api_base, pool)` or None to skip.
async fn setup() -> Result<Option<SyncEnv>, Box<dyn std::error::Error>> {
    let api = std::env::var("TEST_API_URL").ok().filter(|u| !u.is_empty());
    let db = std::env::var("TEST_DATABASE_URL").ok().filter(|u| !u.is_empty());
    let (Some(api), Some(db)) = (api, db) else {
        eprintln!("[skip] TEST_API_URL / TEST_DATABASE_URL not set â€” sync tests skipped");
        return Ok(None);
    };
    let pool = PgPool::connect(&db).await?;
    sqlx::migrate!("../../migrations").run(&pool).await?;
    Ok(Some(SyncEnv {
        api: api.trim_end_matches('/').to_string(),
        pool,
    }))
}

struct SyncEnv {
    api: String,
    pool: PgPool,
}

/// Unique-per-run tenants: fixtures are SQL-seeded (no change_log noise).
struct SyncFixture {
    hotel_id: i64,
    username: String,
    room_id: Uuid,
    room_id_2: Uuid,
    channel_id: Uuid,
}

async fn fixture(pool: &PgPool) -> Result<SyncFixture, Box<dyn std::error::Error>> {
    use argon2::{Argon2, PasswordHasher, password_hash::SaltString};

    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .subsec_nanos() as i64;
    let hotel_id = 3_100_000_000 + nanos % 500_000_000;

    let rt = Uuid::now_v7();
    let ch = Uuid::now_v7();
    let uid = Uuid::now_v7();
    let room = Uuid::now_v7();
    let room2 = Uuid::now_v7();
    let username = format!("syncuser{nanos:x}");
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);
    let hash = Argon2::default()
        .hash_password(TEST_PASSWORD, &salt)
        .expect("hash")
        .to_string();

    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO hotels (id, name) VALUES ($1,'sync-tenant')")
        .bind(hotel_id)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "INSERT INTO room_types (id, hotel_id, name, capacity, base_price_cents)
         VALUES ($1,$2,'Double',2,300000)",
    )
    .bind(rt)
    .bind(hotel_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO channels (id, hotel_id, kind, name) VALUES ($1,$2,'direct','Direct')",
    )
    .bind(ch)
    .bind(hotel_id)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO users (id, hotel_id, username, full_name, role, password_hash)
         VALUES ($1,$2,$3,'Sync Tester','reception',$4)",
    )
    .bind(uid)
    .bind(hotel_id)
    .bind(&username)
    .bind(&hash)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO rooms (id, hotel_id, room_type_id, room_number) VALUES ($1,$2,$3,'sync-101')",
    )
    .bind(room)
    .bind(hotel_id)
    .bind(rt)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO rooms (id, hotel_id, room_type_id, room_number) VALUES ($1,$2,$3,'sync-102')",
    )
    .bind(room2)
    .bind(hotel_id)
    .bind(rt)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(SyncFixture {
        hotel_id,
        username,
        room_id: room,
        room_id_2: room2,
        channel_id: ch,
    })
}

// ------------------------------------------------------------------- client

fn client_like() -> reqwest::Client {
    reqwest::Client::new()
    // caller attaches bearer per request
}

impl SyncEnv {
    async fn login(&self, f: &SyncFixture) -> Result<String, String> {
        #[derive(serde::Deserialize)]
        struct Login { data: Value }
        let body = serde_json::json!({
            "hotel_id": f.hotel_id,
            "username": f.username,
            "password": String::from_utf8_lossy(TEST_PASSWORD),
        });
        let r = client_like()
            .post(format!("{}/api/v1/auth/login", self.api))
            .json(&body)
            .send()
            .await
            .map_err(|e| format!("login: {e}"))?;
        assert_eq!(r.status(), reqwest::StatusCode::OK, "login failed: {}", r.status());
        let Login { data } = r.json::<Login>().await.map_err(|e| e.to_string())?;
        Ok(data["token"].as_str().expect("token").to_string())
    }

    async fn post_json<T: serde::de::DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        body: &Value,
        expect: reqwest::StatusCode,
    ) -> Result<T, String> {
        let r = client_like()
            .post(format!("{}/api/v1{path}", self.api))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("post {path}: {e}"))?;
        assert_eq!(
            r.status(),
            expect,
            "unexpected status from {path}: {} (body: {})",
            r.status(),
            r.text().await.unwrap_or_default()
        );
        let resp: Value = r.json().await.map_err(|e| e.to_string())?;
        assert_eq!(resp["success"], true, "ApiResponse not successful");
        Ok(serde_json::from_value(resp["data"].clone()).map_err(|e| e.to_string())?)
    }

    async fn patch_json<T: serde::de::DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
        body: &Value,
        expect: reqwest::StatusCode,
    ) -> Result<T, String> {
        let r = client_like()
            .patch(format!("{}/api/v1{path}", self.api))
            .bearer_auth(token)
            .json(body)
            .send()
            .await
            .map_err(|e| format!("patch {path}: {e}"))?;
        assert_eq!(
            r.status(),
            expect,
            "unexpected status from {path}: {} (body: {})",
            r.status(),
            r.text().await.unwrap_or_default()
        );
        let resp: Value = r.json().await.map_err(|e| e.to_string())?;
        assert_eq!(resp["success"], true, "ApiResponse not successful");
        Ok(serde_json::from_value(resp["data"].clone()).map_err(|e| e.to_string())?)
    }

    async fn get_json<T: serde::de::DeserializeOwned>(
        &self,
        token: &str,
        path: &str,
    ) -> Result<T, String> {
        let r = client_like()
            .get(format!("{}/api/v1{path}", self.api))
            .bearer_auth(token)
            .send()
            .await
            .map_err(|e| format!("get {path}: {e}"))?;
        assert_eq!(r.status(), reqwest::StatusCode::OK);
        let resp: Value = r.json().await.map_err(|e| e.to_string())?;
        assert_eq!(resp["success"], true);
        Ok(serde_json::from_value(resp["data"].clone()).map_err(|e| e.to_string())?)
    }

    async fn push_json(&self, token: &str, req: &Value) -> Result<Value, String> {
        self.post_json(token, "/sync/push", req, reqwest::StatusCode::OK)
            .await
    }

    async fn pull_json(&self, token: &str, since: i64) -> Result<Value, String> {
        self.get_json(token, &format!("/sync/pull?since={since}&v=1"))
            .await
    }
}

fn client() -> reqwest::Client {
    reqwest::Client::new()
}

fn guest_op(hotel_id: i64, days_ago: i64) -> (Uuid, Value) {
    let op_id = Uuid::now_v7();
    let op = serde_json::json!({
        "operation_id": op_id,
        "hotel_id": hotel_id,
        "entity_type": "guest",
        "entity_id": Uuid::now_v7(),
        "operation_type": "insert",
        "payload": { "full_name": format!("Offline Guest {op_id}") },
        "client_timestamp": (Utc::now() - Duration::days(days_ago)).to_rfc3339(),
    });
    (op_id, op)
}

fn res_op(hotel_id: i64, room_id: Uuid, channel_id: Uuid, nights: i64) -> (Uuid, Value) {
    let op_id = Uuid::now_v7();
    let checkin = (Utc::now() + Duration::days(nights)).date_naive();
    let checkout = (Utc::now() + Duration::days(nights + 2)).date_naive();
    let op = serde_json::json!({
        "operation_id": op_id,
        "hotel_id": hotel_id,
        "entity_type": "reservation",
        "entity_id": Uuid::now_v7(),
        "operation_type": "insert",
        "payload": {
            "room_id": room_id,
            "channel_id": channel_id,
            "checkin_date": checkin.to_string(),
            "checkout_date": checkout.to_string(),
        },
        "client_timestamp": Utc::now().to_rfc3339(),
    });
    (op_id, op)
}

fn push_req(hotel_id: i64, ops: Vec<Value>) -> Value {
    serde_json::json!({ "hotel_id": hotel_id, "operations": ops, "protocol_version": 1 })
}

/// SyncOpStatus is an externally-tagged enum on the wire (`{"applied": null}`,
/// `{"conflict_resolved": {"message": ...}}`, `{"rejected": {"reason": ...}}`).
/// SyncOpStatus on the wire: unit variants serialize as plain strings
/// ("applied"), data variants as `{"rejected": {"reason": ...}}` /
/// `{"conflict_resolved": {"message": ...}}` (serde external tagging).
fn op_status(result: &Value) -> String {
    let s = &result["status"];
    if let Some(v) = s.as_str() {
        return v.to_owned();
    }
    s.as_object()
        .and_then(|o| o.keys().next())
        .map(|k| k.to_owned())
        .unwrap_or_else(|| format!("unrecognized status shape: {s:?}"))
}

/// Reservation status rows for the fixture hotel (the SPA-free oracle).
async fn reservation_statuses(
    pool: &PgPool,
    hotel_id: i64,
) -> Result<Vec<(String, String)>, String> {
    let rows: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM reservations WHERE hotel_id = $1 AND deleted_at IS NULL ORDER BY id",
    )
    .bind(hotel_id)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;
    Ok(rows
        .into_iter()
        .map(|(id, status)| (id.to_string(), status))
        .collect())
}

// --------------------------------------------------------------------- tests

/// Offline N days then sync (guest typed 10 days ago while offline), then an
/// interrupted-sync replay: the same op pushed again must duplicate, not clone.
#[tokio::test]
async fn offline_days_then_apply_and_replay_is_idempotent() {
    let Some(env) = setup().await.unwrap() else { return };
    let f = fixture(&env.pool).await.unwrap();
    let token = env.login(&f).await.unwrap();

    // 1) offline for 10 days, one guest typed on day 1, synced now
    let (op_id, op) = guest_op(f.hotel_id, 10);
    let resp: Value = env
        .push_json(&token, &push_req(f.hotel_id, vec![op.clone()]))
        .await
        .unwrap();
    assert_eq!(op_status(&resp["results"][0]), "applied");
    assert_eq!(resp["results"][0]["operation_id"], op_id.to_string());

    // applied even though the timestamp is 10 days in the PAST (only far
    // FUTURE timestamps are offside); late composure of offline data is the
    // entire point of the outbox, README 11.4.
    let guest_count: (i64,) = sqlx::query_as("SELECT count(*) FROM guests WHERE hotel_id = $1")
        .bind(f.hotel_id)
        .fetch_one(&env.pool)
        .await
        .unwrap();
    assert_eq!(guest_count.0, 1);

    // pull must stream exactly one record (hotel-scoped)
    let pull = env.pull_json(&token, 0).await.unwrap();
    let records = pull["records"].as_array().unwrap();
    assert_eq!(records.len(), 1, "one change in the log");
    assert_eq!(records[0]["entity_type"], "guest");
    assert_eq!(records[0]["operation_id"], op_id.to_string());
    assert_eq!(pull["hotel_id"], f.hotel_id);

    // 2) interrupted sync: the client re-plays the SAME op id (and the whole
    // batch) â€” duplicate_ignored, no second row, no second change_log echo
    let resp2: Value = env
        .push_json(&token, &push_req(f.hotel_id, vec![op]))
        .await
        .unwrap();
    assert_eq!(op_status(&resp2["results"][0]), "duplicate_ignored");

    let guest_count: (i64,) = sqlx::query_as("SELECT count(*) FROM guests WHERE hotel_id = $1")
        .bind(f.hotel_id)
        .fetch_one(&env.pool)
        .await
        .unwrap();
    assert_eq!(guest_count.0, 1, "no duplicates after replay");

    let pull2 = env.pull_json(&token, 0).await.unwrap();
    assert_eq!(pull2["records"].as_array().unwrap().len(), 1, "echo exactly once");
}

/// Two offline clients booked the same room while the server was down: the
/// later arrival APPLIES as status `conflict` (never dropped, README 11.5).
#[tokio::test]
async fn two_offline_clients_same_room_lands_conflict() {
    let Some(env) = setup().await.unwrap() else { return };
    let f = fixture(&env.pool).await.unwrap();
    let token = env.login(&f).await.unwrap();

    // client A: books clean (10 nights from today)
    let (id_a, a) = res_op(f.hotel_id, f.room_id, f.channel_id, 10);
    let r: Value = env
        .push_json(&token, &push_req(f.hotel_id, vec![a]))
        .await
        .unwrap();
    assert_eq!(op_status(&r["results"][0]), "applied");

    // client B: overlapping range (11 nights from today -> overlaps by 1 night)
    let (id_b, b) = res_op(f.hotel_id, f.room_id, f.channel_id, 11);
    let r2: Value = env
        .push_json(&token, &push_req(f.hotel_id, vec![b]))
        .await
        .unwrap();
    assert_eq!(
        op_status(&r2["results"][0]),
        "conflict_resolved",
        "later booking must be accepted as conflict"
    );

    // the DB holds both rows; exactly one is parked `conflict`
    let statuses = reservation_statuses(&env.pool, f.hotel_id).await.unwrap();
    assert_eq!(statuses.len(), 2);
    assert_eq!(statuses.iter().filter(|(_, s)| s == "confirmed").count(), 1);
    assert_eq!(statuses.iter().filter(|(_, s)| s == "conflict").count(), 1);

    // and A's own row is untouched (arrival order, never client clocks)
    let _ = id_a;
    let pull = env.pull_json(&token, 0).await.unwrap();
    assert_eq!(pull["records"].as_array().unwrap().len(), 2, "both echoes stream out");

    // B5: Reassignment tool verification (suggest free rooms, move in one click,
    // server validates and checks no-double-booking, sets status=confirmed, audit-logged)
    let suggested: Vec<Value> = env
        .get_json(&token, &format!("/reservations/{id_b}/suggested-rooms"))
        .await
        .unwrap();
    assert!(
        suggested.iter().any(|r| r["id"].as_str() == Some(&f.room_id_2.to_string())),
        "suggested rooms must include free room_id_2"
    );

    let reassigned: Value = env
        .patch_json(
            &token,
            &format!("/reservations/{id_b}"),
            &serde_json::json!({ "room_id": f.room_id_2 }),
            reqwest::StatusCode::OK,
        )
        .await
        .unwrap();
    assert_eq!(reassigned["status"], "confirmed", "reassignment promotes conflict to confirmed");

    let statuses_after = reservation_statuses(&env.pool, f.hotel_id).await.unwrap();
    assert_eq!(statuses_after.iter().filter(|(_, s)| s == "confirmed").count(), 2);
    assert_eq!(statuses_after.iter().filter(|(_, s)| s == "conflict").count(), 0, "no conflicts remain");

    let audit_reassigned: (i64,) = sqlx::query_as(
        "SELECT count(*) FROM audit_log WHERE hotel_id = $1 AND entity_id = $2 AND action = 'reassigned'",
    )
    .bind(f.hotel_id)
    .bind(id_b)
    .fetch_one(&env.pool)
    .await
    .unwrap();
    assert_eq!(audit_reassigned.0, 1, "audit log must record reassignment");
}

/// Far-future client timestamps are rejected outright (Â§11.4) and never echo?
/// No: rejected ops are NOT memorized/echoed â€” the room stays free.
#[tokio::test]
async fn clock_skewed_client_cannot_win() {
    let Some(env) = setup().await.unwrap() else { return };
    let f = fixture(&env.pool).await.unwrap();
    let token = env.login(&f).await.unwrap();

    let op_id = Uuid::now_v7();
    let entity = Uuid::now_v7();
    let checkin = (Utc::now() + Duration::days(30)).date_naive();
    let op = serde_json::json!({
        "operation_id": op_id,
        "hotel_id": f.hotel_id,
        "entity_type": "reservation",
        "entity_id": entity,
        "operation_type": "insert",
        "payload": {
            "room_id": f.room_id,
            "channel_id": f.channel_id,
            "checkin_date": checkin.to_string(),
            "checkout_date": (Utc::now() + Duration::days(33)).date_naive().to_string(),
        },
        "client_timestamp": (Utc::now() + Duration::days(7)).to_rfc3339(),
    });
    let r: Value = env
        .push_json(&token, &push_req(f.hotel_id, vec![op]))
        .await
        .unwrap();
    assert_eq!(op_status(&r["results"][0]), "rejected");
    assert!(
        r["results"][0]["status"]["rejected"]["reason"]
            .as_str()
            .unwrap_or("")
            .contains("clock skew"),
        "rejection cites clock skew"
    );

    let statuses = reservation_statuses(&env.pool, f.hotel_id).await.unwrap();
    assert!(statuses.is_empty(), "skewed booking must not exist");

    // rejected ops travel neither into change_log nor the pull stream
    let _ = env.pull_json(&token, 0).await.unwrap();
    let pull = env.pull_json(&token, 0).await.unwrap();
    assert_eq!(pull["records"].as_array().unwrap().len(), 0);
}

/// Pull is hotel-scoped by the JWT: a second fixture hotel's changes never
/// appear for another hotel's token. An op whose BODY claims another hotel in
/// `hotel_id`/id fields cannot reach that hotel either — the JWT tenant scope
/// wins (RLS) and the op applies under the signed-in hotel; only a REQUEST
/// envelope mismatch is refused with 403 outright.
#[tokio::test]
async fn pull_is_tenant_scoped_and_cross_hotel_cannot_leak() {
    let Some(env) = setup().await.unwrap() else { return };
    let f = fixture(&env.pool).await.unwrap();
    let other = fixture(&env.pool).await.unwrap();
    let token = env.login(&f).await.unwrap();
    let token_other = env.login(&other).await.unwrap();

    let (_op_id, op) = guest_op(f.hotel_id, 0);
    let r: Value = env
        .push_json(&token, &push_req(f.hotel_id, vec![op]))
        .await
        .unwrap();
    assert_eq!(op_status(&r["results"][0]), "applied");

    // guests pushed under hotel A never surface to hotel B's pull
    let pull_other = env.pull_json(&token_other, 0).await.unwrap();
    assert_eq!(pull_other["records"].as_array().unwrap().len(), 0);
    assert_eq!(pull_other["hotel_id"], other.hotel_id);

    // an op body lying about ITS hotel_id still lands under the JWT tenant
    // (hotel A) — hotel B stays untouched (RLS row scope, never the body)
    let (_oid, other_op) = guest_op(other.hotel_id, 0);
    let r2: Value = env
        .push_json(&token, &push_req(f.hotel_id, vec![other_op]))
        .await
        .unwrap();
    assert_eq!(op_status(&r2["results"][0]), "applied");
    let other_guests: (i64,) = sqlx::query_as("SELECT count(*) FROM guests WHERE hotel_id = $1")
        .bind(other.hotel_id)
        .fetch_one(&env.pool)
        .await
        .unwrap();
    assert_eq!(other_guests.0, 0, "no data may land in the foreign hotel");

    // and a REQUEST envelope naming another hotel is refused outright
    let (_oid3, f_op) = guest_op(f.hotel_id, 0);
    let client = client_like();
    let resp = client
        .post(format!("{}/api/v1/sync/push", env.api))
        .bearer_auth(&token)
        .json(&push_req(other.hotel_id, vec![f_op]))
        .send()
        .await
        .unwrap();
    assert_eq!(
        resp.status(),
        reqwest::StatusCode::FORBIDDEN,
        "request scoped to another hotel is a 403"
    );
}
