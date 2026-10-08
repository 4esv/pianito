//! Piano profiling screen for measuring deviation of all 88 keys.

use std::collections::HashMap;

use ratatui::{
    buffer::Buffer,
    layout::{Alignment, Constraint, Layout, Rect},
    widgets::{Block, Borders, Paragraph, Widget},
};

use crate::audio::Partial;
use crate::tuning::notes::{Note, NOTES};
use crate::tuning::profile::PianoProfile;
use crate::tuning::temperament::Temperament;
use crate::ui::components::{Meter, Piano, Progress};
use crate::ui::theme::{Shortcuts, Theme};

/// Profiling screen for measuring all 88 keys sequentially.
pub struct ProfilingScreen {
    /// Current note index (0-87, chromatic order A0→C8).
    current_note_idx: usize,
    /// Current detected frequency.
    current_freq: Option<f32>,
    /// Current cents deviation.
    current_cents: Option<f32>,
    /// Current pitch-detection confidence, alongside `current_freq`/
    /// `current_cents` (set/cleared together so a confirmed note always has
    /// all three or none).
    current_confidence: Option<f32>,
    /// Target frequency for the current note. Kept in sync by App so the
    /// info panel shows the same target the meter's cents are computed from.
    target_freq: f32,
    /// In-tune tolerance in cents (from config; drives the meter zone).
    tolerance: f32,
    /// Partials measured for the current reading, reduced from every frame
    /// captured since the last note advance (issue #86). Set by `App`
    /// alongside `update`, once a raw audio window is available to run the
    /// FFT analyzer against; empty until then, or after
    /// `confirm_note`/`skip_note`/`go_back`.
    current_partials: Vec<Partial>,
    /// The raw per-frame partial captures `current_partials` is reduced from
    /// (issue #86). The analyzer runs once per pitch update and each run is
    /// noisy, so a confirmed note is fitted from the median across the
    /// strike's frames instead of whichever one happened to arrive last.
    /// Reset on a note advance only - NOT by `clear`, which fires whenever a
    /// frame drops below the confidence floor, i.e. mid-strike.
    partial_frames: Vec<Vec<Partial>>,
    /// The profile being built.
    profile: PianoProfile,
}

impl ProfilingScreen {
    /// Create a new profiling screen.
    pub fn new() -> Self {
        Self {
            current_note_idx: 0,
            current_freq: None,
            current_cents: None,
            current_confidence: None,
            // NOTE: pre-sync default only; App overwrites this via
            // set_target_freq with the stretch/a4-adjusted target
            target_freq: Temperament::new().frequency(NOTES[0].midi),
            tolerance: 5.0,
            current_partials: Vec::new(),
            partial_frames: Vec::new(),
            profile: PianoProfile::new(),
        }
    }

    /// Set the in-tune tolerance in cents.
    pub fn set_tolerance(&mut self, tolerance: f32) {
        self.tolerance = tolerance;
    }

    /// Set the A4 reference frequency this profile is being measured under.
    pub fn set_a4_reference(&mut self, a4_reference: f32) {
        self.profile.set_a4_reference(a4_reference);
    }

