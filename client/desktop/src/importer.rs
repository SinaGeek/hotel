//! Bulk import (Settings ▸ Bulk import + `hms-import` CLI): load the hotel's
//! pre-existing data — guest register CSV, collections ledger CSV, the
//! Reservation.xlsx workbook and the `userpass.md` credentials file — into the
//! standalone local database.
//!
//! Guarantees (README §11 "server stays authority" still applies when online):
//! - Idempotent by external reference: source ids (`Guest id`, `Booking No`,
//!   collection `#`, `legacy-collection-summary-row-N`) are stored on the
//!   mirror rows, so re-running an import updates instead of duplicating.
//! - Sync rows are claimed, never duplicated: a register row whose booking no
//!   is unknown first tries to claim an existing mirror reservation with the
//!   same room + dates.
//! - Payments are deduped by reference; collection-ledger rows are skipped
//!   when the reservation already has a payment with the same amount on the
//!   same day (protects against double counting a ledger line and the
//!   workbook's per-reservation collection summary).
//! - The Reservation.xlsx workbook has NO dates or booking numbers; unmatched
//!   rows get sequential dates anchored at a user-provided start date
//!   (default 2026-06-01, the register's first day) and a traceable
//!   external_ref `xlsx-res-row-N`.
use crate::local_store::LocalStore;
use calamine::Reader;
use chrono::{Duration, NaiveDate, NaiveDateTime, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde_json::{json, Value};
use std::path::Path;
use uuid::Uuid;

pub struct ImportSummary {
    pub inserted: usize,
    pub updated: usize,
    pub skipped: usize,
    pub errors: Vec<String>,
}

impl ImportSummary {
    fn new() -> Self {
        Self { inserted: 0, updated: 0, skipped: 0, errors: Vec::new() }
    }
    fn into_json(self, what: &str) -> Value {
        json!({
            "dataset": what,
            "inserted": self.inserted,
            "updated": self.updated,
            "skipped": self.skipped,
            "error_count": self.errors.len(),
            "errors": self.errors.into_iter().take(20).collect::<Vec<_>>(),
        })
    }
}

type R<T> = Result<T, String>;

fn err(msg: String) -> String {
    msg
}

// ---------- small parsers ----------

fn parse_amount_cents(raw: &str) -> R<i64> {
    let cleaned: String = raw
        .trim()
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '.' || *c == ',' || *c == '-')
        .collect();
    // thousands separators can be '.' or ','; treat the LAST separator as the
    // decimal point when it is followed by exactly 2 digits.
    let normalized = match (cleaned.rfind(','), cleaned.rfind('.')) {
        (Some(_c), Some(d)) if d > _c => cleaned.replace(',', ""),
        (Some(c), Some(d)) if c > d => cleaned.replace('.', "").replace(',', "."),
        (Some(_c), None) => {
            let frac = cleaned.split(',').next_back().unwrap_or("").len();
            if frac == 2 { cleaned.replace(',', ".") } else { cleaned.replace(',', "") }
        }
        (Some(_c), Some(_d)) if _c <= _d => cleaned.replace(',', ""),
        _ => cleaned,
    };
    let f: f64 = normalized
        .parse()
        .map_err(|_| format!("unparsable amount '{raw}'"))?;
    Ok((f * 100.0).round() as i64)
}

fn parse_dt(raw: &str) -> R<NaiveDateTime> {
    let t = raw.trim();
    for fmt in ["%d-%m-%Y %H:%M:%S", "%d-%m-%Y %H:%M", "%d-%m-%Y", "%Y-%m-%d %H:%M:%S", "%Y-%m-%d"] {
        if let Ok(d) = NaiveDateTime::parse_from_str(t, fmt) {
            return Ok(d);
        }
        if let Ok(d) = NaiveDate::parse_from_str(t, fmt) {
            return Ok(d.and_hms_opt(12, 0, 0).unwrap());
        }
    }
    Err(err(format!("unparsable date '{t}'")))
}

