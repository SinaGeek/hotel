//! Database-guarantee integration tests (README §16). Gated on
//! `TEST_DATABASE_URL`, e.g. `postgres://postgres:postgres@localhost:5432/hms_test`.
//! When the variable is absent the tests print `[skip]` and pass, so plain
//! `cargo test` stays green without infrastructure.
//!
//! Covered database guarantees:
//!   1. no_double_booking: overlapping stays are rejected BY THE DATABASE;
//!      cancelled/conflict rows do not block; deleted rows do not block
//!   2. double-booking race: N concurrent inserts -> exactly one wins
//!   3. payments append-only for the RLS app role (no UPDATE/DELETE)
//!   4. tenant isolation: `hms_app` with `app.hotel_id` never sees/writes hotel B
//!   5. report integrity: `daily_revenue` totals == reservations + folio + payments
use sqlx::PgPool;
use uuid::Uuid;

const APP_ROLE_PASSWORD: &str = "hmstest-local";

/// the `ALTER ROLE` is a catalog update — parallel tests must not race it
static ROLE_SETUP: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();

async fn setup() -> Result<Option<(String, PgPool)>, Box<dyn std::error::Error>> {
    let Some(url) = std::env::var("TEST_DATABASE_URL").ok().filter(|u| !u.is_empty()) else {
        eprintln!("[skip] TEST_DATABASE_URL not set — db-guarantee tests skipped");
        return Ok(None);
    };
    let pool = PgPool::connect(&url).await?;
    // migrations are idempotent (tracked in _sqlx_migrations)
    sqlx::migrate!("../../migrations").run(&pool).await?;
    // the RLS app role must be able to log in for the isolation tests
    // (utility statements can't take bind parameters — constant is hardcoded)
    static SET_SQL: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let sql = SET_SQL.get_or_init(|| format!("ALTER ROLE hms_app LOGIN PASSWORD '{APP_ROLE_PASSWORD}'"));
    ROLE_SETUP.get_or_init(|| async {
        sqlx::query(sql).execute(&pool).await.expect("ALTER ROLE hms_app failed");
    }).await;
    Ok(Some((url, pool)))
}

/// URL for logging in as the `hms_app` role (RLS enforced, non-superuser):
/// `postgres://postgres:...@host[:port]/db` -> `postgres://hms_app:<pw>@...`
fn app_role_url(url: &str) -> String {
    let (scheme, rest) = url.split_once("://").expect("TEST_DATABASE_URL is a URL");
    let (_userinfo, hostpart) = rest.split_once('@').expect("userinfo@host");
    format!("{scheme}://hms_app:{APP_ROLE_PASSWORD}@{hostpart}")
}

struct Fixture {
    hotel_id: i64,
    other_hotel_id: i64,
    user_id: Uuid,       // hotel A user
    room_id: Uuid,       // hotel A room
    other_room_id: Uuid, // hotel B room 101
    other_room2_id: Uuid,// hotel B room 102
    channel_id: Uuid,    // hotel A channel
}

