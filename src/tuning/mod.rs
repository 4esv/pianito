//! Tuning logic, temperament calculations, and session management.

pub mod notes;
pub mod order;
pub mod session;
pub mod stretch;
pub mod temperament;

pub use notes::{Note, NOTES, NOTE_COUNT};
pub use order::TuningOrder;
pub use session::{CompletedNote, Session, TuningMode};
pub use stretch::StretchCurve;
pub use temperament::Temperament;
