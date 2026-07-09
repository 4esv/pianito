//! Pitch-detection worker thread (issue #28).
//!
//! YIN detection used to run on the render thread, gated behind the same
//! 50 ms poll that drove input handling - capping both redraw and pitch
//! updates at that cadence. This moves mic read + detection onto a
//! dedicated thread that runs at its own pace; the render loop reads
//! whatever the worker most recently produced and ticks independently.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use super::pitch::PitchDetector;
use super::smoothing::MedianFilter;
use super::traits::AudioSource;

/// A pitch reading (or its absence) delivered from the worker thread to the
/// render loop.
#[derive(Debug, Clone, PartialEq)]
pub enum PitchUpdate {
    /// A confident, median-filtered pitch reading, plus the raw samples that
    /// produced it. `App::capture_profiling_partials` needs those samples
    /// for its FFT partial fit and can only run on the render thread (it
    /// mutates `App`), so the worker hands them along rather than doing that
    /// analysis itself.
    Detected {
        /// Median-filtered detected frequency, in Hz.
        frequency: f32,
        /// Detector confidence for the raw (pre-filter) reading.
        confidence: f32,
        /// Raw audio window that produced this reading.
        samples: Vec<f32>,
    },
    /// No confident pitch for a sustained gap (see [`SilenceGate`]) -
    /// reported once per gap, not once per failed detection attempt.
    Silence,
}

/// How long a run of failed detections must persist before it counts as
/// real silence rather than one mistracked frame. Once decoupled from the
/// render thread's 50 ms poll, the worker can attempt detection far more
/// often than before, so a single missed frame (a struck note's attack
/// transient, a stray glitch) is now much more likely to happen in isolation.
/// `SilenceGate` keeps that from flickering the meter to "no reading"
/// between two real detections of the same held note.
const SILENCE_TIMEOUT: Duration = Duration::from_millis(120);

/// How long to sleep after a read that produced no new audio, so the worker
/// polls the mic's ring buffer instead of busy-waiting a full core while
/// idle between strikes.
const IDLE_POLL_INTERVAL: Duration = Duration::from_millis(3);

/// Debounces a raw stream of per-frame detection outcomes into "genuine
/// silence" events. `observe_*` takes `now` explicitly (rather than calling
/// `Instant::now()` itself) so it stays unit-testable without real sleeps.
struct SilenceGate {
    timeout: Duration,
    last_success: Option<Instant>,
    reported: bool,
}

impl SilenceGate {
    fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            last_success: None,
            reported: false,
        }
    }

    /// Record a failed detection at `now`. Returns `true` the first time the
    /// gap since the last confident detection reaches `timeout` - callers
    /// should treat that (and only that) as the signal to clear state and
    /// notify the render loop.
    fn observe_failure(&mut self, now: Instant) -> bool {
        let past_timeout = match self.last_success {
            Some(t) => now.duration_since(t) >= self.timeout,
            // Nothing has ever been detected: there is no "held note" to
            // debounce against, so the first failure already is silence.
            None => true,
        };
        if past_timeout && !self.reported {
            self.reported = true;
            return true;
        }
        false
    }

    /// Record a confident detection at `now`, re-arming the gate so the next
    /// gap gets its own report.
    fn observe_success(&mut self, now: Instant) {
        self.last_success = Some(now);
        self.reported = false;
    }
}

/// Drain every value currently queued on `rx`, keeping only the most recent
/// one. The render loop only ever wants the freshest reading: with the
/// worker and render running on independent cadences, whichever is slower
/// would otherwise fall behind a backlog of stale messages.
fn latest<T>(rx: &Receiver<T>) -> Option<T> {
    let mut last = None;
    while let Ok(value) = rx.try_recv() {
        last = Some(value);
    }
    last
}

/// Handle to the running pitch-detection worker thread.
///
/// The render loop calls [`Self::set_target`] once per tick (guided
/// detection needs the current note's target frequency) and
/// [`Self::latest_update`] to fetch whatever the worker has produced since
/// the last tick - never blocking, since render cadence must stay
/// independent of pitch cadence.
pub struct PitchWorker {
    results: Receiver<PitchUpdate>,
    warnings: Receiver<String>,
    target_tx: Sender<Option<f32>>,
    running: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

impl PitchWorker {
    /// Spawn the worker thread over `source`. `source` is read in a loop
    /// until the returned handle is dropped.
    pub fn spawn<S>(mut source: S) -> Self
    where
        S: AudioSource + Send + 'static,
    {
        let sample_rate = source.sample_rate();
        let (result_tx, results) = mpsc::channel();
        let (warning_tx, warnings) = mpsc::channel();
        let (target_tx, target_rx) = mpsc::channel::<Option<f32>>();
        let running = Arc::new(AtomicBool::new(true));
        let running_thread = Arc::clone(&running);

        let handle = thread::spawn(move || {
            let detector = PitchDetector::new(sample_rate);
            let mut audio_buffer = vec![0.0f32; PitchDetector::max_window_samples(sample_rate)];
            let mut filter = MedianFilter::new(MedianFilter::DEFAULT_WINDOW);
            let mut gate = SilenceGate::new(SILENCE_TIMEOUT);
            let mut target: Option<f32> = None;

            while running_thread.load(Ordering::Relaxed) {
                if let Some(err) = source.take_stream_error() {
                    // A disconnected receiver just means the render side
                    // shut down while this send was in flight; nothing to
                    // do but let the loop's `running` check catch up.
                    let _ = warning_tx.send(err);
                }

                if let Some(new_target) = latest(&target_rx) {
                    target = new_target;
                }

                let read = source.read_samples(&mut audio_buffer);
                if read == 0 {
                    thread::sleep(IDLE_POLL_INTERVAL);
                    continue;
                }

                let slice = &audio_buffer[..read];
                let detection = match target {
                    Some(t) => detector.detect_for_target(slice, t),
                    None => detector.detect(slice),
                };

                let now = Instant::now();
                match detection {
                    Ok(pitch_result) => {
                        let frequency = filter.push(pitch_result.frequency);
                        gate.observe_success(now);
                        let _ = result_tx.send(PitchUpdate::Detected {
                            frequency,
                            confidence: pitch_result.confidence,
                            samples: slice.to_vec(),
                        });
                    }
                    Err(_) => {
                        if gate.observe_failure(now) {
                            filter.clear();
                            let _ = result_tx.send(PitchUpdate::Silence);
                        }
                    }
                }
            }
        });

        Self {
            results,
            warnings,
            target_tx,
            running,
            handle: Some(handle),
        }
    }

