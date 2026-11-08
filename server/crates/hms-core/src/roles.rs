use serde::{Deserialize, Serialize};

/// Canonical list of permission strings used in `role_permissions` / overrides.
pub const PERMISSIONS: &[Permission] = {
    use Permission::*;
    &[
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
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Manager,
    Reception,
    Housekeeping,
    Accountant,
}

impl Role {
    /// DB / wire representation
    pub fn as_str(&self) -> &'static str {
        match self {
            Role::Admin => "admin",
            Role::Manager => "manager",
            Role::Reception => "reception",
            Role::Housekeeping => "housekeeping",
            Role::Accountant => "accountant",
        }
    }

    pub fn from_str(s: &str) -> Option<Role> {
        Some(match s {
            "admin" => Role::Admin,
            "manager" => Role::Manager,
            "reception" => Role::Reception,
            "housekeeping" => Role::Housekeeping,
            "accountant" => Role::Accountant,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    // Hotel & Platform
    HotelCreate,
    HotelEdit,
    HotelDisable,
    HotelViewAll,

    // Users
    UserManage,
    UserView,

    // Inventory & Catalog
    RoomTypeManage,
    RoomManage,
    PriceRuleManage,

    // Reservations
    ReservationView,
    ReservationCreate,
    ReservationEdit,
    ReservationCheckIn,
    ReservationCheckOut,
    ReservationCancel,

    // Financial & Folio
    FinancialView,
    PaymentRecord,
    PaymentRefund,
    FolioManage,

    // Housekeeping
    HousekeepingView,
    HousekeepingUpdateStatus,

    // Reports
    ReportViewTenant,
    ReportViewPlatform,

    // Audit
    AuditLogView,
    // Guest data (GDPR)
    GuestExport,
    GuestDelete,
    // Site-2 modules (G6): e-invoice, stock, expenses, agencies, FX
    InvoiceManage,
    StockManage,
    ExpenseManage,
    AgencyManage,
}

impl Permission {
    /// DB / wire representation (snake_case)
    pub fn as_str(&self) -> &'static str {
        match self {
            Permission::HotelCreate => "hotel_create",
            Permission::HotelEdit => "hotel_edit",
            Permission::HotelDisable => "hotel_disable",
            Permission::HotelViewAll => "hotel_view_all",
            Permission::UserManage => "user_manage",
            Permission::UserView => "user_view",
            Permission::RoomTypeManage => "room_type_manage",
            Permission::RoomManage => "room_manage",
            Permission::PriceRuleManage => "price_rule_manage",
            Permission::ReservationView => "reservation_view",
            Permission::ReservationCreate => "reservation_create",
            Permission::ReservationEdit => "reservation_edit",
            Permission::ReservationCheckIn => "reservation_check_in",
            Permission::ReservationCheckOut => "reservation_check_out",
            Permission::ReservationCancel => "reservation_cancel",
            Permission::FinancialView => "financial_view",
            Permission::PaymentRecord => "payment_record",
            Permission::PaymentRefund => "payment_refund",
            Permission::FolioManage => "folio_manage",
            Permission::HousekeepingView => "housekeeping_view",
            Permission::HousekeepingUpdateStatus => "housekeeping_update_status",
            Permission::ReportViewTenant => "report_view_tenant",
            Permission::ReportViewPlatform => "report_view_platform",
            Permission::AuditLogView => "audit_log_view",
            Permission::GuestExport => "guest_export",
            Permission::GuestDelete => "guest_delete",
            Permission::InvoiceManage => "invoice_manage",
            Permission::StockManage => "stock_manage",
            Permission::ExpenseManage => "expense_manage",
            Permission::AgencyManage => "agency_manage",
        }
    }

    pub fn parse(s: &str) -> Option<Permission> {
        Some(match s {
            "hotel_create" => Permission::HotelCreate,
            "hotel_edit" => Permission::HotelEdit,
            "hotel_disable" => Permission::HotelDisable,
            "hotel_view_all" => Permission::HotelViewAll,
            "user_manage" => Permission::UserManage,
            "user_view" => Permission::UserView,
            "room_type_manage" => Permission::RoomTypeManage,
            "room_manage" => Permission::RoomManage,
            "price_rule_manage" => Permission::PriceRuleManage,
            "reservation_view" => Permission::ReservationView,
            "reservation_create" => Permission::ReservationCreate,
            "reservation_edit" => Permission::ReservationEdit,
            "reservation_check_in" => Permission::ReservationCheckIn,
            "reservation_check_out" => Permission::ReservationCheckOut,
            "reservation_cancel" => Permission::ReservationCancel,
            "financial_view" => Permission::FinancialView,
            "payment_record" => Permission::PaymentRecord,
            "payment_refund" => Permission::PaymentRefund,
            "folio_manage" => Permission::FolioManage,
            "housekeeping_view" => Permission::HousekeepingView,
            "housekeeping_update_status" => Permission::HousekeepingUpdateStatus,
            "report_view_tenant" => Permission::ReportViewTenant,
            "report_view_platform" => Permission::ReportViewPlatform,
            "audit_log_view" => Permission::AuditLogView,
            "guest_export" => Permission::GuestExport,
            "guest_delete" => Permission::GuestDelete,
            "invoice_manage" => Permission::InvoiceManage,
            "stock_manage" => Permission::StockManage,
            "expense_manage" => Permission::ExpenseManage,
            "agency_manage" => Permission::AgencyManage,
            _ => return None,
        })
    }
}

impl Role {
    pub fn is_platform_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }

    /// Hardcoded fallback matrix — the DB matrix in `role_permissions` +
    /// `hotel_permission_overrides` is authoritative (README §6).
    /// NOTE: reception's ReportViewTenant here mirrors the DB default (OFF) —
    /// the matrix is configurable per hotel via overrides.
    pub fn can(&self, perm: Permission) -> bool {
        match self {
            Role::Admin => true, // Platform admin has full permissions
            Role::Manager => matches!(
                perm,
                Permission::UserManage
                    | Permission::UserView
                    | Permission::RoomTypeManage
                    | Permission::RoomManage
                    | Permission::PriceRuleManage
                    | Permission::ReservationView
                    | Permission::ReservationCreate
                    | Permission::ReservationEdit
                    | Permission::ReservationCheckIn
                    | Permission::ReservationCheckOut
                    | Permission::ReservationCancel
                    | Permission::FinancialView
                    | Permission::PaymentRecord
                    | Permission::PaymentRefund
                    | Permission::FolioManage
                    | Permission::HousekeepingView
                    | Permission::HousekeepingUpdateStatus
                    | Permission::ReportViewTenant
                    | Permission::AuditLogView
                    | Permission::GuestExport
                    | Permission::GuestDelete
                    | Permission::InvoiceManage
                    | Permission::StockManage
                    | Permission::ExpenseManage
                    | Permission::AgencyManage
            ),
            Role::Reception => matches!(
                perm,
                Permission::UserView
                    | Permission::ReservationView
                    | Permission::ReservationCreate
                    | Permission::ReservationEdit
                    | Permission::ReservationCheckIn
                    | Permission::ReservationCheckOut
                    | Permission::ReservationCancel
                    | Permission::FinancialView
                    | Permission::PaymentRecord
                    | Permission::PaymentRefund
                    | Permission::FolioManage
                    | Permission::HousekeepingView
                    | Permission::HousekeepingUpdateStatus
                    | Permission::ReportViewTenant
                    | Permission::InvoiceManage
            ),
            Role::Housekeeping => matches!(
                perm,
                Permission::HousekeepingView | Permission::HousekeepingUpdateStatus
            ),
            Role::Accountant => matches!(
                perm,
                Permission::FinancialView
                    | Permission::PaymentRecord
                    | Permission::PaymentRefund
                    | Permission::FolioManage
                    | Permission::ReservationView
                    | Permission::ReportViewTenant
                    | Permission::GuestExport
            ),
        }
    }

    /// Housekeeping cannot see guest names, prices, or payments
    pub fn can_see_financials_and_guests(&self) -> bool {
        !matches!(self, Role::Housekeeping)
    }
}