    /// Get the current note to profile.
    pub fn current_note(&self) -> &'static Note {
        &NOTES[self.current_note_idx]
    }

    /// Get the current note index.
    pub fn current_note_idx(&self) -> usize {
        self.current_note_idx
    }

    /// Update with detected pitch and the detector's confidence in it.
    pub fn update(&mut self, freq: f32, cents: f32, confidence: f32) {
        self.current_freq = Some(freq);
        self.current_cents = Some(cents);
        self.current_confidence = Some(confidence);
    }

    /// Clear detected pitch (silence).
    ///
    /// The strike's accumulated partial frames are deliberately kept (issue
    /// #86): `App` calls this whenever a frame drops below the confidence
    /// floor, which happens mid-strike, so wiping them here would discard the
    /// very frames confirm is meant to reduce. Only a note advance resets
    /// them.
    pub fn clear(&mut self) {
        self.current_freq = None;
        self.current_cents = None;
        self.current_confidence = None;
    }

    /// Add one frame's measured partial spectrum to the current strike
    /// (issues #22 / #86). Called by `App` alongside `update`, once a raw
    /// audio window is available to analyze. Frames accumulate until the note
    /// is confirmed/skipped or stepped back; [`Self::current_partials`]
    /// exposes their reduction.
    pub fn set_current_partials(&mut self, partials: Vec<Partial>) {
        self.partial_frames.push(partials);
        self.current_partials = reduce_partial_frames(&self.partial_frames);
    }

    /// The partials measured for the current reading: the per-partial median
    /// across every frame captured since the last note advance (issue #86).
    pub fn current_partials(&self) -> &[Partial] {
        &self.current_partials
    }

    /// Forget the strike's accumulated frames: the next note starts from
    /// nothing (issue #86).
    fn reset_partials(&mut self) {
        self.partial_frames.clear();
        self.current_partials.clear();
    }

    /// Set the target frequency for the current note.
    pub fn set_target_freq(&mut self, freq: f32) {
        self.target_freq = freq;
    }

    /// Get the target frequency for the current note.
    pub fn target_freq(&self) -> f32 {
        self.target_freq
    }

    /// Confirm the current note measurement.
    /// Returns true if profiling is now complete.
    ///
    /// Without a pitch reading this is a no-op: advancing silently would
    /// leave a hole that `TuningOrder::from_profile` treats as unknown/worst
    /// deviation and queues first (after the temperament octave). Use skip
    /// to pass over a note deliberately.
    pub fn confirm_note(&mut self) -> bool {
        let (Some(freq), Some(cents), Some(confidence)) = (
            self.current_freq,
            self.current_cents,
            self.current_confidence,
        ) else {
            return false;
        };

        let note = self.current_note();
        // Record the reading with BOTH its measurement context (#20: the
        // target it was measured against + detection confidence) and its
        // captured partial spectrum (#22) - the latter reduced across the
        // strike's frames rather than taken from the last one (#86).
        let partials = std::mem::take(&mut self.current_partials);
        self.partial_frames.clear();
        self.profile.record_note_full(
            note.midi,
            freq,
            cents,
            self.target_freq,
            confidence,
            partials,
        );

        self.current_note_idx += 1;
        self.current_freq = None;
        self.current_cents = None;
        self.current_confidence = None;

        self.is_complete()
    }

    /// Skip the current note without recording.
    /// Returns true if profiling is now complete.
    pub fn skip_note(&mut self) -> bool {
        self.current_note_idx += 1;
        self.current_freq = None;
        self.current_cents = None;
        self.current_confidence = None;
        self.reset_partials();

        self.is_complete()
    }

    /// Go back to the previous note.
    pub fn go_back(&mut self) {
        if self.current_note_idx > 0 {
            self.current_note_idx -= 1;
            self.current_freq = None;
            self.current_cents = None;
            self.current_confidence = None;
            self.reset_partials();
        }
    }

    /// Check if profiling is complete (all 88 notes visited).
    pub fn is_complete(&self) -> bool {
        self.current_note_idx >= 88
    }

    /// Take the completed profile.
    pub fn take_profile(self) -> PianoProfile {
        self.profile
    }

    /// Get a reference to the profile.
    pub fn profile(&self) -> &PianoProfile {
        &self.profile
    }

    /// Get progress as (current, total).
    pub fn progress(&self) -> (usize, usize) {
        (self.current_note_idx, 88)
    }
}

/// Frames whose own f0 estimate sits further than this from the strike's
/// median f0 are dropped before reducing (issue #86). The analyzer
/// occasionally locks onto an octave (or two) of a noisy strike - the field
/// test saw f0 errors of -1267 and -1775 cents - while genuine frame-to-frame
/// scatter on a clean strike is a few cents, so a semitone separates the two
/// cleanly.
const F0_AGREEMENT_CENTS: f32 = 100.0;

