# pianito

**A piano tuner for the terminal.** macOS and Linux, with guided coaching.

[![CI](https://github.com/4esv/pianito/actions/workflows/ci.yml/badge.svg)](https://github.com/4esv/pianito/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/pianito.svg)](https://crates.io/crates/pianito)
[![downloads](https://img.shields.io/crates/d/pianito.svg)](https://crates.io/crates/pianito)
[![license: MIT](https://img.shields.io/crates/l/pianito.svg)](./LICENSE)

![pianito — a piano tuner for the terminal](docs/demo.gif)

pianito is a command-line piano tuner. It does real-time pitch detection (the
YIN algorithm), measures each piano's inharmonicity from its own overtones, and
tunes to an equal-tempered scale with a Railsback stretch curve — then walks you
through the strings note by note. No GUI, no phone, no account. A microphone, a
terminal, and a piano.

If you searched "CLI piano tuner" or "open source piano tuning app" and landed
here: as far as I can tell there isn't another terminal piano tuner, so this is
the one.

## What it stacks up against

The professional apps — **Entropy Piano Tuner** (open source), **Verituner**,
**TuneLab**, and **PianoMeter** — set the vocabulary piano technicians scan for:
inharmonicity measurement, per-piano stretch, temperaments, pitch raise, a live
partial display. Here is honestly where pianito stands against that bar today.

| Capability | pianito | Notes |
|---|:---:|---|
| Real-time pitch detection | ✅ | YIN fundamental + FFT partial analysis, with an octave sanity check |
| Inharmonicity measurement | ✅ | fits the stiffness coefficient *B* per note from recorded partials during Profile mode |
| Per-piano stretch tuning | ✅ | the measured curve drives the target frequencies; a Railsback default is the fallback |
| Equal temperament | ✅ | A4 configurable (concert pitch or the piano's own center) |
| Guided multi-string coaching | ✅ | the mute-and-match unison workflow — pianito's own thing; the pro apps measure, they don't coach |
| Scriptable CLI | ✅ | analyze a WAV, emit a reference tone, dump session history — composable with the rest of your shell |
| Historical / well temperaments | ❌ *not yet* | Verituner and TuneLab ship dozens; pianito is equal-temperament only |
| Pitch raise / overpull | ❌ *not yet* | TuneLab and Verituner compute overpull targets; pianito does not |
| Live spectrum / phase scope | ❌ | partials are measured during profiling, not shown as a running analyzer |
| Mobile / desktop GUI | ❌ *by design* | it runs in a terminal |

Short version: pianito already does the two things that separate a real tuner
from a chromatic-tuner app — it measures a specific piano's inharmonicity and
stretches the scale to fit it — and it adds step-by-step unison coaching the
others don't. It does not yet do historical temperaments or pitch-raise
overpull, and it will never be a phone app.

## Installation

### Homebrew (macOS)

```bash
brew install 4esv/tap/pianito
```

### Shell installer (macOS / Linux)

```bash
curl --proto '=https' --tlsv1.2 -LsSf https://github.com/4esv/pianito/releases/latest/download/pianito-installer.sh | sh
```

### cargo

```bash
cargo install pianito
```

### cargo-binstall (prebuilt binary, no compile)

```bash
cargo binstall pianito
```

### From source

Requires Rust 1.82+ and a working microphone. On Linux, also install the ALSA
development headers and `pkg-config` first (cpal's ALSA backend links against
`libasound` at build time):

```bash
# Debian/Ubuntu
sudo apt install libasound2-dev pkg-config
```

macOS needs nothing extra — it links CoreAudio directly.

```bash
git clone https://github.com/4esv/pianito.git
cd pianito
cargo build --release
```

The binary will be at `target/release/pianito`.

On a minimal Linux install, the runtime also needs `libasound2` (present on
every desktop distro; `sudo apt install libasound2` if it isn't).

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
| `?` | Toggle the help overlay |
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

Configuration is read from the pianito config directory:

- macOS: `~/Library/Application Support/pianito/config.toml`
- Linux: `~/.config/pianito/config.toml` (or `$XDG_CONFIG_HOME/pianito/config.toml`)

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
profile is saved under the pianito data directory:

- macOS: `~/Library/Application Support/pianito/profiles`
- Linux: `~/.local/share/pianito/profiles` (or `$XDG_DATA_HOME/pianito/profiles`)

Then tuning starts with the order reshuffled: the temperament octave (F3-F4)
stays first, and the remaining notes follow worst-deviation-first.

On the profiling screen: `Space` confirms the current note, `B` goes back,
`S` skips a note, `Q`/`Esc` quits.

Profiling also records each note's partial spectrum, which pianito fits
into a per-piano inharmonicity curve for stretch tuning - see
[`docs/inharmonicity.md`](docs/inharmonicity.md) for why that stretch is
necessary and how it's measured.

## Requirements

- Working microphone with permissions granted
- Terminal with Unicode support

| | macOS | Linux |
|---|---|---|
| Audio backend | CoreAudio, via cpal - works out of the box | ALSA, via cpal |
| Build-time deps | none beyond Rust | `libasound2-dev`, `pkg-config` |
| Runtime deps | none beyond the OS | `libasound2` |

The codebase itself has no `cfg(target_os)` branches and no macOS-specific
APIs - audio I/O goes through cpal and paths through the `directories` crate,
so Linux support falls out of the same code, not a separate port. CI builds and
tests on both macOS and Ubuntu (see
[`.github/workflows/ci.yml`](.github/workflows/ci.yml)).

One caveat: cpal's ALSA backend `dlopen`s `libasound` at runtime, so
statically-linked musl builds aren't supported - use the glibc target (the
prebuilt Linux binary already does).

## License

MIT
