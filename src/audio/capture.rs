//! Microphone input capture using cpal.

use super::traits::AudioSource;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use cpal::{FromSample, Sample};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Error type for audio capture.
#[derive(Debug, thiserror::Error)]
pub enum CaptureError {
    #[error("No input device available")]
    NoInputDevice,
    #[error("No output device available")]
    NoOutputDevice,
    #[error("Failed to get device config: {0}")]
    ConfigError(#[from] cpal::DefaultStreamConfigError),
    #[error("Failed to build stream: {0}")]
    BuildStreamError(#[from] cpal::BuildStreamError),
    #[error("Failed to play stream: {0}")]
    PlayStreamError(#[from] cpal::PlayStreamError),
}

/// Retained capture window for pitch detection. Sized in *time* so the sample
/// count tracks the device rate: the old fixed 22050-sample cap held only
/// 0.459 s at the 48 kHz device default, capping any future bass window.
const RETAINED_WINDOW: Duration = Duration::from_millis(500);

/// Samples needed to hold `window` of audio at `sample_rate`.
fn window_samples(sample_rate: u32, window: Duration) -> usize {
    (sample_rate as f64 * window.as_secs_f64()).round() as usize
}

/// Analysis frame for the energy-based onset detector (~5.8 ms at 44.1 kHz).
const ONSET_FRAME_LEN: usize = 256;
/// A frame is a strike onset when its short-term energy jumps to at least this
/// multiple of the previous frame's energy.
const ONSET_RISE: f32 = 4.0;
/// ...and also clears this absolute energy floor, so quiet noise-floor ratios
/// (0 -> tiny) don't register as strikes.
const ONSET_ENERGY_FLOOR: f32 = 1e-4;

/// Index of the first frame whose short-term energy rises sharply above the
/// previous frame — the attack of a strike. Returns `None` when there is no
/// clear energy edge (silence, or a steady sustained tone).
///
/// Consumes an iterator so it can scan a `VecDeque` in logical order without
/// allocating or making it contiguous.
fn energy_onset(samples: impl Iterator<Item = f32>, frame_len: usize) -> Option<usize> {
    if frame_len == 0 {
        return None;
    }

    let mut prev_energy: Option<f32> = None;
    let mut frame_start = 0usize;
    let mut idx = 0usize;
    let mut sum_sq = 0.0f32;
    let mut count = 0usize;

    for s in samples {
        sum_sq += s * s;
        count += 1;
        idx += 1;

        if count == frame_len {
            let energy = sum_sq / frame_len as f32;
            if let Some(prev) = prev_energy {
                if energy > ONSET_ENERGY_FLOOR && energy >= ONSET_RISE * prev {
                    return Some(frame_start);
                }
            }
            prev_energy = Some(energy);
            frame_start = idx;
            sum_sq = 0.0;
            count = 0;
        }
    }

    None
}

/// Shared buffer for audio samples.
struct SharedBuffer {
    samples: VecDeque<f32>,
    /// Flag to indicate new samples are available.
    new_data: bool,
    /// Ring-buffer ceiling in samples, sized from the device rate in `new`.
    max_samples: usize,
}

/// Downmix interleaved frames to mono f32 and append them to the shared
/// buffer, trimming the front so at most `buf.max_samples` are retained.
fn push_mono_frames<T>(buf: &mut SharedBuffer, data: &[T], channels: usize)
where
    T: Sample,
    f32: FromSample<T>,
{
    for frame in data.chunks(channels) {
        let mono = frame.iter().map(|&s| f32::from_sample(s)).sum::<f32>() / channels as f32;
        buf.samples.push_back(mono);
    }

    // Keep the retained window at its duration-sized ceiling.
    if buf.samples.len() > buf.max_samples {
        let excess = buf.samples.len() - buf.max_samples;
        buf.samples.drain(0..excess);
    }

    buf.new_data = true;
}

/// Copy up to `buffer.len()` samples beginning at logical index `start`
/// (clamped to the deque length) into `buffer`. Returns the number written.
///
/// NOTE: the deque may wrap; copy from both backing slices instead of
/// make_contiguous so the read path never shuffles elements.
fn copy_from(samples: &VecDeque<f32>, start: usize, buffer: &mut [f32]) -> usize {
    let available = samples.len();
    if start >= available {
        return 0;
    }
    let to_read = buffer.len().min(available - start);
    if to_read == 0 {
        return 0;
    }

    let (front, back) = samples.as_slices();
    if start < front.len() {
        let n = (front.len() - start).min(to_read);
        buffer[..n].copy_from_slice(&front[start..start + n]);
        buffer[n..to_read].copy_from_slice(&back[..to_read - n]);
    } else {
        let bstart = start - front.len();
        buffer[..to_read].copy_from_slice(&back[bstart..bstart + to_read]);
    }

    to_read
}

/// Copy the most recent samples (sliding window) into `buffer`.
fn copy_latest(samples: &VecDeque<f32>, buffer: &mut [f32]) -> usize {
    let start = samples.len().saturating_sub(buffer.len());
    copy_from(samples, start, buffer)
}

/// Fill an interleaved output buffer from queued mono samples, zero-filling
/// once the queue runs dry.
// PERF: pop_front is O(1); the previous Vec::remove(0) memmoved the entire
// remaining queue on every frame — O(n^2) inside the real-time callback.
fn fill_output_frames<T>(data: &mut [T], channels: usize, queued: &mut VecDeque<f32>)
where
    T: Sample + FromSample<f32>,
{
    for frame in data.chunks_mut(channels) {
        let sample = T::from_sample(queued.pop_front().unwrap_or(0.0));
        for s in frame.iter_mut() {
            *s = sample;
        }
    }
}

/// Microphone capture using the system's default input device.
pub struct MicCapture {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<SharedBuffer>>,
    error: Arc<Mutex<Option<String>>>,
    sample_rate: u32,
    max_samples: usize,
}

impl MicCapture {
    /// Create a new microphone capture instance.
    pub fn new() -> Result<Self, CaptureError> {
        let host = cpal::default_host();

        let device = host
            .default_input_device()
            .ok_or(CaptureError::NoInputDevice)?;

        let config = device.default_input_config()?;
        let sample_rate = config.sample_rate().0;
        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();

        let max_samples = window_samples(sample_rate, RETAINED_WINDOW);
        let buffer = Arc::new(Mutex::new(SharedBuffer {
            samples: VecDeque::with_capacity(max_samples),
            new_data: false,
            max_samples,
        }));
        let error = Arc::new(Mutex::new(None));

        let buf = Arc::clone(&buffer);
        let err = Arc::clone(&error);

        use cpal::SampleFormat as Sf;
        let stream = match sample_format {
            Sf::I8 => Self::build_stream::<i8>(&device, &stream_config, buf, err),
            Sf::I16 => Self::build_stream::<i16>(&device, &stream_config, buf, err),
            Sf::I32 => Self::build_stream::<i32>(&device, &stream_config, buf, err),
            Sf::I64 => Self::build_stream::<i64>(&device, &stream_config, buf, err),
            Sf::U8 => Self::build_stream::<u8>(&device, &stream_config, buf, err),
            Sf::U16 => Self::build_stream::<u16>(&device, &stream_config, buf, err),
            Sf::U32 => Self::build_stream::<u32>(&device, &stream_config, buf, err),
            Sf::U64 => Self::build_stream::<u64>(&device, &stream_config, buf, err),
            Sf::F32 => Self::build_stream::<f32>(&device, &stream_config, buf, err),
            Sf::F64 => Self::build_stream::<f64>(&device, &stream_config, buf, err),
            // NOTE: SampleFormat is #[non_exhaustive]; anything cpal adds
            // later lands here until an arm exists for it.
            _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
        }?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            buffer,
            error,
            sample_rate,
            max_samples,
        })
    }

    /// Number of samples retained in the capture ring buffer.
    pub fn retained_samples(&self) -> usize {
        self.max_samples
    }

    /// Wall-clock audio retained in the ring buffer at the device sample rate.
    pub fn retained_duration(&self) -> Duration {
        Duration::from_secs_f64(self.max_samples as f64 / self.sample_rate as f64)
    }

    /// Onset-aware read for long-window (bass) analysis: copy the retained
    /// window starting `skip` after the most recent detected strike so the
    /// attack transient doesn't bias a long estimate. Falls back to the plain
    /// latest window when no onset is present. Returns samples written.
    ///
    /// PERF: onset scanning and the copy run here on the read thread, never in
    /// the cpal callback — the audio hot path stays allocation- and scan-free.
    pub fn read_after_onset(&mut self, buffer: &mut [f32], skip: Duration) -> usize {
        let mut buf = self.buffer.lock().unwrap();

        if !buf.new_data {
            return 0;
        }

        let start = match energy_onset(buf.samples.iter().copied(), ONSET_FRAME_LEN) {
            Some(onset) => {
                let skip_samples = window_samples(self.sample_rate, skip);
                (onset + skip_samples).min(buf.samples.len())
            }
            None => buf.samples.len().saturating_sub(buffer.len()),
        };

        let to_read = copy_from(&buf.samples, start, buffer);
        buf.new_data = false;
        to_read
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<SharedBuffer>>,
        error: Arc<Mutex<Option<String>>>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample,
        f32: FromSample<T>,
    {
        let channels = config.channels as usize;

        device.build_input_stream(
            config,
            move |data: &[T], _: &cpal::InputCallbackInfo| {
                let mut buf = buffer.lock().unwrap();
                push_mono_frames(&mut buf, data, channels);
            },
            move |err| {
                // NOTE: no eprintln! here — the TUI owns the terminal (raw
                // mode + alternate screen), so stream errors are parked for
                // the main loop to poll via `take_error`.
                *error.lock().unwrap() = Some(err.to_string());
            },
            None,
        )
    }

    /// Take the most recent stream error reported by the audio backend, if
    /// any (e.g. the microphone was unplugged mid-session).
    ///
    /// The cpal error callback stores errors here so the UI can surface them
    /// instead of writing to stderr while the terminal is in raw mode.
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }
}

impl AudioSource for MicCapture {
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize {
        let mut buf = self.buffer.lock().unwrap();

        // Only return samples if we have new data
        if !buf.new_data {
            return 0;
        }

        // Copy the most recent samples (sliding window)
        let to_read = copy_latest(&buf.samples, buffer);

        buf.new_data = false;
        to_read
    }

