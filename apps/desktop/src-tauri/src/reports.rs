//! Offline reports from the local mirror (README §10 / todo 201): when the
//! device is offline the React reports page cannot reach the REST endpoints,
//! so the desktop shell computes the same four aggregates from the synced
//! SQLite mirror.
//!
//! Fidelity, honestly stated:
//! - the server accrues each reservation's **per-night** price into
//!   `daily_revenue`; the mirror only carries `total_price_cents`, so we
//!   spread the total evenly over the stay nights (identical whenever the
//!   price is uniform, which is how UI-entered reservations are stored).
//! - extras (folio transactions) accrue over the stay nights, mirroring the
//!   server-side folio spread across the stay's `daily_revenue` rows.
//! - collected payments count on their record day. The server recorded the
//!   payment at UTC "now"; the mirror stores the apply-time UTC timestamp,
//!   so day-granular collected totals are exact except at the midnight edge
//!   (pull delay is ~30 s).
//! - cancelled / conflict reservations accrue nothing, matching
//!   `upsert_daily_revenue`'s `non_revenue` rule.
//!
//! All aggregates converge to the live numbers as soon as the device is back
//! online; the web view shows the mirror's staleness from the sync pill.

use crate::local_store::LocalStore;
use chrono::{Duration, NaiveDate};
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DAYS: i64 = 400; // same bound as the REST reports

fn parse_date(s: &str) -> Option<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok()
}

fn es(e: rusqlite::Error) -> String {
    e.to_string()
}

#[derive(Default)]
struct Acc {
    /// day -> sum
    day_room: BTreeMap<NaiveDate, i64>,
    day_extras: BTreeMap<NaiveDate, i64>,
    day_collected: BTreeMap<NaiveDate, i64>,
    day_occupied: BTreeMap<NaiveDate, i64>,
    /// room_id / channel_id -> sum
    room_room: BTreeMap<String, i64>,
    room_extras: BTreeMap<String, i64>,
    room_occupied: BTreeMap<String, i64>,
    channel_room: BTreeMap<String, i64>,
    channel_extras: BTreeMap<String, i64>,
    channel_collected: BTreeMap<String, i64>,
    channel_occupied: BTreeMap<String, i64>,
}

impl Acc {
    fn add_room_night(&mut self, day: NaiveDate, cents: i64, room_id: &str, channel_id: &str) {
        *self.day_room.entry(day).or_default() += cents;
        *self.room_room.entry(room_id.to_owned()).or_default() += cents;
        *self.channel_room.entry(channel_id.to_owned()).or_default() += cents;
    }

    fn add_extra_night(&mut self, day: NaiveDate, cents: i64, room_id: &str, channel_id: &str) {
        *self.day_extras.entry(day).or_default() += cents;
        *self.room_extras.entry(room_id.to_owned()).or_default() += cents;
        *self
            .channel_extras
            .entry(channel_id.to_owned())
            .or_default() += cents;
    }

    fn add_occupied_night(&mut self, day: NaiveDate, room_id: &str, channel_id: &str) {
        *self.day_occupied.entry(day).or_default() += 1;
        *self.room_occupied.entry(room_id.to_owned()).or_default() += 1;
        *self
            .channel_occupied
            .entry(channel_id.to_owned())
            .or_default() += 1;
    }
}

/// Per-night split of `total` over `nights` nights (even, rounding dust on the
/// earlier nights so the nights sum to the total exactly).
fn nightly_split(total: i64, nights: i64) -> Vec<i64> {
    if nights <= 0 {
        return vec![];
    }
    let base = total.div_euclid(nights);
    let rem = total.rem_euclid(nights);
    (0..nights)
        .map(|i| if i < rem { base + 1 } else { base })
        .collect()
}