/// A frame's own fundamental estimate: the median of its partials'
/// `freq_hz / n`. Independent of whether partial 1 was recovered at all (the
/// bass fundamental often isn't), and a median rather than a mean so one
/// mis-located high partial can't drag it.
fn frame_f0(frame: &[Partial]) -> Option<f32> {
    let mut ratios: Vec<f32> = frame
        .iter()
        .filter(|p| p.n > 0 && p.freq_hz > 0.0)
        .map(|p| p.freq_hz / f32::from(p.n))
        .collect();
    median(&mut ratios)
}

/// Median of `values`, averaging the two middle entries for an even count.
/// `None` for an empty slice, so callers have a single empty case to handle.
fn median(values: &mut [f32]) -> Option<f32> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(f32::total_cmp);
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 0 {
        (values[mid - 1] + values[mid]) / 2.0
    } else {
        values[mid]
    })
}

/// `a` relative to `b` in cents.
fn cents_between(a: f32, b: f32) -> f32 {
    1200.0 * (a / b).log2()
}

/// Reduce a strike's per-frame partial captures to the one partial set a
/// confirmed note is recorded with (issue #86).
///
/// The strike's reference f0 is the median of the frames' own f0 estimates
/// (robust to the minority of frames the analyzer locks onto an octave of),
/// and frames disagreeing with it by more than [`F0_AGREEMENT_CENTS`] are
/// dropped - their "partials" describe a different pitch family. Each
/// surviving partial number then takes the median of its frequency and
/// amplitude across the remaining frames, and a number that fewer than half
/// of them found is left out: one stray partial isn't a measurement.
///
/// A single frame reduces to itself, so a note captured once behaves exactly
/// as it did before frames accumulated.
fn reduce_partial_frames(frames: &[Vec<Partial>]) -> Vec<Partial> {
    let f0s: Vec<Option<f32>> = frames.iter().map(|frame| frame_f0(frame)).collect();
    let mut present: Vec<f32> = f0s.iter().flatten().copied().collect();
    let Some(reference_f0) = median(&mut present) else {
        return Vec::new();
    };

    let kept: Vec<&[Partial]> = frames
        .iter()
        .zip(&f0s)
        .filter_map(|(frame, f0)| match f0 {
            Some(f0) if cents_between(*f0, reference_f0).abs() <= F0_AGREEMENT_CENTS => {
                Some(frame.as_slice())
            }
            _ => None,
        })
        .collect();
    if kept.is_empty() {
        return Vec::new();
    }

    let min_frames = kept.len().div_ceil(2);
    let mut orders: Vec<u16> = kept
        .iter()
        .flat_map(|frame| frame.iter().map(|p| p.n))
        .collect();
    orders.sort_unstable();
    orders.dedup();

    let mut reduced = Vec::with_capacity(orders.len());
    for n in orders {
        let mut freqs: Vec<f32> = Vec::new();
        let mut amplitudes: Vec<f32> = Vec::new();
        for frame in &kept {
            if let Some(partial) = frame.iter().find(|p| p.n == n) {
                freqs.push(partial.freq_hz);
                amplitudes.push(partial.amplitude);
            }
        }
        if freqs.len() < min_frames {
            continue;
        }
        let (Some(freq_hz), Some(amplitude)) = (median(&mut freqs), median(&mut amplitudes)) else {
            continue;
        };
        reduced.push(Partial {
            n,
            freq_hz,
            amplitude,
        });
    }
    reduced
}

impl Default for ProfilingScreen {
    fn default() -> Self {
        Self::new()
    }
}

impl Widget for &ProfilingScreen {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let note = self.current_note();
        let title = format!(" Profile: {} ", note.display_name());

        let block = Block::default()
            .borders(Borders::ALL)
            .border_style(Theme::border())
            .title(title)
            .title_style(Theme::title());

        let inner = block.inner(area);
        block.render(area, buf);

        // Minimum for the reduced layout below (progress + info + meter + help)
        if inner.height < 16 || inner.width < 40 {
            let msg = "Terminal too small";
            buf.set_string(inner.x, inner.y, msg, Theme::warning());
            return;
        }

