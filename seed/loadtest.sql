-- Load-test seed (README §13): hotel 4443 with a realistic inventory so the
-- benchmarks probe a real-sized dataset, not the 2-room dev fixture.
-- Idempotent: re-running wipes the load-test hotel and re-seeds it.
-- Dev data (hotel 4442 / platform 0) is untouched.
--
-- Run:  psql -U postgres -d hms -h localhost -f seed/loadtest.sql

-- ---------------------------------------------------------------------------
-- 0. wipe any previous load-test hotel
-- ---------------------------------------------------------------------------
DELETE FROM payments           WHERE hotel_id = 4443;
DELETE FROM folio_transactions WHERE hotel_id = 4443;
DELETE FROM daily_revenue      WHERE hotel_id = 4443;
DELETE FROM reservations       WHERE hotel_id = 4443;
DELETE FROM price_rules        WHERE hotel_id = 4443;
DELETE FROM channels           WHERE hotel_id = 4443;
DELETE FROM rooms              WHERE hotel_id = 4443;
DELETE FROM room_types         WHERE hotel_id = 4443;
DELETE FROM users              WHERE hotel_id = 4443;
DELETE FROM hotels             WHERE id = 4443;

-- ---------------------------------------------------------------------------
-- 1. hotel + catalog: 3 room types, 60 rooms (floors 1-6), standard channels
-- ---------------------------------------------------------------------------
INSERT INTO hotels (id, name, default_currency, time_zone)
VALUES (4443, 'Load Test Hotel', 'TRY', 'Europe/Istanbul');

INSERT INTO room_types (id, hotel_id, name, capacity, base_price_cents)
VALUES
  (gen_random_uuid(), 4443, 'Standart Double', 2, 300000),
  (gen_random_uuid(), 4443, 'Family Suite',    4, 550000),
  (gen_random_uuid(), 4443, 'Deluxe King',     2, 800000);

INSERT INTO rooms (id, hotel_id, room_type_id, room_number, floor, cleanliness)
SELECT gen_random_uuid(), 4443,
       (SELECT id FROM room_types WHERE hotel_id = 4443 AND name =
          CASE WHEN g <= 20 THEN 'Standart Double'
               WHEN g <= 40 THEN 'Family Suite'
               ELSE 'Deluxe King' END),
       (CASE WHEN g <= 20 THEN '1' WHEN g <= 40 THEN '2' ELSE '3' END || LPAD(g::text, 3, '0')),
       (g - 1) / 20,
       'clean'
FROM generate_series(1, 60) AS g;

INSERT INTO channels (id, hotel_id, kind, name)
SELECT gen_random_uuid(), 4443, k, n
FROM (VALUES ('direct','Direct'), ('walk_in','Walk-in'), ('booking_com','Booking.com'),
             ('expedia','Expedia'), ('agency','Agency')) AS ch(k, n);

-- seasonal price rule per room type (+25% for a year, rules take precedence)
INSERT INTO price_rules (id, hotel_id, room_type_id, date_from, date_to, price_cents, currency)
SELECT gen_random_uuid(), 4443, rt.id,
       make_date(2026, 1, 10), make_date(2026, 12, 31),
       round(rt.base_price_cents * 1.25), 'TRY'
FROM room_types rt WHERE rt.hotel_id = 4443;

INSERT INTO users (id, hotel_id, username, full_name, role, password_hash, is_active)
VALUES (gen_random_uuid(), 4443, 'loadmgr', 'Load Test Manager', 'manager', 'x', TRUE)
ON CONFLICT (hotel_id, username) DO NOTHING;

-- dev-only: let loadmgr log in by copying the local dev manager's password
-- hash (references the hash column — no secret in this file)
UPDATE users SET password_hash = (
  SELECT password_hash FROM users WHERE hotel_id = 4442 AND username = 'mtest'
) WHERE hotel_id = 4443 AND username = 'loadmgr';

-- ---------------------------------------------------------------------------
-- 2. reservations: every room gets 5 non-overlapping 2-night stays
--    (2 in the past, 1 near-today, 2 upcoming)
-- ---------------------------------------------------------------------------
INSERT INTO reservations (id, hotel_id, room_id, channel_id, guest_id, stay,
                          checkin_date, checkout_date, status,
                          total_price_cents, currency,
                          adults, children, created_by)
