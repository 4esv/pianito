# Real piano samples (optional)

The committed detection corpus (`tests/fixtures/mod.rs`) is **synthetic** and
hermetic: it models real piano physics (inharmonic partials, weak bass
fundamentals, register rolloff, decay, noise) and regenerates deterministically
at test time, so CI needs no network and the repo carries no audio. That is the
authoritative Stage 1 accuracy gate.

This directory is an **opt-in** hook for cross-checking against real recordings.
`tests/detection.rs::real_piano_samples_lock_when_present` scans it at test time:

- Drop **mono WAV** files named by note — `A0.wav`, `C1.wav`, `A1.wav`,
  `C4.wav`, `C6.wav`, `C7.wav`, `C8.wav`, etc. (the name is parsed by
  `Note::from_name`, so `C#4.wav` works too).
- The test locks each through the production detection path and asserts the
  detected note matches the filename.
- With no files present (the default, and CI), the test is a no-op — it never
  silently green-lights a note it did not actually check.

## Sourcing

Public-domain single-note piano recordings: the **University of Iowa Musical
Instrument Samples** (MIS), <https://theremin.music.uiowa.edu/MISpiano.html>.
Those are AIFF; convert to mono WAV and name by note, e.g.:

```sh
ffmpeg -i "Piano.ff.A4.aiff" -ac 1 A4.wav
```

These files are intentionally **git-ignored** (see `.gitignore` in this
directory) so large binaries never land in the repo or the crates.io package.
