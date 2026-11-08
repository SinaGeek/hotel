//! G6: site-2 deferred modules — e-invoice, stock/expenses, agencies, currency
//! exchange.
//!
//! These models are the domain layer for the four modules that were deferred in
//! `docs/feature-inventory.md` until a product decision. They are pure data +
//! invariants (no I/O) so both the server and the desktop mirror can reuse them.
//!
//! Money convention (same as the rest of HMS): integer minor units, ISO-4217
//! 3-letter currency. Exchange rates use *micro* units (1/1,000,000) so a
//! TRY→minor-unit conversion never loses precision before rounding.

use chrono::{DateTime, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

// ---------------------------------------------------------------- agencies

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct Agency {
    pub id: Uuid,
    pub hotel_id: i64,
    pub name: String,
    pub tax_number: Option<String>,
    pub tax_office: Option<String>,
    pub contact_name: Option<String>,
    pub phone: Option<String>,
    pub email: Option<String>,
    pub address: Option<String>,
    /// Positive = the agency owes the hotel.
    pub balance_cents: i64,
    pub currency: String,
    pub credit_limit_cents: i64,
    pub is_active: bool,
    pub notes: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

impl Agency {
    /// A receivable is within terms while the outstanding balance stays under
    /// the (non-zero) credit limit. A zero limit means "no credit extended".
    pub fn is_within_credit_limit(&self) -> bool {
        if self.credit_limit_cents <= 0 {
            return self.balance_cents <= 0;
        }
        self.balance_cents <= self.credit_limit_cents
    }
}

// ---------------------------------------------------------------- FX

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct CurrencyRate {
    pub id: Uuid,
    pub hotel_id: i64,
    pub currency: String,
    /// 1 unit of `currency` = rate_micro / 1_000_000 units of the base currency.
    pub rate_micro: i64,
    pub valid_on: NaiveDate,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

/// 1 unit of `currency` expressed in micro base units.
pub const MICRO: i64 = 1_000_000;

/// Convert an amount in `currency` minor units to base-currency minor units,
/// rounding half away from zero. `rate_micro` is the `CurrencyRate` value.
pub fn convert_to_base_cents(amount_cents: i64, rate_micro: i64) -> Result<i64, String> {
    if rate_micro <= 0 {
        return Err("rate_micro must be positive".into());
    }
    let numerator = (amount_cents as i128) * (rate_micro as i128);
    let denominator = MICRO as i128;
    // round half away from zero
    let abs = numerator.abs();
    let q = (abs + denominator / 2) / denominator;
    let signed = if numerator < 0 { -q } else { q };
    i64::try_from(signed).map_err(|_| "conversion overflow".to_string())
}

// ---------------------------------------------------------------- stock

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StockMovementType {
    /// Purchase / restock
    In,
    /// Consumption (minibar, linen)
    Out,
    /// Manual correction (stock count)
    Adjustment,
}

impl StockMovementType {
    pub fn as_str(&self) -> &'static str {
        match self {
            StockMovementType::In => "in",
            StockMovementType::Out => "out",
            StockMovementType::Adjustment => "adjustment",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "in" => StockMovementType::In,
            "out" => StockMovementType::Out,
            "adjustment" => StockMovementType::Adjustment,
            _ => return None,
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct StockItem {
    pub id: Uuid,
    pub hotel_id: i64,
    pub sku: Option<String>,
    pub name: String,
    pub unit: String,
    pub unit_cost_cents: i64,
    pub min_level: i32,
    pub currency: String,
    pub is_active: bool,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

/// Append-only stock movement. `movement_type` is the DB text value; use
/// [`StockMovement::kind`] for the typed enum (FromRow cannot decode a TEXT
/// column into a Rust enum without sqlx's `Type` derive on Postgres).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct StockMovement {
    pub id: Uuid,
    pub hotel_id: i64,
    pub stock_item_id: Uuid,
    pub movement_type: String,
    /// Signed: +in, -out, ±adjustment. Never zero (DB CHECK).
    pub quantity: i32,
    pub unit_cost_cents: i64,
    pub room_id: Option<Uuid>,
    pub reservation_id: Option<Uuid>,
    pub note: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}

impl StockMovement {
    /// Typed view of `movement_type`.
    pub fn kind(&self) -> Option<StockMovementType> {
        StockMovementType::parse(&self.movement_type)
    }
}

/// Validate the sign convention for a movement type.
pub fn validate_movement_sign(
    movement_type: StockMovementType,
    quantity: i32,
) -> Result<(), String> {
    if quantity == 0 {
        return Err("quantity must not be zero".into());
    }
    match movement_type {
        StockMovementType::In if quantity < 0 => Err("stock-in quantity must be positive".into()),
        StockMovementType::Out if quantity > 0 => Err("stock-out quantity must be negative".into()),
        _ => Ok(()),
    }
}

/// On-hand quantity from an append-only movement log.
pub fn on_hand(movements: &[StockMovement]) -> i64 {
    movements.iter().map(|m| m.quantity as i64).sum()
}

/// Items at or below their reorder level.
pub fn needs_restock(item: &StockItem, achieved: i64) -> bool {
    achieved <= item.min_level as i64
}

// ---------------------------------------------------------------- expenses

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct Expense {
    pub id: Uuid,
    pub hotel_id: i64,
    pub category: String,
    pub description: String,
    pub amount_cents: i64,
    pub currency: String,
    pub spent_on: NaiveDate,
    pub agency_id: Option<Uuid>,
    pub payment_type: Option<String>,
    pub invoice_number: Option<String>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    pub deleted_at: Option<DateTime<Utc>>,
}

// ---------------------------------------------------------------- e-invoice

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EInvoiceStatus {
    Draft,
    Queued,
    Sent,
    Accepted,
    Rejected,
    Cancelled,
}

impl EInvoiceStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            EInvoiceStatus::Draft => "draft",
            EInvoiceStatus::Queued => "queued",
            EInvoiceStatus::Sent => "sent",
            EInvoiceStatus::Accepted => "accepted",
            EInvoiceStatus::Rejected => "rejected",
            EInvoiceStatus::Cancelled => "cancelled",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "draft" => EInvoiceStatus::Draft,
            "queued" => EInvoiceStatus::Queued,
            "sent" => EInvoiceStatus::Sent,
            "accepted" => EInvoiceStatus::Accepted,
            "rejected" => EInvoiceStatus::Rejected,
            "cancelled" => EInvoiceStatus::Cancelled,
            _ => return None,
        })
    }

    /// Legal forward transitions for the GİB lifecycle. Terminal states are
    /// `Accepted`, `Rejected` and `Cancelled`.
    pub fn can_transition_to(self, next: EInvoiceStatus) -> bool {
        use EInvoiceStatus::*;
        match (self, next) {
            (Draft, Queued) | (Draft, Cancelled) => true,
            (Queued, Sent) | (Queued, Rejected) | (Queued, Cancelled) => true,
            (Sent, Accepted) | (Sent, Rejected) => true,
            _ => false,
        }
    }

    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            EInvoiceStatus::Accepted | EInvoiceStatus::Rejected | EInvoiceStatus::Cancelled
        )
    }
}

