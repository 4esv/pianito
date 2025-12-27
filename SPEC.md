# onkey — CLI Piano Tuner

## Overview

`onkey` is a terminal-based piano tuning application for macOS. It provides guided, coached tuning with real-time visual feedback, designed to help anyone tune an upright piano to either concert pitch or a "quick tune" relative to the piano's current center.

## Design Philosophy

- **Guided experience**: Not just a meter — a tuning coach that tells you what to do next
- **Handle complexity for the user**: Trichord management, tuning order, stretch tuning — all handled automatically
- **Clear visual feedback**: Tall, expressive ASCII meter using box-drawing characters
- **TDD from the ground up**: Audio code is hard to debug by ear; comprehensive test coverage is mandatory

## Modes

### Quick Tune
Detects the piano's current pitch center by sampling A4, then tunes all strings relative to that center. Ideal for practice pianos that have drifted but shouldn't be stressed with full tension adjustment.

### Concert Pitch
Tunes to A4 = 440Hz (or configurable). Applies stretch tuning compensation for proper inharmonicity handling.

### Advanced (accessible via flags)
- Custom A4 reference (e.g., 442Hz for orchestral work)
- Manual note selection
- Adjustable tolerance threshold

## CLI Interface

```bash
# Interactive guided tuning (main flow)
onkey

# Resume interrupted session
onkey --resume

# Quick tune mode
onkey --quick

# Concert pitch with custom A4
onkey --a4 442

# Analyze a recording (for testing/debugging)
onkey analyze <file.wav>

# Generate reference tone
onkey reference <note> [--duration <seconds>]

# Show tuning history
onkey history

# Clear saved sessions
onkey reset
```

## Tuning Order

Traditional piano tuning order for stability:

1. **Temperament octave (F3–F4)**: 12 notes, the foundation
2. **Octaves upward (F4→C8)**: Each note tuned as octave from below
3. **Octaves downward (F3→A0)**: Each note tuned as octave from above

This order is enforced by default. Tuning linearly would cause early work to drift as later string tension affects the frame.

## Trichord Handling

Most notes in the mid-upper range have three strings (trichords). For each trichord note, `onkey` guides through:

1. **Mute outer strings** (user uses felt strip or rubber mutes)
2. **Tune center string** to target pitch
3. **Unmute left string, tune to unison** with center (eliminate beats)
4. **Unmute right string, tune to unison** with center

The UI displays clear step-by-step instructions. User confirms each step with spacebar.

## Reference Tone System

**Alternating mode** to avoid interference:
1. Display "Listen to reference..." 
2. Play reference tone for 2 seconds
3. Silence, display "Now play and tune..."
4. Listen to mic input, show deviation meter
5. User can press `R` to replay reference at any time

Reference tones are pure sine waves at the target frequency.

## Visual Design

### Main Tuning Screen

```
╭─────────────────────────────────────────────────────────╮
│  onkey                                    F4 · 12/88    │
├─────────────────────────────────────────────────────────┤
│                                                         │
│  Target: F4 (349.23 Hz)                                 │
│  Detected: 347.91 Hz                                    │
│                                                         │
│            ♭                        ♯                   │
│   -50    -25     -10   0   +10    +25    +50           │
│    ┊      ┊       ┊    ┃    ┊      ┊      ┊            │
│    ┊      ┊       ┊    ┃    ┊      ┊      ┊            │
│    ┊      ┊      ▕████ ┃    ┊      ┊      ┊            │
│    ┊      ┊       ┊    ┃    ┊      ┊      ┊            │
│    ┊      ┊       ┊    ┃    ┊      ┊      ┊            │
│                   -6 cents                              │
│                                                         │
│  ┌───────────────────────────────────────────────────┐  │
│  │  Step 2 of 4: Tune center string                  │  │
│  │                                                   │  │
│  │  Turn tuning pin CLOCKWISE (tighten) slightly     │  │
│  └───────────────────────────────────────────────────┘  │
│                                                         │
├─────────────────────────────────────────────────────────┤
│  [Space] Confirm  [R] Reference  [S] Skip  [Q] Quit    │
╰─────────────────────────────────────────────────────────╯
```

### Meter Behavior

- **Vertical bars** using Unicode block characters (▏▎▍▌▋▊▉█)
- **Color coding**: 
  - Green: within ±5 cents (tolerance)
  - Yellow: ±5–15 cents
  - Red: beyond ±15 cents