    fn sample_rate(&self) -> u32 {
        self.sample_rate
    }
}

/// Audio output sink using cpal.
pub struct AudioOutput {
    _stream: cpal::Stream,
    buffer: Arc<Mutex<VecDeque<f32>>>,
    error: Arc<Mutex<Option<String>>>,
    sample_rate: u32,
}

impl AudioOutput {
    /// Create a new audio output instance.
    pub fn new() -> Result<Self, CaptureError> {
        let host = cpal::default_host();

        let device = host
            .default_output_device()
            .ok_or(CaptureError::NoOutputDevice)?;

        let config = device.default_output_config()?;
        let sample_rate = config.sample_rate().0;
        let sample_format = config.sample_format();
        let stream_config: cpal::StreamConfig = config.into();

        let buffer: Arc<Mutex<VecDeque<f32>>> = Arc::new(Mutex::new(VecDeque::new()));
        let error = Arc::new(Mutex::new(None));
        let buf = Arc::clone(&buffer);
        let err = Arc::clone(&error);

        use cpal::SampleFormat as Sf;
        let stream = match sample_format {
            Sf::I8 => Self::build_stream::<i8>(&device, &stream_config, buf, err),
            Sf::I16 => Self::build_stream::<i16>(&device, &stream_config, buf, err),
            Sf::I32 => Self::build_stream::<i32>(&device, &stream_config, buf, err),
            Sf::I64 => Self::build_stream::<i64>(&device, &stream_config, buf, err),
            Sf::U8 => Self::build_stream::<u8>(&device, &stream_config, buf, err),
            Sf::U16 => Self::build_stream::<u16>(&device, &stream_config, buf, err),
            Sf::U32 => Self::build_stream::<u32>(&device, &stream_config, buf, err),
            Sf::U64 => Self::build_stream::<u64>(&device, &stream_config, buf, err),
            Sf::F32 => Self::build_stream::<f32>(&device, &stream_config, buf, err),
            Sf::F64 => Self::build_stream::<f64>(&device, &stream_config, buf, err),
            _ => Err(cpal::BuildStreamError::StreamConfigNotSupported),
        }?;

        stream.play()?;

        Ok(Self {
            _stream: stream,
            buffer,
            error,
            sample_rate,
        })
    }