/// E-invoice document. `status` is the DB text value; use
/// [`EInvoice::kind`] for the typed lifecycle enum.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, sqlx::FromRow)]
pub struct EInvoice {
    pub id: Uuid,
    pub hotel_id: i64,
    pub reservation_id: Option<Uuid>,
    pub agency_id: Option<Uuid>,
    pub invoice_number: String,
    pub scenario: String,
    pub invoice_type: String,
    pub uuid_gib: Option<String>,
    pub tax_number: Option<String>,
    pub status: String,
    pub net_cents: i64,
    pub vat_cents: i64,
    pub total_cents: i64,
    pub currency: String,
    pub ubl_xml: Option<String>,
    pub error_message: Option<String>,
    pub issued_at: Option<DateTime<Utc>>,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl EInvoice {
    /// Typed view of `status`.
    pub fn kind(&self) -> Option<EInvoiceStatus> {
        EInvoiceStatus::parse(&self.status)
    }

    /// `total = net + vat` and the parts are non-negative. Enforced server-side
    /// both on create and on any later re-pricing.
    pub fn totals_consistent(&self) -> bool {
        self.net_cents >= 0
            && self.vat_cents >= 0
            && self.net_cents + self.vat_cents == self.total_cents
    }

    /// VAT rate in basis points derived from net/total (0 when net is zero).
    /// Used to pre-fill the invoice dialog from a folio total.
    pub fn vat_rate_bps(&self) -> i64 {
        if self.net_cents <= 0 {
            return 0;
        }
        ((self.vat_cents as i128) * 10_000 / (self.net_cents as i128)) as i64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn movement(t: StockMovementType, quantity: i32) -> StockMovement {
        StockMovement {
            id: Uuid::now_v7(),
            hotel_id: 4442,
            stock_item_id: Uuid::now_v7(),
            movement_type: t.as_str().to_string(),
            quantity,
            unit_cost_cents: 100,
            room_id: None,
            reservation_id: None,
            note: None,
            created_by: None,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn fx_conversion_rounds_half_away_from_zero() {
        // 100.00 EUR at 34.123456 TRY → 3412.3456 TRY = 341235 kuruş (rounded)
        assert_eq!(convert_to_base_cents(10_000, 34_123_456).unwrap(), 341_235);
        // negative amounts round symmetrically
        assert_eq!(
            convert_to_base_cents(-10_000, 34_123_456).unwrap(),
            -341_235
        );
        // identity rate
        assert_eq!(convert_to_base_cents(123_456, MICRO).unwrap(), 123_456);
        assert!(convert_to_base_cents(100, 0).is_err());
    }

    #[test]
    fn stock_sign_conventions_and_on_hand() {
        assert!(validate_movement_sign(StockMovementType::In, 10).is_ok());
        assert!(validate_movement_sign(StockMovementType::In, -10).is_err());
        assert!(validate_movement_sign(StockMovementType::Out, -3).is_ok());
        assert!(validate_movement_sign(StockMovementType::Out, 3).is_err());
        assert!(validate_movement_sign(StockMovementType::Adjustment, -1).is_ok());
        assert!(validate_movement_sign(StockMovementType::In, 0).is_err());

        let log = vec![
            movement(StockMovementType::In, 24),
            movement(StockMovementType::Out, -5),
            movement(StockMovementType::Adjustment, -2),
        ];
        assert_eq!(on_hand(&log), 17);
    }

    #[test]
    fn stock_restock_threshold() {
        let item = StockItem {
            id: Uuid::now_v7(),
            hotel_id: 1,
            sku: None,
            name: "Minibar water".into(),
            unit: "adet".into(),
            unit_cost_cents: 250,
            min_level: 10,
            currency: "TRY".into(),
            is_active: true,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            deleted_at: None,
        };
        assert!(needs_restock(&item, 10));
        assert!(needs_restock(&item, 3));
        assert!(!needs_restock(&item, 11));
    }

    #[test]
    fn agency_credit_limit() {
        let mut a = Agency {
            id: Uuid::now_v7(),
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
            created_at: Utc::now(),
            updated_at: Utc::now(),
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
    fn einvoice_lifecycle_and_totals() {
        use EInvoiceStatus::*;
        assert!(Draft.can_transition_to(Queued));
        assert!(Queued.can_transition_to(Sent));
        assert!(Sent.can_transition_to(Accepted));
        assert!(!Draft.can_transition_to(Accepted), "must go through sent");
        assert!(!Accepted.can_transition_to(Queued), "terminal");
        assert!(Rejected.is_terminal() && Cancelled.is_terminal());

        let inv = EInvoice {
            id: Uuid::now_v7(),
            hotel_id: 1,
            reservation_id: None,
            agency_id: None,
            invoice_number: "HMS2026000001".into(),
            scenario: "TEMELFATURA".into(),
            invoice_type: "SATIS".into(),
            uuid_gib: None,
            tax_number: None,
            status: Draft.as_str().to_string(),
            net_cents: 100_000,
            vat_cents: 10_000,
            total_cents: 110_000,
            currency: "TRY".into(),
            ubl_xml: None,
            error_message: None,
            issued_at: None,
            created_by: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        assert!(inv.totals_consistent());
        assert_eq!(inv.vat_rate_bps(), 1_000); // 10%
    }
}
