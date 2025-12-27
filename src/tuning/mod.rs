//! Tuning logic, temperament calculations, and session management.

pub mod notes;
pub mod order;
pub mod session;
pub mod stretch;
pub mod temperament;

pub use notes::{Note, NOTE_COUNT, NOTES};
pub use order::TuningOrder;
pub use session::{CompletedNote, Session, TuningMode};
pub use stretch::StretchCurve;
pub use temperament::Temperament;
