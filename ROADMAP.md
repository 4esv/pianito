# Roadmap

From "bare minimum CLI that works for a couple of octaves" to **The Piano Tuning CLI**.
Filed 2026-07-04 as GitHub [milestones](https://github.com/4esv/pianito/milestones) Stage 0–7, issues #5–#43.
This file is the one-page view; each issue body carries the audit evidence (file:line) for its item.

## Why this shape

Findings from the July 2026 codebase audit + landscape research:

- **The niche is verified empty.** No terminal piano tuner exists anywhere (GitHub topics,
  crates.io, HN archives checked). "First piano tuner for the terminal" is a factual claim.
- **The range limit is arithmetic, not tuning.** The fixed 100 ms window gives YIN 1.75 periods
  of A0 (needs ~2); treble frequency comes from integer lag steps — 165 cents per step at C8.
  The test suite stays green because every fixture is a pure sine; real pianos have weak bass
  fundamentals and inharmonic partials.
- **Per-piano inharmonicity is table stakes.** Entropy Piano Tuner, Verituner, TuneLab, and
  PianoMeter all lead with it. The bass detection fix and the inharmonicity engine share the
  FFT partial analyzer (#13), so Stage 2 reuses Stage 1's hardest component.
- **Linux is nearly free.** The code is already portable (`directories` paths, backend-generic
  cpal, zero `cfg(target_os)`); only CI, docs, and a smoke test are missing.
- **Release stack is settled.** `dist` for binaries + shell installer, own Homebrew tap
  (homebrew-core's self-submission bar is 225 stars), no musl builds (ALSA requires dlopen).
  Real MSRV is 1.82, not the README's claimed 1.70.

## Stages

| Stage | Theme | Issues | Version gate |
|-------|-------|--------|--------------|
| [0](https://github.com/4esv/pianito/milestone/1) | Housekeeping — publishable metadata, community files, honest MSRV | #5–#9 | — |
| [1](https://github.com/4esv/pianito/milestone/2) | All 88 keys — bass partials, treble spectral refinement, real fixtures | #10–#17 | v0.2.0 |
| [2](https://github.com/4esv/pianito/milestone/3) | Inharmonicity engine — per-piano stretch from measured B | #18–#23 | v0.3.0 |
| [3](https://github.com/4esv/pianito/milestone/4) | Linux — CI matrix, ALSA deps, docs | #24–#26 | — |
| [4](https://github.com/4esv/pianito/milestone/5) | TUI professionalization — hero meter, help overlay, 80×24, NO_COLOR | #27–#34 | v0.4.0 |
| [5](https://github.com/4esv/pianito/milestone/6) | Ship — dist binaries, Homebrew tap, crates.io | #35–#37 | — |
| [6](https://github.com/4esv/pianito/milestone/7) | Docs & demo — README overhaul, GIF, tuning-theory explainer | #38–#40 | — |
| [7](https://github.com/4esv/pianito/milestone/8) | Launch — awesome lists, Terminal Trove, Show HN + r/rust | #41–#43 | v1.0.0 |

## Dependency spine

- **#13 (FFT partial analyzer) is the highest-leverage issue** — it feeds both #15
  (bass f0 estimation) and #23 (inharmonicity engine). Build it once.
- Stage 1 → Stage 2 is a hard dependency: profiling quality depends on detection quality,
  and the engine consumes the analyzer.
- Within Stage 4: #27 (TuningFlow extraction) goes first — everything else lands on top;
  #32 (hero meter animations) needs #28 (worker thread + 60fps tick).
- **#42 (Show HN) is gated on Stages 1, 2, 4, 6 done.** Accuracy ships before marketing —
  HN will refute a tuner that can't tune A0 within the hour.
- Stage 3 is cheap and independent — land it early so CI protects Linux from day one.

## Out-of-order quick wins

- #6 — publish 0.1.x to claim the `pianito` crate name (verified free 2026-07-04).
- #7 — repo topics + description: GitHub's `piano-tuning` topic is empty; five minutes to be
  the #1 result.