- **Needle position** updates in real-time (~30fps visual refresh)
- **Stable reading**: Apply smoothing to avoid jitter; display "Listening..." if no clear pitch detected

### Audio Feedback Toggle

Off by default. When enabled (`--beep`), plays a short confirmation tone when pitch locks within tolerance for 1+ second.

## Session Persistence

Sessions save to `~/.local/share/onkey/sessions/`:

```json
{
  "id": "2024-01-15T14:30:00Z",
  "mode": "concert",
  "a4_reference": 440.0,
  "piano_offset_cents": 0,
  "current_note_index": 24,
  "completed_notes": [
    {"note": "F3", "final_cents": -1.2, "timestamp": "..."},
    {"note": "F#3", "final_cents": 0.5, "timestamp": "..."}
  ],
  "created_at": "...",
  "updated_at": "..."
}
```

`onkey --resume` loads the most recent incomplete session.

## Technical Architecture

```
onkey/
├── src/
│   ├── main.rs                 # Entry point, CLI parsing
│   ├── lib.rs                  # Library root for testing
│   ├── audio/
│   │   ├── mod.rs
│   │   ├── capture.rs          # Mic input via CoreAudio/cpal
│   │   ├── pitch.rs            # YIN algorithm for pitch detection
│   │   ├── reference.rs        # Sine wave generation for reference tones
│   │   └── traits.rs           # AudioSource/AudioSink traits for mocking
│   ├── tuning/
│   │   ├── mod.rs
│   │   ├── temperament.rs      # Equal temperament calculations
│   │   ├── stretch.rs          # Railsback curve / stretch tuning
│   │   ├── notes.rs            # 88-key note definitions, frequencies
│   │   ├── order.rs            # Tuning order logic (temperament-first)
│   │   └── session.rs          # Session state, persistence
│   ├── ui/
│   │   ├── mod.rs
│   │   ├── app.rs              # Main application state machine
│   │   ├── screens/
│   │   │   ├── mod.rs
│   │   │   ├── mode_select.rs  # Quick/Concert/Advanced selection
│   │   │   ├── calibration.rs  # Initial A4 detection
│   │   │   ├── tuning.rs       # Main tuning screen
│   │   │   └── complete.rs     # Session complete summary
│   │   ├── components/
│   │   │   ├── mod.rs
│   │   │   ├── meter.rs        # Cents deviation meter
│   │   │   ├── instructions.rs # Coaching text box
│   │   │   └── progress.rs     # Note progress indicator
│   │   └── theme.rs            # Colors, box-drawing chars
│   └── config.rs               # CLI args, configuration
├── tests/
│   ├── fixtures/
│   │   ├── synthetic/          # Generated test tones
│   │   └── piano_samples/      # Real piano recordings
│   ├── audio_tests.rs          # Pitch detection accuracy
│   ├── tuning_tests.rs         # Temperament math, stretch curves
│   ├── session_tests.rs        # Persistence, state transitions
│   └── integration_tests.rs    # End-to-end with mock audio
├── Cargo.toml
├── README.md
├── SPEC.md
└── .github/
    └── workflows/
        └── ci.yml              # GitHub Actions for macOS
```

## Dependencies

| Crate | Version | Purpose |
|-------|---------|---------|
| `cpal` | latest | Cross-platform audio I/O (CoreAudio on macOS) |
| `dasp` | latest | Sample format conversion, ring buffers |
| `rustfft` | latest | FFT for spectral analysis |
| `ratatui` | latest | Terminal UI framework |
| `crossterm` | latest | Terminal backend |
| `serde` | latest | Serialization |
| `serde_json` | latest | JSON for session files |
| `clap` | 4.x | CLI argument parsing |
| `directories` | latest | XDG-compliant paths |
| `hound` | latest | WAV file I/O (tests + analyze command) |
| `anyhow` | latest | Error handling |
| `thiserror` | latest | Custom error types |

## Pitch Detection Algorithm

**YIN algorithm** for monophonic pitch detection:

1. Compute difference function from autocorrelation
2. Apply cumulative mean normalization
3. Find first dip below threshold (typically 0.1)
4. Parabolic interpolation for sub-sample accuracy

YIN handles piano's inharmonicity better than pure FFT and works well with the quieter fundamental typical of piano strings.

**Detection parameters:**
- Sample rate: 44100 Hz
- Buffer size: 4096 samples (~93ms, sufficient for A0 at 27.5Hz)
- Overlap: 50% for smoother updates
- Confidence threshold: Reject detections below confidence score

