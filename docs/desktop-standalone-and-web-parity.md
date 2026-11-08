# Desktop Standalone & Web App Parity Reference

Desktop is the active scope. Do not modify server/web-only behavior until requested. Keep the standalone `client/` folder buildable without `server/` and PostgreSQL.

## Standalone distribution
- Tauri embeds `client/web/dist` with the `custom-protocol` feature enabled by default.
- Local data uses SQLCipher at `%APPDATA%\com.hms.desktop\hms.sqlite`; passwords use Argon2 and Windows DPAPI.
- Local login, hotel administration, hotel domain records, and shift handover must work with no server process.

## Default local users

| Hotel | Username | Password | Role |
|---:|---|---|---|
| 0 | admin | 1234 | admin |
| 4442 | manager | 4321 | manager |
| 4442 | reception | 123 | reception |
| 4442 | sina | 123 | reception |
| 4442 | ac | 432 | accountant |
| 4442 | hk | 321 | housekeeping |

No forced password change on login. Password changes are user-initiated through Profile or manager-admin user management.

## Confirmed role access matrix

| Route | Menu | Admin | Manager | Reception | Housekeeper | Accountant | Multi-role |
|---|---|---|---|---|---|---|---|
| `/` | Dashboard | Visible | Visible | Visible | Hidden | Hidden | Union of roles |
| `/board` | Monthly Room | Visible | Visible | Visible | Hidden | Hidden | Union of roles |
| `/reservations` | Bookings | Visible | Visible | Visible | Hidden | Visible | Union of roles |
| `/rooms` | Rooms | Visible | Visible | Visible | Status only | Hidden | Union of roles |
| `/housekeeping` | Housekeeping | Visible | Full | Visible | Update only | Hidden | Union of roles |
| `/guests` | Guests | Visible | Visible | Visible | Hidden | Hidden | Union of roles |
| `/reports` | Reports | Visible | Visible | Visible | Hidden | Visible | Union of roles |
| `/users` | Staff | Visible | Visible | Hidden | Hidden | Hidden | Manager/Admin only |
| `/channels` | Channels | Visible | Visible | Hidden | Hidden | Hidden | Manager/Admin only |
| `/settings` | Settings | Visible | Visible | Visible | Visible | Visible | All desktop roles |
| `/admin` | Admin Panel | Visible | Hidden | Hidden | Hidden | Hidden | Admin only |
| `/import` | Bulk Import | Visible | Visible | Hidden | Hidden | Hidden | Admin/Manager only |

## Desktop UI/workflow contract
- Hamburger collapses/expands the persistent sidebar; collapsed mode shows icons only.
- Avatar dropdown contains Profile, Shift Change, Logout. Logout is not in the navigation sidebar.
- Profile supports user-initiated local password update.
- Shift change stores a pending transfer in encrypted SQLite. Incoming staff must explicitly approve/reject before proceeding. Decision is recipient-bound and persisted.
- Admin hotel list filters by code/name/status and shows up to 10 rows. Hotel name opens profile. Staff list and filters live inside the selected hotel profile.
- Hotel code and hotel name are editable. Desktop updates local hotel records and moves scoped records if the code/ID is changed. Do not silently overwrite an existing hotel on a code collision.
- Hotel profiles store address/contact, room inventory/config, parking, restaurants, and facilities locally in encrypted storage.
- Each restaurant will need its own specifications and cash-flow accounting model; this is intentionally deferred until designed.

## Room pricing & availability model

Fees are stored in layers; for any given night the most specific applicable layer wins:

1. **Type defaults** — `room_types.base_price_cents` (normal / midweek fee) and `room_types.weekend_price_cents` (weekend fee; blank/0 = same as mid fee). Managed from the room type card ("Save Fees").
2. **Weekend definition** — the hotel profile (`desktop_kv` key `hotel_profile:{id}`) stores `weekendDays` (0=Sunday … 6=Saturday; default Saturday+Sunday). The manager defines which days are weekend per hotel under Basic Information; the weekend fee applies to nights falling on those days.
3. **Calendar overrides** — `price_rules` rows per room type: `date_from`, `date_to`, `price_cents`. Same date in both fields = a single-night (day-by-day) price; a range acts as a batch change for that period. One room type can hold any number of overrides; overrides beat weekend and mid fees. Managed from the room type card's "Calendar Price Overrides" menu.

Availability is **computed, not stored**: a room is available for a night when it exists and is not soft-deleted, its cleanliness is not out-of-order, and no overlapping reservation in `confirmed`/`checked_in` status covers that night (see `reservations.room_id`, `checkin_date`, `checkout_date`). Prices are always stored; availability is always derived.

## Canonical desktop domain model
All records are hotel-scoped and editable by users whose permissions allow edits.

### Reservation
Hotel, room type, room number, check-in date, check-out date, guests, payments, other transactions, and reservation group. Multiple guests may be linked to one reservation. Payments and other transactions remain distinct ledgers.

### Other core entities
- Hotel: editable code/name, currency, time zone, contact/location, facilities.
- Room type: name, capacity, base price, amenities.
- Room: hotel, room type, room number, floor, cleanliness/out-of-service status.
- Guest: hotel, full name, contact/identity details, reservation associations.
- Restaurant: hotel, name/capacity now; per-restaurant specifications/cash-flow deferred.
- Channel, price rule, payment, folio/other transaction, reservation group: hotel-owned and independently editable where role permissions allow.

### Access enforcement expectation
UI hiding is not sufficient. Every desktop command that creates, edits, or deletes a domain record must validate the current user's hotel scope and required permission. Platform admin may manage any hotel; manager-level permissions remain hotel scoped; reception/housekeeping/accountant actions follow the confirmed matrix.

## Desktop local schema migrations
- `0004_desktop_workflows.sql`: encrypted desktop metadata and shift transfer records.
- `0005_desktop_domain_records.sql`: local hotels, restaurants, reservation groups, guest-reservation links.
- `0006_hotel_scoped_domain.sql`: hotel ownership on guests, room types, rooms, channels and reservations; migrations must be idempotent for existing installs.
- `0007_room_type_pricing.sql`: `room_types.weekend_price_cents`, `rooms.name`, and hotel scoping on `price_rules` (calendar price overrides).

## Future web/server parity (later, only when requested)
- Define server schemas/APIs for hotel/profile/facility/restaurant data.
- Design restaurant-by-restaurant specifications and cash-flow schema before implementation.
- Add role-checked CRUD APIs and audit trail for domain records and shift cash handovers.
- Port desktop profile, CRUD, and shift approval UX to web without Tauri dependencies.
- Align multi-role persistence and permission union with the access matrix above.
