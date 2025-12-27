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

- [ ] Define Note struct and 88-key note definitions in tuning/notes.rs
- [ ] Implement frequency calculation for equal temperament in tuning/temperament.rs
- [ ] Implement cents-to-frequency and frequency-to-cents conversion
- [ ] Implement custom A4 reference support
- [ ] Implement Railsback stretch curve in tuning/stretch.rs
- [ ] Implement tuning order logic in tuning/order.rs (temperament-first)
- [ ] Write tests for all 88 note frequencies at A4=440Hz
- [ ] Write tests for custom A4 (442Hz)
- [ ] Write tests for cents conversion round-trip
- [ ] Write tests for stretch curve values
- [ ] Write tests for tuning order (F3-F4 first, then up, then down)
- [ ] Verify all tuning tests pass

## Phase 4: Session Persistence (with tests)

- [ ] Define Session and CompletedNote structs in tuning/session.rs
- [ ] Implement serde serialization for session data
- [ ] Implement session save to ~/.local/share/onkey/sessions/
- [ ] Implement session load and resume logic
- [ ] Implement session history listing
- [ ] Implement session reset (clear all)
- [ ] Write tests for serialize/deserialize round-trip
- [ ] Write tests for session state transitions
- [ ] Write tests for resume finding most recent incomplete session
- [ ] Verify all session tests pass

## Phase 5: TUI Screens

- [ ] Set up ratatui with crossterm backend in ui/mod.rs
- [ ] Define color theme and box-drawing constants in ui/theme.rs
- [ ] Implement cents deviation meter component in ui/components/meter.rs
- [ ] Implement coaching instructions component in ui/components/instructions.rs
- [ ] Implement progress indicator component in ui/components/progress.rs
- [ ] Implement mode select screen in ui/screens/mode_select.rs
- [ ] Implement calibration screen in ui/screens/calibration.rs
- [ ] Implement main tuning screen in ui/screens/tuning.rs
- [ ] Implement complete/summary screen in ui/screens/complete.rs
- [ ] Implement app state machine in ui/app.rs
- [ ] Wire up keyboard input handling (Space, R, S, Q)
- [ ] Verify TUI renders correctly

## Phase 6: Integration + Polish

- [ ] Implement CLI argument parsing with clap in config.rs
- [ ] Implement config file loading from ~/.config/onkey/config.toml
- [ ] Wire main.rs entry point with all components
- [ ] Implement `onkey` interactive guided tuning flow
- [ ] Implement `onkey --resume` session resume
- [ ] Implement `onkey --quick` quick tune mode
- [ ] Implement `onkey --a4 <freq>` custom reference
- [ ] Implement `onkey analyze <file.wav>` command
- [ ] Implement `onkey reference <note>` command
- [ ] Implement `onkey history` command
- [ ] Implement `onkey reset` command
- [ ] Add proper error handling for mic/audio issues
- [ ] Write integration tests with mock audio
- [ ] Run full test suite and fix any failures
- [ ] Run clippy and fix warnings
- [ ] Run cargo fmt
- [ ] Verify `cargo build --release` succeeds
- [ ] Create PR with gh pr create