/// Compute the report payload from the mirror. The shape matches the REST
/// DTO field names (`summary` + `by_day` + `by_channel` + `by_room`), so the
/// web fallback can feed the same React state directly. `currency` is
/// omitted — the mirror has no hotels table; the web layer caches the last
/// currency it saw online.
pub fn compute(store: &LocalStore, from: NaiveDate, to: NaiveDate) -> Result<Value, String> {
    let days = (to - from).num_days();
    if to <= from {
        return Err("to must be after from".into());
    }
    if days > MAX_DAYS {
        return Err("range too large (max 400 days)".into());
    }

    let mut acc = Acc::default();

    // A3: mirror-side pricing inputs — the server computes room revenue from
    // `daily_rates` (upsert_daily_revenue), with price_rules and the room
    // type's base price as fallbacks; mirror the exact same precedence so
    // offline reports agree with the server's daily_revenue aggregation.
    let mut all_rules: Vec<(String, String, String, i64)> = Vec::new();
    {
        let mut stmt = store
            .conn
            .prepare("SELECT room_type_id, date_from, date_to, price_cents FROM price_rules")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            all_rules.push((
                r.get(0).map_err(es)?,
                r.get(1).map_err(es)?,
                r.get(2).map_err(es)?,
                r.get(3).map_err(es)?,
            ));
        }
    }

    // ---- reservations: room accrual + occupied nights
    {
        let mut stmt = store
            .conn
            .prepare(
                r#"SELECT res.room_id, res.channel_id, res.checkin_date, res.checkout_date,
                          res.total_price_cents, res.daily_rates, ro.room_type_id,
                          COALESCE(rt.base_price_cents, 0)
                     FROM reservations res
                     LEFT JOIN rooms ro ON ro.id = res.room_id
                     LEFT JOIN room_types rt ON rt.id = ro.room_type_id
                    WHERE res.deleted_at IS NULL AND res.status NOT IN ('cancelled','conflict')"#,
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let room_id: String = r.get(0).map_err(es)?;
            let channel_id: String = r
                .get::<_, Option<String>>(1)
                .map_err(es)?
                .unwrap_or_default();
            let ci = r
                .get::<_, Option<String>>(2)
                .map_err(es)?
                .as_deref()
                .and_then(parse_date);
            let co = r
                .get::<_, Option<String>>(3)
                .map_err(es)?
                .as_deref()
                .and_then(parse_date);
            let total: i64 = r.get(4).map_err(es)?;
            let rates_json: Option<String> = r.get(5).ok();
            let room_type_id: Option<String> = r.get(6).ok();
            let base_price: i64 = r.get(7).unwrap_or(0);
            let (Some(ci), Some(co)) = (ci, co) else {
                continue;
            };
            let nights_all = (co - ci).num_days();
            if nights_all <= 0 {
                continue;
            }
            // per-night priced rates with the exact server fallback chain;
            // if nothing prices a night we fall back to the even split below
            let priced: Vec<Option<i64>> = (0..nights_all)
                .map(|i| {
                    let day = ci + Duration::days(i);
                    let day_str = day.to_string();
                    if let Some(js) = &rates_json {
                        if let Ok(serde_json::Value::Array(arr)) = serde_json::from_str::<serde_json::Value>(js) {
                            for e in &arr {
                                if e["date"].as_str() == Some(&day_str) {
                                    return e["price_cents"].as_i64().filter(|p| *p >= 0);
                                }
                            }
                        }
                    }
                    if let Some(rt) = &room_type_id {
                        let rule: Option<i64> = all_rules
                            .iter()
                            .filter(|(rt_id, from, to, _)| {
                                rt_id == rt
                                    && parse_date(from).is_some_and(|f| f <= day)
                                    && parse_date(to).is_some_and(|t| t >= day)
                            })
                            .max_by_key(|(_, from, _, _)| from.clone())
                            .map(|(_, _, _, price)| *price)
                            .filter(|p| *p > 0);
                        if let Some(p) = rule {
                            return Some(p);
                        }
                        if base_price > 0 {
                            return Some(base_price);
                        }
                    }
                    None
                })
                .collect();
            let per_night: Vec<i64> = if priced.iter().all(|p| p.is_some()) {
                priced.into_iter().map(|p| p.unwrap()).collect()
            } else {
                // unpriced (e.g. an offline create staged before its pull):
                // near-even spread over the stay nights
                nightly_split(total, nights_all)
            };
            // only the nights inside the report range contribute dollars,
            // but occupancy counts the same nights so the shapes agree
            for (i, cents) in per_night.into_iter().enumerate() {
                let day = ci + Duration::days(i as i64);
                if day < from || day >= to {
                    continue;
                }
                acc.add_room_night(day, cents, &room_id, &channel_id);
                acc.add_occupied_night(day, &room_id, &channel_id);
            }
        }
    }

    // ---- folio extras spread over their stay nights
    {
        let mut stmt = store
            .conn
            .prepare(
                r#"SELECT f.reservation_id, f.total_cents,
                          res.room_id, res.channel_id, res.checkin_date, res.checkout_date
                     FROM folio_transactions f
                     JOIN reservations res ON res.id = f.reservation_id
                    WHERE res.deleted_at IS NULL AND res.status NOT IN ('cancelled','conflict')"#,
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let res_id: String = r.get(0).map_err(es)?;
            let cents: i64 = r.get(1).map_err(es)?;
            if cents == 0 {
                continue;
            }
            let room_id: String = r
                .get::<_, Option<String>>(2)
                .map_err(es)?
                .unwrap_or_else(|| res_id.clone());
            let channel_id: String = r
                .get::<_, Option<String>>(3)
                .map_err(es)?
                .unwrap_or_default();
            let ci = r
                .get::<_, Option<String>>(4)
                .map_err(es)?
                .as_deref()
                .and_then(parse_date);
            let co = r
                .get::<_, Option<String>>(5)
                .map_err(es)?
                .as_deref()
                .and_then(parse_date);
            let (Some(ci), Some(co)) = (ci, co) else {
                continue;
            };
            let nights = (co - ci).num_days().max(1);
            for (i, cents_night) in nightly_split(cents, nights).into_iter().enumerate() {
                if cents_night == 0 {
                    continue;
                }
                let day = ci + Duration::days(i as i64);
                if day >= from && day < to {
                    acc.add_extra_night(day, cents_night, &room_id, &channel_id);
                }
            }
        }
    }

    // ---- collected payments on their (UTC) record day
    {
        // attribute each payment to the reservation's room/channel when the
        // reservation row exists (this matches the server's attribution)
        let mut stmt = store
            .conn
            .prepare(
                r#"SELECT p.reservation_id, p.amount_cents, p.created_at,
                          res.room_id, COALESCE(res.channel_id, '')
                     FROM payments p
                     LEFT JOIN reservations res ON res.id = p.reservation_id"#,
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            let cents: i64 = r.get(1).map_err(es)?;
            let at_ms: i64 = r.get(2).unwrap_or(0);
            let Some(day) = chrono::DateTime::from_timestamp_millis(at_ms).map(|d| d.date_naive())
            else {
                continue;
            };
            if day < from || day >= to {
                continue;
            }
            *acc.day_collected.entry(day).or_default() += cents;
            if let Some(room) = r.get::<_, Option<String>>(3).map_err(es)? {
                let channel: String = r.get(4).map_err(es)?;
                *acc.channel_collected.entry(channel).or_default() += cents;
                // room-level collected revenue is not reported (by_room has no
                // collected column on the REST DTO either)
                let _ = room;
            }
        }
    }

    // ---- dimensions (mirrors the REST LEFT JOINs: every live row, zeros kept)
    let mut room_dims: Vec<(String, String, String)> = Vec::new();
    {
        let mut stmt = store
            .conn
            .prepare(
                r#"SELECT ro.id, ro.room_number, COALESCE(rt.name, '')
                     FROM rooms ro LEFT JOIN room_types rt ON rt.id = ro.room_type_id
                    WHERE ro.deleted_at IS NULL
                    ORDER BY ro.room_number"#,
            )
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            room_dims.push((
                r.get(0).map_err(es)?,
                r.get(1).map_err(es)?,
                r.get(2).map_err(es)?,
            ));
        }
    }
    let room_count = room_dims.len() as i64;

    let mut channel_dims: Vec<(String, String, String)> = Vec::new();
    {
        let mut stmt = store
            .conn
            .prepare("SELECT id, name, kind FROM channels ORDER BY name")
            .map_err(|e| e.to_string())?;
        let mut rows = stmt.query([]).map_err(|e| e.to_string())?;
        while let Some(r) = rows.next().map_err(|e| e.to_string())? {
            channel_dims.push((
                r.get(0).map_err(es)?,
                r.get(1).map_err(es)?,
                r.get(2).map_err(es)?,
            ));
        }
    }

    // ---- by_day: dates that carry anything (matches the server's GROUP BY
    // emitting only dates with daily_revenue rows)
    let mut dates: BTreeSet<NaiveDate> = BTreeSet::new();
    dates.extend(acc.day_room.keys().copied());
    dates.extend(acc.day_extras.keys().copied());
    dates.extend(acc.day_collected.keys().copied());
    dates.extend(acc.day_occupied.keys().copied());
    let by_day: Vec<Value> = dates
        .iter()
        .filter(|d| **d >= from && **d < to)
        .map(|d| {
            json!({
                "date": d.to_string(),
                "room_revenue_cents": acc.day_room.get(d).copied().unwrap_or(0),
                "extra_revenue_cents": acc.day_extras.get(d).copied().unwrap_or(0),
                "payments_collected_cents": acc.day_collected.get(d).copied().unwrap_or(0),
                "occupied_nights": acc.day_occupied.get(d).copied().unwrap_or(0),
            })
        })
        .collect();

    // by_channel sort like the REST report: total revenue desc, then name
    channel_dims.sort_by(|a, b| {
        let rev = |c: &(String, String, String)| -> i64 {
            let (room, extra) = (
                acc.channel_room.get(&c.0).copied().unwrap_or(0),
                acc.channel_extras.get(&c.0).copied().unwrap_or(0),
            );
            room + extra
        };
        rev(b).cmp(&rev(a)).then_with(|| a.1.cmp(&b.1))
    });
    let by_channel: Vec<Value> = channel_dims
        .iter()
        .map(|(id, name, kind)| {
            json!({
                "channel_id": id,
                "channel_name": name,
                "kind": kind,
                "room_revenue_cents": acc.channel_room.get(id).copied().unwrap_or(0),
                "extra_revenue_cents": acc.channel_extras.get(id).copied().unwrap_or(0),
                "payments_collected_cents": acc.channel_collected.get(id).copied().unwrap_or(0),
                "occupied_nights": acc.channel_occupied.get(id).copied().unwrap_or(0),
            })
        })
        .collect();

    let by_room: Vec<Value> = room_dims
        .iter()
        .map(|(id, number, type_name)| {
            json!({
                "room_id": id,
                "room_number": number,
                "room_type_name": type_name,
                "room_revenue_cents": acc.room_room.get(id).copied().unwrap_or(0),
                "extra_revenue_cents": acc.room_extras.get(id).copied().unwrap_or(0),
                "occupied_nights": acc.room_occupied.get(id).copied().unwrap_or(0),
            })
        })
        .collect();

    let occupied_nights: i64 = acc.day_occupied.values().sum();
    let room_revenue: i64 = acc.day_room.values().sum();
    let extra_revenue: i64 = acc.day_extras.values().sum();
    let collected: i64 = acc.day_collected.values().sum();
    let available = room_count * days;
    let adr = if occupied_nights > 0 {
        Some(room_revenue as f64 / occupied_nights as f64)
    } else {
        None
    };
    let revpar = if available > 0 {
        Some(room_revenue as f64 / available as f64)
    } else {
        None
    };

    Ok(json!({
        "summary": {
            "from": from.to_string(),
            "to": to.to_string(),
            "currency": store.report_currency().map_err(|e| e.to_string())?,
            "room_revenue_cents": room_revenue,
            "extra_revenue_cents": extra_revenue,
            "payments_collected_cents": collected,
            "occupied_nights": occupied_nights,
            "available_room_nights": available,
            "occupancy_pct": if available > 0 { 100.0 * occupied_nights as f64 / available as f64 } else { 0.0 },
            "adr_cents": adr,
            "revpar_cents": revpar,
        },
        "by_day": by_day,
        "by_channel": by_channel,
        "by_room": by_room,
    }))
}

