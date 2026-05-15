# Hotel PMS (Property Management System)

A modular **Django + Django REST Framework** hotel management platform for daily operations: front office, rooms, housekeeping, guests, reservations, payments, inventory, and reporting.

## What this project is

This project is an API-first PMS with server-rendered operational dashboards. It is structured as domain apps under `apps/`, each with models, services, and API layers.

## Main classes (models)

Core business entities include:
- `Hotel` (`apps/hotels/models.py`)
- `Room`, `RoomType` (`apps/rooms/models.py`)
- `Guest` (`apps/guests/models.py`)
- `Reservation`, `ReservationGuest` (`apps/reservations/models.py`)
- `Stay` (`apps/stays/models.py`)
- `Folio`, folio transactions (`apps/folios/models.py`)
- `Payment` (`apps/payments/models.py`)
- `InventoryItem`, inventory transaction classes (`apps/inventory/models.py`)
- `HousekeepingTask`/housekeeping entities and lost & found (`apps/housekeeping/models.py`)
- `Role`, `User`, `ActivityLog` (`apps/accounts/models.py`)
- Integration/audit/report entities (`apps/integrations/models.py`, `apps/audit/models.py`, `apps/reports/models.py`)

## How to use

### 1) Setup
```bash
python -m venv .venv
source .venv/bin/activate
pip install -r requirements.txt
python manage.py migrate
python manage.py createsuperuser
python manage.py runserver
```

### 2) Key entry points
- Web UI: `/`
- Admin: `/admin/`
- API routes are mounted from app API views in `config/urls.py` and `apps/*/api/views.py`.

### 3) Typical flow
1. Configure rooms and room types.
2. Register guests.
3. Create reservations/check-ins.
4. Track folio and payments.
5. Use housekeeping and inventory dashboards daily.

## Architecture overview

- `apps/<domain>/models.py` → persistence layer
- `apps/<domain>/services/` → business rules
- `apps/<domain>/api/` → DRF serializers/views
- `templates/` → operational web interfaces
- `config/` → Django settings, URL routing, ASGI/WSGI

## How much bigger it can become

This codebase can scale from a single-property system to a multi-property platform by adding:
- Multi-tenant isolation and per-property RBAC
- Revenue management and pricing rules engine
- Channel manager sync hardening (OTA, webhooks, retries, reconciliation)
- POS/spa/restaurant posting into folios
- BI data warehouse + scheduled analytics exports
- Mobile staff workflows (housekeeping/maintenance)
- Advanced audit/compliance policies and SSO

In practice, the current modular app boundaries are suitable for phased growth without rewriting the whole system.
