//! End-to-end binary smoke test (issue #25).
//!
//! Every other integration test in this crate calls into `pianito`'s library
//! API directly. That never touches the thing a packager, `cargo install`, or
//! CI's `cargo build --release` actually produces: the compiled binary,
//! wired up through `clap`, `main()`, and `Config::load()`. On Linux
//! specifically, nothing had ever run that binary before issue #25 - the
//! audit that cleared the library code found zero blockers (portable
//! `directories::ProjectDirs` paths, backend-generic `cpal` usage), but a
//! packaging or linking regression (e.g. a missing ALSA runtime lib, a
//! musl-static build breaking ALSA's dlopen'd plugins) would only show up by
//! actually executing the artifact.
//!
//! This drives `pianito analyze <wav>` specifically because it is the one
//! subcommand that never opens a live audio device: `analyze_file` decodes a
//! WAV via `WavAudioSource` and never touches `cpal::default_host()` (see
//! `src/audio/capture.rs`). `reference` and the interactive session both open
//! a real input/output stream, which CI runners' headless, deviceless
//! environment cannot support unattended - those stay a manual step on real
//! hardware per issue #25. `analyze` still exercises `Config::load()` (the
//! `directories`/XDG path from the same audit) since `main()` calls it before
//! dispatching on the subcommand.
//!
//! No `cfg(target_os)` here, matching the rest of the codebase: this test
//! runs unconditionally in `cargo test --all-features` on every OS in the CI
//! matrix, and the Ubuntu leg (issue #24) is what actually makes it a Linux
//! smoke test.

mod fixtures;

use std::io::Write;
use std::process::Command;

use fixtures::{fundamental_hz, synth_wav, SAMPLE_RATE};

/// A4, MIDI 69 - inside the detector's reliable register (C4-C6, see
/// `tests/detection.rs`) so a correct build locks both the note and the
/// cents accuracy, not just a rough frequency estimate.
const MIDI_A4: u8 = 69;

/// 0.7 s: matches the detection harness fixture length, comfortably longer
/// than `analyze_file`'s 250 ms chunk size in `main.rs`.
const FIXTURE_SECS: f32 = 0.7;

#[test]
fn analyze_subcommand_locks_a_synthesized_a4() {
    let expected_freq = fundamental_hz(MIDI_A4);
    assert!(
        (expected_freq - 440.0).abs() < 0.1,
        "sanity: MIDI 69 should synthesize as concert A4 (~440 Hz), got {expected_freq}"
    );

    let wav_bytes = synth_wav(MIDI_A4, SAMPLE_RATE, FIXTURE_SECS);
    let mut wav_file = tempfile::Builder::new()
        .suffix(".wav")
        .tempfile()
        .expect("create temp wav file");
    wav_file
        .write_all(&wav_bytes)
        .expect("write synthesized wav bytes");
    wav_file.flush().expect("flush wav file");

    let bin = env!("CARGO_BIN_EXE_pianito");
    let output = Command::new(bin)
        .arg("analyze")
        .arg(wav_file.path())
        .output()
        .unwrap_or_else(|e| panic!("failed to spawn compiled binary {bin}: {e}"));

    assert!(
        output.status.success(),
        "`pianito analyze` exited with {:?}\nstdout:\n{}\nstderr:\n{}",
        output.status.code(),
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr),
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("A4"),
        "expected the synthesized A4 fixture to be identified as A4; got:\n{stdout}"
    );
}