// ------------------------------------------------------------------ tests

#[cfg(test)]
mod tests {
    use super::*;
    use crate::local_store::LocalStore;
    use chrono::Utc;
    use hms_core::sync::{ChangeOperation, EntityType, OperationType, ServerChangeRecord};
    use hms_sync::store::SyncStore;
    use uuid::Uuid;

    const HOTEL: i64 = 4443;

    fn seeded() -> LocalStore {
        let mut s = LocalStore::with_in_memory().unwrap();
        s.set_hotel_id(HOTEL).unwrap();
        // two room types / two rooms / two channels
        s.conn
            .execute_batch(
                r#"
                INSERT INTO room_types (id, name, capacity, base_price_cents) VALUES
                  ('t1','Standart Double',2,10000),
                  ('t2','Suite',3,25000);
                INSERT INTO rooms (id, room_type_id, room_number) VALUES
                  ('r1','t1','1001'),
                  ('r2','t2','2001');
                INSERT INTO channels (id, kind, name) VALUES
                  ('c1','other','Direct'),
                  ('c2','ota','Booking.com');
                INSERT INTO reservations (id, room_id, channel_id, checkin_date, checkout_date,
                                          status, total_price_cents, adults) VALUES
                  ('resA','r1','c1','2026-09-20','2026-09-23','confirmed', 30000, 2);
                "#,
            )
            .unwrap();
        s
    }

