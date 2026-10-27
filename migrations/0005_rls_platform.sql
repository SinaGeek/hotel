-- 0005: row-level security (README §8) + platform tenant seed

-- ---------------------------------------------------------------------------
-- Application role: non-superuser, RLS enforced.
-- The server sets per-transaction:
--   SET LOCAL app.hotel_id = '<id>';        -- tenant scope of this request
--   SET LOCAL app.is_platform_admin = 'true';  -- only for hotel-0 admin sessions
-- ---------------------------------------------------------------------------
DO $$
BEGIN
  IF NOT EXISTS (SELECT 1 FROM pg_roles WHERE rolname = 'hms_app') THEN
    CREATE ROLE hms_app LOGIN;   -- password set by DBA / compose bootstrap
  END IF;
END
$$;

-- ---------------------------------------------------------------------------
-- Row-level security on every tenant table
-- ---------------------------------------------------------------------------
DO $$
DECLARE
  t TEXT;
BEGIN
  FOREACH t IN ARRAY ARRAY[
    'users','room_types','rooms','guests','channels','price_rules',
    'reservations','payments','folio_transactions',
    'hotels_permission_none_check_placeholder'  -- hotels uses its own policy below
  ]
  LOOP
    IF t = 'hotels_permission_none_check_placeholder' THEN CONTINUE; END IF;
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

-- hotels: a hotel user sees only its own tenant row; platform admin sees all
ALTER TABLE hotels ENABLE ROW LEVEL SECURITY;
CREATE POLICY tenant_hotels ON hotels
  USING (
    id = NULLIF(current_setting('app.hotel_id', true), '')::bigint
    OR NULLIF(current_setting('app.is_platform_admin', true), '') = 'true'
  )
  WITH CHECK (
    NULLIF(current_setting('app.is_platform_admin', true), '') = 'true'
    OR id = NULLIF(current_setting('app.hotel_id', true), '')::bigint
  );

-- change_log / sync_ops / daily_revenue / audit_log carry hotel_id too
DO $$
DECLARE
  t TEXT;
BEGIN
  FOREACH t IN ARRAY ARRAY['change_log','sync_ops','daily_revenue','audit_log','hotel_permission_overrides']
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

-- ---------------------------------------------------------------------------
-- Grants for hms_app
-- ---------------------------------------------------------------------------
GRANT USAGE ON SCHEMA public TO hms_app;
GRANT SELECT, INSERT, UPDATE ON
  hotels, users, roles, role_permissions, hotel_permission_overrides,
  room_types, rooms, guests, channels, price_rules,
  reservations, folio_transactions,
  change_log, sync_ops, daily_revenue, audit_log
TO hms_app;
GRANT SELECT, INSERT ON payments TO hms_app;          -- append-only: no UPDATE/DELETE

-- ---------------------------------------------------------------------------
-- Platform tenant (hotel 0). The admin user is created by the server bootstrap
-- using ADMIN_BOOTSTRAP_PASSWORD (README §15) — never a fixed default password.
-- ---------------------------------------------------------------------------
INSERT INTO hotels (id, name, is_active)
VALUES (0, 'Platform (all hotels)', TRUE)
ON CONFLICT (id) DO NOTHING;
