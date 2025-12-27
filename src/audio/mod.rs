//! Audio capture, pitch detection, and reference tone generation.

pub mod capture;
pub mod pitch;
pub mod reference;
pub mod traits;

pub use capture::MicCapture;
pub use pitch::PitchDetector;
pub use reference::ReferenceTone;
pub use traits::{AudioSink, AudioSource};