    fn apply(
        store: &mut LocalStore,
        seq: i64,
        entity: EntityType,
        id: Uuid,
        payload: serde_json::Value,
    ) {
        let rec = ServerChangeRecord {
            change_counter: seq,
            hotel_id: HOTEL,
            operation_id: Uuid::now_v7(),
            entity_type: entity,
            entity_id: id,
            operation_type: OperationType::Insert,
            payload,
            server_timestamp: Utc::now(),
        };
        store
            .apply_remote(&rec)
            .unwrap_or_else(|e| panic!("apply failed for seq {seq}: {e}"));
    }

    #[test]
    fn accrues_per_night_with_occupancy_and_dimensions() {
        let mut s = seeded();
        // folio extras + collected payment via the real sync applier
        apply(
            &mut s,
            3,
            EntityType::FolioTransaction,
            Uuid::now_v7(),
            serde_json::json!({
                "reservation_id": "resA",
                "category": "tv",
                "total_cents": 5000,
            }),
        );
        // Payment rows record their collection date in UTC milliseconds. Use
        // an in-range date here; a sync-applied payment would use today's date.
        s.conn.execute(
            "INSERT INTO payments (id, reservation_id, amount_cents, created_at) VALUES ('payA','resA',10000,?1)",
            [chrono::DateTime::parse_from_rfc3339("2026-09-20T12:00:00Z").unwrap().timestamp_millis()],
        ).unwrap();

        let v = compute(&s, date(2026, 9, 20), date(2026, 9, 23))
            .unwrap_or_else(|e| panic!("compute failed: {e}"));

        let by_day = v["by_day"].as_array().unwrap();
        assert_eq!(by_day.len(), 3, "one row per accrued night");
        // room total spread evenly: 10000/10000/10000
        assert!(
            by_day
                .iter()
                .all(|d| d["room_revenue_cents"].as_i64().unwrap() == 10_000),
            "{by_day:?}"
        );
        // extras spread 1667/1667/1666
        let extras: Vec<i64> = by_day
            .iter()
            .map(|d| d["extra_revenue_cents"].as_i64().unwrap())
            .collect();
        assert_eq!(extras, vec![1_667, 1_667, 1_666]);
        // occupancy 1 room per night
        assert!(by_day
            .iter()
            .all(|d| d["occupied_nights"].as_i64().unwrap() == 1));

        let sum = &v["summary"];
        assert_eq!(sum["room_revenue_cents"].as_i64().unwrap(), 30_000);
        assert_eq!(sum["extra_revenue_cents"].as_i64().unwrap(), 5_000);
        assert_eq!(sum["payments_collected_cents"].as_i64().unwrap(), 10_000);
        assert_eq!(sum["occupied_nights"].as_i64().unwrap(), 3);
        assert_eq!(sum["available_room_nights"].as_i64().unwrap(), 6); // 2 rooms x 3 nights
        assert!((sum["occupancy_pct"].as_f64().unwrap() - 50.0).abs() < 1e-9);
        assert!((sum["adr_cents"].as_f64().unwrap() - 10_000.0).abs() < 1e-9);
        assert!((sum["revpar_cents"].as_f64().unwrap() - 5_000.0).abs() < 1e-9);

        let by_channel = v["by_channel"].as_array().unwrap();
        assert_eq!(by_channel.len(), 2, "zero-revenue channel still listed");
        assert_eq!(by_channel[0]["channel_name"], "Direct");
        assert_eq!(
            by_channel[0]["room_revenue_cents"].as_i64().unwrap(),
            30_000
        );
        assert_eq!(
            by_channel[0]["payments_collected_cents"].as_i64().unwrap(),
            10_000
        );

        let by_room = v["by_room"].as_array().unwrap();
        assert_eq!(by_room.len(), 2);
        assert_eq!(by_room[0]["room_number"], "1001");
        assert_eq!(by_room[0]["room_type_name"], "Standart Double");
        assert_eq!(by_room[0]["room_revenue_cents"].as_i64().unwrap(), 30_000);
        assert_eq!(by_room[0]["occupied_nights"].as_i64().unwrap(), 3);
        assert_eq!(by_room[1]["room_number"], "2001");
        assert_eq!(by_room[1]["room_revenue_cents"].as_i64().unwrap(), 0);
    }