fn date_only(raw: &str) -> R<String> {
    Ok(parse_dt(raw)?.date().format("%Y-%m-%d").to_string())
}

fn norm_name(raw: &str) -> String {
    // Turkish source names arrive uppercase; normalize case + whitespace so
    // cross-file matching (xlsx vs register) can work.
    raw.split_whitespace().collect::<Vec<_>>().join(" ").to_lowercase()
}

// ---------- rows ----------

struct RegisterRow {
    row_no: String,
    guest_id: String,
    guest_name: String,
    checkin: String, // yyyy-MM-dd
    checkout: String,
    room_no: String,
    booking_no: String,
    total_cents: i64,
    nationality: String,
}

struct ContentRow {
    // `#`, Full Name, Room, Total, Booking No, Payment type lang, Currency,
    // Registration Date, Section
    row_no: String,
    guest_name: String,
    room_no: String,
    amount_cents: i64,
    booking_no: String,
    payment_label: String,
    currency: String,
    registered: String, // yyyy-MM-dd HH:mm
}

struct WorkbookRow {
    row_index: usize,
    guest_name: String,
    room_no: String,
    nights: i64,
    adults: i64,
    children: i64,
    room_rate_cents: i64,
    collection_cents: i64,
}

// ---------- shared helpers ----------