        // Full layout needs 23 rows; below that, drop the piano and spacers
        // so the meter (the core feedback) keeps its full height.
        let (progress_area, piano_area, info_area, meter_area, help_area) = if inner.height >= 23 {
            let chunks = Layout::vertical([
                Constraint::Length(2), // Progress bar
                Constraint::Length(1), // Spacer
                Constraint::Length(4), // Piano visualization
                Constraint::Length(1), // Spacer
                Constraint::Length(4), // Note info
                Constraint::Length(1), // Spacer
                Constraint::Length(8), // Meter
                Constraint::Length(2), // Help text
            ])
            .split(inner);
            (chunks[0], Some(chunks[2]), chunks[4], chunks[6], chunks[7])
        } else {
            let chunks = Layout::vertical([
                Constraint::Length(2), // Progress bar
                Constraint::Length(4), // Note info
                Constraint::Length(8), // Meter
                Constraint::Min(0),    // Filler
                Constraint::Length(2), // Help text
            ])
            .split(inner);
            (chunks[0], None, chunks[1], chunks[2], chunks[4])
        };

        // Progress indicator
        let (completed, total) = self.progress();
        let progress = Progress::new(completed, total, note.display_name(), "Profiling");
        progress.render(progress_area, buf);

        // Piano visualization with profiled notes colored by deviation
        if let Some(piano_area) = piano_area {
            let deviations: HashMap<usize, f32> = self
                .profile
                .notes
                .iter()
                .enumerate()
                .filter_map(|(i, n)| n.as_ref().map(|note| (i, note.cents)))
                .collect();

            let piano = Piano::full()
                .with_deviations(deviations)
                .current(Some(self.current_note_idx));
            piano.render(piano_area, buf);
        }

        // Note info panel
        render_note_info(note, self.target_freq, &self.profile, info_area, buf);

        // Cents meter
        if let Some(cents) = self.current_cents {
            let meter = Meter::new(cents).tolerance(self.tolerance);
            meter.render(meter_area, buf);
        } else {
            let meter = Meter::listening().tolerance(self.tolerance);
            meter.render(meter_area, buf);
        }

        // Help text
        let help_text = format!(
            "{} Confirm  {} Back  {} Skip  {} Quit",
            Shortcuts::SPACE,
            Shortcuts::BACK,
            Shortcuts::SKIP,
            Shortcuts::QUIT
        );
        let help = Paragraph::new(help_text)
            .style(Theme::muted())
            .alignment(Alignment::Center);
        help.render(help_area, buf);
    }
}

