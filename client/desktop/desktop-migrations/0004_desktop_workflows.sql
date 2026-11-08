-- Desktop-only durable operational records. Stored in the encrypted SQLCipher DB.
CREATE TABLE IF NOT EXISTS desktop_kv (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    updated_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS shift_transfers (
    id TEXT PRIMARY KEY,
    hotel_id INTEGER NOT NULL,
    from_user TEXT NOT NULL,
    to_user TEXT NOT NULL,
    amount_cents INTEGER NOT NULL,
    currency TEXT NOT NULL DEFAULT 'TRY',
    notes TEXT,
    status TEXT NOT NULL CHECK (status IN ('pending', 'approved', 'rejected')),
    created_ms INTEGER NOT NULL,
    decided_ms INTEGER,
    decided_by TEXT
);
CREATE INDEX IF NOT EXISTS idx_shift_transfers_pending
    ON shift_transfers (hotel_id, to_user, status, created_ms DESC);
