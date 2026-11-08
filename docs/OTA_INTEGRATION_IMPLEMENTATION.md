# OTA Channel Manager Integration Implementation

**Date:** November 5, 2026  
**Status:** UI Complete, Backend Pending Partnership Approval

---

## Overview

Implemented a comprehensive OTA (Online Travel Agency) integration management panel for Booking.com and Expedia Group within the HMS Channels section. This interface allows hotel managers to configure credentials, test connections, and manually sync reservations, rates, and availability.

---

## What Was Built

### 1. OTA Integrations Management Page

**Location:** `apps/web/src/pages/OTAIntegrations.tsx`

**Features:**
- **Dual OTA Support:**
  - Booking.com (OTA XML 2003B + B.XML protocols)
  - Expedia Group (GraphQL + REST APIs)
  
- **Per-OTA Configuration:**
  - Enable/disable toggle
  - Credential management (username/password, API keys)
  - Hotel/Property ID mapping
  - Connection status indicators (disconnected/connected/syncing/error)
  - Last sync timestamp
  
- **Actions:**
  - Test Connection (validates credentials)
  - Manual Sync Now (pulls reservations, pushes rates/availability)
  - Save Configuration
  
- **Integration Details Panel:**
  - Base URLs for each OTA
  - Protocol specifications
  - Endpoint documentation
  - Polling intervals
  - Fallback mechanisms

- **Production Onboarding Checklist:**
  - Partnership application steps
  - Agreement requirements (NDA, license, PCI compliance)
  - Certification process
  - Go-live requirements

### 2. Enhanced Channels Page

**Location:** `apps/web/src/pages/Channels.tsx`

Added prominent call-to-action banner:
- Visual gradient design (blue/indigo)
- Direct navigation to OTA configuration
- Clear value proposition

### 3. Routing

**Location:** `apps/web/src/App.tsx`

Added route: `/ota-integrations` → `OTAIntegrations` component

---

## Technical Details

### Booking.com Integration Specs

**Protocol:** OTA XML 2003B + B.XML  
**Base URLs:**
- `https://supply-xml.booking.com` — Non-PCI (rates/availability)
- `https://secure-supply-xml.booking.com` — PCI (reservations)

**Key Endpoints:**
- `GET /hotels/ota/OTA_HotelResNotif` — Retrieve new reservations
- `POST /hotels/ota/OTA_HotelResNotif` — Acknowledge new reservations
- `GET /hotels/ota/OTA_HotelResModifyNotif` — Retrieve modified/cancelled reservations
- `POST /hotels/ota/OTA_HotelResModifyNotif` — Acknowledge modifications/cancellations
- `POST /hotels/ota/OTA_HotelAvailNotif` — Push availability updates
- `POST /hotels/ota/OTA_HotelRateAmountNotif` — Push rate updates

**Authentication:** Machine Account (username/password from Provider Portal)

**Polling Strategy:**
- New reservations: Every 5 minutes
- Modifications: Every 5 minutes (lower priority)
- Fallback: Email if not acknowledged within 30 minutes

**Rate Limits:**
- All endpoints: 10,000 calls/minute
- `/xml/reservationssummary`: 700 calls/minute
- Content/product endpoints: 75 calls/minute

### Expedia Group Integration Specs

**Protocol:** GraphQL + REST  
**Base URL:** `https://api.expediagroup.com/supply/lodging`

**Key Operations:**
- **GraphQL Queries:**
  - `reservationRetrieval` — Pull reservations on demand
  - `propertyStatus` — Get property details
  
- **GraphQL Mutations:**
  - `updateAvailability` — Push availability changes
  - `updateRates` — Push rate changes
  - `confirmReservation` — Acknowledge booking
  - `updateReservation` — Modify existing booking
  
**Authentication:** API Key + Secret (from Partner Central)

**Polling Strategy:**
- Reservation updates: Every 10 minutes
- Real-time webhooks available (requires webhook endpoint)

**Requirements:**
- PCI DSS compliance (mandatory for production)
- Connectivity Partner Program membership

---

## Data Flow

### Booking.com Reservation Sync

```
1. HMS polls GET /ota/OTA_HotelResNotif every 5 minutes
2. Booking.com returns XML with new reservations
3. HMS parses OTA XML → creates local reservation
4. HMS acknowledges via POST /ota/OTA_HotelResNotif
5. Booking.com removes from queue
6. If not acknowledged within 30 min → email fallback to hotel
```

### Expedia Reservation Sync

```
1. HMS queries reservationRetrieval GraphQL endpoint every 10 minutes
2. Expedia returns JSON with reservation updates
3. HMS creates/updates local reservation
4. HMS calls confirmReservation mutation
5. Expedia marks as processed
```

### Rate/Availability Push