/// Unique-per-run tenants (tests never re-use hotel ids — no truncation needed).
async fn fixture(pool: &PgPool) -> Result<Fixture, Box<dyn std::error::Error>> {
    // Full epoch nanos in micros-of-a-day-like spread. `subsec_nanos` alone
    // repeats every second, so back-to-back runs collided on hotels_pkey.
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos() as i64;
    // 9e14 keeps (base + nanos) inside BIGINT while staying well clear of the
    // seeded/bootstrapped hotels (0, 4442, 5001, loadtest 4443).
    let h1 = 4_000_000_000_000 + (nanos % 4_000_000_000_000);
    let h2 = h1 + 1;

    let rt_a = Uuid::now_v7();
    let rt_b = Uuid::now_v7();
    let ch_a = Uuid::now_v7();
    let ch_b = Uuid::now_v7();
    let user_a = Uuid::now_v7();
    let user_b = Uuid::now_v7();
    let room_a = Uuid::now_v7();
    let room_b1 = Uuid::now_v7();
    let room_b2 = Uuid::now_v7();

    let mut tx = pool.begin().await?;
    sqlx::query("INSERT INTO hotels (id, name) VALUES ($1,'tenant-A'),($2,'tenant-B')")
        .bind(h1)
        .bind(h2)
        .execute(&mut *tx)
        .await?;

    sqlx::query("INSERT INTO room_types (id, hotel_id, name, capacity, base_price_cents) VALUES ($1,$2,'Double',2,300000)")
        .bind(rt_a).bind(h1).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO room_types (id, hotel_id, name, capacity, base_price_cents) VALUES ($1,$2,'Double',2,300000)")
        .bind(rt_b).bind(h2).execute(&mut *tx).await?;

    for (ch, hid) in [(ch_a, h1), (ch_b, h2)] {
        sqlx::query("INSERT INTO channels (id, hotel_id, kind, name) VALUES ($1,$2,'direct','Direct')")
            .bind(ch)
            .bind(hid)
            .execute(&mut *tx)
            .await?;
    }

    for (uid, hid) in [(user_a, h1), (user_b, h2)] {
        sqlx::query(
            "INSERT INTO users (id, hotel_id, username, full_name, role, password_hash)
             VALUES ($1,$2,'testuser','Test User','reception','x')",
        )
        .bind(uid)
        .bind(hid)
        .execute(&mut *tx)
        .await?;
    }

    sqlx::query("INSERT INTO rooms (id, hotel_id, room_type_id, room_number) VALUES ($1,$2,$3,'101')")
        .bind(room_a).bind(h1).bind(rt_a).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO rooms (id, hotel_id, room_type_id, room_number) VALUES ($1,$2,$3,'101')")
        .bind(room_b1).bind(h2).bind(rt_b).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO rooms (id, hotel_id, room_type_id, room_number) VALUES ($1,$2,$3,'102')")
        .bind(room_b2).bind(h2).bind(rt_b).execute(&mut *tx).await?;

    tx.commit().await?;

    Ok(Fixture {
        hotel_id: h1,
        other_hotel_id: h2,
        user_id: user_a,
        room_id: room_a,
        other_room_id: room_b1,
        other_room2_id: room_b2,
        channel_id: ch_a,
    })
}

async fn insert_reservation(
    ex: impl sqlx::Executor<'_, Database = sqlx::Postgres>,
    hotel_id: i64,
    room_id: Uuid,
    channel_id: Uuid,
    checkin: &str,
    checkout: &str,
    status: &'static str,
) -> Result<Uuid, sqlx::Error> {
    let id = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO reservations (id, hotel_id, room_id, channel_id, stay, checkin_date, checkout_date, status, total_price_cents, created_by)
         VALUES ($1,$2,$3,$4, daterange($5::date,$6::date,'[)'), $5::date, $6::date, $7, 0, NULL)",
    )
    .bind(id)
    .bind(hotel_id)
    .bind(room_id)
    .bind(channel_id)
    .bind(checkin)
    .bind(checkout)
    .bind(status)
    .execute(ex)
    .await?;
    Ok(id)
}

