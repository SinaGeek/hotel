# Reference UI inventory: app.hmsflow.com (HMS Flow)

Source: live logged-in captures on 2026-10-28 (hotel 4442 "Kuzen Otel") using REA connect-mode
browser scenarios (CDP, screenshots + DOM + accessibility + network events). Raw capture
evidence stays in `%TEMP%\opencode\cap_*.out`; this document is the committed, sanitized summary.
Companion: `docs/apis.md` (62 unique backend paths, per page).

## Design language

- **Frame:** fixed dark sidebar (~244px, near-black navy `#161629`-range) with app logo/name on
  top; light gray content area; white rounded cards (`border-radius` ~12px, soft shadow).
- **Top header** inside the content area: hotel name (bold) + "Hotel Code: 4442" beneath;
  right side icon row: notifications bell, language switcher, dark-theme toggle, (avatars) user
  menu. Header is white, full-width of the content column.
- **Sidebar nav (order):** Dashboard, Monthly Room, Guests, Bookings (expander: List,
  Group Booking, Channel Bookings), Facility (expander: Facilities, Accommodation Type,
  Room Type, Room > Rooms, Update Status, Housekeeping), Transactions (expander: Open Sale,
  Expense, Stock, List), Periodic Prices (Calendar, Overview, Manual Availability, Batch Updates,
  Advanced Updates), Reports, Invoices (Purchase Invoice, Sales Invoice), Agencies & Accounts
  (Agency, Account), Tools (Settings, Exchange, Staff, Channels, Applications); Help pinned at
  the bottom with a collapse control. Active item = filled navy pill with white text + white
  left accent bar; expanders slide open in place.
- **Buttons:** violet/indigo primary (`#6366f1`-range, white text, rounded ~8px); destructive
  and secondary use outlined/light pill styles. A round violet FAB with a plus icon floats
  bottom-right on nearly every page ("new record" shortcut).
- **Data tables:** white card, uppercase gray column headers, generous row height, row
  separator lines; status shown as soft pills (Active/Clean/In-House = light green,
  Upcoming = light gray, Inactive/Cancelled = light gray/red tones).
- **List footer pattern:** "1-2 / 2" range text + "Per page 20" select + first/prev/next/last
  chevron buttons. All list pages use limit=page paged APIs with `order=-id`.
- **Filters row:** search box + dropdown filters styled as labeled pills under the page title;
  date-range pickers show a calendar icon. Filters pass query params straight to the API.
- **Empty state:** centered "No records found." in the table card.
- **Error boundary:** full-screen "Something went wrong!" with a "Show Error" button
  (caught on /channel-bookings during capture - reference bug, not ours).
- **Reports print/export:** every report page has a Download dropdown (Excel/PDF export
  affordance) and a Table/Chart tab pair.

## Page-by-page notes (with observed endpoints)

All endpoints under `app.hmsflow.com`; full paths + per-page details in `docs/apis.md`.

- **Dashboard `/`**: suggestion banner, "Search room or guest", status chip row with counts
  (Departing Today/Tomorrow, Long Term, Will Check-in, Out of Service, Dirty Room), room-card
  grid grouped as white cards (room number header colored by state, guest name OR room-type
  name, checkout datetime, "Make Reservation" button on vacant rooms), stat cards (Occupied
  Rooms, Guests Today, Vacant Rooms, Total Rooms) + more sections below. Endpoints: `/apiv2/home`,
  `/apiv2/home/warning`, `/apiv2/home/ckeckin/{hotelId}/list`, `/apiv2/home/ckeckout/{hotelId}/list`.
