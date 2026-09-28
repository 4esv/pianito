# Field test: Everett spinet, 2026-09-26

The first run of pianito on a real piano, scored against an independent
measurement of the same audio. It produced nine issues, [#80–#88](https://github.com/4esv/pianito/issues?q=label%3Afield-test).

## Setup

- **Piano.** An Everett spinet ("Dyna-Tension" scale), 88 keys: old, free, and
  out of tune.
- **Mic.** The MacBook Pro's built-in mic, laptop resting on the open lid.
  Room noise −56 dB, strikes peaking at −7 to −18 dB. The room has a steady
  hum at 33, 54 and 59 Hz.
- **pianito.** v0.1.0 (`src/` is unchanged since).
- **Method.** [`tools/field-test/`](../../tools/field-test/). A spoken
  conductor recorded one WAV per key. Each take was replayed through
  pianito's detection paths and compared against `truth.py`, an independent
  inharmonic partial fit. Then a real 21-note tuning session in the TUI was
  recorded alongside and scored.

Audio is kept locally, not in the repo (it includes room speech). A
single-note set could be published on request.

## The piano

Cents vs A4 = 440, from the 88-key sweep (medians):

| Range | Offset | Notes |
|---|---|---|
| A0–C1 | unreliable | weak and hum-masked; A0 sounds near 29.4 Hz |
| C#1–B1 | −84 | the whole wound section is about a semitone flat |
| C2–B2 | −20 | sharp break at C2 |
| C3–G#4 | −12 | the part that held; D#3 −88 is an outlier |
| A4–C6 | −82 | |
| C#6–C8 | −88 | |

Unisons: the strings of one note are 19 c apart (median of 56 notes), up to
48 c. Measured B (median): 0.00060 for C#1–B1, 0.00053 for C2–B2, 0.00047
for C3–B4.

It was tuned at **A4 = 430**. At 440, 49 keys needed raising by more than
50 c, and pianito has no pitch-raise or overpull support. At 430 the median
move is 35 c.

## pianito vs the independent fit

- **Tuning path (`detect_for_target`), C2–C4:** within ±3 c.
- **Tuning path, C#4–C6:** reads 5–25 c sharp of the fit. These are
  three-string notes with unisons about 20 c apart, where a single "pitch" is
  ill-defined. With A4 muted to one string, pianito read −67.6 against −70.4
  (center) and −54.2 against −56.2 (left). **Unresolved.** Re-measure with
  single strings before calling it a bug.
- **Held up fine:** pedal down, tremolo, talking over the note, A1 soft and
  loud (100% readings, both within 1 c of each other).
- **Expected misses:** staccato A4 gets no reading after 0.5 s. The note is
  gone.

## Live tuning session

21 notes, F3–C#5, Concert mode, `--a4 430`, 46 min. Scored with
`live_result.py`: the last stable reading per note, from pianito's own
detector on the recorded audio.

| | Before | After |
|---|---|---|
| Median \|cents\| from target | 26 | 6 |
| Within ±5 c | – | 8 / 21 |
| Worst | – | B4 +37, G#4 −31, D4 +16, G4 +11 |

The saved session claims 18 of the 21 notes at exactly 0.0 c ([#80](https://github.com/4esv/pianito/issues/80)).

## Findings

| # | Finding | Severity |
|---|---|---|
| [#80](https://github.com/4esv/pianito/issues/80) | Confirm after decay saves 0.0 c; history is fiction | high |
| [#81](https://github.com/4esv/pianito/issues/81) | A note at +61 c confirms without a warning | medium |
| [#82](https://github.com/4esv/pianito/issues/82) | The key an octave below reads as in tune (A3 on A4 target, 5/5) | high |
| [#83](https://github.com/4esv/pianito/issues/83) | Profile mode: full-range path, 0% in the low bass, octave errors | high |
| [#84](https://github.com/4esv/pianito/issues/84) | Bass window ±100 c loses a semitone-flat piano | high |
| [#85](https://github.com/4esv/pianito/issues/85) | F#7–C8: 0% readings; C7 at 25% | high |
| [#86](https://github.com/4esv/pianito/issues/86) | Profile partial fit is last-frame luck (10–20% garbage frames) | medium |
| [#87](https://github.com/4esv/pianito/issues/87) | Quitting a profile midway discards it | medium |
| [#88](https://github.com/4esv/pianito/issues/88) | Railsback default not anchored at A4 (+0.84 c) | low |

## Retracted during analysis

- **"Profile mode reads A4 an octave low."** Those takes were A3. The A3/A4
  mix-up is the real bug, #82.
- **"Room hum poses as A0."** Hum does sit at 59 Hz, but A0's partial series
  (88, 118, 148, 179, 210 Hz) is a real string at about 29.4 Hz. pianito and
  the fit agreed.

## Lessons for the next run

- Unmute the Mac first. The spoken prompts were silent at the start and the
  noise floor got measured over playing.
- The conductor's wrong-key gate (±250 c) lets one-key slips and octave-below
  strikes through. Check odd takes with `peaks.py`.
- The screen capture froze on frame 1 for 53 min. Check a frame after a
  minute ([#39](https://github.com/4esv/pianito/issues/39)).
- Don't score the last strike before Space: players strike the next key
  before confirming.
- Players tire. A full 88-key Profile pass by hand is too long without
  autosave (#87).
