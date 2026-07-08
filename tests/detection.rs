//! 88-key detection integration harness (issue #17).
//!
//! This is the acceptance harness the rest of Stage 1 is measured against. It
//! synthesizes a physically realistic tone for every key (inharmonic partial
//! stack, register-dependent rolloff, weak/absent bass fundamental, attack
//! transient, per-partial decay, -30 dB noise — see [`fixtures`]) and runs it
//! through the production detection path.
//!
//! Every earlier test fed the detector pure sines, so "194 green" coexisted
//! with a product that fails across roughly three octaves. This corpus makes
//! those failures visible and pins them to a baseline:
//!
//! - **Reliable registers (C4–C6, C7–C8):** must lock the right note within 5
//!   cents. This is the regression guard — if either range ever drifts, CI
//!   breaks. The treble half was promoted here by issue #14's spectral
//!   refinement (see the note on [`RELIABLE_TREBLE`] for what "5 cents"
//!   means once the corpus's own inharmonicity model is accounted for).
//! - **Note-level lock (C2–C8):** must at least identify the correct note, even
//!   where the cents accuracy is not yet tuner-grade.
//! - **Bass f0 recovery (A0–B2, issue #15):** the weak/absent fundamental
//!   defeats a time-domain estimator outright, so this band is scored via
//!   guided mode's real call path (`detect_for_target`) rather than the blind
//!   scan the rest of the corpus uses — see [`detect_lock_bass`] for why.
//! - **Known-limitation keys:** a residual sliver of bass (C3–F#3) still
//!   drags the plain YIN clamp sharp (> 5 cents). Asserted to *still fail* so
//!   that when its own accuracy work lands the harness forces the baseline to
//!   be promoted rather than letting the win go unnoticed. (The stiff treble
//!   and the weak-fundamental deep bass that used to sit here have been
//!   promoted by #14 and #15 respectively.)

mod fixtures;

use std::io::Cursor;
use std::ops::RangeInclusive;

use pianito::audio::{AudioSource, MedianFilter, PartialAnalyzer, PitchDetector, WavAudioSource};
use pianito::tuning::notes::Note;
use pianito::tuning::temperament::Temperament;

use fixtures::{fundamental_hz, inharmonicity_b, MIDI_MAX, MIDI_MIN, SAMPLE_RATE};

/// Fixture length. 0.7 s is an exact multiple of the 100 ms production window
/// (seven windows), so the harness never feeds the detector a short runt frame,
/// and it fills the length-5 median filter comfortably.
const FIXTURE_SECS: f32 = 0.7;

/// Note-level lock threshold on the synthetic corpus (issue #17: < 5 cents).
const CENTS_TOLERANCE: f32 = 5.0;

// ---- Baseline registers (MIDI), measured from the committed generator. ------
//
// Boundaries are chosen with margin, not fitted to the exact cents of edge
// keys: the reliable band clears 5 cents by ~1 cent, the known-fail bands
// exceed it by ~1–2 cents, so ordinary cross-platform float drift cannot flip a
// verdict — only a real detector change can.

/// C4–C6: the detector's sweet spot. Must lock the right note within tolerance.
const RELIABLE: RangeInclusive<u8> = 60..=84;
/// C2–C8: the fundamental is present, so the correct *note* must be identified
/// (cents may still be out of tolerance in the known-fail bands below).
const RIGHT_NOTE: RangeInclusive<u8> = 36..=108;
/// A0–B2: weak/absent fundamental, previously unrecoverable by a time-domain
/// estimator at all. Promoted by issue #15's partial-based f0 estimator,
/// which `detect_for_target` now routes to below C3 (mirrors
/// `PitchDetector::BASS_PARTIAL_MAX_TARGET_HZ` — the two must stay in sync
/// for this band to mean what it says). Scored through
/// [`detect_lock_bass`], not the blind [`detect_lock`]: guided mode always
/// knows the target here (this app has no free-listening mode), so blind
/// detection could never observe the fix this band exists to pin.
const BASS_PARTIAL_RECOVERED: RangeInclusive<u8> = 21..=47;
/// C3–F#3: right note, but inharmonic partials still pull the plain YIN
/// clamp > 5 cents sharp. Below issue #15's target register; a residual left
/// for future work.
const KNOWN_FAIL_BASS: RangeInclusive<u8> = 48..=54;
/// C7–C8: promoted by issue #14's spectral refinement pass. Once YIN's
/// coarse candidate (a ~41-to-165-cents/step integer-lag grid up here) is
/// re-read from the FFT peak of the same window, the detector's reading of
/// the *actual played fundamental* is precise to a small fraction of a cent
/// (see `pianito::audio::pitch::tests::test_detect_refines_treble_beyond_integer_lag_grid`
/// and this module's own `treble_spectral_refinement_locks_the_true_partial_within_one_cent`).
///
/// What's left of the ~1–4 cent reading here is *not* residual detector
/// error: it is the corpus's own stiff-string model. `synth_note` places
/// partial 1 at `f0 * sqrt(1 + B)`, and treble `B` (issue #17: up to 0.005 at
/// C8, see `fixtures::inharmonicity_b`) stretches that real partial sharp of
/// the flat equal-tempered grid this harness's note-lock scores against —
/// about 2.3 cents at C7 growing to 4.3 at C8. A detector with *zero* error
/// would still read exactly that offset here. Reconciling it is a tuning
/// concern (a Railsback-style stretch curve, #22/#23's inharmonicity
/// engine), not a detection one, so this band still clears the same 5-cent
/// bar as [`RELIABLE`] rather than getting a bespoke looser tolerance for
/// treble.
const RELIABLE_TREBLE: RangeInclusive<u8> = 96..=108;