SELECT gen_random_uuid(), 4443, r.id,
       (SELECT id FROM channels WHERE hotel_id = 4443 AND kind =
          (ARRAY['direct','booking_com','expedia','agency'])[1 + abs(hashtext(r.id::text)) % 4]),
       NULL,
       daterange(d.ci, d.ci + 2, '[)'),
       d.ci, d.ci + 2,
       CASE WHEN d.ci + 2 <= current_date THEN 'checked_out'
            WHEN d.ci <= current_date AND current_date < d.ci + 2 THEN 'checked_in'
            ELSE 'confirmed' END,
       rt.base_price_cents * 2,
       'TRY',
       1, 0,
       (SELECT id FROM users WHERE hotel_id = 4443 AND username = 'loadmgr')
FROM rooms r
JOIN room_types rt ON rt.id = r.room_type_id AND rt.hotel_id = 4443
CROSS JOIN LATERAL (
  SELECT (current_date + (k - 5) * 2 + 1)::date AS ci
  FROM generate_series(0, 7) AS k
) d;   -- 8 waves x 60 rooms = 480 reservations (past / in-house / upcoming)

-- ---------------------------------------------------------------------------
-- 3. daily_revenue mirrors the reservations (the server keeps this table
--    in-txn at runtime; the seed fills it directly) + payments/folio extras
-- ---------------------------------------------------------------------------
INSERT INTO daily_revenue (hotel_id, date, room_id, room_type_id, channel_id,
                           room_revenue_cents, extra_revenue_cents,
                           payment_collected_cents, occupied)
SELECT res.hotel_id, day, res.room_id, r.room_type_id, res.channel_id,
       res.total_price_cents / (res.checkout_date - res.checkin_date),
       0, 0, TRUE
FROM reservations res
JOIN rooms r ON r.id = res.room_id
CROSS JOIN LATERAL generate_series(res.checkin_date, res.checkout_date - 1, INTERVAL '1 day') AS day
WHERE res.hotel_id = 4443;

-- payments: every 3rd reservation got a 50% deposit
INSERT INTO payments (id, hotel_id, reservation_id, amount_cents, currency, payment_type,
                      is_reversal, created_by)
SELECT gen_random_uuid(), res.hotel_id, res.id,
       res.total_price_cents / 2, res.currency,
       (ARRAY['cash','credit_card','bank_transfer','agency'])[1 + abs(hashtext(res.id::text)) % 4],
       FALSE,
       (SELECT id FROM users WHERE hotel_id = 4443 AND username = 'loadmgr')
FROM reservations res
WHERE res.hotel_id = 4443
  AND abs(hashtext(res.id::text)) % 3 = 0;

-- mirror deposits onto the payment day (day 1 of the stay)
UPDATE daily_revenue dr
SET payment_collected_cents = p.amount_cents
FROM payments p
JOIN reservations res ON res.id = p.reservation_id
WHERE p.hotel_id = 4443
  AND dr.hotel_id = 4443
  AND dr.room_id = res.room_id
  AND dr.date = res.checkin_date
  AND dr.channel_id = res.channel_id;

-- folio: every 5th reservation got a 1500 cent minibar on its first night
INSERT INTO folio_transactions (id, hotel_id, reservation_id, category, description,
                                unit_price_cents, quantity, total_cents, currency)
SELECT gen_random_uuid(), res.hotel_id, res.id, 'minibar', 'seed minibar',
       1500, 1, 1500, 'TRY'
FROM reservations res
WHERE res.hotel_id = 4443
  AND abs(hashtext(res.id::text)) % 5 = 0;

UPDATE daily_revenue dr
SET extra_revenue_cents = extra_revenue_cents + f.total_cents
FROM folio_transactions f
JOIN reservations res ON res.id = f.reservation_id
WHERE f.hotel_id = 4443 AND f.deleted_at IS NULL
  AND dr.hotel_id = 4443
  AND dr.room_id = res.room_id
  AND dr.date = res.checkin_date
  AND dr.channel_id = res.channel_id;

SELECT 'load-test hotel seeded: ' ||
       (SELECT count(*) FROM reservations WHERE hotel_id = 4443) || ' reservations, ' ||
       (SELECT count(*) FROM rooms WHERE hotel_id = 4443) || ' rooms, ' ||
       (SELECT count(*) FROM daily_revenue WHERE hotel_id = 4443) || ' daily_revenue rows' AS status;
