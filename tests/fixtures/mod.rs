//! Deterministic synthesizer for the 88-key detection corpus (issue #17).
//!
//! Every earlier fixture in this crate was a pure sine or an exact-integer
//! harmonic stack. Those hide the two mechanisms that make the real product
//! fail across roughly three octaves: weak or missing bass fundamentals, and
//! inharmonic (stiff-string) partials. This module models both, plus
//! register-appropriate partial rolloff, an attack transient, per-partial
//! exponential decay, and broadband noise, so a key the detector genuinely
//! cannot handle *stays* failed in the harness instead of being papered over
//! by an unrealistically clean tone.
//!
//! The corpus is generated at test time from committed code and a per-key seed
//! rather than committed as megabytes of WAVs: it is byte-for-byte reproducible
//! from a key's MIDI number, and it exercises the production decode path because
//! [`synth_wav`] encodes 16-bit PCM that the harness reads back through
//! [`pianito::audio::WavAudioSource`].

// Only the detection harness links this module; the WAV encoder and a couple of
// physics helpers are part of its public surface even when a given test binary
// does not touch every one.
#![allow(dead_code)]

use std::f32::consts::PI;
use std::io::Cursor;

use pianito::tuning::temperament::Temperament;

/// Production capture rate (matches `MicCapture` / the interactive loop).
pub const SAMPLE_RATE: u32 = 44_100;

/// Lowest / highest MIDI note on an 88-key piano (A0 .. C8).
pub const MIDI_MIN: u8 = 21;
pub const MIDI_MAX: u8 = 108;

/// Equal-temperament fundamental (A4 = 440) for a MIDI note. This is the target
/// the harness scores detected pitch against — the synthesizer places partial 1
/// exactly here.
pub fn fundamental_hz(midi: u8) -> f32 {
    Temperament::new().frequency(midi)
}

/// Inharmonicity coefficient `B` for a MIDI note.
///
/// Stiff strings push partial `n` sharp of `n*f0` by `sqrt(1 + B*n^2)`. `B` is
/// U-shaped across the keyboard: largest in the bass (short, stiff, wound
/// strings), smallest around the tenor break (~C4), and rising again into the
/// treble (short plain-wire strings). Anchored to the figures in issue #17
/// (`B ~ 0.0004` mid, `~0.01` low bass) and interpolated log-linearly because
/// `B` spans two orders of magnitude.
pub fn inharmonicity_b(midi: u8) -> f32 {
    const A0: f32 = MIDI_MIN as f32; // 21
    const C4: f32 = 60.0;
    const C8: f32 = MIDI_MAX as f32; // 108
    const B_BASS: f32 = 0.010;
    const B_MID: f32 = 0.0004;
    const B_TREBLE: f32 = 0.005;

    let log_lerp = |m: f32, m0: f32, m1: f32, b0: f32, b1: f32| {
        let t = (m - m0) / (m1 - m0);
        (b0.ln() + t * (b1.ln() - b0.ln())).exp()
    };

    let m = midi as f32;
    if m <= C4 {
        log_lerp(m, A0, C4, B_BASS, B_MID)
    } else {
        log_lerp(m, C4, C8, B_MID, B_TREBLE)
    }
}

/// Linear gain applied to the fundamental (partial 1).
///
/// The bass fundamental radiates weakly and often sits 20–40 dB below its
/// partials through a microphone; issue #17 calls for an attenuated fundamental
/// across A0–B1 (MIDI 21–35). Ramps from near-absent at A0 up to full by C2.
pub fn fundamental_gain(midi: u8) -> f32 {
    if midi >= 36 {
        return 1.0;
    }
    // A0 (21) -> 0.05, B1 (35) -> ~0.94, C2 (36) -> 1.0.
    let t = (midi - MIDI_MIN) as f32 / 15.0;
    0.05 + 0.95 * t
}

/// Spectral rolloff exponent: partial `k` gets a base amplitude of `1/k^p`.
///
/// Register-appropriate and physically motivated: bass strings carry many
/// strong partials (shallow rolloff, ~-6 dB/octave), while treble strings are
/// nearly pure (steep rolloff, ~-16 dB/octave, and few partials fit under
/// Nyquist anyway). Ramped linearly in MIDI from `p=1.0` at A0 to `p=2.6` at
/// C8. This is what makes the corpus expose the detector's real accuracy
/// curve: the bright, partial-rich bass and the stiff treble both drag a
/// time-domain estimator sharp, while the mid register stays clean.
fn rolloff_exponent(midi: u8) -> f32 {
    let m = midi as f32;
    let t = (m - MIDI_MIN as f32) / (MIDI_MAX as f32 - MIDI_MIN as f32);
    1.0 + t * (2.6 - 1.0)
}

/// Overall exponential decay time constant (seconds).
///
/// Bass strings ring for seconds; treble notes decay fast. Higher partials of a
/// given note decay faster than the fundamental (`tau_k = tau / sqrt(k)`).
fn decay_tau(midi: u8) -> f32 {
    let m = midi as f32;
    let t = (m - MIDI_MIN as f32) / (MIDI_MAX as f32 - MIDI_MIN as f32);
    // A0 ~3.0 s down to C8 ~0.35 s.
    3.0 + t * (0.35 - 3.0)
}

