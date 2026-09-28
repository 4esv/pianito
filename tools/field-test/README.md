# Field-test harness

Tools for testing pianito on a real piano and scoring it against an
independent measurement. They are not part of the crate. Report from the first
run: [`docs/field-tests/2026-09-26-everett-spinet.md`](../../docs/field-tests/2026-09-26-everett-spinet.md).

| Tool | Does |
|---|---|
| `conductor.py` | Hands-free recorder. Speaks each step (`say`), waits for the strike, saves one WAV per take, retries on silence, clipping or a far-off key. Steer it live with `echo pause\|resume\|redo\|skip\|back\|next\|quit > <dir>/ctl`. |
| `plans.py` | Writes the step plans: `setup`, `sweep`, `edges`, `recheck`, `record`. |
| `replay/` | Runs takes through pianito's real code: guided (Tuning), full-range (Profile), profile partial fit. `--railsback` dumps the default stretch curve. |
| `truth.py` | Independent reference: grid search + least squares for f0 and B, room-noise subtraction, unison spread. Not pianito code. |
| `sweep_report.py` | truth vs replay for a sweep, with `report.json`. |
| `live_result.py` | Scores a real TUI tuning session from a `record` capture made alongside it. |
| `peaks.py` | Raw spectral peak list, for when numbers disagree. |

## Run

macOS, `uv` on PATH, mic permission for the terminal. Unmute the output first,
or the prompts are silent.

```sh
cd tools/field-test
cargo build --release --manifest-path replay/Cargo.toml
python3 plans.py plans
D=~/pianito-field/$(date +%F)
uv run --script conductor.py plans/setup.json $D/p0_setup  # silence take = noise profile
uv run --script conductor.py plans/sweep.json $D/p1_sweep
python3 sweep_report.py $D/p1_sweep
```

For a live session, start `plans/record.json` into `$D/p3_live`, tune in the
TUI, then `live_result.py $D/p3_live <session.json> <a4>`.

## Gotchas (learned the hard way)

- **The wrong-key gate is ±250 c.** A one-key slip passes, and so does the key
  an octave below (its 2nd partial is right on target). When a take looks off,
  run `peaks.py` before believing it.
- **Noise floor.** It's measured from the quiet percentile and clamped at
  −45 dB, so playing during startup can't disable onset detection.
- **Don't score the last strike before Space** (see `live_result.py`).
- **Screen capture.** If you capture video alongside, check a frame ≥1 min
  in. avfoundation can freeze on frame 1 and keep writing.
