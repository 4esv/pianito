# Test Coverage Analysis Report

**Date:** 2026-01-04
**Project:** Pianito - CLI Piano Tuner

## Executive Summary

Current test coverage is **31%** (9 out of 29 source files have tests). The core tuning logic has excellent coverage, but critical areas like configuration, application state machine, and UI components lack tests.

## Current Coverage

### Files WITH Tests (9 files - 31%)

| File | Test Count | Coverage Quality |
|------|-----------|------------------|
| `tuning/temperament.rs` | 15 | ✅ Excellent |
| `tuning/notes.rs` | 11 | ✅ Excellent |
| `tuning/order.rs` | 14 | ✅ Excellent |
| `tuning/session.rs` | 13 | ✅ Good |
| `tuning/profile.rs` | 6 | ✅ Good |
| `tuning/stretch.rs` | 7 | ✅ Good |
| `audio/pitch.rs` | 10 | ✅ Good |
| `ui/components/piano.rs` | 15 | ✅ Excellent |
| `audio/traits.rs` | 3 | ⚠️ Basic |

**Total:** ~94 unit tests

### Files WITHOUT Tests (20 files - 69%)

**Critical Business Logic:**
- ❌ `config.rs` - Configuration management
- ❌ `ui/app.rs` - Application state machine (539 lines!)

**Audio System:**
- ❌ `audio/capture.rs` - Microphone capture
- ❌ `audio/reference.rs` - Tone generation

**UI Components:**
- ❌ `ui/components/meter.rs` - Cents deviation meter (248 lines)
- ❌ `ui/components/progress.rs` - Progress indicators
- ❌ `ui/components/instructions.rs` - Instructions display

**UI Screens:**
- ❌ `ui/screens/calibration.rs`
- ❌ `ui/screens/mode_select.rs`
- ❌ `ui/screens/complete.rs`
- ❌ `ui/screens/profiling.rs`
- ❌ `ui/screens/tuning.rs`

**Infrastructure:**
- ❌ `ui/theme.rs` - UI theming
- ❌ `ui/mod.rs`, `main.rs`, `lib.rs`

## Critical Gaps

### 1. Config Module (HIGH PRIORITY)

**File:** `src/config.rs` (160 lines)
**Impact:** Affects entire application behavior

**What to test:**
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_default_config_values() {
        let config = Config::default();
        assert_eq!(config.a4, 440.0);
        assert_eq!(config.tolerance, 5.0);
        assert!(!config.beep);
        assert_eq!(config.default_mode, "concert");
    }

    #[test]
    fn test_merge_with_args_overrides_a4() {
        let config = Config::default();
        let args = Args {
            a4: Some(442.0),
            beep: false,
            quick: false,
            resume: false,
            command: None,
        };
        let effective = config.merge_with_args(&args);
        assert_eq!(effective.a4, 442.0);
    }

    #[test]
    fn test_config_load_missing_file_returns_default() {
        let config = Config::load();
        assert_eq!(config.a4, 440.0);
    }

    #[test]
    fn test_config_save_and_load_roundtrip() {
        let temp_dir = TempDir::new().unwrap();
        std::env::set_var("HOME", temp_dir.path());

        let mut config = Config::default();
        config.a4 = 442.0;
        config.save().unwrap();

        let loaded = Config::load();
        assert_eq!(loaded.a4, 442.0);
    }

    #[test]
    fn test_invalid_toml_falls_back_to_default() {
        // Test malformed config file handling
    }
}
```

### 2. Application State Machine (HIGH PRIORITY)

**File:** `src/ui/app.rs` (539 lines)
**Impact:** Core application logic and workflows

**What to test:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_initial_state() {
        let app = App::new();
        assert_eq!(app.state(), AppState::ModeSelect);
        assert!(!app.should_quit());
        assert!(app.session().is_none());
    }

    #[test]
    fn test_quit_sets_flag() {
        let mut app = App::new();
        app.quit();
        assert!(app.should_quit());
    }

    #[test]
    fn test_concert_mode_transitions_to_tuning() {
        let mut app = App::new();
        app.mode_select = ModeSelectScreen::new();
        // Set to concert mode
        app.start_session();
        assert_eq!(app.state(), AppState::Tuning);
        assert!(app.session().is_some());
    }

    #[test]
    fn test_note_completion_advances_index() {
        let mut app = App::new();
        // Setup in tuning state with first note
        let initial_idx = app.current_note_idx;
        app.confirm_note();
        assert_eq!(app.current_note_idx, initial_idx + 1);
    }

    #[test]
    fn test_go_back_decrements_index() {
        let mut app = App::new();
        // Setup with index > 0
        app.current_note_idx = 5;
        app.go_back();
        assert_eq!(app.current_note_idx, 4);
    }

    #[test]
    fn test_skip_note_marks_as_complete() {
        let mut app = App::new();
        // Setup in tuning state
        app.skip_note();
        // Verify session has completed note with 0 cents
    }

    #[test]
    fn test_session_completion_transitions_to_complete() {
        let mut app = App::new();
        app.current_note_idx = 87;
        app.confirm_note();
        assert_eq!(app.state(), AppState::Complete);
    }

    #[test]
    fn test_resume_session_restores_state() {
        let mut session = Session::new(TuningMode::Concert, 440.0);
        session.current_note_index = 10;

        let app = App::with_session(session);
        assert_eq!(app.state(), AppState::Tuning);
        assert_eq!(app.current_note_idx, 10);
    }

    #[test]
    fn test_pitch_update_in_tuning_state() {
        let mut app = App::new();
        // Setup in tuning state
        app.update_pitch(440.0, 0.95);
        // Verify tuning screen updated
    }

    #[test]
    fn test_pitch_update_ignored_low_confidence() {
        let mut app = App::new();
        // Setup in tuning state
        app.update_pitch(440.0, 0.3);
        // Verify no update occurred
    }
}
```