/// Deterministic per-note PRNG (xorshift64), so the corpus is byte-for-byte
/// reproducible from the committed generator and a key's MIDI number.
struct Rng(u64);

impl Rng {
    fn new(seed: u64) -> Self {
        // Avoid the zero fixed point.
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.0 = x;
        x
    }
    /// Uniform in [-1, 1).
    fn bipolar(&mut self) -> f32 {
        (self.next_u64() as f64 / u64::MAX as f64) as f32 * 2.0 - 1.0
    }
    /// Uniform in [0, 1).
    fn unit(&mut self) -> f32 {
        (self.next_u64() as f64 / u64::MAX as f64) as f32
    }
}

/// Noise floor relative to the pre-noise signal peak: -30 dB (issue #17).
const NOISE_DB: f32 = -30.0;
/// Attack rise time constant (seconds).
const ATTACK_TAU: f32 = 0.004;
/// Hard cap on partials synthesized per note.
const MAX_PARTIALS: u16 = 40;
/// Partials are only synthesized below this fraction of the sample rate.
const NYQUIST_MARGIN: f32 = 0.45;

/// Synthesize a physically-flavored piano note for `midi`.
///
/// Models stiff-string inharmonicity, a register-dependent partial stack with a
/// weak/absent bass fundamental, an attack transient, per-partial exponential
/// decay, and -30 dB broadband noise. Deterministic in `midi`. Output is peak
/// normalized to +/-0.9 so 16-bit encoding does not clip.
pub fn synth_note(midi: u8, sample_rate: u32, duration_secs: f32) -> Vec<f32> {
    let f0 = fundamental_hz(midi);
    let b = inharmonicity_b(midi);
    let p = rolloff_exponent(midi);
    let tau = decay_tau(midi);
    let fund_gain = fundamental_gain(midi);
    let nyquist_cutoff = NYQUIST_MARGIN * sample_rate as f32;

    let n = (sample_rate as f32 * duration_secs) as usize;
    let mut samples = vec![0.0f32; n];

    let mut rng = Rng::new(midi as u64);

    // Pre-compute each partial's frequency, amplitude, per-partial decay tau,
    // and a random start phase (so partials do not all spike together at t=0).
    struct PartialOsc {
        freq: f32,
        amp: f32,
        tau: f32,
        phase: f32,
    }
    let mut oscs: Vec<PartialOsc> = Vec::new();
    for k in 1..=MAX_PARTIALS {
        let kf = k as f32;
        let freq = kf * f0 * (1.0 + b * kf * kf).sqrt();
        if freq >= nyquist_cutoff {
            break;
        }
        let mut amp = 1.0 / kf.powf(p);
        if k == 1 {
            amp *= fund_gain;
        }
        oscs.push(PartialOsc {
            freq,
            amp,
            tau: tau / kf.sqrt(),
            phase: rng.unit() * 2.0 * PI,
        });
    }

    for (i, out) in samples.iter_mut().enumerate() {
        let t = i as f32 / sample_rate as f32;
        let attack = 1.0 - (-t / ATTACK_TAU).exp();
        let mut acc = 0.0f32;
        for osc in &oscs {
            let env = (-t / osc.tau).exp();
            acc += osc.amp * env * (2.0 * PI * osc.freq * t + osc.phase).sin();
        }
        *out = attack * acc;
    }

    // Broadband noise at -30 dB relative to the (pre-noise) peak, plus a brief
    // extra burst inside the attack to mimic the hammer-strike transient.
    let peak = samples
        .iter()
        .fold(0.0f32, |m, &s| m.max(s.abs()))
        .max(1e-9);
    let noise_amp = peak * 10f32.powf(NOISE_DB / 20.0);
    let attack_samples = (0.01 * sample_rate as f32) as usize; // 10 ms
    for (i, out) in samples.iter_mut().enumerate() {
        let mut noise = noise_amp * rng.bipolar();
        if i < attack_samples {
            noise += 2.0 * noise_amp * rng.bipolar();
        }
        *out += noise;
    }

    // Peak-normalize to +/-0.9.
    let peak = samples
        .iter()
        .fold(0.0f32, |m, &s| m.max(s.abs()))
        .max(1e-9);
    let scale = 0.9 / peak;
    for s in &mut samples {
        *s *= scale;
    }

    samples
}

/// Encode mono `f32` samples to an in-memory 16-bit PCM WAV.
///
/// The harness reads these back through [`pianito::audio::WavAudioSource`], so
/// the corpus exercises the same decode path production uses on real files.
pub fn to_wav_i16(samples: &[f32], sample_rate: u32) -> Vec<u8> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut cursor = Cursor::new(Vec::new());
    {
        let mut writer = hound::WavWriter::new(&mut cursor, spec).unwrap();
        for &s in samples {
            let v = (s.clamp(-1.0, 1.0) * i16::MAX as f32) as i16;
            writer.write_sample(v).unwrap();
        }
        writer.finalize().unwrap();
    }
    cursor.into_inner()
}

/// Convenience: synthesize `midi` and return it as 16-bit PCM WAV bytes.
pub fn synth_wav(midi: u8, sample_rate: u32, duration_secs: f32) -> Vec<u8> {
    to_wav_i16(&synth_note(midi, sample_rate, duration_secs), sample_rate)
}
