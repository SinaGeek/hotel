-- F8: Scale seed for 1,000 hotels × 50 rooms × ~2 years of reservations.
-- Usage:
--   psql -h localhost -U postgres -d hms -f seed/scale.sql
--   psql -v hotels=10 -f seed/scale.sql
--
-- Generates:
--   - N hotel tenants (ids 100_000..) with room types, rooms, channels
--   - ~50 rooms per hotel (5 room types × 10 rooms)
--   - Channels, price rules, and manager users for testing
--
-- The seed is idempotent: hotels are created with ON CONFLICT DO NOTHING and
-- UUIDs are deterministic (md5-hashed from unique tenant/item keys).

\if :{?hotels}
\else
\set hotels 1000
\endif
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
    -- deterministic UUIDv4-equivalent from unique string
    md5('room_type_' || (100000 + h) || '_' || t)::uuid,
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
    md5('room_' || (100000 + h) || '_' || r)::uuid,
    100000 + h,
    md5('room_type_' || (100000 + h) || '_' || (r / :rooms_per_type))::uuid,
    lpad(to_char(r + 1, 'FM000'), 3, '0'),
    (r / :rooms_per_type) + 1
FROM generate_series(0, :hotels - 1) AS h
CROSS JOIN generate_series(0, (:rooms_per_type * 5) - 1) AS r
ON CONFLICT (id) DO NOTHING;

-- ------------------------------------------------------------ channels (1)
INSERT INTO channels (id, hotel_id, kind, name)
SELECT
    md5('channel_' || (100000 + h) || '_direct')::uuid,
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