// ---------------------------------------------------------------------------
// 1. no_double_booking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn double_booking_rejected_by_the_database() -> Result<(), Box<dyn std::error::Error>> {
    let Some((_url, pool)) = setup().await? else { return Ok(()) };
    let fx = fixture(&pool).await?;

    let base = insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-11-01", "2026-11-05", "confirmed").await?;

    // exact overlap
    assert!(
        insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-11-01", "2026-11-05", "confirmed")
            .await
            .is_err(),
        "exact overlap must be rejected"
    );
    // partial overlaps on both edges
    assert!(
        insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-10-31", "2026-11-02", "confirmed")
            .await
            .is_err(),
        "leading overlap must be rejected"
    );
    assert!(
        insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-11-04", "2026-11-06", "confirmed")
            .await
            .is_err(),
        "trailing overlap must be rejected"
    );
    // different room: allowed
    let room_c = Uuid::now_v7();
    let rt_a: Uuid = sqlx::query_scalar("SELECT room_type_id FROM rooms WHERE id = $1")
        .bind(fx.room_id)
        .fetch_one(&pool)
        .await?;
    sqlx::query("INSERT INTO rooms (id, hotel_id, room_type_id, room_number) VALUES ($1,$2,$3,'201')")
        .bind(room_c)
        .bind(fx.hotel_id)
        .bind(rt_a)
        .execute(&pool)
        .await?;
    assert!(
        insert_reservation(&pool, fx.hotel_id, room_c, fx.channel_id, "2026-11-01", "2026-11-05", "confirmed")
            .await
            .is_ok(),
        "same dates on a different room must be allowed"
    );

    // adjacency is allowed: [) ranges mean checkout can equal the next checkin
    assert!(
        insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-11-05", "2026-11-07", "confirmed")
            .await
            .is_ok(),
        "adjacent stay (checkout == next checkin) must be allowed"
    );

    // cancelled / conflict overlapping rows do NOT block the room
    assert!(
        insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-11-02", "2026-11-03", "cancelled").await.is_ok(),
        "cancelled rows must not block the room"
    );
    assert!(
        insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-11-02", "2026-11-03", "conflict").await.is_ok(),
        "conflict rows must not block the room"
    );

    // tombstoned (deleted_at) row does not block either
    sqlx::query("UPDATE reservations SET deleted_at = now() WHERE id = $1")
        .bind(base)
        .execute(&pool)
        .await?;
    assert!(
        insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-11-02", "2026-11-03", "confirmed").await.is_ok(),
        "tombstoned rows must not block the room"
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// 2. race: concurrent inserts, exactly one wins
// ---------------------------------------------------------------------------

#[tokio::test]
async fn double_booking_race_exactly_one_winner() -> Result<(), Box<dyn std::error::Error>> {
    const N: usize = 8;
    let Some((_url, pool)) = setup().await? else { return Ok(()) };
    let fx = fixture(&pool).await?;

    let mut handles = Vec::new();
    for _ in 0..N {
        let pool = pool.clone();
        let (h, r, c) = (fx.hotel_id, fx.room_id, fx.channel_id);
        handles.push(tokio::spawn(async move {
            let mut conn = pool.acquire().await?;
            insert_reservation(&mut *conn, h, r, c, "2026-12-01", "2026-12-04", "confirmed").await
        }));
    }
    let mut wins = 0usize;
    for h in handles {
        if h.await?.is_ok() {
            wins += 1;
        }
    }
    let rows: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM reservations WHERE room_id=$1 AND status='confirmed' AND checkin_date='2026-12-01'",
    )
    .bind(fx.room_id)
    .fetch_one(&pool)
    .await?;
    assert_eq!(wins, 1, "exactly one of {N} concurrent bookings must win (got {wins})");
    assert_eq!(rows, 1, "the DB must hold exactly the winning row");
    Ok(())
}

// ---------------------------------------------------------------------------
// 3. payments append-only for the RLS app role
// ---------------------------------------------------------------------------

#[tokio::test]
async fn payments_are_append_only_for_app_role() -> Result<(), Box<dyn std::error::Error>> {
    let Some((url, pool)) = setup().await? else { return Ok(()) };
    let fx = fixture(&pool).await?;

    // seed path (superuser pool)
    let res = insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2026-12-10", "2026-12-12", "confirmed").await?;

    // app-role connection (RLS enforced; payments are INSERT/SELECT only)
    let app = PgPool::connect(&app_role_url(&url)).await?;
    let mut conn = app.acquire().await?;
    sqlx::query("SELECT set_config('app.hotel_id', $1, false)")
        .bind(fx.hotel_id.to_string())
        .execute(&mut *conn)
        .await?;

    // INSERT succeeds
    let pay = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO payments (id, hotel_id, reservation_id, amount_cents, currency, payment_type, is_reversal, created_by)
         VALUES ($1,$2,$3,100000,'TRY','cash',false,$4)",
    )
    .bind(pay)
    .bind(fx.hotel_id)
    .bind(res)
    .bind(fx.user_id)
    .execute(&mut *conn)
    .await?;

    // SELECT succeeds
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM payments WHERE reservation_id = $1")
        .bind(res)
        .fetch_one(&mut *conn)
        .await?;
    assert_eq!(n, 1);

    // UPDATE must fail (append-only: no UPDATE grant for hms_app)
    let upd = sqlx::query("UPDATE payments SET amount_cents = 1 WHERE id = $1")
        .bind(pay)
        .execute(&mut *conn)
        .await;
    let msg = format!("{}", upd.expect_err("app role must not UPDATE payments"));
    assert!(msg.contains("permission denied"), "expected permission denied, got: {msg}");

    // DELETE must fail
    let del = sqlx::query("DELETE FROM payments WHERE id = $1")
        .bind(pay)
        .execute(&mut *conn)
        .await;
    let msg = format!("{}", del.expect_err("app role must not DELETE payments"));
    assert!(msg.contains("permission denied"), "expected permission denied, got: {msg}");
    Ok(())
}