### 3. Meter Component (MEDIUM PRIORITY)

**File:** `src/ui/components/meter.rs` (248 lines)
**Impact:** User experience - displays tuning accuracy

**What to test:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_log_position_at_zero() {
        let pos = Meter::log_position(0.0, 500.0, 50.0, 5.0);
        assert_eq!(pos, 0.0);
    }

    #[test]
    fn test_log_position_within_tolerance() {
        let pos = Meter::log_position(3.0, 500.0, 50.0, 5.0);
        assert_eq!(pos, 0.0);
    }

    #[test]
    fn test_log_position_symmetry() {
        let pos_pos = Meter::log_position(50.0, 500.0, 50.0, 5.0);
        let pos_neg = Meter::log_position(-50.0, 500.0, 50.0, 5.0);
        assert!((pos_pos + pos_neg).abs() < 0.01);
    }

    #[test]
    fn test_log_position_bounds() {
        let pos = Meter::log_position(1000.0, 500.0, 50.0, 5.0);
        assert!(pos.abs() <= 50.0);
    }

    #[test]
    fn test_log_position_monotonic() {
        let p1 = Meter::log_position(10.0, 500.0, 50.0, 5.0);
        let p2 = Meter::log_position(50.0, 500.0, 50.0, 5.0);
        let p3 = Meter::log_position(100.0, 500.0, 50.0, 5.0);
        assert!(p1.abs() < p2.abs());
        assert!(p2.abs() < p3.abs());
    }

    #[test]
    fn test_meter_listening_state() {
        let meter = Meter::listening();
        assert!(!meter.detecting);
        assert_eq!(meter.cents, 0.0);
    }

    #[test]
    fn test_meter_with_custom_tolerance() {
        let meter = Meter::new(0.0).tolerance(10.0);
        assert_eq!(meter.tolerance, 10.0);
    }
}
```

### 4. Reference Tone Generator (MEDIUM PRIORITY)

**File:** `src/audio/reference.rs` (36 lines)
**Impact:** Audio output correctness

**What to test:**
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_correct_sample_count() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 1.0);
        assert_eq!(samples.len(), 44100);
    }

    #[test]
    fn test_generate_half_second() {
        let gen = ReferenceTone::new(48000);
        let samples = gen.generate(440.0, 0.5);
        assert_eq!(samples.len(), 24000);
    }

    #[test]
    fn test_sine_wave_range() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 0.1);

        let max = samples.iter().cloned().fold(f32::NEG_INFINITY, f32::max);
        let min = samples.iter().cloned().fold(f32::INFINITY, f32::min);

        assert!(max > 0.99 && max <= 1.0);
        assert!(min < -0.99 && min >= -1.0);
    }

    #[test]
    fn test_zero_crossings_match_frequency() {
        let gen = ReferenceTone::new(44100);
        let samples = gen.generate(440.0, 1.0);

        let mut crossings = 0;
        for i in 1..samples.len() {
            if samples[i-1] < 0.0 && samples[i] >= 0.0 {
                crossings += 1;
            }
        }

        // 440 Hz should have ~440 positive zero crossings in 1 second
        assert!((crossings as f32 - 440.0).abs() < 2.0);
    }
}
```

