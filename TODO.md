# onkey Implementation Plan

## Phase 1: Project Scaffolding + CI Setup

- [x] Initialize Cargo project with `cargo init`
- [x] Add all dependencies to Cargo.toml
- [x] Create directory structure (src/audio, src/tuning, src/ui, tests)
- [x] Create lib.rs with module declarations
- [x] Create placeholder mod.rs files for each module
- [x] Create .github/workflows/ci.yml for GitHub Actions
- [x] Verify `cargo build` and `cargo test` pass

## Phase 2: Audio Capture + Pitch Detection (with tests)

- [x] Implement AudioSource and AudioSink traits in audio/traits.rs
- [x] Implement TestAudioSource for mocking in tests
- [x] Implement WavAudioSource for reading WAV files
- [x] Implement mic capture with cpal in audio/capture.rs
- [x] Implement sine wave generation in audio/reference.rs
- [x] Implement YIN pitch detection algorithm in audio/pitch.rs
- [x] Create synthetic test fixtures generator (build script or test helper)
- [x] Write tests for pitch detection with synthetic sine waves
- [x] Write tests for pitch detection with harmonics
- [x] Write tests for confidence threshold rejection
- [x] Verify all audio tests pass

## Phase 3: Tuning Logic + Temperament Math (with tests)

- [x] Define Note struct and 88-key note definitions in tuning/notes.rs
- [x] Implement frequency calculation for equal temperament in tuning/temperament.rs
- [x] Implement cents-to-frequency and frequency-to-cents conversion
- [x] Implement custom A4 reference support
- [x] Implement Railsback stretch curve in tuning/stretch.rs
- [x] Implement tuning order logic in tuning/order.rs (temperament-first)
- [x] Write tests for all 88 note frequencies at A4=440Hz
- [x] Write tests for custom A4 (442Hz)
- [x] Write tests for cents conversion round-trip
- [x] Write tests for stretch curve values
- [x] Write tests for tuning order (F3-F4 first, then up, then down)
- [x] Verify all tuning tests pass

## Phase 4: Session Persistence (with tests)

- [x] Define Session and CompletedNote structs in tuning/session.rs
- [x] Implement serde serialization for session data
- [x] Implement session save to ~/.local/share/onkey/sessions/
- [x] Implement session load and resume logic
- [x] Implement session history listing
- [x] Implement session reset (clear all)
- [x] Write tests for serialize/deserialize round-trip
- [x] Write tests for session state transitions
- [x] Write tests for resume finding most recent incomplete session
- [x] Verify all session tests pass

## Phase 5: TUI Screens

- [x] Set up ratatui with crossterm backend in ui/mod.rs
- [x] Define color theme and box-drawing constants in ui/theme.rs
- [x] Implement cents deviation meter component in ui/components/meter.rs
- [x] Implement coaching instructions component in ui/components/instructions.rs
- [x] Implement progress indicator component in ui/components/progress.rs
- [x] Implement mode select screen in ui/screens/mode_select.rs
- [x] Implement calibration screen in ui/screens/calibration.rs
- [x] Implement main tuning screen in ui/screens/tuning.rs
- [x] Implement complete/summary screen in ui/screens/complete.rs
- [x] Implement app state machine in ui/app.rs
- [x] Wire up keyboard input handling (Space, R, S, Q)
- [x] Verify TUI renders correctly

## Phase 6: Integration + Polish

- [x] Implement CLI argument parsing with clap in config.rs
- [x] Implement config file loading from ~/.config/onkey/config.toml
- [x] Wire main.rs entry point with all components
- [x] Implement `onkey` interactive guided tuning flow
- [x] Implement `onkey --resume` session resume
- [x] Implement `onkey --quick` quick tune mode
- [x] Implement `onkey --a4 <freq>` custom reference
- [x] Implement `onkey analyze <file.wav>` command
- [x] Implement `onkey reference <note>` command
- [x] Implement `onkey history` command
- [x] Implement `onkey reset` command
- [x] Add proper error handling for mic/audio issues
- [x] Run full test suite and fix any failures
- [x] Run clippy and fix warnings
- [x] Run cargo fmt
- [x] Verify `cargo build --release` succeeds
- [ ] Create PR with gh pr create