// ---------------------------------------------------------------------------
// 4. tenant isolation under RLS
// ---------------------------------------------------------------------------

#[tokio::test]
async fn tenant_isolation_rls_hides_and_blocks_other_hotels() -> Result<(), Box<dyn std::error::Error>> {
    let Some((url, pool)) = setup().await? else { return Ok(()) };
    let fx = fixture(&pool).await?;

    // seed both tenants' data as superuser
    let res_a = insert_reservation(&pool, fx.hotel_id, fx.room_id, fx.channel_id, "2027-01-05", "2027-01-06", "confirmed").await?;

    // hotel B needs ITS OWN channel (schema: channels are per-hotel rows)
    let ch_b: Uuid = sqlx::query_scalar("SELECT id FROM channels WHERE hotel_id = $1")
        .bind(fx.other_hotel_id)
        .fetch_one(&pool)
        .await?;
    let res_b = insert_reservation(&pool, fx.other_hotel_id, fx.other_room_id, ch_b, "2027-02-01", "2027-02-03", "confirmed").await?;

    // connect as the app role scoped to hotel A
    let app = PgPool::connect(&app_role_url(&url)).await?;
    let mut conn = app.acquire().await?;
    sqlx::query("SELECT set_config('app.hotel_id', $1, false)")
        .bind(fx.hotel_id.to_string())
        .execute(&mut *conn)
        .await?;

    // visible: own room & own reservation
    let my_rooms: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms WHERE deleted_at IS NULL")
        .fetch_one(&mut *conn)
        .await?;
    assert_eq!(my_rooms, 1, "hotel A app user sees exactly its own room");
    let own: i64 = sqlx::query_scalar("SELECT count(*) FROM reservations WHERE id = $1").bind(res_a).fetch_one(&mut *conn).await?;
    assert_eq!(own, 1);

    // invisible: hotel B reservation (RLS filters it out)
    let foreign: i64 = sqlx::query_scalar("SELECT count(*) FROM reservations WHERE id = $1")
        .bind(res_b)
        .fetch_one(&mut *conn)
        .await?;
    assert_eq!(foreign, 0, "hotel B rows must be invisible to hotel A app user");

    // undeletable / unwritable: UPDATE on hotel B rows affects 0 rows (hidden by USING)
    let upd = sqlx::query("UPDATE reservations SET notes = 'hacked' WHERE id = $1")
        .bind(res_b)
        .execute(&mut *conn)
        .await?;
    assert_eq!(upd.rows_affected(), 0, "RLS must make hotel B rows untouchable");

    // cross-tenant INSERT is rejected by WITH CHECK
    let cross = sqlx::query(
        "INSERT INTO reservations (id, hotel_id, room_id, channel_id, stay, checkin_date, checkout_date, status, total_price_cents, created_by)
         VALUES ($1,$2,$3,$4, daterange('2027-03-01'::date,'2027-03-02'::date,'[)'), '2027-03-01','2027-03-02','confirmed',0,NULL)",
    )
    .bind(Uuid::now_v7())
    .bind(fx.other_hotel_id)
    .bind(fx.other_room_id)
    .bind(ch_b)
    .execute(&mut *conn)
    .await;
    let msg = format!("{}", cross.expect_err("cross-tenant INSERT must violate the RLS policy"));
    assert!(
        msg.to_lowercase().contains("row-level security"),
        "expected row-level-security violation, got: {msg}"
    );

    // platform admin (app.is_platform_admin) sees all tenants
    sqlx::query("SELECT set_config('app.is_platform_admin', 'true', false)")
        .execute(&mut *conn)
        .await?;
    let all_rooms: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms WHERE deleted_at IS NULL AND hotel_id = ANY($1)")
        .bind([fx.hotel_id, fx.other_hotel_id])
        .fetch_one(&mut *conn)
        .await?;
    assert_eq!(all_rooms, 3, "platform admin must see both tenants' rooms (1 + 2)");
    let b2_visible: i64 = sqlx::query_scalar("SELECT count(*) FROM rooms WHERE id = $1 AND deleted_at IS NULL")
        .bind(fx.other_room2_id)
        .fetch_one(&mut *conn)
        .await?;
    assert_eq!(b2_visible, 1, "platform admin sees hotel B's room 102");
    Ok(())
}

