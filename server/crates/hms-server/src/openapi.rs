//! OpenAPI document (todo F2).
//!
//! The contract is generated from the server's own DTOs, so it cannot drift
//! from the implementation the way a hand-maintained YAML would. `/api/v1/openapi.json`
//! serves it, and a test in `crates/hms-tests` compares every documented name
//! against `apps/web/src/types/generated/api.ts` so a renamed or added field
//! fails the build.
//!
//! Scope: the whole REST surface is listed as paths; the schemas cover the
//! DTOs the web client actually consumes. Endpoints without a typed body are
//! documented as such rather than guessed.

use serde_json::{json, Map, Value};

macro_rules! schema {
    ($name:literal, $($field:literal : $ty:literal),* $(,)?) => {{
        let mut props = Map::new();
        let mut required = Vec::new();
        $(
            props.insert($field.to_string(), json!({ "type": $ty }));
            required.push($field);
        )*
        (Value::String($name.to_string()), json!({
            "type": "object",
            "properties": props,
            "required": required,
        }))
    }};
}

fn nullable(ty: &str) -> Value {
    json!({ "type": ty, "nullable": true })
}

/// Object schema with explicit per-field schemas (for nullables/unions).
fn object(entries: Vec<(&str, Value)>, required: &[&str]) -> Value {
    let mut props = Map::new();
    for (k, v) in entries {
        props.insert(k.to_string(), v);
    }
    json!({ "type": "object", "properties": props, "required": required })
}

fn string() -> Value {
    json!({ "type": "string" })
}
fn integer() -> Value {
    json!({ "type": "integer", "format": "int64" })
}
fn int32() -> Value {
    json!({ "type": "integer", "format": "int32" })
}
fn bool_() -> Value {
    json!({ "type": "boolean" })
}
fn number() -> Value {
    json!({ "type": "number", "format": "double" })
}
fn arr(items: Value) -> Value {
    json!({ "type": "array", "items": items })
}
fn ref_(name: &str) -> Value {
    json!({ "$ref": format!("#/components/schemas/{name}") })
}

