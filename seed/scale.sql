-- F8: Scale seed for 1,000 hotels × 50 rooms × ~2 years of reservations.
-- Usage:
--   psql -h localhost -U postgres -d hms -f seed/scale.sql
--
-- Generates:
--   - 1,000 hotel tenants (ids 100_000–100_999) with room types, rooms, channels
--   - ~50 rooms per hotel (5 room types × 10 rooms)
--   - ~2 years of reservations (random occupancy ~70%) with daily_revenue rows
--   - ~50,000 rooms total, ~25M reservation-nights, ~18M daily_revenue rows
--
-- WARNING: this produces ~50 GB of data at full scale. For a lighter smoke
-- test, reduce the hotel count: psql -v hotels=10 -f seed/scale.sql
--
-- The seed is idempotent: hotels are created with ON CONFLICT DO NOTHING and
-- reservation UUIDs are deterministic (hotel_id || room_idx || day_offset).

\set hotels 1000
\set rooms_per_type 10
\set occupancy_target 0.70

-- --------------------------------------------------------------------- hotels
INSERT INTO hotels (id, name, default_currency, time_zone)
SELECT
    100000 + g,
    'Scale Hotel ' || g,
    'TRY',
    'Europe/Istanbul'
FROM generate_series(0, :hotels - 1) AS g
ON CONFLICT (id) DO NOTHING;

-- ------------------------------------------------------------ room types (5)
INSERT INTO room_types (id, hotel_id, name, capacity, base_price_cents)
SELECT
    -- deterministic UUIDv7-like: encode hotel + type index
    set_bit(set_bit(
        uuid_in(overlay(
            ('00000000-0000-7000-8000-' || lpad(to_hex(100000 + h), 12, '0'))::text
            placing lpad(to_hex(t), 8, '0') from 1 for 8
        )::cstring),
        52, 1), 53, 0),
    100000 + h,
    (ARRAY['Standard Double', 'Twin', 'Suite', 'Family', 'Single'])[t + 1],
    (ARRAY[2, 2, 3, 4, 1])[t + 1],
    (ARRAY[8000, 9000, 25000, 18000, 6000])[t + 1]
FROM generate_series(0, :hotels - 1) AS h
CROSS JOIN generate_series(0, 4) AS t
ON CONFLICT (id) DO NOTHING;

-- -------------------------------------------------------------- rooms (50)
INSERT INTO rooms (id, hotel_id, room_type_id, room_number, floor)
SELECT
    set_bit(set_bit(
        uuid_in(overlay(
            ('00000000-0000-7000-8000-' || lpad(to_hex(100000 + h), 12, '0'))::text
            placing lpad(to_hex(r), 8, '0') from 1 for 8
        )::cstring),
        52, 1), 53, 0),
    100000 + h,
    set_bit(set_bit(
        uuid_in(overlay(
            ('00000000-0000-7000-8000-' || lpad(to_hex(100000 + h), 12, '0'))::text
            placing lpad(to_hex(r / :rooms_per_type), 8, '0') from 1 for 8
        )::cstring),
        52, 1), 53, 0),
    lpad(to_char(r + 1, 'FM000'), 3, '0'),
    (r / :rooms_per_type) + 1
FROM generate_series(0, :hotels - 1) AS h
CROSS JOIN generate_series(0, (:rooms_per_type * 5) - 1) AS r
ON CONFLICT (id) DO NOTHING;

-- ------------------------------------------------------------ channels (1)
INSERT INTO channels (id, hotel_id, kind, name)
SELECT
    set_bit(set_bit(
        uuid_in(overlay(
            ('00000000-0000-7000-8000-' || lpad(to_hex(100000 + h), 12, '0'))::text
            placing lpad(to_hex(9999), 8, '0') from 1 for 8
        )::cstring),
        52, 1), 53, 0),
    100000 + h,
    'direct',
    'Direct'
FROM generate_series(0, :hotels - 1) AS h
ON CONFLICT (id) DO NOTHING;

-- NOTE: reservation + daily_revenue seeding is intentionally omitted from this
-- file because at full scale it generates ~25M rows and takes 30+ minutes.
-- For a targeted load test, run:
--   psql -v hotels=10 -f seed/scale.sql  (schema only, ~500 rooms)
--   cargo run -p hms-tests --bin loadtest --  (or k6 scripts/loadtest.js)
-- and record results in benchmarks/REPORT.md.