/// What the harness resolved a fixture to.
#[derive(Debug, Clone, Copy)]
enum Lock {
    /// Detector locked a note: nearest MIDI + cents deviation from its target.
    Note { midi: u8, cents: f32, freq: f32 },
    /// No frame ever produced a confident, in-range pitch.
    None,
}

impl Lock {
    /// Correct note identified, regardless of cents accuracy.
    fn locks_note(self, expected: u8) -> bool {
        matches!(self, Lock::Note { midi, .. } if midi == expected)
    }

    /// Correct note *and* within tuning tolerance — a full tuner-grade lock.
    fn passes(self, expected: u8) -> bool {
        matches!(self, Lock::Note { midi, cents, .. } if midi == expected && cents.abs() < CENTS_TOLERANCE)
    }
}

/// Run the *blind* production detection path over an in-memory WAV and return
/// the stabilized frequency lock: 100 ms windows, the target-agnostic
/// [`PitchDetector::detect`], median-filtered stream with the filter cleared
/// on lost frames. The lock is the median of the smoothed stream — the
/// sustained pitch, robust to a stray attack-frame outlier. `None` if no
/// window ever produced a confident, in-range pitch.
///
/// This is deliberately the harder of the app's two call paths (see
/// [`detect_lock_bass`] for the guided one): it is what pins the
/// reliable/note-lock/known-fail bands above [`BASS_PARTIAL_RECOVERED`],
/// none of which issue #15 touches.
fn detect_lock_frequency(wav: &[u8]) -> Option<f32> {
    let mut source = WavAudioSource::new(Cursor::new(wav.to_vec())).expect("valid WAV");
    let detector = PitchDetector::new(SAMPLE_RATE);
    let mut filter = MedianFilter::new(MedianFilter::DEFAULT_WINDOW);

    let window = SAMPLE_RATE as usize / 10; // 100 ms, matches the interactive loop
    let mut buffer = vec![0.0f32; window];
    let mut smoothed: Vec<f32> = Vec::new();

    loop {
        let read = source.read_samples(&mut buffer);
        // Only feed full production-size windows; a short trailing remainder is
        // a harness artifact the mic loop never sees.
        if read < window {
            break;
        }
        match detector.detect(&buffer[..read]) {
            Ok(result) => smoothed.push(filter.push(result.frequency)),
            Err(_) => filter.clear(),
        }
    }

    median_frequency(smoothed)
}

