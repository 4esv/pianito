//! Tuning logic, temperament calculations, and session management.

pub mod notes;
pub mod order;
pub mod session;
pub mod stretch;
pub mod temperament;

pub use notes::{Note, NOTES};
pub use order::TuningOrder;
pub use session::Session;
pub use stretch::StretchCurve;
pub use temperament::Temperament;