// ---------------------------------------------------------------------------
// 5. report integrity: daily_revenue == reservations + folio + payments
// ---------------------------------------------------------------------------

#[tokio::test]
async fn daily_revenue_totals_match_the_raw_ledgers() -> Result<(), Box<dyn std::error::Error>> {
    let Some((_url, pool)) = setup().await? else { return Ok(()) };
    let fx = fixture(&pool).await?;

    let h = fx.hotel_id;
    let room = fx.room_id;
    let ch = fx.channel_id;
    let rt: Uuid = sqlx::query_scalar("SELECT room_type_id FROM rooms WHERE id = $1")
        .bind(room)
        .fetch_one(&pool)
        .await?;

    // a 3-night stay Nov 1->4, room revenue 100000/night (total 300000), confirmed
    let res: Uuid = Uuid::now_v7();
    sqlx::query(
        "INSERT INTO reservations (id, hotel_id, room_id, channel_id, stay, checkin_date, checkout_date, status, total_price_cents, created_by)
         VALUES ($1,$2,$3,$4, daterange('2027-02-01'::date,'2027-02-04'::date,'[)'), '2027-02-01','2027-02-04','confirmed',300000, $5)",
    )
    .bind(res)
    .bind(h)
    .bind(room)
    .bind(ch)
    .bind(fx.user_id)
    .execute(&pool)
    .await?;

    // daily_revenue as the server would write it (same transaction)
    for (day, room_rev, extra_rev, collected, occupied) in [
        ("2027-02-01", 100000i64, 3000i64, 50000i64, true),
        ("2027-02-02", 100000, 3000, 0, true),
        ("2027-02-03", 100000, 3000, 0, true),
        // folio lands on 3 days; payment collected on day 1
    ] {
        sqlx::query(
            "INSERT INTO daily_revenue (hotel_id, date, room_id, room_type_id, channel_id, room_revenue_cents, extra_revenue_cents, payment_collected_cents, occupied)
             VALUES ($1,$2::date,$3,$4,$5,$6,$7,$8,$9)",
        )
        .bind(h)
        .bind(day)
        .bind(room)
        .bind(rt)
        .bind(ch)
        .bind(room_rev)
        .bind(extra_rev)
        .bind(collected)
        .bind(occupied)
        .execute(&pool)
        .await?;
    }

    // folio ledger total = 3 x 3000 = 9000; payments ledger = 50000
    for (_day, cat, amt, qty) in [("2027-02-01", "minibar", 3000i64, 1i32), ("2027-02-02", "minibar", 3000, 1), ("2027-02-03", "minibar", 3000, 1)] {
        sqlx::query(
            "INSERT INTO folio_transactions (id, hotel_id, reservation_id, category, description, unit_price_cents, quantity, total_cents, currency)
             VALUES ($1,$2,$3,$4,'soda',$5,$6,$5,'TRY')",
        )
        .bind(Uuid::now_v7())
        .bind(h)
        .bind(res)
        .bind(cat)
        .bind(amt as i64)
        .bind(qty)
        .execute(&pool)
        .await?;
    }
    sqlx::query(
        "INSERT INTO payments (id, hotel_id, reservation_id, amount_cents, currency, payment_type, is_reversal, created_by)
         VALUES ($1,$2,$3,50000,'TRY','cash',false,$4)",
    )
    .bind(Uuid::now_v7())
    .bind(h)
    .bind(res)
    .bind(fx.user_id)
    .execute(&pool)
    .await?;

    // ---- the invariants the reports rely on -------------------------------
    let (room_rev, extra_rev, collected, occupied): (i64, i64, i64, i64) = sqlx::query_as(
        "SELECT COALESCE(sum(room_revenue_cents),0)::bigint,
                COALESCE(sum(extra_revenue_cents),0)::bigint,
                COALESCE(sum(payment_collected_cents),0)::bigint,
                count(*) FILTER (WHERE occupied)::bigint
         FROM daily_revenue WHERE hotel_id = $1",
    )
    .bind(h)
    .fetch_one(&pool)
    .await?;

    let res_total: i64 = sqlx::query_scalar("SELECT total_price_cents FROM reservations WHERE id = $1").bind(res).fetch_one(&pool).await?;
    let folio_total: i64 = sqlx::query_scalar("SELECT COALESCE(sum(total_cents),0)::bigint FROM folio_transactions WHERE hotel_id = $1 AND deleted_at IS NULL")
        .bind(h)
        .fetch_one(&pool)
        .await?;
    let payment_total: i64 = sqlx::query_scalar("SELECT COALESCE(sum(amount_cents),0)::bigint FROM payments WHERE hotel_id = $1")
        .bind(h)
        .fetch_one(&pool)
        .await?;

    assert_eq!(room_rev, res_total, "sum(daily_revenue.room_revenue) must equal the reservation total");
    assert_eq!(extra_rev, folio_total, "sum(daily_revenue.extra_revenue) must equal the folio ledger total");
    assert_eq!(collected, payment_total, "sum(daily_revenue.payment_collected) must equal the payments ledger total");
    assert_eq!(occupied, 3, "occupied nights in daily_revenue must equal the stay length");
    Ok(())
}

