//! Permission matrix test (README §6): every role × every permission must
//! match the spec. The expected sets below are written by hand from §6 so the
//! test cannot drift with the implementation.
use hms_core::roles::{Permission, Role};

fn all() -> Vec<Permission> {
    use Permission::*;
    vec![
        HotelCreate,
        HotelEdit,
        HotelDisable,
        HotelViewAll,
        UserManage,
        UserView,
        RoomTypeManage,
        RoomManage,
        PriceRuleManage,
        ReservationView,
        ReservationCreate,
        ReservationEdit,
        ReservationCheckIn,
        ReservationCheckOut,
        ReservationCancel,
        FinancialView,
        PaymentRecord,
        PaymentRefund,
        FolioManage,
        HousekeepingView,
        HousekeepingUpdateStatus,
        ReportViewTenant,
        ReportViewPlatform,
        AuditLogView,
        GuestExport,
        GuestDelete,
        InvoiceManage,
        StockManage,
        ExpenseManage,
        AgencyManage,
    ]
}

/// README §6 — hardcoded expected grants per role
fn expected(role: Role) -> Vec<Permission> {
    use Permission::*;
    match role {
        // Platform admin: everything
        Role::Admin => all(),
        Role::Manager => vec![
            UserManage, UserView, RoomTypeManage, RoomManage, PriceRuleManage,
            ReservationView, ReservationCreate, ReservationEdit, ReservationCheckIn,
            ReservationCheckOut, ReservationCancel,
            FinancialView, PaymentRecord, PaymentRefund, FolioManage,
            HousekeepingView, HousekeepingUpdateStatus,
            ReportViewTenant, AuditLogView, GuestExport, GuestDelete,
            InvoiceManage, StockManage, ExpenseManage, AgencyManage,
        ],
        // Reception: report_view_tenant intentionally DEFAULT OFF (§6)
        Role::Reception => vec![
            UserView,
            ReservationView, ReservationCreate, ReservationEdit, ReservationCheckIn,
            ReservationCheckOut, ReservationCancel,
            FinancialView, PaymentRecord, PaymentRefund, FolioManage,
            HousekeepingView, HousekeepingUpdateStatus,
            InvoiceManage,
        ],
        Role::Housekeeping => vec![HousekeepingView, HousekeepingUpdateStatus],
    }
}

#[test]
fn every_role_times_permission_matches_spec() {
    for role in [Role::Admin, Role::Manager, Role::Reception, Role::Housekeeping] {
        let expected = expected(role);
        assert_eq!(
            all().len(),
            usize::from(all().len()),
            "sanity: all permissions list"
        );
        for perm in all() {
            assert_eq!(
                role.can(perm),
                expected.contains(&perm),
                "{role:?} x {}: expected can={} to match the matrix",
                perm.as_str(),
                expected.contains(&perm),
            );
        }
        // report_view_platform is platform-admin only
        assert!(
            !expected.contains(&Permission::ReportViewPlatform) || role == Role::Admin,
            "report_view_platform must be platform-admin only (role {role:?})"
        );
    }
}

#[test]
fn permission_wire_names_round_trip() {
    for perm in all() {
        let s = perm.as_str();
        let parsed = Permission::parse(s).expect("wire name must parse back");
        assert_eq!(parsed, perm, "round-trip failed for {s}");
    }
}

#[test]
fn housekeeping_cannot_see_financials_or_guests() {
    assert!(!Role::Housekeeping.can_see_financials_and_guests());
    assert!(Role::Manager.can_see_financials_and_guests());
    assert!(Role::Reception.can_see_financials_and_guests());
    assert!(Role::Admin.can_see_financials_and_guests());
}
