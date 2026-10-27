-- 0004: sync + reporting + audit infrastructure
-- (gen_random_uuid() is built into PostgreSQL 13+)

-- ---------------------------------------------------------------------------
-- change_log: per-hotel monotonic change number (drives sync pull)
-- change_seq comes from hotels.next_change_seq incremented with row lock
-- inside the same write transaction (README §11.3, §13)
-- ---------------------------------------------------------------------------
CREATE TABLE change_log (
    hotel_id        BIGINT NOT NULL REFERENCES hotels(id),
    change_seq      BIGINT NOT NULL,
    operation_id    UUID NOT NULL,
    entity_type     TEXT NOT NULL CHECK (entity_type IN
                     ('room_type','room','room_status','reservation','guest','payment',
                      'folio_transaction','price_rule','user')),
    entity_id       UUID NOT NULL,
    operation       TEXT NOT NULL CHECK (operation IN ('insert','update','delete')),
    payload         JSONB NOT NULL DEFAULT '{}',
    actor_user_id   UUID,
    server_time     TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (hotel_id, change_seq)
);
CREATE INDEX idx_change_log_operation ON change_log (hotel_id, operation_id);

-- ---------------------------------------------------------------------------
-- sync_ops: processed operation IDs (idempotency of /sync/push)
-- ---------------------------------------------------------------------------
CREATE TABLE sync_ops (
    hotel_id      BIGINT NOT NULL REFERENCES hotels(id),
    operation_id  UUID NOT NULL,
    applied_at    TIMESTAMPTZ NOT NULL DEFAULT now(),
    status        TEXT NOT NULL CHECK (status IN ('applied','duplicate_ignored','conflict_resolved','rejected')),
    detail        TEXT,
    PRIMARY KEY (hotel_id, operation_id)
);

-- ---------------------------------------------------------------------------
-- daily_revenue: pre-aggregated per hotel/day/room/channel, updated in the SAME
-- transaction as every reservation or payment change (README §10)
-- ---------------------------------------------------------------------------
CREATE TABLE daily_revenue (
    hotel_id               BIGINT NOT NULL REFERENCES hotels(id),
    date                   DATE NOT NULL,
    room_id                UUID NOT NULL REFERENCES rooms(id),
    room_type_id           UUID NOT NULL REFERENCES room_types(id),
    channel_id             UUID NOT NULL REFERENCES channels(id),
    room_revenue_cents     BIGINT NOT NULL DEFAULT 0,
    extra_revenue_cents    BIGINT NOT NULL DEFAULT 0,
    payment_collected_cents BIGINT NOT NULL DEFAULT 0,
    occupied               BOOLEAN NOT NULL DEFAULT FALSE,
    updated_at             TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (hotel_id, date, room_id, channel_id)
);
CREATE INDEX idx_daily_revenue_hotel_date ON daily_revenue (hotel_id, date);

-- ---------------------------------------------------------------------------
-- audit_log: who / what / when / old / new for every change (README §7/§8)
-- ---------------------------------------------------------------------------
CREATE TABLE audit_log (
    id          UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hotel_id    BIGINT NOT NULL REFERENCES hotels(id),
    user_id     UUID REFERENCES users(id),
    entity_type TEXT NOT NULL,
    entity_id   UUID NOT NULL,
    action      TEXT NOT NULL,               -- created | updated | deleted | login | ...
    old_value   JSONB,
    new_value   JSONB,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
CREATE INDEX idx_audit_hotel_time ON audit_log (hotel_id, created_at DESC);
CREATE INDEX idx_audit_entity     ON audit_log (hotel_id, entity_type, entity_id);