/// Render note info panel.
fn render_note_info(
    note: &Note,
    target_freq: f32,
    profile: &PianoProfile,
    area: Rect,
    buf: &mut Buffer,
) {
    if area.height < 3 {
        return;
    }

    // Note name and target frequency (as synced from App's temperament)
    let info_line = format!(
        "{}  Target: {:.1} Hz  Strings: {}",
        note.display_name(),
        target_freq,
        note.strings
    );

    let info = Paragraph::new(info_line)
        .style(Theme::accent())
        .alignment(Alignment::Center);

    let info_area = Rect {
        x: area.x,
        y: area.y,
        width: area.width,
        height: 1,
    };
    info.render(info_area, buf);

    // Profile summary
    let (completed, total) = profile.progress();
    let avg_deviation = profile.average_deviation();
    let summary = format!(
        "Profiled: {}/{}  Avg deviation: {:.1} cents",
        completed, total, avg_deviation
    );

    let summary_para = Paragraph::new(summary)
        .style(Theme::muted())
        .alignment(Alignment::Center);

    let summary_area = Rect {
        x: area.x,
        y: area.y + 2,
        width: area.width,
        height: 1,
    };
    summary_para.render(summary_area, buf);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn render_to_string(screen: &ProfilingScreen, width: u16, height: u16) -> String {
        let area = Rect::new(0, 0, width, height);
        let mut buf = Buffer::empty(area);
        screen.render(area, &mut buf);
        (0..height)
            .map(|y| {
                (0..width)
                    .map(|x| buf[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn test_confirm_without_reading_is_noop() {
        let mut screen = ProfilingScreen::new();

        assert!(!screen.confirm_note());
        assert_eq!(screen.current_note_idx(), 0, "must not advance");
        assert_eq!(screen.profile().progress(), (0, 88), "must not record");
    }

    #[test]
    fn test_confirm_records_at_current_index() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0, 0.9);

        assert!(!screen.confirm_note());
        assert_eq!(screen.current_note_idx(), 1);
        assert_eq!(screen.profile().progress(), (1, 88));

        let recorded = screen.profile().notes[0].as_ref().expect("A0 recorded");
        assert_eq!(recorded.midi, 21);
        assert!((recorded.cents - 3.0).abs() < 0.01);
    }

    fn sample_partials() -> Vec<Partial> {
        vec![
            Partial {
                n: 1,
                freq_hz: 27.6,
                amplitude: 0.2,
            },
            Partial {
                n: 2,
                freq_hz: 55.3,
                amplitude: 1.0,
            },
        ]
    }

    #[test]
    fn test_confirm_persists_current_partials() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0, 0.9);
        screen.set_current_partials(sample_partials());

        assert_eq!(screen.current_partials(), sample_partials().as_slice());
        screen.confirm_note();

        let recorded = screen.profile().notes[0].as_ref().expect("A0 recorded");
        assert_eq!(recorded.partials, sample_partials());
    }

    #[test]
    fn test_confirm_without_partials_records_empty() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0, 0.9);
        screen.confirm_note();

        let recorded = screen.profile().notes[0].as_ref().expect("A0 recorded");
        assert!(recorded.partials.is_empty());
    }

    #[test]
    fn test_confirm_does_not_leak_partials_into_next_note() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0, 0.9);
        screen.set_current_partials(sample_partials());
        screen.confirm_note();

        // Next note's reading arrives with no partials set yet.
        screen.update(55.0, -1.0, 0.9);
        screen.confirm_note();

        let recorded = screen.profile().notes[1].as_ref().expect("A#0 recorded");
        assert!(
            recorded.partials.is_empty(),
            "stale partials from the previous note must not carry over"
        );
    }

    #[test]
    fn test_clear_keeps_the_strikes_partials() {
        // Issue #86: `clear` fires whenever a frame drops below the
        // confidence floor, which happens mid-strike - wiping the capture
        // there would discard exactly the frames confirm is meant to reduce.
        let mut screen = ProfilingScreen::new();
        screen.set_current_partials(sample_partials());
        screen.clear();
        assert_eq!(screen.current_partials(), sample_partials().as_slice());
    }

    #[test]
    fn test_skip_note_resets_partials() {
        let mut screen = ProfilingScreen::new();
        screen.set_current_partials(sample_partials());
        screen.skip_note();
        assert!(screen.current_partials().is_empty());
    }

    #[test]
    fn test_go_back_resets_partials() {
        let mut screen = ProfilingScreen::new();
        screen.skip_note();
        screen.set_current_partials(sample_partials());
        screen.go_back();
        assert!(screen.current_partials().is_empty());
    }

    #[test]
    fn test_confirm_clears_reading_for_next_note() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0, 0.9);
        screen.confirm_note();

        // The old reading must not leak into the next note
        assert!(!screen.confirm_note());
        assert_eq!(screen.current_note_idx(), 1);
    }

    #[test]
    fn test_skip_advances_without_recording() {
        let mut screen = ProfilingScreen::new();
        screen.update(27.5, 3.0, 0.9);

        assert!(!screen.skip_note());
        assert_eq!(screen.current_note_idx(), 1);
        assert_eq!(screen.profile().progress(), (0, 88));
    }

    #[test]
    fn test_go_back_at_zero_stays_at_zero() {
        let mut screen = ProfilingScreen::new();
        screen.go_back();
        assert_eq!(screen.current_note_idx(), 0);
    }

    #[test]
    fn test_go_back_steps_to_previous_note() {
        let mut screen = ProfilingScreen::new();
        screen.skip_note();
        screen.skip_note();
        screen.go_back();
        assert_eq!(screen.current_note_idx(), 1);
    }

    #[test]
    fn test_confirming_all_88_notes_completes() {
        let mut screen = ProfilingScreen::new();

        for i in 0..88 {
            screen.update(440.0, 1.0, 0.9);
            let done = screen.confirm_note();
            assert_eq!(done, i == 87, "only the 88th confirm completes");
        }

        assert!(screen.is_complete());
        assert_eq!(screen.profile().progress(), (88, 88));
        assert!(screen.profile().is_complete());
    }

    #[test]
    fn test_target_freq_roundtrip() {
        let mut screen = ProfilingScreen::new();
        screen.set_target_freq(27.3);
        assert!((screen.target_freq() - 27.3).abs() < f32::EPSILON);
    }

    #[test]
    fn test_confirm_records_measurement_context() {
        // #20: the note recorded on confirm must carry the a4 reference,
        // the target it was measured against, and detection confidence.
        let mut screen = ProfilingScreen::new();
        screen.set_a4_reference(442.0);
        screen.set_target_freq(27.3);
        screen.update(27.5, 3.0, 0.87);

        assert!(!screen.confirm_note());

        assert!((screen.profile().a4_reference - 442.0).abs() < 0.01);
        let recorded = screen.profile().notes[0].as_ref().expect("A0 recorded");
        assert!((recorded.target_freq - 27.3).abs() < 0.01);
        assert!((recorded.confidence - 0.87).abs() < 0.01);
    }

    // ---- Partial accumulation across a strike (issue #86) ----------------

    /// A harmonic frame of `f0`: partial `n` at `n * f0` with the given
    /// amplitudes, in ascending partial order like the analyzer's output.
    fn harmonic_frame(f0: f32, amplitudes: &[(u16, f32)]) -> Vec<Partial> {
        amplitudes
            .iter()
            .map(|&(n, amplitude)| Partial {
                n,
                freq_hz: f32::from(n) * f0,
                amplitude,
            })
            .collect()
    }

    #[test]
    fn test_partials_accumulate_across_frames_of_one_strike() {
        // Issue #86: the second frame must not simply overwrite the first.
        let mut screen = ProfilingScreen::new();
        screen.set_current_partials(harmonic_frame(55.0, &[(1, 0.8), (2, 0.4)]));
        screen.set_current_partials(harmonic_frame(55.0, &[(1, 0.9), (2, 0.6)]));

        let reduced = screen.current_partials();
        assert_eq!(reduced.len(), 2);
        assert!(
            (reduced[0].amplitude - 0.85).abs() < 1e-6,
            "expected the median of both frames, got {}",
            reduced[0].amplitude
        );
        assert!((reduced[1].amplitude - 0.5).abs() < 1e-6);
    }

    #[test]
    fn test_reduce_partial_frames_medians_out_noisy_frames() {
        // Issue #86: several noisy frames plus one good frame must reduce to
        // a partial set close to the good frame - not to the last frame, and
        // not to whatever a single noisy frame happened to show.
        let good = harmonic_frame(55.0, &[(1, 0.90), (2, 0.50), (3, 0.20)]);
        let frames = vec![
            harmonic_frame(55.02, &[(1, 0.86), (2, 0.47), (3, 0.23)]),
            good.clone(),
            harmonic_frame(54.99, &[(1, 0.94), (2, 0.52), (3, 0.18)]),
            harmonic_frame(55.01, &[(1, 0.89), (2, 0.51), (3, 0.19)]),
        ];

        let reduced = reduce_partial_frames(&frames);

        assert_eq!(reduced.len(), good.len(), "every partial survives");
        for (reduced, good) in reduced.iter().zip(good.iter()) {
            assert_eq!(reduced.n, good.n);
            assert!(
                (reduced.freq_hz - good.freq_hz).abs() < 0.1,
                "partial {} should sit near the good frame's {} Hz, got {}",
                good.n,
                good.freq_hz,
                reduced.freq_hz
            );
            assert!(
                (reduced.amplitude - good.amplitude).abs() < 0.05,
                "partial {} amplitude should track the good frame's {}, got {}",
                good.n,
                good.amplitude,
                reduced.amplitude
            );
        }
    }

    #[test]
    fn test_reduce_partial_frames_drops_octave_wrong_frames() {
        // Issue #86: 10-20% of frames lock onto an octave of the note (in
        // either direction). Their partials describe a different pitch
        // family and must not enter the median at all.
        let good_frames = vec![
            harmonic_frame(55.0, &[(1, 0.9), (2, 0.5), (3, 0.2)]),
            harmonic_frame(55.01, &[(1, 0.9), (2, 0.5), (3, 0.2)]),
            harmonic_frame(54.99, &[(1, 0.9), (2, 0.5), (3, 0.2)]),
        ];
        let mut frames = good_frames.clone();
        frames.push(harmonic_frame(110.0, &[(1, 1.0), (2, 0.95), (3, 0.9)])); // octave up
        frames.push(harmonic_frame(27.5, &[(1, 1.0), (2, 0.95), (3, 0.9)])); // octave down

        let reduced = reduce_partial_frames(&frames);
        let clean = reduce_partial_frames(&good_frames);

        assert_eq!(reduced.len(), clean.len());
        for (reduced, clean) in reduced.iter().zip(clean.iter()) {
            assert_eq!(reduced.n, clean.n);
            assert!(
                (reduced.freq_hz - clean.freq_hz).abs() < 1e-3,
                "an octave-wrong frame leaked into partial {}",
                clean.n
            );
            assert!(
                (reduced.amplitude - clean.amplitude).abs() < 1e-6,
                "an octave-wrong frame's amplitude leaked into partial {}",
                clean.n
            );
        }
    }

    #[test]
    fn test_reduce_partial_frames_is_empty_without_any_partials() {
        assert!(reduce_partial_frames(&[]).is_empty());
        assert!(reduce_partial_frames(&[Vec::new(), Vec::new()]).is_empty());
    }

    #[test]
    fn test_reduce_partial_frames_drops_a_one_off_partial() {
        // A partial number only one frame of four reports is noise, not a
        // measurement: the median of "missing" isn't a value.
        let frames = vec![
            harmonic_frame(55.0, &[(1, 0.9), (2, 0.5)]),
            harmonic_frame(55.0, &[(1, 0.9), (2, 0.5)]),
            harmonic_frame(55.0, &[(1, 0.9), (2, 0.5)]),
            harmonic_frame(55.0, &[(1, 0.9), (2, 0.5), (7, 0.9)]),
        ];

        let reduced = reduce_partial_frames(&frames);

        assert_eq!(reduced.len(), 2, "partial 7 must not survive on one frame");
        assert!(reduced.iter().all(|p| p.n <= 2));
    }

    #[test]
    fn test_small_terminal_drops_piano_keeps_meter() {
        // Stock 80x24 terminal: inner is 78x22, below the 23-row full layout
        let screen = ProfilingScreen::new();
        let rendered = render_to_string(&screen, 80, 24);

        assert!(!rendered.contains("Terminal too small"));
        assert!(rendered.contains("Listening..."), "meter must be visible");
        assert!(rendered.contains("Profiled: 0/88"), "info must be visible");
        assert!(!rendered.contains('╚'), "piano dropped at this height");
    }

    #[test]
    fn test_full_terminal_shows_piano() {
        let screen = ProfilingScreen::new();
        let rendered = render_to_string(&screen, 110, 30);

        assert!(rendered.contains('╚'), "piano visible");
        assert!(rendered.contains("Listening..."));
    }

    #[test]
    fn test_too_small_terminal_shows_message() {
        let screen = ProfilingScreen::new();
        let rendered = render_to_string(&screen, 80, 10);
        assert!(rendered.contains("Terminal too small"));
    }
}
