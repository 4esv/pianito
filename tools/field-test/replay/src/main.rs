//! Replays recorded takes through pianito's real detection paths.
//!
//! Usage:
//!   replay <wav> <midi> [<wav> <midi> ...]   one JSON line per take
//!   replay --railsback                       "<midi> <cents>" per key
//!
//! Per take it runs, frame by frame over [T0, 3 s] with a 20 ms hop and the
//! worker's 400 ms window:
//! - `guided`: `detect_for_target` (Tuning mode's path) + median filter
//! - `full`: `detect` (Profile mode's path, no target) + median filter
//! - `fit_*`: Profile mode's partial capture + `fit_note_inharmonicity`
//!
//! Cents are against 440 ET. Env: `T0` overrides the start offset in seconds.
//! The default is register-aware, matching `truth.py`'s analysis window (bass
//! 0.25 s / mid 0.15 s / treble 0.05 s), so fast-decaying treble is measured
//! from its loud attack rather than its silent tail (issue #85).
//! `GUIDED_ONLY=1` skips the slow full-range path.

use pianito::audio::{MedianFilter, PartialAnalyzer, PitchDetector};
use pianito::tuning::inharmonicity::fit_note_inharmonicity;
use pianito::tuning::StretchCurve;
use serde_json::{json, Value};

fn et(midi: u8) -> f32 {
    440.0 * 2f32.powf((midi as f32 - 69.0) / 12.0)
}

fn cents(f: f32, target: f32) -> f32 {
    1200.0 * (f / target).log2()
}

/// Register-aware default analysis start offset, mirroring `truth.py`'s
/// window so the replay measures the same loud part of the strike the
/// independent reference does. Issue #85: the old flat 0.5 s default started
/// after a top-octave note's fast decay, so C7-C8 read 0% even though the
/// detector resolves them.
fn default_t0(midi: u8) -> f32 {
    if midi < 48 {
        0.25 // bass: long strings, want resolution
    } else if midi < 84 {
        0.15 // mid
    } else {
        0.05 // treble: decays fast, measure the attack
    }
}

fn summarize(v: &[f32]) -> Value {
    if v.is_empty() {
        return Value::Null;
    }
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.total_cmp(b));
    let q = |p: f32| s[((s.len() - 1) as f32 * p).round() as usize];
    json!({"n": s.len(), "med": q(0.5), "p10": q(0.1), "p90": q(0.9), "min": s[0], "max": s[s.len() - 1]})
}

fn replay(path: &str, midi: u8, t0_secs: f32, guided_only: bool) -> Value {
    let mut reader = hound::WavReader::open(path).expect("open wav");
    let sr = reader.spec().sample_rate;
    let x: Vec<f32> = match reader.spec().sample_format {
        hound::SampleFormat::Float => reader.samples::<f32>().map(|s| s.unwrap()).collect(),
        hound::SampleFormat::Int => reader
            .samples::<i32>()
            .map(|s| s.unwrap() as f32 / i16::MAX as f32)
            .collect(),
    };
    let det = PitchDetector::new(sr);
    let pa = PartialAnalyzer::new(sr);
    let target = et(midi);
    let win = PitchDetector::max_window_samples(sr);
    let hop = sr as usize / 50;
    let register = PitchDetector::window_for_target(target);
    let register_len = (sr as f64 * register.as_secs_f64()).round() as usize;

    let mut guided_filter = MedianFilter::new(MedianFilter::DEFAULT_WINDOW);
    let mut full_filter = MedianFilter::new(MedianFilter::DEFAULT_WINDOW);
    let (mut guided, mut full, mut fit_b, mut fit_c) = (vec![], vec![], vec![], vec![]);
    let (mut frames, mut guided_fail, mut full_fail) = (0, 0, 0);

    let start = (t0_secs * sr as f32) as usize;
    let stop = x.len().min(3 * sr as usize);
    let mut end = start;
    while end <= stop {
        let w = &x[end.saturating_sub(win)..end];
        frames += 1;
        match det.detect_for_target(w, target) {
            Ok(p) => {
                let f = guided_filter.push(p.frequency);
                // Same gate App::update_pitch applies before the meter moves
                if p.confidence > 0.6 {
                    guided.push(cents(f, target));
                }
            }
            Err(_) => guided_fail += 1,
        }
        if !guided_only {
            match det.detect(w) {
                // PitchWorker filters every Ok result; App gates on confidence
                Ok(p) => {
                    let f = full_filter.push(p.frequency);
                    if p.confidence <= 0.6 {
                        end += hop;
                        continue;
                    }
                    full.push(cents(f, target));
                    // App::capture_profiling_partials: register-trimmed window
                    let tw = if w.len() > register_len {
                        &w[w.len() - register_len..]
                    } else {
                        w
                    };
                    if let Some(fit) = fit_note_inharmonicity(&pa.analyze(tw, f)) {
                        fit_b.push(fit.b.max(1e-7).log10());
                        fit_c.push(cents(fit.f0, target));
                    }
                }
                Err(_) => full_fail += 1,
            }
        }
        end += hop;
    }

    json!({
        "path": path, "midi": midi, "target": target, "frames": frames,
        "guided": summarize(&guided), "guided_fail": guided_fail,
        "full": summarize(&full), "full_fail": full_fail,
        "fit_log10b": summarize(&fit_b), "fit_cents": summarize(&fit_c),
    })
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("--railsback") {
        let curve = StretchCurve::railsback_default();
        for midi in 21u8..=108 {
            println!("{} {:.2}", midi, curve.offset_cents(midi));
        }
        return;
    }
    let t0_override: Option<f32> = std::env::var("T0").ok().and_then(|v| v.parse().ok());
    let guided_only = std::env::var("GUIDED_ONLY").is_ok();
    for pair in args.chunks(2) {
        let midi: u8 = pair[1].parse().expect("midi number");
        let t0 = t0_override.unwrap_or_else(|| default_t0(midi));
        println!("{}", replay(&pair[0], midi, t0, guided_only));
    }
}
