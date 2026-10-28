-- Local desktop mirror schema (README §11): the desktop holds a FULL local
-- copy of its own hotel in SQLite and works with no server. Mirrors the
-- server tables the sync pull stream touches plus the engine bookkeeping.
CREATE TABLE IF NOT EXISTS applied_ops (
    operation_id TEXT PRIMARY KEY,   -- server echo of our op OR a server change we saw
    change_seq   INTEGER NOT NULL,   -- server monotonic counter of the record
    applied_at   INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS outbox (
    operation_id     TEXT PRIMARY KEY,     -- ChangeOperation.operation_id
    hotel_id         INTEGER NOT NULL,
    entity_type      TEXT NOT NULL,        -- 'guest','reservation',...
    entity_id        TEXT NOT NULL,
    operation_type   TEXT NOT NULL,        -- 'insert','update','delete'
    payload          TEXT NOT NULL,        -- JSON
    client_timestamp TEXT NOT NULL,
    enqueued_at_ms   INTEGER NOT NULL,
    attempts         INTEGER NOT NULL DEFAULT 0,
    last_error       TEXT,
    next_retry_at_ms INTEGER
);
CREATE INDEX IF NOT EXISTS idx_outbox_order ON outbox (enqueued_at_ms);

CREATE TABLE IF NOT EXISTS failures (             -- permanent rejections, UI-visible
    operation_id TEXT PRIMARY KEY,
    entity_type  TEXT NOT NULL,
    entity_id    TEXT NOT NULL,
    attempts     INTEGER NOT NULL,
    last_error   TEXT,
    occurred_at  INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS kv (   -- pull_cursor / last_synced_at / offline_login_cache / auth_token
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- ---- mirror tables (server-shaped columns, minimal but sufficient) ----

CREATE TABLE IF NOT EXISTS guests (
    id          TEXT PRIMARY KEY,
    full_name   TEXT NOT NULL,
    nationality TEXT,
    license_plate TEXT,
    birth_date  TEXT,
    deleted_at  INTEGER
);

CREATE TABLE IF NOT EXISTS room_types (
    id               TEXT PRIMARY KEY,
    name             TEXT NOT NULL,
    capacity         INTEGER NOT NULL DEFAULT 2,
    base_price_cents INTEGER NOT NULL DEFAULT 0,
    deleted_at       INTEGER
);

CREATE TABLE IF NOT EXISTS rooms (
    id            TEXT PRIMARY KEY,
    room_type_id  TEXT NOT NULL,
    room_number   TEXT NOT NULL,
    floor         INTEGER NOT NULL DEFAULT 0,
    cleanliness   TEXT NOT NULL DEFAULT 'clean',
    deleted_at    INTEGER
);

CREATE TABLE IF NOT EXISTS channels (
    id        TEXT PRIMARY KEY,
    kind      TEXT NOT NULL DEFAULT 'other',
    name      TEXT NOT NULL,
    is_active INTEGER NOT NULL DEFAULT 1
);

CREATE TABLE IF NOT EXISTS price_rules (
    id          TEXT PRIMARY KEY,
    room_type_id TEXT NOT NULL,
    date_from   TEXT NOT NULL,
    date_to     TEXT NOT NULL,
    price_cents INTEGER NOT NULL,
    currency    TEXT NOT NULL DEFAULT 'TRY'
);

CREATE TABLE IF NOT EXISTS reservations (
    id                TEXT PRIMARY KEY,
    room_id           TEXT NOT NULL,
    channel_id        TEXT,
    guest_id          TEXT,
    checkin_date      TEXT NOT NULL,
    checkout_date     TEXT NOT NULL,
    status            TEXT NOT NULL,       -- 'confirmed','checked_in','checked_out','cancelled','conflict'
    total_price_cents INTEGER NOT NULL DEFAULT 0,
    currency          TEXT NOT NULL DEFAULT 'TRY',
    adults            INTEGER NOT NULL DEFAULT 1,
    children          INTEGER NOT NULL DEFAULT 0,
    deleted_at        INTEGER
);
CREATE INDEX IF NOT EXISTS idx_res_room ON reservations (room_id, checkin_date, checkout_date);

CREATE TABLE IF NOT EXISTS payments (      -- append-only locally too
    id             TEXT PRIMARY KEY,
    reservation_id TEXT NOT NULL,
    amount_cents   INTEGER NOT NULL,
    currency       TEXT NOT NULL DEFAULT 'TRY',
    payment_type   TEXT NOT NULL DEFAULT 'cash',
    reference      TEXT,
    is_reversal    INTEGER NOT NULL DEFAULT 0,
    created_at     INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS folio_transactions (
    id               TEXT PRIMARY KEY,
    reservation_id   TEXT NOT NULL,
    category         TEXT NOT NULL,
    description      TEXT NOT NULL DEFAULT '',
    unit_price_cents INTEGER NOT NULL DEFAULT 0,
    quantity         INTEGER NOT NULL DEFAULT 1,
    total_cents      INTEGER NOT NULL DEFAULT 0,
    currency         TEXT NOT NULL DEFAULT 'TRY'
);
