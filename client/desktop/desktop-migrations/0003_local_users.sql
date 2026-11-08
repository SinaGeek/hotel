-- Bulk-imported dataset provenance + always-available local users (README §11):
-- local_users lets a standalone hotel sign in offline WITHOUT first having an
-- online login (Settings ▸ Bulk import ▸ users). Sensitive material (argon2
-- PHC string) is stored DPAPI-protected per the C3 policy.
CREATE TABLE IF NOT EXISTS local_users (
    user_id       TEXT PRIMARY KEY,
    username      TEXT NOT NULL UNIQUE,
    full_name     TEXT,
    role          TEXT NOT NULL,
    hotel_id      INTEGER NOT NULL,
    password_phc  TEXT NOT NULL,            -- hex(DPAPI(argon2 PHC))
    created_ms    INTEGER NOT NULL
);

-- Bulk imports must be idempotent vs sync-pulled server rows: external_ref
-- carries the source-of-data natural key (booking no / guest id).
ALTER TABLE reservations ADD COLUMN external_ref TEXT;
ALTER TABLE guests       ADD COLUMN external_ref TEXT;
CREATE INDEX IF NOT EXISTS idx_res_external_ref ON reservations (external_ref);
CREATE INDEX IF NOT EXISTS idx_gst_external_ref ON guests (external_ref);
