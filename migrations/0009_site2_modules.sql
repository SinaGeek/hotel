-- 0009: site-2 deferred modules (G6) — e-invoice, stock/expenses, agencies,
-- currency exchange.
--
-- Scope note: these tables are the *data foundation* for the four site-2
-- modules that were explicitly deferred in `docs/feature-inventory.md` until a
-- product decision is made. They are tenant-scoped, RLS-protected, and carry
-- the same money conventions as the rest of the schema (integer minor units,
-- 3-letter currency) so they can be wired into the UI incrementally without a
-- migration rewrite.
--
-- The e-invoice table deliberately stores the *integration envelope* only
-- (GİB scenario/reference, UBL-TR payload, status) — the actual GİB submission
-- is an external service (entegratör) and is out of scope for this repo.

-- ---------------------------------------------------------------------------
-- Agencies (site-2 "CARI" accounts): travel agencies / corporate accounts with
-- a running balance. Replacement for the free-text `channels` name when a hotel
-- needs credit terms and statements.
-- ---------------------------------------------------------------------------
CREATE TABLE agencies (
    id             UUID PRIMARY KEY,
    hotel_id       BIGINT NOT NULL REFERENCES hotels(id),
    name           TEXT NOT NULL,
    tax_number     TEXT,
    tax_office     TEXT,
    contact_name   TEXT,
    phone          TEXT,
    email          TEXT,
    address        TEXT,
    -- Positive = the agency owes the hotel (receivable).
    balance_cents  BIGINT NOT NULL DEFAULT 0,
    currency       TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    credit_limit_cents BIGINT NOT NULL DEFAULT 0 CHECK (credit_limit_cents >= 0),
    is_active      BOOLEAN NOT NULL DEFAULT TRUE,
    notes          TEXT,
    created_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at     TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at     TIMESTAMPTZ,
    UNIQUE (hotel_id, name)
);
CREATE INDEX idx_agencies_hotel ON agencies (hotel_id) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- Currency exchange: per-hotel daily rates. The hotel's `default_currency` is
-- the base; a rate row says "1 unit of `currency` = rate_micro base units"
-- (micro = 1/1,000,000 so TRY→minor-unit rounding stays exact).
-- ---------------------------------------------------------------------------
CREATE TABLE currency_rates (
    id          UUID PRIMARY KEY,
    hotel_id    BIGINT NOT NULL REFERENCES hotels(id),
    currency    TEXT NOT NULL CHECK (char_length(currency) = 3),
    rate_micro  BIGINT NOT NULL CHECK (rate_micro > 0),
    valid_on    DATE NOT NULL,
    created_by  UUID REFERENCES users(id),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hotel_id, currency, valid_on)
);
CREATE INDEX idx_currency_rates_lookup ON currency_rates (hotel_id, currency, valid_on DESC);

-- ---------------------------------------------------------------------------
-- Stock items + movements (minibar, linen, amenities, cleaning supplies).
-- Append-only movements; on-hand is derived, never stored, so concurrent
-- consumption cannot silently corrupt the count (same rule as folio/payments).
-- ---------------------------------------------------------------------------
CREATE TABLE stock_items (
    id                UUID PRIMARY KEY,
    hotel_id          BIGINT NOT NULL REFERENCES hotels(id),
    sku               TEXT,
    name              TEXT NOT NULL,
    unit              TEXT NOT NULL DEFAULT 'adet',
    unit_cost_cents   BIGINT NOT NULL DEFAULT 0 CHECK (unit_cost_cents >= 0),
    min_level         INTEGER NOT NULL DEFAULT 0 CHECK (min_level >= 0),
    currency          TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    is_active         BOOLEAN NOT NULL DEFAULT TRUE,
    created_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at        TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at        TIMESTAMPTZ,
    UNIQUE (hotel_id, name)
);
CREATE INDEX idx_stock_items_hotel ON stock_items (hotel_id) WHERE deleted_at IS NULL;

