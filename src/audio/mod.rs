//! Audio capture, pitch detection, and reference tone generation.

pub mod capture;
pub mod pitch;
pub mod reference;
pub mod smoothing;
pub mod spectrum;
pub mod traits;
pub mod watchdog;
pub mod worker;

pub use capture::{AudioOutput, CaptureError, MicCapture, MicReader};
pub use pitch::{DetectError, PitchDetector, PitchResult};
pub use reference::ReferenceTone;
pub use smoothing::MedianFilter;
pub use spectrum::{Partial, PartialAnalyzer};
pub use traits::{AudioSink, AudioSource, TestAudioSink, TestAudioSource, WavAudioSource};
pub use watchdog::{has_signal, SilenceWatchdog};
pub use worker::{PitchUpdate, PitchWorker};