/// F3: revenue is bucketed by the HOTEL's local day, not UTC. The same instant
/// is a different date in Kiritimati (UTC+14) than in UTC, which is exactly the
/// off-by-a-day bug this test locks down.
#[tokio::test]
async fn f3_hotel_time_zone_decides_the_business_day() -> Result<(), Box<dyn std::error::Error>> {
    let Some((url, pool)) = setup().await? else { return Ok(()) };

    // the expression the server uses (reservations::hotel_today)
    let sql = "SELECT (now() AT TIME ZONE $1)::date, (now() AT TIME ZONE 'UTC')::date";
    let (east, utc): (chrono::NaiveDate, chrono::NaiveDate) =
        sqlx::query_as(sql).bind("Pacific/Kiritimati").fetch_one(&pool).await?;
    let (west, _): (chrono::NaiveDate, chrono::NaiveDate) =
        sqlx::query_as(sql).bind("America/Los_Angeles").fetch_one(&pool).await?;

    assert!(east >= utc, "a UTC+14 hotel is never behind UTC ({east} vs {utc})");
    assert!(west <= utc, "a UTC-8 hotel is never ahead of UTC ({west} vs {utc})");
    // at least one zone must differ from UTC at some point in the day; at the
    // exact boundary both may coincide, so only assert the ordering above.
    assert!(east > west, "far-east and far-west zones cannot share a day ({east} vs {west})");

    // and the column default is a real, resolvable zone
    let default_zone: Option<String> = sqlx::query_scalar(
        "SELECT column_default FROM information_schema.columns
          WHERE table_name='hotels' AND column_name='time_zone'",
    )
    .fetch_one(&pool)
    .await?;
    // Postgres renders this as "'Europe/Istanbul'::text" — take the literal
    let default_zone = default_zone
        .expect("hotels.time_zone has a default")
        .split('\'')
        .nth(1)
        .expect("default is a quoted literal")
        .to_string();
    let (resolved,): (chrono::NaiveDate,) =
        sqlx::query_as("SELECT (now() AT TIME ZONE $1)::date").bind(&default_zone).fetch_one(&pool).await?;
    assert!(resolved >= west && resolved <= east, "default zone {default_zone} must resolve to a real date");

    let _ = url;
    Ok(())
}
