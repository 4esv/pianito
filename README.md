# pianito

A terminal-based piano tuning application for macOS with guided coaching.

## Features

- **Real-time pitch detection** using the YIN algorithm
- **Visual cents deviation meter** with color-coded feedback
- **Guided trichord tuning** with step-by-step coaching for 3-string notes
- **Traditional tuning order** (temperament octave F3-F4 first, then up, then down)
- **Session persistence** - resume interrupted tuning sessions
- **Two tuning modes**:
  - **Concert Pitch** - tune to A4 = 440 Hz (or custom reference)
  - **Quick Tune** - calibrate to the piano's current pitch center

## Installation

### From Source

Requires Rust 1.70+ and a working microphone.

```bash
git clone https://github.com/4esv/pianito.git
cd pianito
cargo build --release
```

The binary will be at `target/release/pianito`.

## Usage

### Interactive Tuning

```bash
# Start interactive tuning (concert pitch mode)
pianito

# Resume an interrupted session
pianito --resume

# Quick tune mode (calibrates to piano's current pitch)
pianito --quick

# Custom A4 reference frequency
pianito --a4 442
```

### Keyboard Controls

| Key | Action |
|-----|--------|
| `↑/↓` | Navigate menu options |
| `Enter` | Select / Confirm |
| `Space` | Confirm note is tuned |
| `R` | Play reference tone |
| `S` | Skip current note |
| `Q` | Quit (saves session) |

### Commands

```bash
# Analyze a WAV file for pitch content
pianito analyze recording.wav

# Play a reference tone
pianito reference A4
pianito reference C5 --duration 3.0

# Show tuning session history
pianito history

# Clear all saved sessions
pianito reset
```

## Configuration

Configuration is stored at `~/.config/pianito/config.toml`:

```toml
# Default A4 reference frequency
a4 = 440.0

# Tolerance in cents for "in tune" indicator
tolerance = 5.0

# Enable beep on pitch lock
beep = false

# Default mode: "concert" or "quick"
default_mode = "concert"
```

## How It Works

1. **Pitch Detection**: Uses the YIN algorithm to detect the fundamental frequency from microphone input
2. **Temperament**: Calculates equal temperament frequencies with optional Railsback stretch curve
3. **Tuning Order**: Follows traditional piano tuning order for stability:
   - Temperament octave (F3-F4): 13 notes
   - Octaves upward (F#4-C8): 43 notes
   - Octaves downward (E3-A0): 32 notes
4. **Trichord Coaching**: For 3-string notes, guides through muting, center string, then unisons

## Requirements

- macOS (uses CoreAudio via cpal)
- Working microphone with permissions granted
- Terminal with Unicode support

## License

MIT
