//! Temporal smoothing of the pitch-detection stream.

use std::collections::VecDeque;

/// Sliding-window median filter over detected frequencies.
///
/// Each audio frame is detected independently, so a single frame that
/// mistracks — an octave-off partial during a struck string's attack, a glitch
/// on a decaying tone — would jerk the tuning meter on its own. Taking the
/// median of the last few detections rejects such isolated outliers while
/// passing a sustained pitch through unchanged.
///
/// WARNING: a length-N median adds group delay. A *genuine* pitch change is not
/// fully reflected until `ceil(N/2)` new frames have arrived (about 300 ms at
/// N=5 with the app's 100 ms frames). That latency is the price of outlier
/// rejection, so keep N small: 5 outvotes a two-frame glitch — roughly the most
/// a string's attack transient produces — without feeling sluggish on a note
/// that is held for seconds while being tuned.
pub struct MedianFilter {
    window: usize,
    buf: VecDeque<f32>,
}

impl MedianFilter {
    /// Default window: small enough to stay responsive, large enough to outvote
    /// a two-frame octave glitch. See the type-level note on the latency
    /// tradeoff.
    pub const DEFAULT_WINDOW: usize = 5;

    /// Create a filter with the given window length (clamped to at least 1).
    pub fn new(window: usize) -> Self {
        let window = window.max(1);
        Self {
            window,
            buf: VecDeque::with_capacity(window),
        }
    }

    /// Push a new detection and return the smoothed (median) frequency of the
    /// current window. During warm-up (fewer than `window` samples buffered)
    /// the median is taken over whatever is available.
    pub fn push(&mut self, freq: f32) -> f32 {
        if self.buf.len() == self.window {
            self.buf.pop_front();
        }
        self.buf.push_back(freq);

        let mut sorted: Vec<f32> = self.buf.iter().copied().collect();
        sorted.sort_unstable_by(f32::total_cmp);
        sorted[sorted.len() / 2]
    }

    /// Reset the window. Call when detection is lost (silence between strikes)
    /// so a new strike starts clean instead of blending across the gap.
    pub fn clear(&mut self) {
        self.buf.clear();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rejects_octave_jump_outlier() {
        // A held A2 (~110 Hz) with one frame that mistracks an octave up. The
        // median must never let the reported pitch approach that 220 Hz spike.
        let mut filter = MedianFilter::new(5);
        let seq = [110.0, 110.5, 220.0, 109.5, 110.2];
        let out: Vec<f32> = seq.iter().map(|&x| filter.push(x)).collect();

        let worst = out.iter().cloned().fold(0.0f32, f32::max);
        assert!(
            worst < 130.0,
            "median let an octave-jump outlier through: {:?}",
            out
        );
    }

    #[test]
    fn test_passes_steady_pitch_unchanged() {
        // With no outliers the median returns the sustained value.
        let mut filter = MedianFilter::new(5);
        let mut last = 0.0;
        for _ in 0..8 {
            last = filter.push(220.0);
        }
        assert!(
            (last - 220.0).abs() < 1e-6,
            "steady pitch changed: {}",
            last
        );
    }

    #[test]
    fn test_group_delay_bounded_on_real_change() {
        // Documents the latency tradeoff: a length-5 median lags a *sustained*
        // pitch change by ceil(5/2) = 3 frames, then tracks it fully. This is
        // the ~300 ms cost (at 100 ms/frame) of the outlier rejection above —
        // the filter is not permanently stuck, it just settles.
        let mut filter = MedianFilter::new(5);
        for _ in 0..5 {
            filter.push(110.0); // settle on A2
        }
        let out: Vec<f32> = [220.0, 220.0, 220.0]
            .iter()
            .map(|&x| filter.push(x))
            .collect();

        // The old value is still the majority for the first two new frames...
        assert!(
            out[0] < 130.0 && out[1] < 130.0,
            "median jumped before the change was sustained: {:?}",
            out
        );
        // ...and the change is fully tracked once it becomes the majority.
        assert!(
            (out[2] - 220.0).abs() < 1.0,
            "median failed to track a sustained change by frame 3: {:?}",
            out
        );
    }

    #[test]
    fn test_clear_resets_window() {
        let mut filter = MedianFilter::new(5);
        for _ in 0..5 {
            filter.push(110.0);
        }
        filter.clear();
        // After a clear, the first sample of a new strike is reported as-is
        // (no blending across the silence gap).
        assert!((filter.push(440.0) - 440.0).abs() < 1e-6);
    }
}
