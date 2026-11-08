-- Canonical standalone desktop domain model. IDs are local UUIDs; records are
-- hotel-scoped, editable and retained in the encrypted SQLCipher database.
CREATE TABLE IF NOT EXISTS hotels (
    id INTEGER PRIMARY KEY,
    code TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    default_currency TEXT NOT NULL DEFAULT 'TRY',
    time_zone TEXT NOT NULL DEFAULT 'Europe/Istanbul',
    is_active INTEGER NOT NULL DEFAULT 1,
    address TEXT,
    contact_phone TEXT,
    contact_email TEXT,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS restaurants (
    id TEXT PRIMARY KEY,
    hotel_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    capacity INTEGER NOT NULL DEFAULT 0,
    is_active INTEGER NOT NULL DEFAULT 1,
    configuration_json TEXT NOT NULL DEFAULT '{}',
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS reservation_groups (
    id TEXT PRIMARY KEY,
    hotel_id INTEGER NOT NULL,
    name TEXT NOT NULL,
    notes TEXT,
    created_ms INTEGER NOT NULL,
    updated_ms INTEGER NOT NULL,
    deleted_ms INTEGER
);

CREATE TABLE IF NOT EXISTS guest_reservation_links (
    reservation_id TEXT NOT NULL,
    guest_id TEXT NOT NULL,
    link_type TEXT NOT NULL DEFAULT 'guest',
    PRIMARY KEY (reservation_id, guest_id)
);
