pub mod api_types;
pub mod guest_identity;
pub mod key_rotation;
pub mod models;
pub mod roles;
pub mod sync;
pub mod validation;

// Re-exports for convenience
pub use api_types::*;
pub use models::*;
pub use roles::*;
pub use sync::*;
pub use validation::*;

pub fn version() -> &'static str {
    "0.1.0"
}

