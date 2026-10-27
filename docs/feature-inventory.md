# Feature inventory — verified against app.hms.gen.tr (site 2)

Completed: 27 Oct 2026. Verified live with a logged-in session (Sina E., Kuzen Otel, facility code 4442 — see `.env.example` for the credential variables; credentials are only in the user's possession, not in this repo). Login succeeded with **no captcha**; the OpenCode embedded browser keeps the session, so future sessions may still be logged in — check before asking the user for credentials.

Evidence screenshots live in `docs/screenshots/`.

## Menu / route tree (site 2, v2.08)

Sidebar (verified from DOM, 56+ routes):

- **Home** `/` — dashboard board with per-room quick actions: Add Payment, Add Transaction, Add Guest, Detail, Quick Transaction, Group Res, Agency link; Out-of-order / dirty-room counters.
- **Monthly View** `/aylikoda` → renders `/aylikodaV3` (room board). See screenshot.
- **Guest**: Guests `/musteri/` (Add Guest, Filter, Import/Export, Others; columns Name Surname / ID Number–Passport / Balance); Guest Relations → Detailed Edit `/smsmail/detayli/`, Send SMS `/smsmail/`, Send E-mail `/smsmail/mail/`.
- **Reservation**: List `/rezervasyon/` (Add New Reservation, Filter; columns Customer, Check-in, Check-out, Room Type, Register Date, Remaining Balance); Group Reservation `/rezervasyongrup/`; Channel Reservation `/kanalrezervasyon/`; Occupancy `/rezervasyon/doluluk`.
- **Facility → Room**: Update Status `/oda/durumguncelle`, Housekeeping `/oda/housekeeping`, Reporting `/oda/bildirim`. (Room/room-type CRUD is NOT in this account's menu — verified as data via dropdowns: 7 room types.)
- **Transactions**: Open Sale `/extra/blokaj`, Expense `/gider/`, Stock `/stok/`, (reports list link).
- **Periodic Prices**: Calendar `/donemsel/`, Overview `/donemsel/fiyatmusaitlik`, Manual Availability `/donemsel/doluluk`, Batch Updates `/donemsel/tumu`, Advanced Updates `/donemsel/gelismis`.
- **Reports** `/raporlar/`: Earning `/raporlar/kapasite/kazancozetyeni/`, Collection/Payment (Return) `/raporlar/kapasite/tahsilatozet/`, Guests `/raporlar/misafirler/`, Expenses `/raporlar/giderler/`, Room `/raporlar/kapasite/odalar/`, Occupancy `/raporlar/kapasite/doluluk/`, Early Check-out `/raporlar/erkencikis/`, **Log** `/raporlar/log/`, Instant Cash Account `/raporlar/kasaanlik/`.
- **Invoice**: Purchase Invoice `/fatura/bolum/2`, Sale Invoice `/fatura/bolum/1`.
- **Agency / Current**: Current `/cari/`, Agency `/acenta/`.
- **Tools**: Currency `/dovizalimsatim/`; Case → List `/kasa/`, Cash Transfer `/kasa/devir`; Finance → Banks `/banka/`, Online Payment `/odemeonline/otel`; Channel → Channels `/kanal/`, Channel E-mail `/kanal/mail`, Channel Logs `/kanal/log`, Channel Error Messages `/channel/log/guest`, Online Reservation Coupon `/onlinekupon/`, Conditional Pricing `/availability/price/rules`, Application `/uygulama/entegre/`; Others → Stock Category `/stokkategori/`, Wi-Fi System `/wifi/`, Allowed IP List `/ipizinli/`; SMS & E-mail → E-mail Patterns `/smsmailkalip/mail/1/`, SMS Patterns `/smsmailkalip/`, SMS Settings `/smssistemayarlar/`, Mail Host `/mailhost/`; API `/api`.
- **Help** `/index/yardim/`
- User dropdown: Your Information `/account/profile`, Cash Account, End Of Day `/gunkapat/index`, Upgrade Package `/index/paketler/`, Recent Features `/versiyons`, Logout `/logout`.

## What was observed per README §4 area

| §4 Area | Site-2 evidence | Verdict |
|---|---|---|
| Reservations | List w/ filter + balances; detail page (screenshot): tabs Account Activities / Guests / Collection–Payment / Transactions / Daily Price; actions Check-in (quick, with time), Check-out, Edit, Cancel; per-day price, amount in TRY; board type badge (Bed And Breakfast); "Transaction Logs" + Comments + Save/Cancel/**Print** | **In scope — core** |
| Guests | /musteri/ list, Add Guest, ID/Passport no, balance, Import/Export | **In scope — core** |
| Rooms | Rooms as data everywhere (board, housekeeping); status update page uses **Out of Order / Dirty / Clean** toggles (3 states, NOT site-1's 4-state inspected model) | **In scope — core** |
| Pricing | Periodic calendar `/donemsel/` with month+year (2021–2031) + room-type filter, per-day price:capacity; Conditional Pricing rules `/availability/price/rules` | **In scope — core** |
| Payments | Reservation detail "Collection / Payment" tab, Add Payment, Account Activities; bank transactions list on /profile/ (credit card, transfer, cheque, confirmed status, currency €) | **In scope — core** |
| Channels | Channel manager: channel list w/ 30+ hardcoded channel tiles + CONNECT/CONNECTED/PASSIVE states (screenshot); Channel Reservations, Channel E-mail/Logs/Error Messages, coupons | **Core for list/status; deep channel-manager sync = deferred** |
| Housekeeping | Housekeeping board (screenshot): rooms w/ occupancy + stay dates + checkboxes, Print; Update Status (dirty/clean/OOO) | **In scope — core** |
| Users and roles | /account/profile shows user profile, Change Password, **Two-Factor Authentication**, Notification Settings, Program Language (TR/EN switch). User/role CRUD menu not visible for this user's role (likely admin-only) | **In scope — core (README §6 is the spec)** |
| Reports | Earnings Summary (sections Reservation/Extra/Revenue/Minibar/Spa/Restaurant + Open Sale), search by arrival/departure/registration date, Grand Total, Print; Occupancy w/ Graphic + room rate/percent; Collection/Payment; Guests; Expenses; Room; Early Check-out; Instant Cash | **In scope — core subset, per README §10; rest deferred** |
| Audit | `/raporlar/log/`: filters (date range, user, transaction type Add/Delete/Update, section, Print), columns Log/User/Name Surname/Time/Section/Section Id/IP/Transaction/Comments, human-readable entries | **In scope — core** |
| Localization | Profile "Program Language" switch; UI labels in English, error messages in Turkish | **In scope — TR+EN per README** |
| Export | Print buttons on reports/detail/housekeeping; Import/Export on guests + agencies | **In scope — CSV+PDF per README** |
| Layout | Board renders well in ~900px and ~510px viewports (mobile-friendly) | **In scope** |

## Site-2 modules NOT in README §4 (scope proposal — decision pending)

| Module | Evidence | Proposal |
|---|---|---|
| Invoices (sale/purchase, e-invoice, e-archive numbers) | /fatura/bolum/1|2 | **Deferred** — Turkish e-invoice integration |
| Agencies / Currents (CARI) | /acenta/, /cari/ with balances, Import/Export | **Deferred** (reservations keep agency name only) |
| Currency exchange (buy/sell, 25 currencies) | /dovizalimsatim/ | **Deferred/out** |
| Cash accounts + transfer + banks | /kasa/, /kasa/devir, /banka/ | **Deferred** (keep simple in-app payments) |
| Online payment (collect from guest portal) | /odemeonline/otel | **Out** |
| Open Sale, Expenses, Stock (+stock category) | /extra/blokaj, /gider/, /stok/, /stokkategori/ | **Deferred** |
| SMS / E-mail sending + templates + mail host + Wi-Fi + IP allowlist | tools submenu | **Out** (email templates maybe P2) |
| Apps/integrations, API platform page | /uygulama/entegre/, /api | **Out** |
| Packages/licensing (Upgrade Package, version history v2.08) | /index/paketler/, /versiyons | **Out** (platform-level later) |
| Notifications / announcements / messaging | header panel, /index/bildirimler/, /mesajlasma/ | **Deferred** |
| End-of-day close | /gunkapat/ | **Deferred** |

## Key takeaway for Phase 3–4 design

- Site 2's cleanliness model is **Dirty / Clean (+ Out of Order)**; site 1 uses dirty/cleaning/clean/inspected. README §6 keeps 4 states + OOO — keep README's model, but note the discrepancy for the UI.
- Reservation detail has **Transaction Logs** embedded → per-entity audit is expected by users, matches README's `audit_log`.
- Board shows **per-night availability count + price per room type** → pricing/availability are per room_type × date, matching `price_rules` + board design.
- No per-CHANNEL earnings report on site 2 (earnings are by revenue section); README's per-channel report comes from site 1's model. Keep README per-channel design.