    /// Set (or clear) the target frequency used for guided detection. Mirrors
    /// `App::current_target_freq`; cheap enough to call every render tick (an
    /// unbounded channel send, not a lock).
    pub fn set_target(&self, target: Option<f32>) {
        let _ = self.target_tx.send(target);
    }

    /// Take the freshest pitch update produced since the last call, if any.
    pub fn latest_update(&self) -> Option<PitchUpdate> {
        latest(&self.results)
    }

    /// Take the freshest audio-stream error reported since the last call, if
    /// any (e.g. the microphone was unplugged mid-session).
    pub fn take_audio_warning(&self) -> Option<String> {
        latest(&self.warnings)
    }
}

impl Drop for PitchWorker {
    fn drop(&mut self) {
        self.running.store(false, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            // Worst case this waits one `IDLE_POLL_INTERVAL` plus a detect
            // call - bounded, and only happens once, at shutdown.
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::audio::traits::TestAudioSource;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    // --- message-queue/rotation logic -----------------------------------

    #[test]
    fn latest_returns_none_when_nothing_sent() {
        let (_tx, rx) = mpsc::channel::<u32>();
        assert_eq!(latest(&rx), None);
    }

    #[test]
    fn latest_drains_the_queue_and_keeps_only_the_newest() {
        let (tx, rx) = mpsc::channel::<u32>();
        tx.send(1).unwrap();
        tx.send(2).unwrap();
        tx.send(3).unwrap();

        assert_eq!(latest(&rx), Some(3));
        // The queue is now empty - a second call sees nothing new.
        assert_eq!(latest(&rx), None);
    }

    // --- silence-timeout state machine -----------------------------------

    #[test]
    fn silence_gate_reports_silence_immediately_before_any_success() {
        let mut gate = SilenceGate::new(Duration::from_millis(100));
        assert!(gate.observe_failure(Instant::now()));
    }

    #[test]
    fn silence_gate_ignores_a_transient_failure_within_the_timeout() {
        let mut gate = SilenceGate::new(Duration::from_millis(100));
        let t0 = Instant::now();
        gate.observe_success(t0);

        // A single missed frame 10ms later is not yet "silence".
        assert!(!gate.observe_failure(t0 + Duration::from_millis(10)));
    }

    #[test]
    fn silence_gate_reports_exactly_once_per_gap() {
        let mut gate = SilenceGate::new(Duration::from_millis(100));
        let t0 = Instant::now();
        gate.observe_success(t0);

        assert!(gate.observe_failure(t0 + Duration::from_millis(150)));
        // Still silent, no new report until another success re-arms it.
        assert!(!gate.observe_failure(t0 + Duration::from_millis(200)));

        gate.observe_success(t0 + Duration::from_millis(210));
        assert!(gate.observe_failure(t0 + Duration::from_millis(400)));
    }

    // --- decoupling contract: worker -> channel -> render, independent
    // cadence -------------------------------------------------------------

    #[test]
    fn worker_delivers_a_detected_pitch_for_a_held_tone() {
        let source = TestAudioSource::sine(440.0, 1.0, 44_100);
        let worker = PitchWorker::spawn(source);

        let deadline = Instant::now() + Duration::from_secs(2);
        let mut seen = None;
        while Instant::now() < deadline {
            if let Some(PitchUpdate::Detected { frequency, .. }) = worker.latest_update() {
                seen = Some(frequency);
                break;
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let freq = seen.expect("worker should deliver a Detected update for a held A4");
        assert!((freq - 440.0).abs() < 2.0, "got {freq}");
    }

    #[test]
    fn worker_reports_silence_after_a_held_tone_goes_quiet() {
        // A real strike, decaying into real silence - not the source simply
        // running dry (that's a permanent 0-sample read, handled by the
        // idle-poll branch, and never reaches `SilenceGate` at all). Feed
        // continuous true-silence samples after the tone so the worker keeps
        // attempting detection and the gate's timeout has something to
        // measure against, exactly as a live mic does between strikes.
        let mut samples = TestAudioSource::sine(440.0, 0.5, 44_100).samples().to_vec();
        samples.extend(std::iter::repeat_n(0.0f32, 44_100 * 5));
        let source = TestAudioSource::new(samples, 44_100);
        let worker = PitchWorker::spawn(source);

        let deadline = Instant::now() + Duration::from_secs(5);
        let mut saw_detected = false;
        let mut saw_silence = false;
        while Instant::now() < deadline && !saw_silence {
            match worker.latest_update() {
                Some(PitchUpdate::Detected { .. }) => saw_detected = true,
                Some(PitchUpdate::Silence) => saw_silence = true,
                None => {}
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        assert!(
            saw_detected,
            "worker should detect the held tone before it goes quiet"
        );
        assert!(
            saw_silence,
            "worker should report Silence once the tone decays to true silence"
        );
    }
}