/// Run guided mode's real call path ([`PitchDetector::detect_for_target`])
/// over an in-memory WAV and report the stabilized lock.
///
/// [`detect_lock`]'s blind scan can never observe issue #15's fix: this crate
/// has no free-listening mode (`main.rs` calls `detect_for_target` whenever a
/// note is being tuned, i.e. essentially always), and the partial-based bass
/// estimator only activates when it is *given* a target. Scoring the
/// weak-fundamental register through the blind path would leave that whole
/// band permanently unrecoverable in the harness regardless of how good the
/// estimator gets — the corpus would be asserting a real product capability
/// can never happen.
///
/// Simulates the interactive loop's growing capture read (audio accumulates
/// in the ring buffer between ~50 ms poll ticks) by sliding a
/// [`PitchDetector::MAX_WINDOW`]-sized window forward in 100 ms hops over the
/// whole decoded note, so the bass path sees the same long, onset-trailing
/// view of the signal `main.rs` hands it.
fn detect_lock_bass(wav: &[u8], target_hz: f32) -> Lock {
    let mut source = WavAudioSource::new(Cursor::new(wav.to_vec())).expect("valid WAV");
    let mut all = vec![0.0f32; (SAMPLE_RATE as f32 * FIXTURE_SECS) as usize + SAMPLE_RATE as usize];
    let n = source.read_samples(&mut all);
    let all = &all[..n];

    let detector = PitchDetector::new(SAMPLE_RATE);
    let temperament = Temperament::new();
    let mut filter = MedianFilter::new(MedianFilter::DEFAULT_WINDOW);

    let window = PitchDetector::max_window_samples(SAMPLE_RATE);
    let hop = SAMPLE_RATE as usize / 10; // 100 ms
    let mut smoothed: Vec<f32> = Vec::new();

    if all.len() < window {
        return Lock::None;
    }
    let mut end = window;
    loop {
        match detector.detect_for_target(&all[..end], target_hz) {
            Ok(result) => smoothed.push(filter.push(result.frequency)),
            Err(_) => filter.clear(),
        }
        if end >= all.len() {
            break;
        }
        end = (end + hop).min(all.len());
    }

    lock_from_frequency(&temperament, median_frequency(smoothed))
}

/// Median of a smoothed pitch stream — the sustained pitch, robust to a stray
/// attack-frame outlier. `None` if the stream is empty (no confident frame).
fn median_frequency(mut smoothed: Vec<f32>) -> Option<f32> {
    if smoothed.is_empty() {
        return None;
    }
    smoothed.sort_unstable_by(f32::total_cmp);
    Some(smoothed[smoothed.len() / 2])
}

/// Shared tail of both lock functions: resolve a stabilized frequency to the
/// nearest note — what the accuracy table and baseline checks score against.
fn lock_from_frequency(temperament: &Temperament, f_lock: Option<f32>) -> Lock {
    match f_lock {
        Some(f_lock) => match temperament.nearest_note(f_lock) {
            Some((midi, cents)) => Lock::Note {
                midi,
                cents,
                freq: f_lock,
            },
            None => Lock::None,
        },
        None => Lock::None,
    }
}

/// [`detect_lock_frequency`] (the blind scan) plus the nearest-note lookup.
fn detect_lock(wav: &[u8]) -> Lock {
    let temperament = Temperament::new();
    lock_from_frequency(&temperament, detect_lock_frequency(wav))
}

/// Human label for a key, e.g. "A0" or "C#4".
fn key_label(midi: u8) -> String {
    Note::from_midi(midi).map_or_else(|| format!("MIDI{midi}"), Note::display_name)
}

/// Per-key harness result plus the register it belongs to.
struct KeyReport {
    midi: u8,
    lock: Lock,
}

impl KeyReport {
    fn category(&self) -> &'static str {
        let m = self.midi;
        if BASS_PARTIAL_RECOVERED.contains(&m) {
            "bass-recovered (#15)"
        } else if KNOWN_FAIL_BASS.contains(&m) {
            "bass-sharp (residual)"
        } else if RELIABLE.contains(&m) {
            "reliable"
        } else if RELIABLE_TREBLE.contains(&m) {
            "reliable-treble (#14)"
        } else {
            "transition"
        }
    }

    fn row(&self) -> String {
        let expected = key_label(self.midi);
        match self.lock {
            Lock::Note { midi, cents, freq } => {
                let mark = if self.lock.passes(self.midi) {
                    "ok"
                } else {
                    "  "
                };
                format!(
                    "{mark} {expected:>4} {freq:8.2} Hz -> {:<4} {cents:+7.2}c  [{}]",
                    key_label(midi),
                    self.category()
                )
            }
            Lock::None => format!(
                "   {expected:>4}      (no lock)          [{}]",
                self.category()
            ),
        }
    }
}

/// Synthesize and detect every key A0..=C8.
///
/// [`BASS_PARTIAL_RECOVERED`] keys are scored through guided mode's real call
/// path ([`detect_lock_bass`]); everything else through the blind scan
/// ([`detect_lock`]) — see that function's doc comment for why the split is
/// necessary rather than incidental.
fn build_report() -> Vec<KeyReport> {
    (MIDI_MIN..=MIDI_MAX)
        .map(|midi| {
            let wav = fixtures::synth_wav(midi, SAMPLE_RATE, FIXTURE_SECS);
            let lock = if BASS_PARTIAL_RECOVERED.contains(&midi) {
                detect_lock_bass(&wav, fundamental_hz(midi))
            } else {
                detect_lock(&wav)
            };
            KeyReport { midi, lock }
        })
        .collect()
}

