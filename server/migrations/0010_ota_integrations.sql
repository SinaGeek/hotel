-- Migration 0010: OTA channel manager integration
-- Stores credentials, room mappings, and sync logs for Booking.com and Expedia

CREATE TABLE IF NOT EXISTS ota_configs (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    hotel_id BIGINT NOT NULL REFERENCES hotels(id) ON DELETE CASCADE,
    provider TEXT NOT NULL CHECK (provider IN ('booking_com', 'expedia')),
    enabled BOOLEAN NOT NULL DEFAULT false,
    
    -- Encrypted credentials (use pgcrypto or application-level encryption)
    credentials_encrypted BYTEA NOT NULL,
    
    -- API configuration
    hotel_code TEXT, -- Booking.com hotel ID or Expedia property ID
    base_url TEXT,
    
    -- Sync state
    last_sync_at TIMESTAMPTZ,
    last_success_at TIMESTAMPTZ,
    status TEXT NOT NULL DEFAULT 'disconnected' CHECK (status IN ('disconnected', 'connected', 'syncing', 'error')),
    error_message TEXT,
    
    -- Metadata
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    UNIQUE (hotel_id, provider)
);

CREATE INDEX idx_ota_configs_hotel ON ota_configs(hotel_id);
CREATE INDEX idx_ota_configs_enabled ON ota_configs(hotel_id, enabled) WHERE enabled = true;

-- Room type mappings: local room types ↔ OTA room IDs
CREATE TABLE IF NOT EXISTS ota_room_mappings (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ota_config_id UUID NOT NULL REFERENCES ota_configs(id) ON DELETE CASCADE,
    local_room_type_id UUID NOT NULL REFERENCES room_types(id) ON DELETE CASCADE,
    
    -- OTA-specific identifiers
    ota_room_id TEXT NOT NULL, -- Booking.com room_id or Expedia room type ID
    ota_rate_plan_id TEXT, -- e.g., "BAR" (Best Available Rate)
    
    -- Mapping metadata
    is_active BOOLEAN NOT NULL DEFAULT true,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    UNIQUE (ota_config_id, local_room_type_id),
    UNIQUE (ota_config_id, ota_room_id)
);

CREATE INDEX idx_ota_room_mappings_config ON ota_room_mappings(ota_config_id);

-- Sync operation log for auditing and debugging
CREATE TABLE IF NOT EXISTS ota_sync_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    ota_config_id UUID NOT NULL REFERENCES ota_configs(id) ON DELETE CASCADE,
    
    -- Operation details
    direction TEXT NOT NULL CHECK (direction IN ('pull', 'push')),
    entity_type TEXT NOT NULL CHECK (entity_type IN ('reservation', 'rate', 'availability', 'inventory')),
    operation TEXT NOT NULL CHECK (operation IN ('create', 'update', 'cancel', 'acknowledge')),
    
    -- Identifiers
    external_id TEXT, -- OTA reservation ID, rate plan ID, etc.
    local_id UUID, -- Local reservation ID (if mapped)
    
    -- Result
    status TEXT NOT NULL CHECK (status IN ('success', 'error', 'skipped')),
    error_message TEXT,
    response_data JSONB, -- Store OTA response for debugging
    
    -- Timing
    synced_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    
    -- RLS: tenant isolation
    hotel_id BIGINT NOT NULL
);

CREATE INDEX idx_ota_sync_log_config ON ota_sync_log(ota_config_id, synced_at DESC);
CREATE INDEX idx_ota_sync_log_external ON ota_sync_log(external_id) WHERE external_id IS NOT NULL;
CREATE INDEX idx_ota_sync_log_local ON ota_sync_log(local_id) WHERE local_id IS NOT NULL;
CREATE INDEX idx_ota_sync_log_hotel ON ota_sync_log(hotel_id, synced_at DESC);

-- RLS policies
ALTER TABLE ota_configs ENABLE ROW LEVEL SECURITY;
ALTER TABLE ota_room_mappings ENABLE ROW LEVEL SECURITY;
ALTER TABLE ota_sync_log ENABLE ROW LEVEL SECURITY;

-- ota_configs: users can only access their hotel's configs
CREATE POLICY ota_configs_tenant_isolation ON ota_configs
    USING (hotel_id = current_setting('app.current_hotel_id', true)::bigint);

-- ota_room_mappings: via ota_config relationship
CREATE POLICY ota_room_mappings_tenant_isolation ON ota_room_mappings
    USING (EXISTS (
        SELECT 1 FROM ota_configs oc
        WHERE oc.id = ota_room_mappings.ota_config_id
          AND oc.hotel_id = current_setting('app.current_hotel_id', true)::bigint
    ));

-- ota_sync_log: direct hotel_id check
CREATE POLICY ota_sync_log_tenant_isolation ON ota_sync_log
    USING (hotel_id = current_setting('app.current_hotel_id', true)::bigint);

-- Platform admin bypass (same pattern as other tables)
CREATE POLICY ota_configs_platform_admin ON ota_configs
    USING (current_setting('app.current_user_role', true) = 'platform_admin');

CREATE POLICY ota_room_mappings_platform_admin ON ota_room_mappings
    USING (current_setting('app.current_user_role', true) = 'platform_admin');

CREATE POLICY ota_sync_log_platform_admin ON ota_sync_log
    USING (current_setting('app.current_user_role', true) = 'platform_admin');

-- Permissions: managers and reception can view OTA configs
-- Only managers can edit
-- Platform admins can do everything
