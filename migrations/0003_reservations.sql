-- 0003: reservations (with DB-level double-booking prevention), payments (append-only), folio

-- ---------------------------------------------------------------------------
-- Reservations
-- status: per README §9 — 'conflict' is for offline sync collisions
-- stay is a daterange snapshot of [checkin, checkout)
-- ---------------------------------------------------------------------------
CREATE TABLE reservations (
    id                UUID PRIMARY KEY,            -- client-generated UUIDv7 (offline creation)
    hotel_id          BIGINT NOT NULL REFERENCES hotels(id),
    room_id           UUID NOT NULL REFERENCES rooms(id),
    channel_id        UUID NOT NULL REFERENCES channels(id),
    guest_id          UUID REFERENCES guests(id),  -- primary guest
    stay              DATERANGE NOT NULL,
    checkin_date      DATE NOT NULL,               -- denormalized from stay
    checkout_date     DATE NOT NULL,
    status            TEXT NOT NULL DEFAULT 'confirmed'
                      CHECK (status IN ('confirmed','checked_in','checked_out','cancelled','conflict')),
    daily_rates       JSONB NOT NULL DEFAULT '[]', -- [{date, price_cents}]
    total_price_cents BIGINT NOT NULL CHECK (total_price_cents >= 0),
    currency          TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    adults            SMALLINT NOT NULL DEFAULT 1 CHECK (adults BETWEEN 1 AND 50),
    children          SMALLINT NOT NULL DEFAULT 0 CHECK (children BETWEEN 0 AND 50),
    notes             TEXT,
    voucher_number    TEXT,
    created_by        UUID REFERENCES users(id),
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at        TIMESTAMPTZ,
    CHECK (checkin_date < checkout_date),
    CHECK (stay = daterange(checkin_date, checkout_date, '[)'))
);

-- Double booking prevented BY THE DATABASE (README §9). Cancelled and conflict
-- reservations do not block the room.
ALTER TABLE reservations
  ADD CONSTRAINT no_double_booking
  EXCLUDE USING gist (room_id WITH =, stay WITH &&)
  WHERE (status NOT IN ('cancelled', 'conflict') AND deleted_at IS NULL);

-- §13 indexes: board / availability / list / keyset pagination
CREATE INDEX idx_res_board      ON reservations (hotel_id, room_id, stay);
CREATE INDEX idx_res_status     ON reservations (hotel_id, status);
CREATE INDEX idx_res_created    ON reservations (hotel_id, created_at DESC);
CREATE INDEX idx_res_stay_dates ON reservations (hotel_id, checkin_date, checkout_date) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Payments: APPEND-ONLY ledger. Corrections are new reversal entries.
-- Server role gets INSERT/SELECT only (see 0005).
-- ---------------------------------------------------------------------------
CREATE TABLE payments (
    id             UUID PRIMARY KEY,
    hotel_id       BIGINT NOT NULL REFERENCES hotels(id),
    reservation_id UUID NOT NULL REFERENCES reservations(id),
    amount_cents   BIGINT NOT NULL CHECK (amount_cents <> 0),  -- positive: collection; negative: refund
    currency       TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    payment_type   TEXT NOT NULL CHECK (payment_type IN ('cash','credit_card','bank_transfer','agency','refund')),
    reference      TEXT,
    is_reversal    BOOLEAN NOT NULL DEFAULT FALSE,
    created_by     UUID NOT NULL REFERENCES users(id),
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_payments_res    ON payments (hotel_id, reservation_id, created_at);
CREATE INDEX idx_payments_hotel  ON payments (hotel_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- Folio transactions (Room / Minibar / Spa / Restaurant / Extra ...)
-- ---------------------------------------------------------------------------
CREATE TABLE folio_transactions (
    id               UUID PRIMARY KEY,
    hotel_id         BIGINT NOT NULL REFERENCES hotels(id),
    reservation_id   UUID NOT NULL REFERENCES reservations(id),
    category         TEXT NOT NULL,
    description      TEXT NOT NULL,
    unit_price_cents BIGINT NOT NULL CHECK (unit_price_cents >= 0),
    quantity         INTEGER NOT NULL CHECK (quantity BETWEEN 1 AND 1000),
    total_cents      BIGINT NOT NULL CHECK (total_cents >= 0),
    currency         TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    created_at       TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at       TIMESTAMPTZ
);
CREATE INDEX idx_folio_res ON folio_transactions (hotel_id, reservation_id) WHERE deleted_at IS NULL;