## Stretch Tuning

Piano strings exhibit inharmonicity — higher partials are progressively sharper than ideal harmonics. Professional tuning compensates with "stretch tuning" based on the Railsback curve.

For `onkey`, implement a simplified stretch model:
- Bass notes tuned slightly flat (up to -20 cents at A0)
- Treble notes tuned slightly sharp (up to +20 cents at C8)
- Middle octaves (F3-F5) close to theoretical

Stretch values stored as lookup table, interpolated per-note.

## Testing Strategy

### Unit Tests

| Area | Tests |
|------|-------|
| Pitch detection | Synthetic sine waves at known frequencies (A0-C8), assert ±0.5Hz accuracy |
| Pitch detection | Sine + harmonics, verify fundamental extracted correctly |
| Temperament | Given A4=440, verify all 88 note frequencies |
| Temperament | Custom A4 (442), verify proportional shift |
| Stretch curve | Known inputs produce expected cent offsets |
| Cents math | Frequency → cents conversion, round-trip accuracy |
| Session | Serialize → deserialize preserves all fields |
| Tuning order | F3-F4 first, then up, then down |

### Integration Tests

| Scenario | Test |
|----------|------|
| Real piano samples | Pre-recorded notes, verify detection within tolerance |
| Noisy input | Piano + room noise, verify graceful degradation |
| Multi-note rejection | Two notes played, verify rejection or dominant selection |
| Silence handling | No input → "Listening..." state, no crash |
| Session resume | Create session, "interrupt", resume, verify continuity |

### Test Fixtures

Generate synthetic fixtures at build time or check in pre-generated WAVs:

```
tests/fixtures/
├── synthetic/
│   ├── a4_440hz.wav
│   ├── a4_442hz.wav
│   ├── a0_27.5hz.wav
│   ├── c8_4186hz.wav
│   ├── a4_with_harmonics.wav
│   └── two_notes_c4_e4.wav
├── piano_samples/
│   ├── upright_a4_tuned.wav
│   ├── upright_a4_10cents_flat.wav
│   └── upright_with_ambient.wav
└── edge_cases/
    ├── silence.wav
    ├── white_noise.wav
    └── speech.wav
```

### Mocking Strategy

Abstract audio I/O behind traits:

```rust
pub trait AudioSource: Send {
    fn read_samples(&mut self, buffer: &mut [f32]) -> usize;
    fn sample_rate(&self) -> u32;
}

pub trait AudioSink: Send {
    fn write_samples(&mut self, samples: &[f32]);
    fn sample_rate(&self) -> u32;
}
```

Tests inject `TestAudioSource` backed by WAV files or generated buffers.

## CI/CD

GitHub Actions workflow (`.github/workflows/ci.yml`):

```yaml
name: CI

on: [push, pull_request]

jobs:
  test:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - run: cargo fmt --check
      - run: cargo clippy -- -D warnings
      - run: cargo test --all-features
      - run: cargo build --release
```

macOS runner required for CoreAudio integration tests.

## Configuration Files

### `~/.config/onkey/config.toml` (optional)

```toml
# Default A4 reference
a4 = 440.0

# Default tolerance in cents
tolerance = 5

# Enable audio confirmation beep
beep = false

# Preferred tuning mode
default_mode = "concert"  # or "quick"
```

## Error Handling

- **No microphone access**: Clear error message, instructions to grant permission in System Preferences
- **No audio output device**: Skip reference tones gracefully, warn user
- **Corrupted session file**: Warn and offer to start fresh
- **Pitch detection failure**: Display "Listening..." or "No clear pitch detected", never crash

## Future Considerations (Out of Scope for v1)

- [ ] Multiple temperament support (Werckmeister, etc.)
- [ ] Inharmonicity measurement per-piano
- [ ] Beat rate display for interval tuning
- [ ] MIDI output for electronic tuning verification
- [ ] Record full tuning session audio for review
- [ ] Cross-platform support (Linux, Windows)

## Success Criteria

1. User can complete a full 88-key tuning session with guidance
2. Pitch detection accuracy within ±1 cent on clean input
3. Session can be interrupted and resumed without data loss
4. All core logic has >80% test coverage
5. UI is responsive (<100ms latency from sound to visual update)
6. Works with built-in MacBook microphone (no special hardware required)

---

*Spec version: 1.0*
*Last updated: December 2024*
