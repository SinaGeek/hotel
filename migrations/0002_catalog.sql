-- 0002: hotel catalog — room types, rooms, guests, channels, price rules

-- ---------------------------------------------------------------------------
-- Room types
-- ---------------------------------------------------------------------------
CREATE TABLE room_types (
    id               UUID PRIMARY KEY,           -- client-generated UUIDv7
    hotel_id         BIGINT NOT NULL REFERENCES hotels(id),
    name             TEXT NOT NULL,
    capacity         INTEGER NOT NULL CHECK (capacity BETWEEN 1 AND 100),
    base_price_cents BIGINT NOT NULL CHECK (base_price_cents >= 0),
    amenities        TEXT[] NOT NULL DEFAULT '{}',
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at       TIMESTAMPTZ,
    UNIQUE (hotel_id, name)
);

-- ---------------------------------------------------------------------------
-- Rooms
-- cleanliness per README §6: dirty | cleaning | clean | inspected (+ out_of_order)
-- ---------------------------------------------------------------------------
CREATE TABLE rooms (
    id           UUID PRIMARY KEY,
    hotel_id     BIGINT NOT NULL REFERENCES hotels(id),
    room_type_id UUID NOT NULL REFERENCES room_types(id),
    room_number  TEXT NOT NULL,
    floor        INTEGER NOT NULL DEFAULT 0,
    cleanliness  TEXT NOT NULL DEFAULT 'clean'
                 CHECK (cleanliness IN ('dirty','cleaning','clean','inspected','out_of_order')),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at   TIMESTAMPTZ,
    UNIQUE (hotel_id, room_number)
);
CREATE INDEX idx_rooms_hotel ON rooms (hotel_id) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Guests (KVKK/GDPR personal data — identity fields stored encrypted)
-- ---------------------------------------------------------------------------
CREATE TABLE guests (
    id                UUID PRIMARY KEY,
    hotel_id          BIGINT NOT NULL REFERENCES hotels(id),
    reservation_id    UUID,                             -- set when the reservation exists
    full_name         TEXT NOT NULL,
    id_number_encrypted   TEXT,
    nationality       TEXT,
    phone_encrypted       TEXT,
    email_encrypted       TEXT,
    license_plate     TEXT,
    birth_date        DATE,
    is_primary        BOOLEAN NOT NULL DEFAULT FALSE,
    kbs_reported      BOOLEAN NOT NULL DEFAULT FALSE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at        TIMESTAMPTZ
);
CREATE INDEX idx_guests_hotel ON guests (hotel_id);
CREATE INDEX idx_guests_name  ON guests (hotel_id, lower(full_name)) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Channels: per-hotel configurable (README §4/§7 — not an enum)
-- ---------------------------------------------------------------------------
CREATE TABLE channels (
    id         UUID PRIMARY KEY,
    hotel_id   BIGINT NOT NULL REFERENCES hotels(id),
    kind       TEXT NOT NULL CHECK (kind IN ('direct','walk_in','booking_com','expedia','agency','other')),
    name       TEXT NOT NULL,
    is_active  BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at TIMESTAMPTZ,
    UNIQUE (hotel_id, name)
);

-- ---------------------------------------------------------------------------
-- Price rules (seasonal/special pricing per room type)
-- ---------------------------------------------------------------------------
CREATE TABLE price_rules (
    id            UUID PRIMARY KEY,
    hotel_id      BIGINT NOT NULL REFERENCES hotels(id),
    room_type_id  UUID NOT NULL REFERENCES room_types(id),
    date_from     DATE NOT NULL,
    date_to       DATE NOT NULL,
    price_cents   BIGINT NOT NULL CHECK (price_cents >= 0),
    currency      TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    created_by    UUID REFERENCES users(id),
    created_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at    TIMESTAMPTZ,
    CHECK (date_from <= date_to)
);
CREATE INDEX idx_price_rules_lookup ON price_rules (hotel_id, room_type_id, date_from, date_to)
    WHERE deleted_at IS NULL;