/// Render the full 88-key accuracy table plus a summary line.
fn format_table(report: &[KeyReport]) -> String {
    let passing = report.iter().filter(|r| r.lock.passes(r.midi)).count();
    let mut out = String::from("88-key detection accuracy (production window path):\n");
    for r in report {
        out.push_str(&r.row());
        out.push('\n');
    }
    out.push_str(&format!(
        "-- {passing}/88 keys lock within {CENTS_TOLERANCE:.0} cents --",
    ));
    out
}

#[test]
fn synthetic_corpus_matches_detection_baseline() {
    let report = build_report();
    let mut violations: Vec<String> = Vec::new();

    for r in &report {
        let m = r.midi;

        // Regression guard: the reliable registers must stay tuner-grade.
        if (RELIABLE.contains(&m) || RELIABLE_TREBLE.contains(&m)) && !r.lock.passes(m) {
            violations.push(format!(
                "[regress] {} slipped below tuner-grade in a reliable register",
                key_label(m)
            ));
        }

        // Note-level lock across the fundamental-present range.
        if RIGHT_NOTE.contains(&m) && !r.lock.locks_note(m) {
            violations.push(format!(
                "[note-lock] {} no longer identifies the correct note",
                key_label(m)
            ));
        }

        // Bass f0 recovery (issue #15): the partial-based estimator must
        // deliver a tuner-grade lock across the whole weak-fundamental
        // register, scored via guided mode's real call path
        // (`detect_lock_bass`).
        if BASS_PARTIAL_RECOVERED.contains(&m) && !r.lock.passes(m) {
            violations.push(format!(
                "[regress] {} lost its bass f0 recovery (#15) — no longer locks \
                 within tolerance",
                key_label(m)
            ));
        }

        // Known-limitation keys must still fail: a pass means the detector
        // improved and the baseline must be promoted. Only the C3–F#3 bass
        // residual remains here — the stiff treble (#14) and the weak-
        // fundamental deep bass (#15) have both been promoted into their
        // reliable bands above.
        if KNOWN_FAIL_BASS.contains(&m) && r.lock.passes(m) {
            violations.push(format!(
                "[promote] {} now locks within tolerance — the accuracy work for its \
                 register has progressed; move it into the reliable baseline",
                key_label(m)
            ));
        }
    }

    assert!(
        violations.is_empty(),
        "{}\n\n{} baseline violation(s):\n{}",
        format_table(&report),
        violations.len(),
        violations.join("\n")
    );
}

/// Issue #14's own bar ("<1 cent") is about how precisely the detector reads
/// the string, not about how close a stiff string's fundamental sits to a
/// flat equal-tempered grid — those are different questions, and the corpus
/// bakes in a real answer to the second one (see [`RELIABLE_TREBLE`]). This
/// scores the spectral refinement against what `synth_note` actually put in
/// the fixture — `f0 * sqrt(1 + B)`, the true stretched partial 1 — so it
/// isolates detector precision from the corpus's own inharmonicity model.
#[test]
fn treble_spectral_refinement_locks_the_true_partial_within_one_cent() {
    const TRUE_PARTIAL_TOLERANCE: f32 = 1.0;

    for midi in RELIABLE_TREBLE {
        let f0 = fundamental_hz(midi);
        let b = inharmonicity_b(midi);
        let true_partial = f0 * (1.0 + b).sqrt();

        let wav = fixtures::synth_wav(midi, SAMPLE_RATE, FIXTURE_SECS);
        let f_lock = detect_lock_frequency(&wav)
            .unwrap_or_else(|| panic!("{}: expected a lock, got none", key_label(midi)));

        let cents = 1200.0 * (f_lock / true_partial).log2().abs();
        assert!(
            cents < TRUE_PARTIAL_TOLERANCE,
            "{}: locked {f_lock:.3} Hz, {cents:.3} cents off the true partial \
             ({true_partial:.3} Hz, f0={f0:.3} Hz, B={b:.5})",
            key_label(midi)
        );
    }
}

#[test]
fn corpus_is_deterministic() {
    // Byte-for-byte reproducible from the seed, so the harness is stable and the
    // repo need not carry the WAVs.
    let a = fixtures::synth_wav(60, SAMPLE_RATE, 0.1);
    let b = fixtures::synth_wav(60, SAMPLE_RATE, 0.1);
    assert_eq!(a, b, "same key must synthesize identical bytes");

    // Different keys are genuinely different signals.
    let c4 = fixtures::synth_wav(60, SAMPLE_RATE, 0.1);
    let a4 = fixtures::synth_wav(69, SAMPLE_RATE, 0.1);
    assert_ne!(c4, a4, "distinct keys must differ");
}