## Integration Tests Needed

Create `tests/integration_test.rs`:

```rust
use pianito::*;

#[test]
fn test_complete_tuning_workflow() {
    // 1. Start app
    // 2. Select concert mode
    // 3. Confirm several notes
    // 4. Verify session state
}

#[test]
fn test_profile_mode_workflow() {
    // 1. Start app
    // 2. Select profile mode
    // 3. Profile all 88 notes
    // 4. Verify tuning order based on deviations
}

#[test]
fn test_session_persistence() {
    // 1. Start tuning session
    // 2. Complete 10 notes
    // 3. Save session
    // 4. Load session
    // 5. Verify state restored
}

#[test]
fn test_quick_tune_calibration() {
    // Test calibration workflow
}
```

## Recommended Action Plan

### Phase 1: Critical Tests (Week 1)
1. ✅ Add Config module tests (5-8 tests)
2. ✅ Add App state machine tests (10-15 tests)
3. ✅ Add Meter calculation tests (6-8 tests)

### Phase 2: Medium Priority (Week 2)
4. ✅ Add ReferenceTone tests (4-6 tests)
5. ✅ Add integration tests (3-5 tests)
6. ✅ Add UI screen state tests

### Phase 3: Nice to Have (Week 3+)
7. ⚠️ Add property-based tests for math functions
8. ⚠️ Add benchmark tests for performance
9. ⚠️ Improve coverage in existing test files

## Test Infrastructure Improvements

### 1. Add Test Helpers

Create `tests/common/mod.rs`:
```rust
use pianito::tuning::{Session, TuningMode, PianoProfile};

pub fn mock_session() -> Session {
    Session::new(TuningMode::Concert, 440.0)
}

pub fn mock_profile() -> PianoProfile {
    let mut profile = PianoProfile::new();
    for midi in 21..=108 {
        profile.record_note(midi, 440.0 * 2.0_f32.powf((midi as f32 - 69.0) / 12.0), 0.0);
    }
    profile
}
```

### 2. Add Property-Based Testing

Update `Cargo.toml`:
```toml
[dev-dependencies]
tempfile = "3"
approx = "0.5"
proptest = "1.0"  # NEW
```

Example:
```rust
use proptest::prelude::*;

proptest! {
    #[test]
    fn cents_conversion_roundtrip(cents in -500.0f32..500.0) {
        let temp = Temperament::new();
        let freq = temp.cents_to_frequency(440.0, cents);
        let recovered = temp.cents_from_target(freq, 440.0);
        assert!((recovered - cents).abs() < 0.01);
    }
}
```

### 3. Add Code Coverage Reporting

Add to CI/CD:
```bash
cargo install cargo-tarpaulin
cargo tarpaulin --out Html --output-dir coverage
```

## Success Metrics

**Target Coverage:** 70% (currently 31%)

| Module | Current | Target |
|--------|---------|--------|
| `tuning/*` | 100% | 100% ✅ |
| `audio/*` | 40% | 80% |
| `ui/components/*` | 25% | 70% |
| `ui/screens/*` | 0% | 50% |
| `config.rs` | 0% | 90% |
| `ui/app.rs` | 0% | 80% |

## Conclusion

The codebase has **excellent coverage** of core tuning algorithms but **lacks tests** for:
- Configuration management
- Application state machine
- UI components and screens
- Integration workflows

Priority should be on testing business-critical logic (Config, App state) before UI components.
