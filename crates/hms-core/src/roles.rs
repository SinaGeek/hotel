use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Admin,
    Manager,
    Reception,
    Housekeeping,
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
}

impl Role {
    pub fn is_platform_admin(&self) -> bool {
        matches!(self, Role::Admin)
    }

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
            ),
            Role::Housekeeping => matches!(
                perm,
                Permission::HousekeepingView | Permission::HousekeepingUpdateStatus
            ),
        }
    }

    /// Housekeeping cannot see guest names, prices, or payments
    pub fn can_see_financials_and_guests(&self) -> bool {
        !matches!(self, Role::Housekeeping)
    }
}