```
1. HMS detects rate/availability change (user edit or sync from another channel)
2. HMS pushes to Booking.com via OTA_HotelRateAmountNotif (XML)
3. HMS pushes to Expedia via updateRates mutation (GraphQL)
4. Both OTAs update their inventory
5. Rate parity maintained across channels
```

---

## Room Mapping Strategy

Each OTA requires mapping between local HMS room types and their channel-specific room IDs:

**Example Mapping:**
```typescript
{
  localRoomTypeId: "uuid-123-standard-double",
  bookingComRoomId: "12345678",
  expediaRoomTypeId: "987654321",
  ratePlanId: "BAR" // Best Available Rate
}
```

**Unmapped rooms:**
- Fall back to email delivery (Booking.com)
- Not bookable on that channel (Expedia)

---

## Current Implementation Status

### ✅ Completed
- UI for credential management
- Connection status tracking
- Manual sync triggers
- Integration details reference
- Onboarding checklist documentation
- Navigation from Channels page

### ⚠️ Demo Mode
- Test Connection: Simulated (2-second delay, always succeeds)
- Manual Sync: Simulated (3-second delay, success notification)
- Credentials: Stored in localStorage (not secure for production)

### ❌ Not Implemented (Requires Partnership)
- **Live API calls** — Cannot make real calls without official partner credentials
- **Automatic polling** — Background workers not implemented
- **Webhook receivers** — No endpoints to receive push notifications
- **Room mapping UI** — Advanced mapping interface pending
- **Backend storage** — `/api/v1/ota-configs` endpoint not created
- **PCI compliance** — Secure credential storage and transmission
- **Two-way sync** — Pull reservations + push availability/rates
- **Conflict resolution** — Handle overbookings across channels

---

## Production Deployment Requirements

### 1. Partnership Approval

**Booking.com:**
1. Apply at https://portal.connectivity.booking.com/
2. Sign NDA and Connectivity License Agreement
3. Receive Machine Account credentials for sandbox
4. Complete certification tests (30+ scenarios)
5. Pass technical review
6. Activate production credentials

**Expedia:**
1. Contact Partner Central connectivity team
2. Review and accept license agreement
3. Complete PCI DSS certification (if handling payment data)
4. Receive sandbox API credentials
5. Develop and test integration
6. Pass certification (availability accuracy, booking flow, error handling)
7. Soft launch with test properties
8. Full production rollout

### 2. Backend Implementation

**Required Endpoints:**

```typescript
// Store OTA configurations
POST   /api/v1/ota-configs
GET    /api/v1/ota-configs
PUT    /api/v1/ota-configs/:id
DELETE /api/v1/ota-configs/:id

// Test connection
POST   /api/v1/ota-configs/:id/test

// Manual sync trigger
POST   /api/v1/ota-configs/:id/sync

// Room mappings
GET    /api/v1/ota-configs/:id/room-mappings
PUT    /api/v1/ota-configs/:id/room-mappings
```

**Database Schema:**

```sql
CREATE TABLE ota_configs (
    id UUID PRIMARY KEY,
    hotel_id BIGINT NOT NULL REFERENCES hotels(id),
    ota_provider TEXT NOT NULL, -- 'booking_com' | 'expedia'
    enabled BOOLEAN DEFAULT false,
    credentials JSONB NOT NULL, -- encrypted
    last_sync_at TIMESTAMPTZ,
    status TEXT NOT NULL, -- 'disconnected' | 'connected' | 'error'
    error_message TEXT,
    created_at TIMESTAMPTZ DEFAULT now(),
    UNIQUE (hotel_id, ota_provider)
);

CREATE TABLE ota_room_mappings (
    id UUID PRIMARY KEY,
    ota_config_id UUID NOT NULL REFERENCES ota_configs(id),
    local_room_type_id UUID NOT NULL REFERENCES room_types(id),
    ota_room_id TEXT NOT NULL,
    ota_rate_plan_id TEXT,
    UNIQUE (ota_config_id, local_room_type_id)
);

CREATE TABLE ota_sync_log (
    id UUID PRIMARY KEY,
    ota_config_id UUID NOT NULL REFERENCES ota_configs(id),
    direction TEXT NOT NULL, -- 'pull' | 'push'
    entity_type TEXT NOT NULL, -- 'reservation' | 'rate' | 'availability'
    operation TEXT NOT NULL, -- 'create' | 'update' | 'cancel'
    external_id TEXT,
    local_id UUID,
    status TEXT NOT NULL, -- 'success' | 'error'
    error_message TEXT,
    synced_at TIMESTAMPTZ DEFAULT now()
);
```

### 3. Background Workers