#[test]
fn corpus_models_weak_bass_fundamental_and_inharmonicity() {
    // Guards the two realism properties that pure-sine fixtures lacked. If a
    // future refactor flattened them, the corpus would silently stop exercising
    // the real failure mechanisms even while the accuracy table still "passed".
    let analyzer = PartialAnalyzer::new(SAMPLE_RATE).with_max_partials(6);

    // 1. Weak/absent bass fundamental: A0's first partial is far below its
    //    overtones, which is exactly what defeats a time-domain detector.
    let a0 = fixtures::synth_note(21, SAMPLE_RATE, FIXTURE_SECS);
    let a0_partials = analyzer.analyze(&a0, fundamental_hz(21));
    let fundamental = a0_partials.iter().find(|p| p.n == 1).map(|p| p.amplitude);
    let strongest_overtone = a0_partials
        .iter()
        .filter(|p| p.n >= 2)
        .map(|p| p.amplitude)
        .fold(0.0f32, f32::max);
    assert!(
        strongest_overtone > 0.0,
        "A0 fixture has no overtones to analyze"
    );
    match fundamental {
        Some(f) => assert!(
            f < strongest_overtone,
            "A0 fundamental ({f:.4}) should be weaker than its strongest overtone \
             ({strongest_overtone:.4})"
        ),
        None => { /* fundamental fully masked below the noise floor — even weaker */ }
    }

    // 2. Inharmonic partials: a mid-register overtone sits sharp of the exact
    //    integer multiple of f0 (stiff-string stretch).
    let c4_f0 = fundamental_hz(60);
    let c4 = fixtures::synth_note(60, SAMPLE_RATE, FIXTURE_SECS);
    let c4_partials = analyzer.analyze(&c4, c4_f0);
    let sixth = c4_partials
        .iter()
        .find(|p| p.n == 6)
        .expect("C4 should have a measurable 6th partial");
    assert!(
        sixth.freq_hz > 6.0 * c4_f0 * 1.001,
        "C4 6th partial ({:.2} Hz) should be stretched sharp of 6*f0 ({:.2} Hz)",
        sixth.freq_hz,
        6.0 * c4_f0
    );
}

/// Optional real-recording harness (issue #17: University of Iowa MIS samples).
///
/// The synthetic corpus is the committed, hermetic gate; committing multi-MB
/// real recordings would bloat the repo and cannot ship in the crates.io
/// package anyway. Instead, drop real mono WAVs named by note (e.g. `A4.wav`,
/// `C7.wav`) into `tests/fixtures/piano_samples/` and this test picks them up.
/// See that directory's README for sourcing. Absent samples -> no-op (CI stays
/// hermetic), never a false green on a note that was silently skipped.
#[test]
fn real_piano_samples_lock_when_present() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/piano_samples");
    let Ok(entries) = std::fs::read_dir(&dir) else {
        eprintln!("no piano_samples/ dir; skipping real-recording checks");
        return;
    };

    let mut checked = 0;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("wav") {
            continue;
        }
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let Some(expected) = Note::from_name(stem) else {
            continue; // not a note-named fixture
        };

        let mut source = WavAudioSource::open(&path).expect("open sample WAV");
        let sample_rate = source.sample_rate();
        let detector = PitchDetector::new(sample_rate);
        let temperament = Temperament::new();
        let mut filter = MedianFilter::new(MedianFilter::DEFAULT_WINDOW);
        let window = sample_rate as usize / 10;
        let mut buffer = vec![0.0f32; window];
        let mut smoothed = Vec::new();
        loop {
            let read = source.read_samples(&mut buffer);
            if read < window {
                break;
            }
            match detector.detect(&buffer[..read]) {
                Ok(r) => smoothed.push(filter.push(r.frequency)),
                Err(_) => filter.clear(),
            }
        }
        assert!(
            !smoothed.is_empty(),
            "{stem}: no pitch detected in real sample"
        );
        smoothed.sort_unstable_by(f32::total_cmp);
        let f_lock = smoothed[smoothed.len() / 2];
        let (midi, _) = temperament
            .nearest_note(f_lock)
            .unwrap_or_else(|| panic!("{stem}: locked out of range at {f_lock:.1} Hz"));
        assert_eq!(
            midi,
            expected.midi,
            "{stem}: expected {} but detected {}",
            expected.display_name(),
            key_label(midi)
        );
        checked += 1;
    }

    if checked == 0 {
        eprintln!("piano_samples/ present but held no note-named WAVs; skipping");
    }
}
