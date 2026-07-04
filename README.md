# pianito

A terminal-based piano tuning application for macOS with guided coaching.

## Features

- **Real-time pitch detection** using the YIN algorithm
- **Visual cents deviation meter** with color-coded feedback
- **Guided multi-string tuning** with step-by-step coaching for bichords and trichords
- **Traditional tuning order** (temperament octave F3-F4 first, then up, then down)
- **Session persistence** - resume interrupted tuning sessions
- **Three tuning modes**:
  - **Concert Pitch** - tune to A4 = 440 Hz
  - **Quick Tune** - calibrate to the piano's current pitch center
  - **Profile Piano** - measure all 88 keys first, then tune worst notes first

## Installation

### From Source

Requires Rust 1.82+ and a working microphone.

```bash
git clone https://github.com/4esv/pianito.git
cd pianito
cargo build --release
```

The binary will be at `target/release/pianito`.

## Usage

### Interactive Tuning

```bash
# Start interactive tuning (opens the mode-select menu)
pianito

# Resume an interrupted session
pianito --resume

# Tune to A4 = 442 Hz with a lock beep
pianito --a4 442 --beep

# Open the menu with Quick Tune preselected
pianito --quick
```

Flags:

- `--a4 <HZ>` sets the reference frequency for Concert Pitch and Profile
  sessions (and the fallback when skipping Quick Tune calibration). Also
  applies to the `reference` and `analyze` subcommands. On `--resume`, the
  session's original A4 wins.
- `--quick` preselects Quick Tune in the mode-select menu.
- `--beep` plays a short beep the moment a string first enters the in-tune
  zone (once per strike; silence re-arms it). Requires an audio output
  device - without one, tuning continues and a warning shows in the status
  line.

### Keyboard Controls

| Key | Action |
|-----|--------|
| `↑/↓` / `Tab` | Navigate menu options |
| `Enter` | Select / Confirm |
| `Space` | Confirm current step / note |
| `B` | Back to previous step / note |
| `P` | Toggle piano progress display |
| `S` | Skip current note (on the Quick Tune calibration screen: skip calibration, use the configured A4 reference) |
| `Q` / `Esc` | Quit (progress is saved after each confirmed note) |

Reference tones are played with the `pianito reference` subcommand, not from
the tuning screen.

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

Configuration is read from the pianito config directory
(`~/Library/Application Support/pianito/config.toml` on macOS):

```toml
# Default A4 reference frequency (CLI --a4 overrides)
a4 = 440.0

# Tolerance in cents for the "in tune" zone on the meter,
# direction hints, and the lock beep
tolerance = 5.0

# Beep once when a note locks into the in-tune zone
# (CLI --beep also enables this)
beep = false

# Mode preselected in the menu: "concert" or "quick"
# (CLI --quick preselects quick)
default_mode = "concert"
```

## How It Works

1. **Pitch Detection**: Uses the YIN algorithm to detect the fundamental frequency from microphone input
2. **Temperament**: Calculates equal temperament frequencies with optional Railsback stretch curve
3. **Tuning Order**: Follows traditional piano tuning order for stability:
   - Temperament octave (F3-F4): 13 notes
   - Octaves upward (F#4-C8): 43 notes
   - Octaves downward (E3-A0): 32 notes
4. **Multi-string Coaching**: Coaches each note based on its string count:
   - A0-A#1 (1 string): tune directly
   - B1-G#3 (bichord, 2 strings): 2 steps - mute the right string, tune the left, then unmute and match the right
   - A3-C8 (trichord, 3 strings): 4 steps - mute the outer strings, tune the center, then left and right unisons

### Profile Mode

Profile mode measures the whole piano before tuning it. Play all 88 keys
(A0→C8) one at a time; pianito records each note's deviation in cents. The
profile is saved under the pianito data directory
(`~/Library/Application Support/pianito/profiles` on macOS), then tuning
starts with the order reshuffled: the temperament octave (F3-F4) stays first,
and the remaining notes follow worst-deviation-first.

On the profiling screen: `Space` confirms the current note, `B` goes back,
`S` skips a note, `Q`/`Esc` quits.

## Requirements

- macOS (uses CoreAudio via cpal)
- Working microphone with permissions granted
- Terminal with Unicode support

## License

MIT