    fn build_stream<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<VecDeque<f32>>>,
        error: Arc<Mutex<Option<String>>>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::SizedSample + FromSample<f32>,
    {
        let channels = config.channels as usize;

        device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                let mut buf = buffer.lock().unwrap();
                fill_output_frames(data, channels, &mut buf);
            },
            move |err| {
                // NOTE: no eprintln! here — the beep path runs while the TUI
                // owns the terminal (raw mode + alternate screen), so stream
                // errors are parked for the caller to poll via `take_error`.
                *error.lock().unwrap() = Some(err.to_string());
            },
            None,
        )
    }

    /// Take the most recent stream error reported by the audio backend, if
    /// any. Callers poll this instead of receiving errors on stderr: the
    /// interactive loop routes it into the status line, the `reference`
    /// subcommand prints it after playback.
    pub fn take_error(&self) -> Option<String> {
        self.error.lock().unwrap().take()
    }

    /// Queue samples for playback.
    pub fn queue(&self, samples: &[f32]) {
        let mut buf = self.buffer.lock().unwrap();
        buf.extend(samples.iter().copied());
    }

    /// Get the sample rate.
    pub fn sample_rate(&self) -> u32 {
        self.sample_rate
    }

    /// Play a sine wave at the given frequency for the given duration.
    pub fn play_sine(&self, frequency: f32, duration: f32) -> anyhow::Result<()> {
        let num_samples = (self.sample_rate as f32 * duration) as usize;
        let mut samples = Vec::with_capacity(num_samples);

        for i in 0..num_samples {
            let t = i as f32 / self.sample_rate as f32;
            let sample = 0.3 * (2.0 * std::f32::consts::PI * frequency * t).sin();
            samples.push(sample);
        }

        self.queue(&samples);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shared_buffer_with_cap(max_samples: usize) -> SharedBuffer {
        SharedBuffer {
            samples: VecDeque::new(),
            new_data: false,
            max_samples,
        }
    }

    fn empty_shared_buffer() -> SharedBuffer {
        // Default to the legacy 0.5 s @ 44.1 kHz cap so downmix tests that
        // don't care about trimming keep their old headroom.
        shared_buffer_with_cap(window_samples(44_100, RETAINED_WINDOW))
    }

    #[test]
    fn test_max_analysis_window_fits_retained_capture() {
        // The bass analysis window (pitch::PitchDetector::MAX_WINDOW) must
        // never exceed what capture retains, or detect_for_target could ask
        // for more audio than the ring buffer holds.
        assert!(
            crate::audio::pitch::PitchDetector::MAX_WINDOW <= RETAINED_WINDOW,
            "MAX_WINDOW {:?} exceeds retained capture {:?}",
            crate::audio::pitch::PitchDetector::MAX_WINDOW,
            RETAINED_WINDOW
        );
    }

    #[test]
    fn test_window_samples_scales_with_sample_rate() {
        // The old fixed 22050-sample cap held only 0.459 s at 48 kHz; sizing
        // from a Duration must hold a full 0.5 s at either rate.
        assert_eq!(window_samples(44_100, RETAINED_WINDOW), 22_050);
        assert_eq!(window_samples(48_000, RETAINED_WINDOW), 24_000);
    }

    #[test]
    fn test_push_mono_frames_trims_to_configured_window() {
        // A 48 kHz buffer must retain 0.5 s = 24000 samples, not the legacy
        // 22050 ceiling that dropped ~41 ms of every window at 48 kHz.
        let cap = window_samples(48_000, RETAINED_WINDOW);
        assert_eq!(cap, 24_000);

        let mut buf = shared_buffer_with_cap(cap);
        push_mono_frames(&mut buf, &vec![0.0f32; cap + 500], 1);

        assert_eq!(buf.samples.len(), cap);
    }

    #[test]
    fn test_push_mono_frames_downmixes_i16_stereo() {
        let mut buf = empty_shared_buffer();

        // Frame 1: L/R cancel out; frame 2: L == R == half scale.
        push_mono_frames(&mut buf, &[16384i16, -16384, 16384, 16384], 2);

        assert_eq!(buf.samples.len(), 2);
        assert!(buf.samples[0].abs() < 1e-6, "got {}", buf.samples[0]);
        assert!(
            (buf.samples[1] - 0.5).abs() < 1e-6,
            "got {}",
            buf.samples[1]
        );
        assert!(buf.new_data);
    }

    #[test]
    fn test_push_mono_frames_converts_u16() {
        let mut buf = empty_shared_buffer();

        // u16 is offset-binary: 32768 is the origin (0.0).
        push_mono_frames(&mut buf, &[32768u16, 49152], 1);

        assert!(buf.samples[0].abs() < 1e-6, "got {}", buf.samples[0]);
        assert!(
            (buf.samples[1] - 0.5).abs() < 1e-6,
            "got {}",
            buf.samples[1]
        );
    }

    #[test]
    fn test_push_mono_frames_trims_to_window() {
        let cap = window_samples(44_100, RETAINED_WINDOW);
        let mut buf = shared_buffer_with_cap(cap);

        push_mono_frames(&mut buf, &vec![0.0f32; cap], 1);
        push_mono_frames(&mut buf, &[1.0f32, 2.0, 3.0], 1);

        assert_eq!(buf.samples.len(), cap);
        // Newest samples survive at the back; oldest were trimmed.
        assert_eq!(buf.samples[cap - 1], 3.0);
        assert_eq!(buf.samples[cap - 3], 1.0);
    }

    #[test]
    fn test_copy_latest_reads_most_recent_window() {
        let mut samples: VecDeque<f32> = VecDeque::with_capacity(8);
        // Push 12 values with a cap of 8 so the deque wraps internally.
        for i in 0..12 {
            if samples.len() == 8 {
                samples.pop_front();
            }
            samples.push_back(i as f32);
        }

        let mut out = [0.0f32; 4];
        assert_eq!(copy_latest(&samples, &mut out), 4);
        assert_eq!(out, [8.0, 9.0, 10.0, 11.0]);

        // Request more than available: gets everything.
        let mut big = [0.0f32; 16];
        assert_eq!(copy_latest(&samples, &mut big), 8);
        assert_eq!(&big[..8], &[4.0, 5.0, 6.0, 7.0, 8.0, 9.0, 10.0, 11.0]);
    }

    #[test]
    fn test_fill_output_frames_drains_and_zero_fills() {
        let mut queued: VecDeque<f32> = [0.5f32, -0.5].into_iter().collect();
        let mut data = [0.1f32; 8]; // 2 channels x 4 frames

        fill_output_frames(&mut data, 2, &mut queued);

        assert_eq!(&data[..4], &[0.5, 0.5, -0.5, -0.5]);
        assert_eq!(&data[4..], &[0.0, 0.0, 0.0, 0.0]);
        assert!(queued.is_empty());
    }

    #[test]
    fn test_fill_output_frames_converts_to_i16() {
        let mut queued: VecDeque<f32> = [0.5f32, -0.5].into_iter().collect();
        let mut data = [7i16; 6]; // 2 channels x 3 frames

        fill_output_frames(&mut data, 2, &mut queued);

        assert_eq!(data, [16384, 16384, -16384, -16384, 0, 0]);
    }

    fn rms(samples: &[f32]) -> f32 {
        if samples.is_empty() {
            return 0.0;
        }
        (samples.iter().map(|s| s * s).sum::<f32>() / samples.len() as f32).sqrt()
    }

    #[test]
    fn test_energy_onset_locates_the_strike() {
        // 4000 samples of silence, then a loud 440 Hz strike. The onset must
        // land within one analysis frame of the true attack, not before it.
        let mut sig = vec![0.0f32; 4000];
        for i in 0..8000 {
            let t = i as f32 / 44_100.0;
            sig.push(0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin());
        }

        let onset = energy_onset(sig.iter().copied(), ONSET_FRAME_LEN).expect("onset detected");

        assert!(
            onset <= 4000 && 4000 - onset <= ONSET_FRAME_LEN,
            "onset {} not within one frame before the true attack at 4000",
            onset
        );
    }

    #[test]
    fn test_energy_onset_none_on_steady_signal() {
        // A steady tone has no energy edge, so no onset.
        let mut sig = Vec::new();
        for i in 0..8000 {
            let t = i as f32 / 44_100.0;
            sig.push(0.4 * (2.0 * std::f32::consts::PI * 440.0 * t).sin());
        }
        assert!(energy_onset(sig.iter().copied(), ONSET_FRAME_LEN).is_none());
    }

    #[test]
    fn test_onset_aware_read_skips_stale_pre_onset_samples() {
        // Buffer holds a stale/decayed region (4000 silent samples) followed
        // by a fresh 8000-sample strike. A naive latest-window read long
        // enough for bass reaches back across the onset and blends in the
        // stale samples; an onset-aware read that skips 50 ms past the strike
        // sees only the fresh note, so its RMS stays at the pure-tone level.
        let mut dq: VecDeque<f32> = VecDeque::new();
        for _ in 0..4000 {
            dq.push_back(0.0);
        }
        for i in 0..8000 {
            let t = i as f32 / 44_100.0;
            dq.push_back(0.5 * (2.0 * std::f32::consts::PI * 440.0 * t).sin());
        }

        let onset = energy_onset(dq.iter().copied(), ONSET_FRAME_LEN).expect("onset");
        let skip = window_samples(44_100, Duration::from_millis(50)); // 2205
        let start = onset + skip;

        let mut aware = vec![0.0f32; 4000];
        let n_aware = copy_from(&dq, start, &mut aware);
        let rms_aware = rms(&aware[..n_aware]);

        // Naive long window spans the whole buffer, back across the onset.
        let mut naive = vec![0.0f32; dq.len()];
        let n_naive = copy_latest(&dq, &mut naive);
        let rms_naive = rms(&naive[..n_naive]);

        assert!(
            rms_aware > 0.34,
            "onset-aware window should be a clean tone (RMS ~0.354), got {}",
            rms_aware
        );
        assert!(
            rms_naive < rms_aware,
            "naive window ({}) should be diluted by the stale pre-onset region vs onset-aware ({})",
            rms_naive,
            rms_aware
        );
    }

    #[test]
    fn test_copy_from_reads_arbitrary_offset() {
        let dq: VecDeque<f32> = (0..10).map(|i| i as f32).collect();
        let mut out = [0.0f32; 3];
        assert_eq!(copy_from(&dq, 4, &mut out), 3);
        assert_eq!(out, [4.0, 5.0, 6.0]);

        // Start past the end reads nothing.
        assert_eq!(copy_from(&dq, 20, &mut out), 0);
    }
}