**Reservation Puller (Booking.com):**
```rust
// Runs every 5 minutes
async fn pull_booking_reservations(config: &OTAConfig) {
    let client = BookingComClient::new(&config.credentials);
    loop {
        // GET /ota/OTA_HotelResNotif?limit=50
        let reservations = client.pull_new_reservations().await?;
        for res in reservations {
            // Create local reservation
            create_reservation_from_ota(res).await?;
            // Acknowledge
            client.acknowledge_reservation(res.id).await?;
        }
        tokio::time::sleep(Duration::from_secs(300)).await;
    }
}
```

**Reservation Puller (Expedia):**
```rust
// Runs every 10 minutes
async fn pull_expedia_reservations(config: &OTAConfig) {
    let client = ExpediaClient::new(&config.credentials);
    loop {
        let query = "query { reservationRetrieval(...) }";
        let reservations = client.graphql_query(query).await?;
        for res in reservations {
            create_or_update_reservation(res).await?;
            client.confirm_reservation(res.id).await?;
        }
        tokio::time::sleep(Duration::from_secs(600)).await;
    }
}
```

**Rate/Availability Pusher:**
```rust
// Triggered on local changes + scheduled full refresh every 4 hours
async fn push_rates_availability(hotel_id: i64) {
    let configs = load_ota_configs(hotel_id).await?;
    let room_types = load_room_types(hotel_id).await?;
    let prices = load_price_rules(hotel_id).await?;
    
    for config in configs {
        if config.enabled {
            match config.provider {
                "booking_com" => push_to_booking(room_types, prices).await?,
                "expedia" => push_to_expedia(room_types, prices).await?,
            }
        }
    }
}
```

### 4. Security

- **Credential Encryption:** Encrypt OTA credentials at rest (DPAPI/vault)
- **TLS:** All API calls over HTTPS
- **PCI Compliance:** No cardholder data storage (OTAs handle payments)
- **Rate Limiting:** Respect OTA rate limits, exponential backoff
- **Audit Logging:** Record all sync operations in `ota_sync_log`

---

## Testing Strategy

### Unit Tests
- OTA XML parsing (Booking.com)
- GraphQL query/mutation building (Expedia)
- Room mapping resolution
- Error handling (network failures, auth errors)

### Integration Tests
- **Sandbox environments:**
  - Booking.com: Test hotel IDs provided by Connectivity Support
  - Expedia: Test property IDs from Partner Central
  
- **Scenarios:**
  1. Pull new reservation → local reservation created
  2. Pull modification → local reservation updated
  3. Pull cancellation → local reservation cancelled
  4. Push rate change → OTA inventory updated
  5. Push availability close → OTA shows sold out
  6. Network timeout → retry with exponential backoff
  7. Invalid credentials → error status, don't crash
  8. Unmapped room → fallback to email (Booking.com)

### Certification Tests (Required by OTAs)
- Rate parity across channels
- Overbooking prevention
- Timely acknowledgements (<30 min for Booking.com)
- Error recovery
- Load handling (100+ reservations/day)

---

## Cost Considerations

**Booking.com:**
- Commission: ~15-20% per booking
- No API usage fees
- Connectivity Partner benefits available

**Expedia:**
- Commission: ~15-25% per booking (varies by brand)
- No API usage fees for certified partners
- Connectivity Partner Program rewards

---

## Documentation References

### Booking.com
- Developer Portal: https://developers.booking.com/connectivity/docs
- Reservations API: https://developers.booking.com/connectivity/docs/reservations-api
- Provider Portal: https://connect.booking.com
- Support: connectivity-support@booking.com

### Expedia Group
- Developer Portal: https://developers.expediagroup.com/supply/lodging
- GraphQL Explorer: https://developers.expediagroup.com/supply/lodging/graphql
- Partner Central: https://www.expediapartnercentral.com
- Contact: Via Partner Central help desk

---

## Next Steps

1. **Apply for partnerships:** Submit applications to both OTAs
2. **Implement backend:** Create database schema + API endpoints
3. **Build background workers:** Reservation pullers + rate pushers
4. **Develop room mapping UI:** Advanced interface for mapping local ↔ OTA room types
5. **Integration testing:** Use sandbox credentials to test full flow
6. **Certification:** Pass OTA validation tests
7. **Soft launch:** Enable for 1-2 test properties
8. **Production rollout:** Activate for all interested hotels

---

## Summary

The OTA integration UI is complete and ready for demo. Production activation requires:
- Official OTA partnerships (3-6 months onboarding)
- Backend implementation (~2 weeks development)
- Certification testing (~1 week)
- PCI compliance audit (Expedia only)

**Estimated time to production:** 4-7 months (mostly partnership approval)

**Development effort:** ~40 hours (backend + workers + tests)

This implementation provides a solid foundation for two-way OTA synchronization, maintaining rate parity and preventing overbookings across all channels.
