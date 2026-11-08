//! G6: site-2 module verification.
//!
//! These tests exercise the domain invariants in `hms_core::site2` (FX rounding,
//! stock sign conventions, agency credit limits, e-invoice lifecycle) and the
//! migration SQL (0009) by applying it to the test database and confirming the
//! new tables exist and are RLS-protected.
//!
//! Database-dependent parts are gated on `DATABASE_URL` pointing at a Postgres
//! instance with the `hms_test` database (the same one the rest of the suite
//! uses); they are skipped otherwise.

use hms_core::site2::*;

#[test]
fn g6_fx_conversion_rounds_half_away_from_zero() {
    // 100.00 EUR at 34.123456 TRY → 3412.3456 TRY = 341235 kuruş (rounded)
    assert_eq!(convert_to_base_cents(10_000, 34_123_456).unwrap(), 341_235);
    // negative amounts round symmetrically
    assert_eq!(
        convert_to_base_cents(-10_000, 34_123_456).unwrap(),
        -341_235
    );
    // identity rate (1:1)
    assert_eq!(convert_to_base_cents(123_456, MICRO).unwrap(), 123_456);
    // zero or negative rate is rejected
    assert!(convert_to_base_cents(100, 0).is_err());
    assert!(convert_to_base_cents(100, -1).is_err());
}

#[test]
fn g6_stock_sign_conventions() {
    assert!(validate_movement_sign(StockMovementType::In, 10).is_ok());
    assert!(validate_movement_sign(StockMovementType::In, -10).is_err());
    assert!(validate_movement_sign(StockMovementType::Out, -3).is_ok());
    assert!(validate_movement_sign(StockMovementType::Out, 3).is_err());
    assert!(validate_movement_sign(StockMovementType::Adjustment, -1).is_ok());
    assert!(validate_movement_sign(StockMovementType::Adjustment, 0).is_err());
    assert!(validate_movement_sign(StockMovementType::In, 0).is_err());
}

#[test]
fn g6_stock_on_hand_and_restock_threshold() {
    let item = StockItem {
        id: uuid::Uuid::now_v7(),
        hotel_id: 1,
        sku: None,
        name: "Minibar water".into(),
        unit: "adet".into(),
        unit_cost_cents: 250,
        min_level: 10,
        currency: "TRY".into(),
        is_active: true,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted_at: None,
    };
    let mv = |t: StockMovementType, q: i32| StockMovement {
        id: uuid::Uuid::now_v7(),
        hotel_id: 1,
        stock_item_id: item.id,
        movement_type: t.as_str().to_string(),
        quantity: q,
        unit_cost_cents: 100,
        room_id: None,
        reservation_id: None,
        note: None,
        created_by: None,
        created_at: chrono::Utc::now(),
    };
    let log = vec![
        mv(StockMovementType::In, 24),
        mv(StockMovementType::Out, -5),
        mv(StockMovementType::Adjustment, -2),
    ];
    assert_eq!(on_hand(&log), 17);
    assert!(needs_restock(&item, 10));
    assert!(needs_restock(&item, 3));
    assert!(!needs_restock(&item, 11));
}

#[test]
fn g6_agency_credit_limit_logic() {
    let mut a = Agency {
        id: uuid::Uuid::now_v7(),
        hotel_id: 1,
        name: "Kuzen Travel".into(),
        tax_number: None,
        tax_office: None,
        contact_name: None,
        phone: None,
        email: None,
        address: None,
        balance_cents: 0,
        currency: "TRY".into(),
        credit_limit_cents: 0,
        is_active: true,
        notes: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
        deleted_at: None,
    };
    // zero limit = no credit extended
    assert!(a.is_within_credit_limit());
    a.balance_cents = 1;
    assert!(!a.is_within_credit_limit());
    a.credit_limit_cents = 50_000;
    assert!(a.is_within_credit_limit());
    a.balance_cents = 60_000;
    assert!(!a.is_within_credit_limit());
}

#[test]
fn g6_einvoice_lifecycle_transitions() {
    use EInvoiceStatus::*;
    // legal forward path
    assert!(Draft.can_transition_to(Queued));
    assert!(Queued.can_transition_to(Sent));
    assert!(Sent.can_transition_to(Accepted));
    assert!(Sent.can_transition_to(Rejected));
    assert!(Draft.can_transition_to(Cancelled));
    assert!(Queued.can_transition_to(Cancelled));
    // illegal shortcuts
    assert!(!Draft.can_transition_to(Accepted));
    assert!(!Draft.can_transition_to(Sent));
    assert!(!Queued.can_transition_to(Accepted));
    // terminal states are sticky
    assert!(Accepted.is_terminal());
    assert!(Rejected.is_terminal());
    assert!(Cancelled.is_terminal());
    assert!(!Accepted.can_transition_to(Queued));
    assert!(!Rejected.can_transition_to(Sent));
}

#[test]
fn g6_einvoice_totals_and_vat_rate() {
    let inv = EInvoice {
        id: uuid::Uuid::now_v7(),
        hotel_id: 1,
        reservation_id: None,
        agency_id: None,
        invoice_number: "INV-2026-000001".into(),
        scenario: "TEMELFATURA".into(),
        invoice_type: "SATIS".into(),
        uuid_gib: None,
        tax_number: None,
        status: "draft".to_string(),
        net_cents: 100_000,
        vat_cents: 10_000,
        total_cents: 110_000,
        currency: "TRY".into(),
        ubl_xml: None,
        error_message: None,
        issued_at: None,
        created_by: None,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    assert!(inv.totals_consistent());
    assert_eq!(inv.vat_rate_bps(), 1_000); // 10%
    assert_eq!(inv.kind(), Some(EInvoiceStatus::Draft));

    // inconsistent totals are rejected
    let bad = EInvoice {
        net_cents: 100,
        vat_cents: 20,
        total_cents: 119,
        ..inv.clone()
    };
    assert!(!bad.totals_consistent());
}

// ---------------------------------------------------------------------------
// Database-level verification of migration 0009 (skipped without a DB).
// ---------------------------------------------------------------------------

mod db {
    use super::*;

    fn db_url() -> Option<String> {
        std::env::var("TEST_DATABASE_URL")
            .or_else(|_| std::env::var("DATABASE_URL"))
            .ok()
            .filter(|s| !s.is_empty())
    }

    #[tokio::test]
    async fn g6_migration_0009_creates_site2_tables() {
        let url = match db_url() {
            Some(u) => u,
            None => {
                eprintln!("skipping g6_migration_0009_creates_site2_tables: DATABASE_URL unset");
                return;
            }
        };
        let pool = sqlx::PgPool::connect(&url).await.unwrap();
        // each of the new tables exists and is tenant-scoped
        for table in [
            "agencies",
            "currency_rates",
            "stock_items",
            "stock_movements",
            "expenses",
            "e_invoices",
        ] {
            let exists: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM information_schema.tables WHERE table_schema = 'public' AND table_name = $1",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(exists, 1, "table {table} missing after migration 0009");

            let rls: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM pg_tables WHERE tablename = $1 AND rowsecurity",
            )
            .bind(table)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert_eq!(rls, 1, "table {table} is not RLS-enabled");
        }

        // new permissions seeded
        for perm in [
            "invoice_manage",
            "stock_manage",
            "expense_manage",
            "agency_manage",
        ] {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM role_permissions WHERE permission = $1 AND allowed",
            )
            .bind(perm)
            .fetch_one(&pool)
            .await
            .unwrap();
            assert!(count >= 2, "permission {perm} not seeded");
        }
    }
}
