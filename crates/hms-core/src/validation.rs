use std::error::Error;
use std::fmt;

#[derive(Debug, PartialEq)]
pub enum ValidationError {
    InvalidDateRange,
    StayDurationExceeded(i64),
    DateTooFarInFuture,
    PriceMismatch { total: i64, sum: i64 },
    ClockSkewRejected(String),
    InvalidPax,
    EmptyField(&'static str),
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationError::InvalidDateRange =>
                write!(f, "Check-out date must be strictly after check-in date"),
            ValidationError::StayDurationExceeded(max) =>
                write!(f, "Stay duration cannot exceed {} days", max),
            ValidationError::DateTooFarInFuture =>
                write!(f, "Check-in date cannot be more than 2 years in the future"),
            ValidationError::PriceMismatch { total, sum } =>
                write!(f, "Total price {} does not match sum of daily rates {}", total, sum),
            ValidationError::ClockSkewRejected(msg) =>
                write!(f, "Client timestamp is skewed into the future beyond allowed drift: {}", msg),
            ValidationError::InvalidPax =>
                write!(f, "Invalid guest pax: at least 1 adult required"),
            ValidationError::EmptyField(field) =>
                write!(f, "Field {} cannot be empty", field),
        }
    }
}

impl Error for ValidationError {}