- **Monthly Room `/monthly-room?month&range`**: Calendar/List toggle, range toggle
  (7 days / 15 days / Month) + date pager (prev/next, Today disabled when on current period),
  grid: sticky left columns (period label "Oct 28 - Nov 11, 2026 / 17 rooms", hotel row,
  room rows started by "room type section headers" carrying the per-day prices), day columns
  with weekday+date; booking bars colored green (direct Reservation) vs yellow/gold (channel
  bookings) with guest name, "n nights - Source" subtitle; bottom legend (Reservation, Pending,
  Blocked, Out of Service) + Clean/Dirty chip + "Drag across empty days to add a record" hint;
  violet FAB. Endpoints: `/apiv2/monthly-room/booking|room|booking/unassigned/count?start&end`.
- **Bookings `/bookings`**: title + "86 records" + subtitle describing the active view; tabs
  (Active, Upcoming, Archive, Cancelled, All); filter row (search, Check-in Date, Check-out
  Date, order "Newest", Filters); table columns (#, Guest, Room + type, Check-in datetime,
  Check-out datetime, Status pill, Balance, Total); "+ Add Booking" button. Endpoint:
  `/apiv2/booking?limit&page&order&filterBookingType[]&status&checkoutIsNullOrZero`.
- **Guests `/guests`**: "2418 records", Download dropdown, Birthday + Garage quick views,
  "+ Add Guest"; filters (search, All, Category, Country); columns (#, Full name, ID no,
  Category, Balance). Endpoints: `/apiv2/guest`, `/apiv2/country`, `/apiv2/category?bolum=1`.
- **Facility / Rooms `/facility/rooms`**: columns (Room, Room Type, Building, Status pill) +
  "Active" filter dropdown; linked submenu Rooms > Rooms/Update Status/Housekeeping.
- **Facility / Room Type `/facility/room-types`**: columns (Name, Base Type, Capacity, Adults).
  Endpoint: `/apiv2/roomtypehotel`.
- **Facility / Accommodation Type**: rate/accommodation list + channel per-hotel mapping
  (`/apiv2/rate/type/active/list`, `/apiv2/channel`).
- **Facility / Housekeeping `/facility/housekeeping`**: building/status filters + Download;
  grouped by building; columns (Room, Check-in, Check-out, Status). Endpoint:
  `/apiv2/room/housekeeping`.
- **Facility / Update Status `/facility/update-status`**: Rooms table with inline per-row
  cleanliness dropdown columns (room mutable straight in the grid). Endpoint: `/apiv2/room`.
- **Transactions**: Open Sale (public-sale bills), Expense (product text list, amount, date,
  status; Payment Type + date filters), Stock (stock items + categories), List. Endpoints:
  `/apiv2/expense`, `/apiv2/stock`, `/apiv2/stock/category`, `/apiv2/sale/public/bill`,
  `/apiv2/paymenttypehotel`.
