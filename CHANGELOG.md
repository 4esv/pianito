# Changelog

All notable changes to this project are documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-07-09

First public release.

### Added

**Pitch detection**

- Real-time pitch detection with the YIN algorithm, hardened with
  octave-error validation and a temporal median filter.
- FFT partial analyzer, shared by bass detection, treble refinement, and the
  inharmonicity engine.
- Partial-based bass f0 estimation for A0-B2 and spectral refinement for the
  treble, with per-note adaptive windows - all 88 keys detect reliably.
- An 88-key synthetic detection corpus (inharmonic partials, weak bass
  fundamentals, register rolloff, decay, noise) gates accuracy in CI.

**Tuning**

- Equal-temperament targets with selectable stretch: `off`, `railsback`
  (default), or `profile` - a per-piano curve fit from measured
  inharmonicity (stiffness coefficient B per note).
- Traditional tuning order: temperament octave F3-F4 first, then octaves up,
  then down.
- Guided multi-string coaching for bichords and trichords - the
  mute-and-match unison workflow.
- Three modes: Concert Pitch, Quick Tune (calibrates to the piano's own
  pitch center), and Profile Piano (measure all 88 keys, then tune
  worst-deviation-first). Profiling records each note's partial spectrum and
  measurement context (A4, target, confidence).

**Terminal UI**

- Hero tuning screen: note glyph, needle smoothing and trail, lock flash,
  an in-tune zone derived from the configured tolerance, and sub-tolerance
  drift display.
- Pitch worker thread with a 60 fps render tick.
- `?` help overlay, piano progress display, silence watchdog, and a
  multi-message status line.
- Theme hardening: `NO_COLOR` support, light-terminal palettes, and graceful
  degradation on small terminals.

**CLI and configuration**

- `analyze`, `reference`, `history`, and `reset` subcommands.
- Flags: `--a4`, `--quick`, `--beep`, `--resume`, `--stretch`.
- `config.toml`: `a4`, `tolerance`, `beep`, `default_mode`, `stretch`.

**Persistence and platform**

- Session persistence with schema versions and an explicit
  session-to-profile link; interrupted sessions resume.
- macOS and Linux (ALSA) support; CI builds and tests both, with `cargo
  audit` and MSRV (Rust 1.82) gates.
- Release binaries and installers via dist: shell installer and a Homebrew
  tap.

[unreleased]: https://github.com/4esv/pianito/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/4esv/pianito/releases/tag/v0.1.0