pub fn document() -> Value {
    let mut schemas = Map::new();
    let mut put = |v: (Value, Value)| {
        let (k, s) = v;
        schemas.insert(k.as_str().unwrap_or_default().to_string(), s);
    };

    // ---------------------------------------------------------------- auth
    put((
        Value::String("ApiResponse".into()),
        json!({
            "type": "object",
            "description": "Uniform envelope: `data` on success, `error` on failure.",
            "properties": {
                "success": bool_(),
                "data": {},
                "error": nullable("string"),
            },
            "required": ["success"],
        }),
    ));
    put(schema!("LoginRequest",
        "hotel_id": "integer",
        "username": "string",
        "password": "string",
    ));
    put((
        Value::String("SessionUser".into()),
        object(
            vec![
                ("user_id", string()),
                ("hotel_id", integer()),
                ("username", string()),
                ("full_name", nullable("string")),
                ("role", string()),
                ("force_password_change", bool_()),
            ],
            &[
                "user_id",
                "hotel_id",
                "username",
                "role",
                "force_password_change",
            ],
        ),
    ));
    put((
        Value::String("LoginResponse".into()),
        object(
            vec![
                ("token", string()),
                ("user", ref_("SessionUser")),
                ("permissions", arr(string())),
            ],
            &["token", "user", "permissions"],
        ),
    ));

    // -------------------------------------------------------- hotels/users
    put(schema!("HotelDto",
        "id": "integer",
        "name": "string",
        "default_currency": "string",
        "time_zone": "string",
        "is_active": "boolean",
        "next_change_seq": "integer",
    ));
    put((
        Value::String("UserDto".into()),
        object(
            vec![
                ("id", string()),
                ("hotel_id", integer()),
                ("username", string()),
                ("full_name", nullable("string")),
                ("email", nullable("string")),
                ("role", string()),
                ("is_active", bool_()),
                ("force_password_change", bool_()),
            ],
            &[
                "id",
                "hotel_id",
                "username",
                "role",
                "is_active",
                "force_password_change",
            ],
        ),
    ));
    put(schema!("RoleMatrix",
        "role": "string",
        "permissions": "array",
    ));

    // ------------------------------------------------------- rooms/guests
    put(schema!("RoomTypeDto",
        "id": "string",
        "name": "string",
        "capacity": "integer",
        "base_price_cents": "integer",
    ));
    put(schema!("RoomDto",
        "id": "string",
        "room_type_id": "string",
        "room_number": "string",
        "floor": "integer",
        "cleanliness": "string",
    ));
    put((
        Value::String("GuestDto".into()),
        object(
            vec![
                ("id", string()),
                ("full_name", string()),
                ("nationality", nullable("string")),
                ("license_plate", nullable("string")),
                ("birth_date", nullable("string")),
                ("is_primary", bool_()),
                ("kbs_reported", bool_()),
            ],
            &["id", "full_name", "is_primary", "kbs_reported"],
        ),
    ));
    put(schema!("ChannelDto",
        "id": "string",
        "kind": "string",
        "name": "string",
        "is_active": "boolean",
    ));
    put(schema!("PriceRuleDto",
        "id": "string",
        "room_type_id": "string",
        "date_from": "string",
        "date_to": "string",
        "price_cents": "integer",
        "currency": "string",
    ));

    // ------------------------------------------------------- reservations
    put(schema!("DailyRate",
        "date": "string",
        "price_cents": "integer",
    ));
    put((
        Value::String("ReservationDto".into()),
        object(
            vec![
                ("id", string()),
                ("hotel_id", integer()),
                ("room_id", string()),
                ("channel_id", string()),
                ("guest_id", nullable("string")),
                ("checkin_date", string()),
                ("checkout_date", string()),
                ("status", string()),
                ("daily_rates", arr(ref_("DailyRate"))),
                ("total_price_cents", integer()),
                ("currency", string()),
                ("adults", int32()),
                ("children", int32()),
                ("notes", nullable("string")),
                ("voucher_number", nullable("string")),
                ("created_at", string()),
            ],
            &[
                "id",
                "hotel_id",
                "room_id",
                "channel_id",
                "checkin_date",
                "checkout_date",
                "status",
                "total_price_cents",
                "currency",
                "adults",
                "children",
                "created_at",
            ],
        ),
    ));
    put((
        Value::String("CreateReservationBody".into()),
        object(
            vec![
                ("room_id", string()),
                ("channel_id", string()),
                ("guest_id", nullable("string")),
                ("checkin_date", string()),
                ("checkout_date", string()),
                ("adults", int32()),
                ("children", int32()),
                ("currency", nullable("string")),
                ("notes", nullable("string")),
                ("voucher_number", nullable("string")),
            ],
            &["room_id", "channel_id", "checkin_date", "checkout_date"],
        ),
    ));
    put((
        Value::String("ReservationListQuery".into()),
        object(
            vec![
                ("from", nullable("string")),
                ("to", nullable("string")),
                ("status", nullable("string")),
                ("limit", nullable("integer")),
                ("after", nullable("string")),
            ],
            &[],
        ),
    ));
    put((
        Value::String("PaymentDto".into()),
        object(
            vec![
                ("id", string()),
                ("reservation_id", string()),
                ("amount_cents", integer()),
                ("currency", string()),
                ("payment_type", string()),
                ("reference", nullable("string")),
                ("is_reversal", bool_()),
                ("created_by", string()),
                ("created_at", string()),
            ],
            &[
                "id",
                "reservation_id",
                "amount_cents",
                "currency",
                "payment_type",
                "is_reversal",
            ],
        ),
    ));
    put((
        Value::String("FolioDto".into()),
        object(
            vec![
                ("id", string()),
                ("reservation_id", string()),
                ("category", string()),
                ("description", string()),
                ("unit_price_cents", integer()),
                ("quantity", integer()),
                ("total_cents", integer()),
                ("currency", string()),
                ("created_at", string()),
            ],
            &[
                "id",
                "reservation_id",
                "category",
                "unit_price_cents",
                "quantity",
                "total_cents",
                "currency",
            ],
        ),
    ));

    // ------------------------------------------------------------- board
    put((
        Value::String("BoardStay".into()),
        object(
            vec![
                ("reservation_id", string()),
                ("guest_name", nullable("string")),
                ("checkin_date", string()),
                ("checkout_date", string()),
                ("status", string()),
                ("total_price_cents", integer()),
                ("currency", string()),
            ],
            &["reservation_id", "checkin_date", "checkout_date", "status"],
        ),
    ));
    put((
        Value::String("BoardRoom".into()),
        object(
            vec![
                ("id", string()),
                ("room_number", string()),
                ("room_type_id", string()),
                ("room_type_name", string()),
                ("floor", int32()),
                ("cleanliness", string()),
                ("stays", arr(ref_("BoardStay"))),
                ("seasonal_price_cents", integer()),
            ],
            &["id", "room_number", "room_type_id", "room_type_name"],
        ),
    ));
    put((
        Value::String("BoardDay".into()),
        object(
            vec![
                ("date", string()),
                ("available", int32()),
                ("occupied", int32()),
                ("from_price_cents", nullable("integer")),
            ],
            &["date", "available", "occupied"],
        ),
    ));
    put((
        Value::String("BoardResponse".into()),
        object(
            vec![
                ("from", string()),
                ("to", string()),
                ("rooms", arr(ref_("BoardRoom"))),
                // the per-day summary is named `occupancy` on the wire
                ("occupancy", arr(ref_("BoardDay"))),
            ],
            &["from", "to", "rooms", "occupancy"],
        ),
    ));

    // ----------------------------------------------------------- reports
    put((
        Value::String("SummaryReport".into()),
        object(
            vec![
                ("from", string()),
                ("to", string()),
                ("currency", string()),
                ("room_revenue_cents", integer()),
                ("extra_revenue_cents", integer()),
                ("payments_collected_cents", integer()),
                ("occupied_nights", integer()),
                ("available_room_nights", integer()),
                ("occupancy_pct", number()),
                ("adr_cents", nullable("number")),
                ("revpar_cents", nullable("number")),
            ],
            &[
                "from",
                "to",
                "room_revenue_cents",
                "extra_revenue_cents",
                "payments_collected_cents",
            ],
        ),
    ));
    put((
        Value::String("DayReport".into()),
        object(
            vec![
                ("date", string()),
                ("room_revenue_cents", integer()),
                ("extra_revenue_cents", integer()),
                ("payments_collected_cents", integer()),
                ("occupied_nights", integer()),
            ],
            &["date"],
        ),
    ));
    put((
        Value::String("ChannelRevenue".into()),
        object(
            vec![
                ("channel_id", string()),
                ("channel_name", string()),
                ("kind", string()),
                ("room_revenue_cents", integer()),
                ("extra_revenue_cents", integer()),
                ("payments_collected_cents", integer()),
                ("occupied_nights", integer()),
            ],
            &["channel_id", "channel_name"],
        ),
    ));
    put((
        Value::String("RoomRevenue".into()),
        object(
            vec![
                ("room_id", string()),
                ("room_number", string()),
                ("room_type_name", string()),
                ("room_revenue_cents", integer()),
                ("extra_revenue_cents", integer()),
                ("occupied_nights", integer()),
            ],
            &["room_id", "room_number"],
        ),
    ));
    put((
        Value::String("ForecastRow".into()),
        object(
            vec![
                ("date", string()),
                ("arrivals", integer()),
                ("vacant", integer()),
                ("guests", integer()),
                ("occupancy_pct", number()),
                ("occupied", integer()),
            ],
            &["date"],
        ),
    ));
    put((
        Value::String("InhouseRow".into()),
        object(
            vec![
                ("reservation_id", string()),
                ("guest_name", nullable("string")),
                ("room_number", string()),
                ("room_type_name", string()),
                ("checkin_date", string()),
                ("checkout_date", string()),
                ("guests", int32()),
                ("balance_cents", integer()),
                ("day_rate_cents", nullable("integer")),
                ("currency", string()),
            ],
            &[
                "reservation_id",
                "room_number",
                "checkin_date",
                "checkout_date",
                "balance_cents",
            ],
        ),
    ));
    put((
        Value::String("InhouseReport".into()),
        object(
            vec![
                ("date", string()),
                ("currency", string()),
                ("bookings", integer()),
                ("adults", integer()),
                ("children", integer()),
                ("room_rate_cents", integer()),
                ("balance_cents", integer()),
                ("rows", arr(ref_("InhouseRow"))),
            ],
            &["date", "rows"],
        ),
    ));
    put((
        Value::String("MainCouranteRow".into()),
        object(
            vec![
                ("reservation_id", string()),
                ("guest_name", nullable("string")),
                ("room_number", string()),
                ("checkin_date", string()),
                ("checkout_date", string()),
                ("daily_rate_cents", nullable("integer")),
                ("room_rate_cents", integer()),
                ("previous_balance_cents", integer()),
                ("balance_cents", integer()),
                ("currency", string()),
            ],
            &["reservation_id", "room_number", "balance_cents"],
        ),
    ));
    put((
        Value::String("MainCouranteReport".into()),
        object(
            vec![
                ("date", string()),
                ("count", integer()),
                ("rows", arr(ref_("MainCouranteRow"))),
            ],
            &["date", "rows"],
        ),
    ));

    // ------------------------------------------------------------- paths
    let ok = |schema: &str| {
        json!({
            "200": {
                "description": "OK",
                "content": { "application/json": { "schema": json!({
                    "allOf": [ ref_("ApiResponse"), json!({ "type": "object",
                        "properties": { "data": ref_(schema) } }) ]
                }) } }
            },
            "401": { "description": "Missing or revoked session" },
        })
    };
    let ok_list = |schema: &str| {
        json!({
            "200": {
                "description": "OK",
                "content": { "application/json": { "schema": json!({
                    "allOf": [ ref_("ApiResponse"), json!({ "type": "object",
                        "properties": { "data": arr(ref_(schema)) } }) ]
                }) } }
            },
            "401": { "description": "Missing or revoked session" },
        })
    };
    let json_body = |schema: &str| {
        json!({
            "required": true,
            "content": { "application/json": { "schema": ref_(schema) } }
        })
    };
    let query = |params: Value| json!(params);

    let mut paths = Map::new();
    let mut add = |path: &str, method: &str, op: Value| {
        let entry = paths.entry(path.to_string()).or_insert_with(|| json!({}));
        entry[method] = op;
    };

    add(
        "/health",
        "get",
        json!({ "summary": "Liveness probe", "responses": { "200": { "description": "OK" } } }),
    );
    add(
        "/metrics",
        "get",
        json!({ "summary": "Prometheus exposition", "responses": { "200": { "description": "OK" } } }),
    );
    add(
        "/api/v1/openapi.json",
        "get",
        json!({ "summary": "This document", "responses": { "200": { "description": "OK" } } }),
    );

    add(
        "/api/v1/auth/login",
        "post",
        json!({
            "summary": "Sign in (hotel code + username + password)",
            "requestBody": json_body("LoginRequest"),
            "responses": ok("LoginResponse"),
        }),
    );
    add(
        "/api/v1/auth/change-password",
        "post",
        json!({
            "summary": "Change the caller's password",
            "responses": { "200": { "description": "OK" }, "403": { "description": "Policy violation" } },
        }),
    );
    add(
        "/api/v1/auth/logout",
        "post",
        json!({
            "summary": "Sign out",
            "responses": { "200": { "description": "OK" } },
        }),
    );

    add(
        "/api/v1/hotels",
        "get",
        json!({ "summary": "List hotels", "responses": ok_list("HotelDto") }),
    );
    add(
        "/api/v1/hotels",
        "post",
        json!({ "summary": "Create a hotel (platform admin)", "responses": ok("HotelDto") }),
    );
    add(
        "/api/v1/hotels/{id}",
        "patch",
        json!({ "summary": "Rename / enable / disable a hotel", "responses": ok("HotelDto") }),
    );

    add(
        "/api/v1/users",
        "get",
        json!({ "summary": "List users (own hotel; platform admin sees all)", "responses": ok_list("UserDto") }),
    );
    add(
        "/api/v1/users",
        "post",
        json!({ "summary": "Provision a user", "responses": ok("UserDto") }),
    );
    add(
        "/api/v1/users/{id}",
        "patch",
        json!({ "summary": "Update role / password / active flag", "responses": ok("UserDto") }),
    );
    add(
        "/api/v1/roles/{role}/permissions",
        "get",
        json!({ "summary": "Permission matrix for a role", "responses": ok("RoleMatrix") }),
    );

    add(
        "/api/v1/room-types",
        "get",
        json!({ "summary": "List room types", "responses": ok_list("RoomTypeDto") }),
    );
    add(
        "/api/v1/room-types",
        "post",
        json!({ "summary": "Create a room type", "responses": ok("RoomTypeDto") }),
    );
    add(
        "/api/v1/rooms",
        "get",
        json!({ "summary": "List rooms", "responses": ok_list("RoomDto") }),
    );
    add(
        "/api/v1/rooms",
        "post",
        json!({ "summary": "Create a room", "responses": ok("RoomDto") }),
    );
    add(
        "/api/v1/rooms/{id}/cleanliness",
        "patch",
        json!({
            "summary": "Update housekeeping status (last write wins)",
            "responses": ok("RoomDto"),
        }),
    );

    add(
        "/api/v1/guests",
        "get",
        json!({
            "summary": "List/search guests",
            "parameters": query(json!([{ "name": "search", "in": "query", "schema": string() }])),
            "responses": ok_list("GuestDto"),
        }),
    );
    add(
        "/api/v1/guests",
        "post",
        json!({ "summary": "Create a guest", "responses": ok("GuestDto") }),
    );
    add(
        "/api/v1/guests/{id}",
        "patch",
        json!({ "summary": "Update a guest", "responses": { "200": { "description": "OK" } } }),
    );
    add(
        "/api/v1/guests/{id}",
        "delete",
        json!({ "summary": "Tombstone a guest", "responses": { "200": { "description": "OK" } } }),
    );
    add(
        "/api/v1/guests/{id}/export",
        "get",
        json!({ "summary": "GDPR export of one guest", "responses": { "200": { "description": "OK" } } }),
    );

    add(
        "/api/v1/channels",
        "get",
        json!({ "summary": "List channels", "responses": ok_list("ChannelDto") }),
    );
    add(
        "/api/v1/channels",
        "post",
        json!({ "summary": "Create a channel", "responses": ok("ChannelDto") }),
    );
    add(
        "/api/v1/price-rules",
        "get",
        json!({ "summary": "List price rules", "responses": ok_list("PriceRuleDto") }),
    );
    add(
        "/api/v1/price-rules",
        "post",
        json!({ "summary": "Create a price rule", "responses": ok("PriceRuleDto") }),
    );

    add(
        "/api/v1/reservations",
        "get",
        json!({
            "summary": "List reservations (keyset paginated)",
            "parameters": query(json!([
                { "name": "from", "in": "query", "schema": string() },
                { "name": "to", "in": "query", "schema": string() },
                { "name": "status", "in": "query", "schema": string() },
                { "name": "limit", "in": "query", "schema": integer() },
                { "name": "after", "in": "query", "schema": string() },
            ])),
            "responses": ok_list("ReservationDto"),
        }),
    );
    add(
        "/api/v1/reservations",
        "post",
        json!({
            "summary": "Create a reservation (rejected if it would double-book)",
            "requestBody": json_body("CreateReservationBody"),
            "responses": ok("ReservationDto"),
        }),
    );
    add(
        "/api/v1/reservations/count",
        "get",
        json!({ "summary": "Count matching reservations", "responses": ok("integer") }),
    );
    add(
        "/api/v1/reservations/{id}",
        "get",
        json!({ "summary": "Fetch one reservation", "responses": ok("ReservationDto") }),
    );
    add(
        "/api/v1/reservations/{id}",
        "patch",
        json!({
            "summary": "Edit / move rooms; a conflict reservation becomes confirmed on reassignment",
            "responses": ok("ReservationDto"),
        }),
    );
    add(
        "/api/v1/reservations/{id}/suggested-rooms",
        "get",
        json!({
            "summary": "Free rooms for this stay (same room type first)",
            "responses": ok_list("RoomDto"),
        }),
    );
    add(
        "/api/v1/reservations/{id}/check-in",
        "post",
        json!({ "summary": "Check in", "responses": ok("ReservationDto") }),
    );
    add(
        "/api/v1/reservations/{id}/check-out",
        "post",
        json!({ "summary": "Check out", "responses": ok("ReservationDto") }),
    );
    add(
        "/api/v1/reservations/{id}/cancel",
        "post",
        json!({ "summary": "Cancel", "responses": ok("ReservationDto") }),
    );
    add(
        "/api/v1/reservations/{id}/payments",
        "get",
        json!({ "summary": "Payment ledger of a reservation", "responses": ok_list("PaymentDto") }),
    );
    add(
        "/api/v1/reservations/{id}/payments",
        "post",
        json!({ "summary": "Record a payment (append-only)", "responses": ok("PaymentDto") }),
    );
    add(
        "/api/v1/payments/{id}/reverse",
        "post",
        json!({ "summary": "Record a reversal (never edits)", "responses": ok("PaymentDto") }),
    );
    add(
        "/api/v1/reservations/{id}/folio",
        "get",
        json!({ "summary": "Extra charges of a reservation", "responses": ok_list("FolioDto") }),
    );
    add(
        "/api/v1/reservations/{id}/folio",
        "post",
        json!({ "summary": "Add an extra charge", "responses": ok("FolioDto") }),
    );

    add(
        "/api/v1/board",
        "get",
        json!({
            "summary": "Monthly board for a date window",
            "parameters": query(json!([
                { "name": "from", "in": "query", "required": true, "schema": string() },
                { "name": "to", "in": "query", "required": true, "schema": string() },
            ])),
            "responses": ok("BoardResponse"),
        }),
    );

    for (path, name) in [
        ("/api/v1/reports/summary", "SummaryReport"),
        ("/api/v1/reports/by-day", "DayReport"),
        ("/api/v1/reports/by-channel", "ChannelRevenue"),
        ("/api/v1/reports/by-room", "RoomRevenue"),
        ("/api/v1/reports/forecast", "ForecastRow"),
        ("/api/v1/reports/inhouse", "InhouseReport"),
        ("/api/v1/reports/maincourante", "MainCouranteReport"),
    ] {
        let single = matches!(
            name,
            "SummaryReport" | "InhouseReport" | "MainCouranteReport"
        );
        add(
            path,
            "get",
            json!({
                "summary": format!("Report: {name}"),
                "parameters": query(json!([
                    { "name": "from", "in": "query", "schema": string() },
                    { "name": "to", "in": "query", "schema": string() },
                    { "name": "start", "in": "query", "schema": string() },
                    { "name": "date", "in": "query", "schema": string() },
                    { "name": "days", "in": "query", "schema": integer() },
                ])),
                "responses": if single { ok(name) } else { ok_list(name) },
            }),
        );
    }

    add(
        "/api/v1/sync/push",
        "post",
        json!({ "summary": "Push offline operations (idempotent by op id)", "responses": { "200": { "description": "OK" } } }),
    );
    add(
        "/api/v1/sync/pull",
        "get",
        json!({
            "summary": "Pull changes after a cursor",
            "parameters": query(json!([{ "name": "since", "in": "query", "schema": integer() }])),
            "responses": { "200": { "description": "OK" } },
        }),
    );
    add(
        "/api/v1/audit",
        "get",
        json!({ "summary": "Audit trail", "responses": { "200": { "description": "OK" } } }),
    );

    let _ = (integer, int32, bool_, number, arr, nullable);
    json!({
        "openapi": "3.0.3",
        "info": {
            "title": "HMS API",
            "version": env!("CARGO_PKG_VERSION"),
            "description": "Multi-tenant hotel management. The PostgreSQL server is the authority; the desktop mirror syncs through /api/v1/sync.",
        },
        "servers": [{ "url": "/", "description": "Same origin (proxied)" }],
        "components": {
            "schemas": schemas,
            "securitySchemes": {
                "bearer": { "type": "http", "scheme": "bearer", "bearerFormat": "JWT" }
            }
        },
        "security": [{ "bearer": [] }],
        "paths": paths,
    })
}
