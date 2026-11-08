-- 0001: platform tenancy, roles, users
-- Every table carries hotel_id (tenant). Hotel id 0 = platform tenant.
-- No hard deletes: rows get deleted_at tombstones.

CREATE EXTENSION IF NOT EXISTS btree_gist;

-- ---------------------------------------------------------------------------
-- Hotels (tenants)
-- ---------------------------------------------------------------------------
CREATE TABLE hotels (
    id              BIGINT PRIMARY KEY,
    name            TEXT NOT NULL,
    default_currency   TEXT NOT NULL DEFAULT 'TRY',
    time_zone          TEXT NOT NULL DEFAULT 'Europe/Istanbul',
    -- monotonically increasing change counter for the sync pull cursor; row-locked on write
    next_change_seq    BIGINT NOT NULL DEFAULT 1,
    is_active       BOOLEAN NOT NULL DEFAULT TRUE,
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at      TIMESTAMPTZ
);

-- ---------------------------------------------------------------------------
-- Roles: matrix is data, not code (README §6).
-- System roles are seeded by the server bootstrap; hotel-level overrides live in
-- hotel_permission_overrides (e.g. reception report access ON for one hotel).
-- ---------------------------------------------------------------------------
CREATE TABLE roles (
    name         TEXT PRIMARY KEY,           -- 'admin' | 'manager' | 'reception' | 'housekeeping'
    is_system    BOOLEAN NOT NULL DEFAULT FALSE,
    description  TEXT
);

CREATE TABLE role_permissions (
    role         TEXT NOT NULL REFERENCES roles(name) ON DELETE CASCADE,
    permission   TEXT NOT NULL,
    allowed      BOOLEAN NOT NULL DEFAULT FALSE,
    PRIMARY KEY (role, permission)
);

CREATE TABLE hotel_permission_overrides (
    hotel_id     BIGINT NOT NULL REFERENCES hotels(id),
    role         TEXT NOT NULL REFERENCES roles(name) ON DELETE CASCADE,
    permission   TEXT NOT NULL,
    allowed      BOOLEAN NOT NULL,
    PRIMARY KEY (hotel_id, role, permission)
);

-- ---------------------------------------------------------------------------
-- Users
-- ---------------------------------------------------------------------------
CREATE TABLE users (
    id                     UUID PRIMARY KEY,
    hotel_id               BIGINT NOT NULL REFERENCES hotels(id),
    username               TEXT NOT NULL,
    full_name              TEXT NOT NULL,
    email                  TEXT,
    role                   TEXT NOT NULL REFERENCES roles(name),
    password_hash          TEXT NOT NULL,        -- Argon2id
    is_active              BOOLEAN NOT NULL DEFAULT TRUE,
    force_password_change  BOOLEAN NOT NULL DEFAULT FALSE,
    created_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at             TIMESTAMPTZ,
    UNIQUE (hotel_id, username)
);
CREATE INDEX idx_users_hotel ON users (hotel_id) WHERE deleted_at IS NULL;