- **Periodic Prices / Calendar**: channel + start-date + day-count controls
  (default 14; "Hms Programi" = the hotel's own channel) and an availability grid
  (room-type rows x date columns, red numbers = 0) above a per-room-type rates grid
  (accommodation rows + Min.Stay row; editable cells: "Click a rate cell... press Enter to
  save"). Endpoints: `/apiv2/periodic/bring/{date,availability,accommodation-price,restriction}`.
- **Periodic / Overview / Manual Availability / Batch Updates / Advanced Updates**: channel
  selector -> room-type+channel selectors -> bulk update call
  (`/apiv2/periodic/advanced/update/room?start&end&channels[]&rooms[]&days[]`).
- **Reports `/reports`**: card-grid hub grouped by category chips: DAILY (Main Courante,
  Occupancy Forecast, In-House, Meal List, Nationality Breakdown, Channel Analysis,
  Collection Summary, Expense Summary, Free Rooms), BOOKINGS (Expected Arrivals,
  Completed Arrivals, Expected Departures, Early Departures, Date Changes, Deleted Bookings,
  Bookings Without a Room, Transfers), ROOMS & RATES (Room Changes, Room Occupancy Rates,
  Housekeeping, Period Rates, Rate Changes), GUESTS (Daily Guest List), FINANCE (Collection
  Report, Expense Report, Extras Report, ...). Captured sub-pages:
  - `/reports/inhouse`: KPI cards (Total Bookings 12, Guests 18 + 0, Room Rate sum, Today's
    Balance sum) + table (Guest, Room, Room Type as TR label, Accommodation, Check-in/out,
    Guests, Today's Balance); Date picker + Download; Table/Chart tabs.
  - `/reports/forecast`: per-date rows (Today's Arrivals, Occupied, Vacant, Guests, Occupancy
    % as "%76" TR format).
  - `/reports/nationality`, `/reports/maincourante`, `/reports/meal-list`: date + hour range
    filters (captured endpoints in apis.md).
- **Invoices**: Sales/Purchase lists (#, Customer, Invoice No, Date, Status) with "Next
  invoice no: PREFIX00000003" hint and segmented invoice flows. Endpoints: `/apiv2/invoice?bolum=1|2`,
  `/apiv2/invoice/lastinvoice`.
- **Agencies & Accounts**: Agency (isCari=0) and Account (isCari=1) lists over the same
  `/apiv2/current?isCari=` entity (company name, category, tax id/office, balance).
- **Tools**: Settings (card hub: Theme Area, Mail Server, SMS, Social Media, Appearance),
  Exchange (currency-exchange operations), Staff (users with `ROLE_MODERATOR` style params),
  Channels (channel-manager accounts with logo, hotel code, status; search/sort/status
  filters + "In periodic pricing" quick filter), Applications (app store per hotel,
  category + product detail calls). Endpoints: `/apiv2/channel`, `/apiv2/staff?rule=`,
  `/apiv2/currency-exchange`, `/apiv2/application*`.
- **Notifications `/notifications`**, **Help `/help`** (third-party chat widget - out of scope).

## Comparative gap vs our `apps/web`

Historical captures above were taken 2026-10-28. Follow-up implementation through 2026-10-31:

- **Implemented:** dark icon/label sidebar + hotel header/theme controls; Monthly Room calendar/list, range pager and drag-to-create; Dashboard suggestions/chips/room cards/stats; Bookings list tabs and Guests list; Facility/Rooms/Housekeeping/inline cleanliness; Channels/Staff; Reports hub/analysis pages with daily/monthly charts, KPIs, CSV/print and Accrued-vs-Collected basis; English-only responsive layout including off-canvas mobile navigation and horizontally scrollable tables. See `final_product_remains.md` and `docs/server-api.md` for implementation status.
- **Cosmetic pass 1:** Dashboard and Reports got reference styling refinements in commit `2439feb` (room-card density, colored status dots, lavender booking action, icon stat cards, report toggle/monthly card). The remaining style-match item is still open; no claim is made that every list page is pixel-matched.
- **Intentionally deferred/out of scope:** invoices, stock/expenses, agency currents, currency exchange, channel-booking imports/deep channel-manager sync, settings/application store, messaging/notifications/help integrations. These need separate domain models or external integrations and are not represented as completed UI.
- **Responsive verification:** 375px/700px same-origin iframe checks were used on 2026-10-30 for Dashboard, Reports, Reservations, and Monthly Room. This was local-app validation, not a fresh live REA capture.
- **Positive confirmations from the original capture:** status vocabulary, pagination conventions and date-range semantics informed the server/UI mapping; current tenant scope remains token-derived rather than trusting body hotel IDs.

## Implementation decision (recorded)

Match the frame + pattern system (design language above) and implement the modules that map to
our server's data model (calendar, bookings, guests, facility/housekeeping, reports, channels
pricing). Turkish-only report labels on the reference (e.g. "Tek Kisilik", "%76") are
rendered in English in our UI per the English-only decision. Invoices/stock/agency-currents/
expense/currency/application-store surfaces stay out of scope (README ss4 deferral) - the
routes exist in the shell as disabled placeholders only if needed.