/// Room id for a room number; creates the room (plus a fallback room type)
/// when the mirror has never seen that room.
fn ensure_room(conn: &Connection, room_no: &str) -> R<String> {
    let existing: Option<String> = conn
        .query_row(
            "SELECT id FROM rooms WHERE room_number = ?1 AND deleted_at IS NULL",
            params![room_no],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    if let Some(id) = existing {
        return Ok(id);
    }
    let type_id: Option<String> = conn
        .query_row(
            "SELECT id FROM room_types ORDER BY name LIMIT 1",
            [],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?;
    let type_id = match type_id {
        Some(t) => t,
        None => {
            let id = Uuid::now_v7().to_string();
            conn.execute(
                "INSERT INTO room_types (id, name, capacity, base_price_cents) VALUES (?1, ?2, 2, 0)",
                params![id, "Standard"],
            )
            .map_err(|e| e.to_string())?;
            id
        }
    };
    let id = Uuid::now_v7().to_string();
    let floor: i64 = room_no
        .trim()
        .chars()
        .next()
        .and_then(|c| c.to_digit(10))
        .unwrap_or(0) as i64;
    conn.execute(
        "INSERT INTO rooms (id, room_type_id, room_number, floor, cleanliness) VALUES (?1, ?2, ?3, ?4, 'clean')",
        params![id, type_id, room_no, floor],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

/// Guest id by external_ref, then by normalized name; creates when absent.
fn ensure_guest(
    conn: &Connection,
    external_ref: &str,
    full_name: &str,
    nationality: &str,
) -> R<String> {
    if !external_ref.is_empty() {
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM guests WHERE external_ref = ?1",
                params![external_ref],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        {
            return Ok(id);
        }
    }
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM guests WHERE full_name = ?1 COLLATE NOCASE AND deleted_at IS NULL",
            params![full_name],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        if !external_ref.is_empty() {
            conn.execute(
                "UPDATE guests SET external_ref = ?1 WHERE id = ?2 AND external_ref IS NULL",
                params![external_ref, id],
            )
            .map_err(|e| e.to_string())?;
        }
        return Ok(id);
    }
    let id = Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO guests (id, full_name, nationality, external_ref) VALUES (?1, ?2, ?3, ?4)",
        params![
            id,
            full_name,
            if nationality.is_empty() { None } else { Some(nationality) },
            if external_ref.is_empty() { None } else { Some(external_ref) }
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(id)
}

fn even_split_daily_rates(checkin: &str, checkout: &str, total_cents: i64) -> String {
    let start = NaiveDate::parse_from_str(checkin, "%Y-%m-%d")
        .unwrap_or_else(|_| Utc::now().date_naive());
    let end = NaiveDate::parse_from_str(checkout, "%Y-%m-%d")
        .unwrap_or_else(|_| Utc::now().date_naive());
    let nights = (end - start).num_days().max(1) as i64;
    let base = total_cents / nights;
    let rem = total_cents - base * nights;
    let mut arr = Vec::new();
    for i in 0..nights {
        let d = (start + Duration::days(i)).format("%Y-%m-%d").to_string();
        let cents = base + if i < rem { 1 } else { 0 };
        arr.push(json!({ "date": d, "price_cents": cents }));
    }
    serde_json::to_string(&arr).unwrap_or_default()
}

fn derive_status(checkin: &str, checkout: &str) -> &'static str {
    let today = Utc::now().date_naive();
    let ci = NaiveDate::parse_from_str(checkin, "%Y-%m-%d").ok();
    let co = NaiveDate::parse_from_str(checkout, "%Y-%m-%d").ok();
    match (ci, co) {
        (Some(_ci), Some(co)) if co <= today => "checked_out",
        (Some(ci), Some(co)) if ci <= today && today < co => "checked_in",
        _ => "confirmed",
    }
}

/// Upsert a reservation by external_ref; when the ref is new, try to claim an
/// existing ref-less mirror row with the same room+dates first (bookings that
/// arrived from the server side), so the sync mirror is not duplicated.
fn upsert_reservation(
    conn: &Connection,
    external_ref: &str,
    room_id: &str,
    guest_id: &str,
    checkin: &str,
    checkout: &str,
    status: &str,
    total_cents: i64,
    adults: i64,
    children: i64,
) -> R<(String, bool)> {
    if !external_ref.is_empty() {
        if let Some(id) = conn
            .query_row(
                "SELECT id FROM reservations WHERE external_ref = ?1",
                params![external_ref],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        {
            conn.execute(
                "UPDATE reservations SET room_id = ?1, guest_id = ?2, checkin_date = ?3,
                 checkout_date = ?4, status = ?5, total_price_cents = ?6, daily_rates = ?7,
                 adults = MAX(adults, ?8), children = MAX(children, ?9) WHERE id = ?10",
                params![
                    room_id,
                    if guest_id.is_empty() { None } else { Some(guest_id) },
                    checkin,
                    checkout,
                    status,
                    total_cents,
                    even_split_daily_rates(checkin, checkout, total_cents),
                    adults,
                    children,
                    id
                ],
            )
            .map_err(|e| e.to_string())?;
            return Ok((id, true));
        }
    }
    if let Some(id) = conn
        .query_row(
            "SELECT id FROM reservations WHERE external_ref IS NULL AND deleted_at IS NULL
               AND room_id = ?1 AND checkin_date = ?2 AND checkout_date = ?3 LIMIT 1",
            params![room_id, checkin, checkout],
            |r| r.get(0),
        )
        .optional()
        .map_err(|e| e.to_string())?
    {
        conn.execute(
            "UPDATE reservations SET external_ref = ?1, guest_id = COALESCE(?2, guest_id),
             status = ?3, total_price_cents = ?4, daily_rates = ?5 WHERE id = ?6",
            params![
                if external_ref.is_empty() { None } else { Some(external_ref) },
                if guest_id.is_empty() { None } else { Some(guest_id) },
                status,
                total_cents,
                even_split_daily_rates(checkin, checkout, total_cents),
                id
            ],
        )
        .map_err(|e| e.to_string())?;
        return Ok((id, true));
    }
    let id = Uuid::now_v7().to_string();
    conn.execute(
        "INSERT INTO reservations (id, room_id, guest_id, checkin_date, checkout_date, status,
            total_price_cents, currency, adults, children, external_ref, daily_rates)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, 'TRY', ?8, ?9, ?10, ?11)",
        params![
            id,
            room_id,
            if guest_id.is_empty() { None } else { Some(guest_id) },
            checkin,
            checkout,
            status,
            total_cents,
            adults,
            children,
            if external_ref.is_empty() { None } else { Some(external_ref) },
            even_split_daily_rates(checkin, checkout, total_cents)
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok((id, false))
}

/// Mark overlapping same-room stays of an imported reservation as conflict
/// (the server settles conflicts when online; README §2.3).
fn mark_conflicts(conn: &Connection, exclude_id: &str, room_id: &str, checkin: &str, checkout: &str) -> R<usize> {
    let n = conn
        .execute(
            // Imported↔imported same-room overlaps only: rows carrying an
            // external_ref from ANOTHER import source (register bookings
            // overlapping each other). Pure server rows and the just-upserted
            // row (exclude_id) are never touched. The synthesized `xlsx-res-`
            // rows have placeholder dates, so they are excluded too.
            "UPDATE reservations SET status = 'conflict'
             WHERE deleted_at IS NULL AND room_id = ?1
               AND external_ref IS NOT NULL AND external_ref NOT LIKE 'xlsx-%'
               AND id != ?2
               AND checkin_date < ?4 AND checkout_date > ?3",
            params![room_id, exclude_id, checkin, checkout],
        )
        .map_err(|e| e.to_string())?;
    Ok(n)
}

fn insert_payment_if_absent(
    conn: &Connection,
    reservation_id: &str,
    amount_cents: i64,
    currency: &str,
    payment_type: &str,
    reference: Option<&str>,
    created_ms: i64,
    dedupe_day: bool,
) -> R<bool> {
    if let Some(r) = reference {
        let exists: Option<String> = conn
            .query_row(
                "SELECT id FROM payments WHERE reference = ?1",
                params![r],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?;
        if exists.is_some() {
            return Ok(false);
        }
    }
    if dedupe_day {
        let day = chrono::DateTime::from_timestamp_millis(created_ms)
            .map(|d| d.format("%Y-%m-%d").to_string())
            .unwrap_or_default();
        let matched: Option<String> = if day.is_empty() {
            conn.query_row(
                "SELECT id FROM payments WHERE reservation_id = ?1 AND amount_cents = ?2 LIMIT 1",
                params![reservation_id, amount_cents],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        } else {
            conn.query_row(
                "SELECT id FROM payments WHERE reservation_id = ?1 AND amount_cents = ?2
                  AND date(created_at/1000, 'unixepoch', 'localtime') = ?3 LIMIT 1",
                params![reservation_id, amount_cents, day],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        };
        if matched.is_some() {
            return Ok(false);
        }
    }
    conn.execute(
        "INSERT INTO payments (id, reservation_id, amount_cents, currency, payment_type, reference, is_reversal, created_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)",
        params![
            Uuid::now_v7().to_string(),
            reservation_id,
            amount_cents,
            currency,
            payment_type,
            reference,
            created_ms
        ],
    )
    .map_err(|e| e.to_string())?;
    Ok(true)
}

// ---------- CSV guests register (guest-daily_*.csv) ----------

pub fn import_guest_register(store: &mut LocalStore, path: &Path) -> R<Value> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .has_headers(true)
        .from_path(path)
        .map_err(|e| e.to_string())?;
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.trim().trim_matches('"').to_lowercase())
        .collect();
    let idx = |name: &str| headers.iter().position(|h| h.contains(name));

    let (i_row, i_guestid, i_name, i_ci, i_co, i_room, i_book, i_total, i_nat) = (
        idx("#").ok_or(err("# column missing".into()))?,
        idx("guest id").ok_or(err("Guest id column missing".into()))?,
        idx("full name").ok_or(err("Full Name column missing".into()))?,
        idx("check-in").ok_or(err("Check-in column missing".into()))?,
        idx("check-out").ok_or(err("Check-out column missing".into()))?,
        idx("room").ok_or(err("Room column missing".into()))?,
        idx("booking").ok_or(err("Booking No column missing".into()))?,
        idx("total").ok_or(err("Total column missing".into()))?,
        idx("nationality").unwrap_or(usize::MAX),
    );

    let mut summary = ImportSummary::new();
    let mut rows: Vec<RegisterRow> = Vec::new();
    for rec in reader.records() {
        let rec = match rec.map_err(|e| e.to_string()) {
            Ok(r) => r,
            Err(e) => {
                summary.errors.push(e);
                continue;
            }
        };
        let get = |i: usize| rec.get(i).unwrap_or("").trim().to_string();
        let row = RegisterRow {
            row_no: get(i_row),
            guest_id: get(i_guestid),
            guest_name: get(i_name),
            checkin: date_only(&rec.get(i_ci).unwrap_or("").to_string()).unwrap_or_default(),
            checkout: date_only(&rec.get(i_co).unwrap_or("").to_string()).unwrap_or_default(),
            room_no: get(i_room),
            booking_no: get(i_book),
            total_cents: parse_amount_cents(&rec.get(i_total).unwrap_or("").to_string()).unwrap_or(0),
            nationality: if i_nat == usize::MAX { String::new() } else { get(i_nat) },
        };
        if row.booking_no.is_empty() && row.guest_name.is_empty() {
            continue;
        }
        rows.push(row);
    }

    let conn = &store.conn;
    for row in rows {
        if row.checkin.is_empty() || row.checkout.is_empty() || row.room_no.is_empty() {
            summary.errors.push(format!("row {} missing dates/room", row.row_no));
            continue;
        }
        let room_id = match ensure_room(conn, &row.room_no) {
            Ok(id) => id,
            Err(e) => {
                summary.errors.push(e);
                continue;
            }
        };
        let guest_id = match ensure_guest(conn, &row.guest_id, &row.guest_name, &row.nationality) {
            Ok(id) => id,
            Err(e) => {
                summary.errors.push(e);
                continue;
            }
        };
        match upsert_reservation(
            conn,
            &row.booking_no,
            &room_id,
            &guest_id,
            &row.checkin,
            &row.checkout,
            derive_status(&row.checkin, &row.checkout),
            row.total_cents,
            1,
            0,
        ) {
            Ok((id, updated)) => {
                if updated {
                    summary.updated += 1
                } else {
                    summary.inserted += 1
                }
                if let Err(e) =
                    mark_conflicts(conn, &id, &room_id, &row.checkin, &row.checkout)
                {
                    summary.errors.push(e);
                }
            }
            Err(e) => summary.errors.push(format!("row {}: {e}", row.row_no)),
        }
    }
    Ok(summary.into_json("guest register"))
}

// ---------- CSV collections ledger (collection_*.csv) ----------

pub fn import_collection_ledger(store: &mut LocalStore, path: &Path) -> R<Value> {
    let mut reader = csv::ReaderBuilder::new()
        .delimiter(b';')
        .has_headers(true)
        .from_path(path)
        .map_err(|e| e.to_string())?;
    let headers: Vec<String> = reader
        .headers()
        .map_err(|e| e.to_string())?
        .iter()
        .map(|h| h.trim().trim_matches('"').to_lowercase())
        .collect();
    let idx = |name: &str| headers.iter().position(|h| h.contains(name));
    let (i_row, i_name, i_total, i_book, i_ptype, i_cur, i_date) = (
        idx("#").ok_or(err("# column missing".into()))?,
        idx("full name").ok_or(err("Full Name column missing".into()))?,
        idx("total").ok_or(err("Total column missing".into()))?,
        idx("booking").ok_or(err("Booking No column missing".into()))?,
        idx("payment type lang").unwrap_or(usize::MAX),
        idx("currency").unwrap_or(usize::MAX),
        idx("registration date").ok_or(err("Registration Date column missing".into()))?,
    );
    let slug = |s: &str| -> String {
        s.chars()
            .filter(|c| c.is_ascii_alphanumeric() || c.is_ascii_whitespace())
            .collect::<String>()
            .split_whitespace()
            .collect::<Vec<_>>()
            .join("_")
            .to_lowercase()
    };

    let mut summary = ImportSummary::new();
    let mut rows: Vec<ContentRow> = Vec::new();
    for rec in reader.records() {
        let rec = match rec.map_err(|e| e.to_string()) {
            Ok(r) => r,
            Err(e) => {
                summary.errors.push(e);
                continue;
            }
        };
        let get = |i: usize| rec.get(i).unwrap_or("").trim().to_string();
        let row = ContentRow {
            row_no: get(i_row),
            guest_name: get(i_name),
            room_no: String::new(),
            amount_cents: parse_amount_cents(&rec.get(i_total).unwrap_or("").to_string()).unwrap_or(0),
            booking_no: get(i_book),
            payment_label: if i_ptype == usize::MAX { String::new() } else { slug(&get(i_ptype)) },
            currency: if i_cur == usize::MAX { "TRY".into() } else { get(i_cur) },
            registered: get(i_date),
        };
        if row.booking_no.is_empty() && row.guest_name.is_empty() {
            continue;
        }
        rows.push(row);
    }

    let conn = &store.conn;
    for row in rows {
        let created: NaiveDateTime = match parse_dt(&row.registered) {
            Ok(d) => d,
            Err(e) => {
                summary.errors.push(format!("row {}: {e}", row.row_no));
                continue;
            }
        };
        let created_ms = created.and_utc().timestamp_millis();
        let reservation_id: Option<String> = if !row.booking_no.is_empty() {
            conn.query_row(
                "SELECT id FROM reservations WHERE external_ref = ?1",
                params![row.booking_no],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())?
        } else {
            None
        };
        let Some(reservation_id) = reservation_id else {
            summary.errors.push(format!(
                "row {}: no reservation for booking {}",
                row.row_no, row.booking_no
            ));
            continue;
        };
        if row.amount_cents == 0 {
            continue;
        }
        match insert_payment_if_absent(
            conn,
            &reservation_id,
            row.amount_cents,
            &row.currency,
            if row.payment_label.is_empty() { "tahsilat" } else { &row.payment_label },
            Some(&format!("collection-{}", row.row_no)),
            created_ms,
            /*dedupe_day*/ true,
        ) {
            Ok(true) => summary.inserted += 1,
            Ok(false) => summary.skipped += 1,
            Err(e) => summary.errors.push(format!("row {}: {e}", row.row_no)),
        }
    }
    summary.updated = 0;
    Ok(summary.into_json("collections"))
}

// ---------- Reservation.xlsx workbook ----------

pub fn import_reservation_workbook(store: &mut LocalStore, path: &Path, anchor: &str) -> R<Value> {
    let anchor_date = NaiveDate::parse_from_str(anchor.trim(), "%Y-%m-%d")
        .map_err(|_| err(format!("anchor date '{anchor}' must be YYYY-MM-DD")))?;
    let mut book: calamine::Xlsx<std::io::BufReader<std::fs::File>> =
        calamine::open_workbook::<calamine::Xlsx<std::io::BufReader<std::fs::File>>, _>(path)
            .map_err(|e: calamine::XlsxError| e.to_string())?;
    let sheet = book
        .sheet_names()
        .first()
        .cloned()
        .ok_or(err("workbook has no sheets".into()))?;
    let range = book.worksheet_range(&sheet).map_err(|e| e.to_string())?;
    if range.get_size().0 < 2 {
        return Ok(json!({
            "dataset": "reservations xlsx", "inserted": 0, "updated": 0, "skipped": 0,
            "error_count": 1, "errors": ["no data rows"]
        }));
    }

    let mut rows: Vec<WorkbookRow> = Vec::new();
    for (i, row) in range.rows().enumerate() {
        if i == 0 {
            continue; // header: Guest, Room, Night, Adult, Child, Room Rate, Transactions, Collection, Balance
        }
        let cell_str = |c: usize| row.get(c).map(|v| v.to_string()).unwrap_or_default();
        let cell_num = |c: usize| {
            row.get(c)
                .and_then(|v| match v {
                    calamine::Data::Int(n) => Some(*n),
                    calamine::Data::Float(f) => Some(*f as i64),
                    calamine::Data::String(s) => s.parse::<i64>().ok(),
                    _ => None,
                })
                .unwrap_or(0)
        };
        let name = cell_str(0);
        if name.trim().is_empty() {
            continue;
        }
        rows.push(WorkbookRow {
            row_index: i,
            guest_name: name.trim().to_string(),
            room_no: cell_str(1).trim().replace(".0", ""),
            nights: cell_num(2).max(0),
            adults: cell_num(3).max(1),
            children: cell_num(4).max(0),
            room_rate_cents: parse_amount_cents(&cell_str(5)).unwrap_or(0),
            collection_cents: parse_amount_cents(&cell_str(7)).unwrap_or(0),
        });
    }

    let conn = &store.conn;
    let mut summary = ImportSummary::new();
    let mut cursor = anchor_date;

    for row in rows {
        let room_id = match ensure_room(conn, &row.room_no) {
            Ok(id) => id,
            Err(e) => {
                summary.errors.push(format!("row {}: {e}", row.row_index));
                continue;
            }
        };
        let guest_id = match ensure_guest(conn, "", &row.guest_name, "") {
            Ok(id) => id,
            Err(e) => {
                summary.errors.push(format!("row {}: {e}", row.row_index));
                continue;
            }
        };

        // Claim an exact natural match: same room + normalized guest name + a
        // night count within ±1, preferring register-imported rows (booking
        // ref) over synthesized ones.
        let norm = norm_name(&row.guest_name);
        let nights = row.nights.max(1);
        let matched: Option<String> = conn
            .query_row(
                "SELECT r.id FROM reservations r JOIN guests g ON g.id = r.guest_id
                 WHERE r.deleted_at IS NULL AND r.room_id = ?1
                   AND lower(g.full_name) LIKE ?2
                   AND (julianday(r.checkout_date) - julianday(r.checkin_date)) BETWEEN ?3 - 1 AND ?3 + 1
                 ORDER BY CASE WHEN r.external_ref IS NULL THEN 1 ELSE 0 END, r.checkin_date LIMIT 1",
                params![room_id, format!("%{norm}%"), nights],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| e.to_string())
            .unwrap_or(None);

        let external_ref = format!("xlsx-res-row-{}", row.row_index);

        // Dates: real ones when the row matched the register, otherwise a
        // sequential synthesized stay anchored at `anchor` (the workbook has
        // no dates at all — this is documented in Settings).
        let (checkin, checkout) = match &matched {
            Some(id) => {
                let dates: (String, String) = conn
                    .query_row(
                        "SELECT checkin_date, checkout_date FROM reservations WHERE id = ?1",
                        params![id],
                        |r| Ok((r.get(0)?, r.get(1)?)),
                    )
                    .map_err(|e| e.to_string())?;
                dates
            }
            None => {
                let ci = cursor;
                let co = ci + Duration::days(nights.max(1));
                cursor = co;
                (ci.format("%Y-%m-%d").to_string(), co.format("%Y-%m-%d").to_string())
            }
        };

        match upsert_reservation(
            conn,
            &external_ref,
            &room_id,
            &guest_id,
            &checkin,
            &checkout,
            derive_status(&checkin, &checkout),
            row.room_rate_cents,
            row.adults,
            row.children,
        ) {
            Ok((id, updated)) => {
                if updated {
                    summary.updated += 1
                } else {
                    summary.inserted += 1
                }
                if row.collection_cents > 0 {
                    match insert_payment_if_absent(
                        conn,
                        &id,
                        row.collection_cents,
                        "TRY",
                        "bank_transfer",
                        Some(&format!("legacy-collection-summary-row-{}", row.row_index)),
                        Utc::now().timestamp_millis(),
                        /*dedupe_day*/ true,
                    ) {
                        Ok(true) => {}
                        Ok(false) => summary.skipped += 1,
                        Err(e) => summary.errors.push(format!("row {}: {e}", row.row_index)),
                    }
                }
            }
            Err(e) => summary.errors.push(format!("row {}: {e}", row.row_index)),
        }
    }
    Ok(summary.into_json("reservations xlsx"))
}

// ---------- userpass.md ----------

/// Parse the credentials file. Blocks: `- HMS Admin Panel:` (hotel 0, users
/// without a role line are admins) and `- HMS Hotel Login:` (Hotel Code gives
/// the hotel; role lines precede user/pass pairs). Returns
/// (hotel_id, username, password, role).
pub fn parse_userpass(content: &str) -> Vec<(i64, String, String, String)> {
    let mut out = Vec::new();
    let mut hotel_code = 0i64;
    let mut pending_role = String::new();
    let mut pending_user: Option<(String, String)> = None; // (username, password)

    let flush = |pending: &mut Option<(String, String)>,
                 role: &str,
                 hotel: i64,
                 out: &mut Vec<(i64, String, String, String)>| {
        if let Some((user, pass)) = pending.take() {
            if !user.is_empty() && !pass.is_empty() {
                let role_l = role.to_lowercase();
                let role = match role_l.as_str() {
                    "reseption" | "reception" => "reception",
                    "housekeeper" | "housekeeping" => "housekeeping",
                    "accountant" => "manager", // no accountant role exists server-side
                    "" => "admin",
                    _ => role,
                };
                out.push((hotel, user, pass, role.to_string()));
            }
        }
    };

    for line in content.lines() {
        let line = line.trim_start();
        let lower = line.to_lowercase();
        if lower.starts_with("- hms admin panel")
            || lower.starts_with("- hsm admin panel")
            || lower.starts_with("- hms hotel login")
            || lower.starts_with("- postgresql")
            || lower.starts_with("- booking")
            || lower.starts_with("- accounts.expediagroup")
            || lower.starts_with("- expediapartnercentral")
        {
            flush(&mut pending_user, &pending_role, hotel_code, &mut out);
            pending_role.clear();
            if !lower.starts_with("- hms hotel login") && !lower.starts_with("- hms admin panel") && !lower.starts_with("- hsm admin panel") {
                // any other top-level block ends the hotel section
                hotel_code = 0;
            }
            continue;
        }
        if let Some(v) = line
            .strip_prefix("- Hotel Code")
            .or_else(|| line.strip_prefix("- Hotel code"))
        {
            let code: String = v.trim_start_matches(':').trim().to_string();
            hotel_code = code.parse::<i64>().unwrap_or(0);
            continue;
        }
        if line.starts_with("- Role :") || line.starts_with("- role :") {
            flush(&mut pending_user, &pending_role, hotel_code, &mut out);
            pending_role = line.split(':').nth(1).unwrap_or("").trim().to_string();
            continue;
        }
        if lower.starts_with("- user :") || lower.starts_with("- user:") {
            flush(&mut pending_user, &pending_role, hotel_code, &mut out);
            pending_user = Some((
                line.split(':').nth(1).unwrap_or("").trim().to_string(),
                String::new(),
            ));
            continue;
        }
        if lower.contains("pass :") || lower.contains("pass:") {
            if let Some((_, pass)) = pending_user.as_mut() {
                if pass.is_empty() {
                    *pass = line.split(':').nth(1).unwrap_or("").trim().to_string();
                }
            }
            continue;
        }
    }
    flush(&mut pending_user, &pending_role, hotel_code, &mut out);
    out
}

pub fn import_users(store: &mut LocalStore, path: &Path) -> R<Value> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let users = parse_userpass(&content);
    if users.is_empty() {
        return Ok(json!({
            "dataset": "users", "inserted": 0, "updated": 0, "skipped": 0, "error_count": 1,
            "errors": ["no user/pass entries recognized in the file"]
        }));
    }
    let mut summary = ImportSummary::new();
    for (hotel, user, pass, role) in users {
        match store.upsert_local_user(&user, &user, &role, hotel, &pass) {
            Ok(()) => summary.inserted += 1,
            Err(e) => summary.errors.push(format!("{user}: {e}")),
        }
    }
    Ok(summary.into_json("users"))
}