    #[test]
    fn a3_priced_rates_drive_room_revenue_then_price_rules_then_base() {
        let mut s = seeded();
        // server-pulled reservation WITH daily_rates: matches the server's
        // upsert_daily_revenue per-night rates exactly (not an even split)
        apply(
            &mut s,
            3,
            EntityType::Reservation,
            Uuid::now_v7(),
            serde_json::json!({
                "room_id": "r2", "channel_id": "c1",
                "checkin_date": "2026-09-20", "checkout_date": "2026-09-22",
                "status": "confirmed", "total_price_cents": 33000, "adults": 2,
                "daily_rates": [
                    {"date": "2026-09-20", "price_cents": 12000},
                    {"date": "2026-09-21", "price_cents": 21000},
                ],
            }),
        );
        // unpriced reservation (no rates): price_rules apply for the suite,
        // then the room type base price: 2000 + shared
        apply(
            &mut s,
            4,
            EntityType::PriceRule,
            Uuid::now_v7(),
            serde_json::json!({
                "room_type_id": "t1", "date_from": "2026-09-21", "date_to": "2026-09-21",
                "price_cents": 2000, "currency": "TRY"
            }),
        );

        let v = compute(&s, date(2026, 9, 20), date(2026, 9, 22))
            .unwrap_or_else(|e| panic!("compute failed: {e}"));
        let by_day = v["by_day"].as_array().unwrap();
        let by_date = |d: &str| {
            by_day
                .iter()
                .find(|row| row["date"] == d)
                .unwrap_or_else(|| panic!("no row for {d}"))
        };
        // Sep 20: resA unpriced -> base 10000; suite real rate 12000
        assert_eq!(by_date("2026-09-20")["room_revenue_cents"].as_i64().unwrap(), 10000 + 12000);
        // Sep 21: resA price rule 2000; suite real rate 21000
        assert_eq!(by_date("2026-09-21")["room_revenue_cents"].as_i64().unwrap(), 2000 + 21000);
        // summary = both days' accrual
        assert_eq!(v["summary"]["room_revenue_cents"].as_i64().unwrap(), 10000 + 12000 + 2000 + 21000);
        assert_eq!(v["summary"]["occupied_nights"].as_i64().unwrap(), 4);
    }

