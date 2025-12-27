//! Audio capture, pitch detection, and reference tone generation.

pub mod capture;
pub mod pitch;
pub mod reference;
pub mod traits;

pub use capture::{AudioOutput, CaptureError, MicCapture};
pub use pitch::{PitchDetector, PitchResult};
pub use reference::ReferenceTone;
pub use traits::{AudioSink, AudioSource, TestAudioSink, TestAudioSource, WavAudioSource};