CREATE TABLE stock_movements (
    id              UUID PRIMARY KEY,
    hotel_id        BIGINT NOT NULL REFERENCES hotels(id),
    stock_item_id   UUID NOT NULL REFERENCES stock_items(id),
    -- in | out | adjustment; `out` is consumption, `in` is purchase/restock
    movement_type   TEXT NOT NULL CHECK (movement_type IN ('in','out','adjustment')),
    -- signed quantity: +in, -out, ±adjustment. Never zero.
    quantity        INTEGER NOT NULL CHECK (quantity <> 0),
    unit_cost_cents BIGINT NOT NULL DEFAULT 0 CHECK (unit_cost_cents >= 0),
    room_id         UUID REFERENCES rooms(id),
    reservation_id  UUID REFERENCES reservations(id),
    note            TEXT,
    created_by      UUID REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_stock_movements_item ON stock_movements (hotel_id, stock_item_id, created_at DESC);

-- ---------------------------------------------------------------------------
-- Expenses (site-2 "gider"): hotel operating costs, with optional agency /
-- stock linkage and an e-invoice reference for matched purchases.
-- ---------------------------------------------------------------------------
CREATE TABLE expenses (
    id              UUID PRIMARY KEY,
    hotel_id        BIGINT NOT NULL REFERENCES hotels(id),
    category        TEXT NOT NULL,
    description     TEXT NOT NULL,
    amount_cents    BIGINT NOT NULL CHECK (amount_cents > 0),
    currency        TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    spent_on        DATE NOT NULL,
    agency_id       UUID REFERENCES agencies(id),
    payment_type    TEXT,
    invoice_number  TEXT,
    created_by      UUID REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    deleted_at      TIMESTAMPTZ
);
CREATE INDEX idx_expenses_hotel_date ON expenses (hotel_id, spent_on DESC) WHERE deleted_at IS NULL;

-- ---------------------------------------------------------------------------
-- E-invoices (GİB / UBL-TR envelope). One row per issued invoice document;
-- `reservation_id` links it to the folio it settles.
-- ---------------------------------------------------------------------------
CREATE TABLE e_invoices (
    id              UUID PRIMARY KEY,
    hotel_id        BIGINT NOT NULL REFERENCES hotels(id),
    reservation_id  UUID REFERENCES reservations(id),
    agency_id       UUID REFERENCES agencies(id),
    invoice_number  TEXT NOT NULL,
    -- GİB scenario: TEMELFATURA | TICARIFATURA | IHRACAT | ...
    scenario        TEXT NOT NULL DEFAULT 'TEMELFATURA',
    invoice_type    TEXT NOT NULL DEFAULT 'SATIS'
                    CHECK (invoice_type IN ('SATIS','IADE','TEVKIFAT','ISTISNA')),
    uuid_gib        TEXT,                  -- the UBL-TR document UUID
    tax_number      TEXT,
    -- draft | queued | sent | accepted | rejected | cancelled
    status          TEXT NOT NULL DEFAULT 'draft'
                    CHECK (status IN ('draft','queued','sent','accepted','rejected','cancelled')),
    net_cents       BIGINT NOT NULL DEFAULT 0 CHECK (net_cents >= 0),
    vat_cents       BIGINT NOT NULL DEFAULT 0 CHECK (vat_cents >= 0),
    total_cents     BIGINT NOT NULL DEFAULT 0 CHECK (total_cents >= 0),
    currency        TEXT NOT NULL DEFAULT 'TRY' CHECK (char_length(currency) = 3),
    ubL_xml         TEXT,
    error_message   TEXT,
    issued_at       TIMESTAMPTZ,
    created_by      UUID REFERENCES users(id),
    created_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at      TIMESTAMPTZ NOT NULL DEFAULT now(),
    UNIQUE (hotel_id, invoice_number)
);
CREATE INDEX idx_e_invoices_hotel_status ON e_invoices (hotel_id, status, created_at DESC);
CREATE INDEX idx_e_invoices_reservation ON e_invoices (hotel_id, reservation_id);

-- ---------------------------------------------------------------------------
-- Row-level security + grants (same policy shape as 0005)
-- ---------------------------------------------------------------------------
DO $$
DECLARE
  t TEXT;
BEGIN
  FOREACH t IN ARRAY ARRAY['agencies','currency_rates','stock_items','stock_movements','expenses','e_invoices']
  LOOP
    EXECUTE format('ALTER TABLE %I ENABLE ROW LEVEL SECURITY', t);
    EXECUTE format($p$
      CREATE POLICY tenant_%1$s ON %1$s
        USING (
          hotel_id = NULLIF(current_setting('app.hotel_id', true), '')::bigint
          OR NULLIF(current_setting('app.is_platform_admin', true), '') = 'true'
        )
        WITH CHECK (
          hotel_id = NULLIF(current_setting('app.hotel_id', true), '')::bigint
          OR NULLIF(current_setting('app.is_platform_admin', true), '') = 'true'
        )
    $p$, t);
  END LOOP;
END
$$;

GRANT SELECT, INSERT, UPDATE ON agencies, currency_rates, stock_items, expenses, e_invoices TO hms_app;
-- stock movements are append-only: no UPDATE/DELETE
GRANT SELECT, INSERT ON stock_movements TO hms_app;

-- ---------------------------------------------------------------------------
-- Permissions (G6). Defaults: managers own all four, reception can issue
-- invoices. Housekeeping receives no stock-management permission until a more
-- granular consume-only permission is product-approved.
-- ---------------------------------------------------------------------------
INSERT INTO role_permissions (role, permission, allowed) VALUES
  ('admin','invoice_manage', TRUE),('admin','stock_manage', TRUE),
  ('admin','expense_manage', TRUE),('admin','agency_manage', TRUE),
  ('manager','invoice_manage', TRUE),('manager','stock_manage', TRUE),
  ('manager','expense_manage', TRUE),('manager','agency_manage', TRUE),
  ('reception','invoice_manage', TRUE)
ON CONFLICT (role, permission) DO NOTHING;