    #[test]
    fn cancelled_and_partial_range_are_honored() {
        let mut s = seeded();
        apply(
            &mut s,
            3,
            EntityType::Reservation,
            Uuid::now_v7(),
            serde_json::json!({
                "room_id": "r2", "checkin_date": "2026-09-20", "checkout_date": "2026-09-23",
                "status": "cancelled", "total_price_cents": 90000, "adults": 1, "conflict": true,
            }),
        );

        // range starts a day later: only Sep 21-22 nights of resA count
        let v = compute(&s, date(2026, 9, 21), date(2026, 10, 23)).unwrap();
        let sum = &v["summary"];
        assert_eq!(
            sum["room_revenue_cents"].as_i64().unwrap(),
            20_000,
            "clamped to the intersected nights"
        );
        assert_eq!(sum["occupied_nights"].as_i64().unwrap(), 2);
        // the cancelled/conflict stay contributes nothing
        let by_room = v["by_room"].as_array().unwrap();
        let r2 = by_room.iter().find(|r| r["room_number"] == "2001").unwrap();
        assert_eq!(r2["room_revenue_cents"].as_i64().unwrap(), 0);
    }

    #[test]
    fn nightly_split_rounds_dust_on_earlier_nights() {
        assert_eq!(nightly_split(5000, 3), vec![1667, 1667, 1666]);
        assert_eq!(nightly_split(90000, 3), vec![30000, 30000, 30000]);
        assert!(nightly_split(100, 0).is_empty());
    }

    #[test]
    fn invalid_and_oversized_ranges_are_rejected() {
        let s = seeded();
        let start = date(2026, 9, 20);
        assert!(compute(&s, start, start)
            .unwrap_err()
            .contains("to must be after from"));
        assert!(compute(&s, start, start + Duration::days(MAX_DAYS + 1))
            .unwrap_err()
            .contains("range too large"));
    }

    fn date(y: i32, m: u8, d: u8) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m as u32, d as u32).unwrap()
    }

    // a ChangeOperation import guard to keep the trait surface honest
    #[allow(dead_code)]
    fn _op_shape(o: Option<ChangeOperation>) -> Option<ChangeOperation> {
        o
    }
}
