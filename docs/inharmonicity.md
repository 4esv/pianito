# Why pianos aren't tuned to 440 × 2^(n/12)

Equal temperament (ET) says the frequency of the note `n` semitones from A4
is:

```
f(n) = 440 * 2^(n/12)
```

That's the textbook target. No piano is actually tuned to it - not because
tuners are sloppy, but because the formula assumes an ideal string, and
piano strings aren't ideal. This doc explains why, and how pianito measures
the difference on your specific instrument.

## Strings are stiff, not ideal

The formula above comes from the physics of an idealized vibrating string:
massless, perfectly flexible, no resistance to bending. Its overtones
("partials") sit at exact integer multiples of the fundamental - 2×, 3×,
4×, and so on.

A real piano string has stiffness. Bending it costs energy on top of the
tension that sets its pitch, and that extra restoring force pushes every
partial sharp of where the ideal-string formula puts it - more sharp the
higher the partial:

```
f_k = k * f0 * sqrt(1 + B * k^2)
```

`f0` is the fundamental, `k` the partial number (1 = fundamental, 2 = first
overtone, ...), and `B` the *inharmonicity coefficient* - a single number
per string that captures how much stiffness bends its overtone series. `B`
depends on the string's diameter, length, and tension: thick/short strings
push `B` up, long/thin ones push it down. The well-scaled tenor, where
strings are longest relative to their diameter, tends to have the smallest
`B`; both the bass and the treble - for different construction reasons -
run higher.

Real `B` values are lowest in the well-scaled tenor/mid-treble - typically
around `0.0001` - and rise toward both ends of the keyboard, but not
symmetrically. Copper winding in the bass adds mass without adding much
stiffness, so wound bass strings hold `B` down to roughly `0.0002`-`0.002`
even at the bottom of a well-designed piano (short-scaled small pianos,
where the bass strings are cramped for their pitch, run higher). The
unwound treble has no such trick: `B` climbs from `~0.001` in the upper-mid
treble to the order of `0.01` at the very top of the keyboard. `B` spans two
orders of magnitude across a single instrument, which turns out to matter
for how pianito fits it (see below).

## Tuning by beats, not by matching fundamentals

Nobody tunes a piano by holding an ET frequency table up to a tuning fork
and matching fundamentals in isolation - you can't hear a fundamental in
isolation anyway once other notes are ringing. Tuners tune by listening for
**beats**: the slow wah-wah-wah that two nearly-but-not-quite-coincident
frequencies produce when they sound together. Zero beats means the two
frequencies match exactly; the tuner's job is to place each string so the
beats they care about - between specific partials of specific note pairs -
slow to zero, or to a specific, choreographed rate.

The classic case is the octave. Tune an octave by ear and you're not
matching `f0` of the upper note to `2 * f0` of the lower note - you're
listening for the 2nd partial of the lower note to stop beating against the
fundamental of the upper note. Write that condition out with the stiff-string
formula and it stops being a clean 2:1:

```
2 * f0_lower * sqrt(1 + B_lower * 2^2)  ==  1 * f0_upper * sqrt(1 + B_upper * 1^2)
```

Because `B_lower` and `B_upper` are both positive (partials are always
sharp, never flat, of the ideal-string prediction), satisfying this exactly
requires `f0_upper` to sit a little sharp of a pure `2 * f0_lower`. Tune a
"perfect" ET octave instead and that 2nd-partial/fundamental pair beats -
which is audible, and which every piano tuner tunes out by ear whether or
not they know the formula behind it.

The octave test above uses the 2:1 partial pair. Tuners also check 4:2 (the
lower note's 4th partial against the upper note's 2nd) and 6:3 (6th against
3rd) - the same coincidence, one and two octaves further into the overtone
series. All three pairs want the octave widened, but they don't agree by
how much (higher partials carry `B`'s effect harder), so a tuned octave is
a compromise between them, not an exact solution to any single pair.

## The Railsback curve

Do this note-by-note, octave-by-octave, outward from a central reference
(traditionally the temperament octave around F3-F4), and the accumulated
widening produces a curve, not a flat correction: bass notes end up
progressively **flat** of ET, treble notes progressively **sharp** of ET,
with the middle staying close to ET. This is the Railsback curve, named for
O.L. Railsback's 1938 measurements of how real tuned pianos deviate from
ET - and it isn't a design choice, it's the inevitable outcome of tuning by
beats against inharmonic partials.

Railsback's original 1938 measurements put the extremes around -30 cents
flat at A0 and +40 cents sharp at C8; more recent surveys of well-tuned
concert grands land a bit more moderate, roughly -10 to -20 cents at A0 and
+20 to +30 cents at C8. Smaller, stiffer-stringed pianos (higher `B`) get
pushed further in both directions. pianito's own built-in Railsback-inspired
default lands at -15.7 cents at A0 and +23.8 cents at C8 - a deliberately
moderate, population-average approximation, not a measurement of any
particular instrument.

## Every piano's curve is different

Railsback's curve is an average over many pianos. `B` isn't a universal
constant - it's a property of each individual string's geometry, which
means it's a property of each individual piano's scale design, and even of
how that piano has aged. Two pianos of the same model can measure
noticeably different `B` curves; a spinet and a nine-foot concert grand
measure very different ones. A tuner using a fixed textbook stretch is
applying someone else's average to your piano. Measuring `B` on the actual
instrument and deriving its actual beatless curve is strictly better than
guessing from a population average - which is the whole premise of
pianito's Profile mode.

## How pianito measures this

**Recording the partials.** Profile mode plays and confirms all 88 keys
before tuning starts. On each confirmed note, pianito's FFT partial
analyzer (`src/audio/spectrum.rs`) doesn't just detect one fundamental - it
searches the magnitude spectrum for partials 1 through ~8 (n, frequency,
amplitude) and stores that list on the profile alongside the note (issue
[#22](https://github.com/4esv/pianito/issues/22)). That's the raw
material inharmonicity fitting needs: with only a single fundamental per
note, `B` isn't recoverable at all.

**Fitting `B` per note.** For each measured note, `src/tuning/inharmonicity.rs`
fits `(f0, B)` from its partial list. The stiff-string formula linearizes
exactly: squaring and dividing through gives
`(f_k / k)^2 = f0^2 + (f0^2 * B) * k^2`, a straight line in `k^2` that a
weighted least-squares fit recovers directly. The fit is deliberately
robust, not naive: it seeds from the low-order partials (least sensitive to
one bad peak), rejects any partial landing more than 25 cents from that
seed's prediction in either direction, then refits on the survivors. This
matters most in the bass, where the fundamental-recovery search
([#15](https://github.com/4esv/pianito/issues/15)) can occasionally
mislocate a high partial onto the wrong peak; an unguarded fit would let
that one bad point drag `B` (and therefore the whole stretch curve) off in
either direction.

**Smoothing across the keyboard.** Not every key gets a clean fit, and `B`
is noisy note-to-note even when it does. pianito smooths `log(B)` (not `B`
directly - it spans orders of magnitude, and averaging in log space keeps
the bass-bridge break a break rather than blurring it into the tenor) with
a Gaussian kernel roughly 4 semitones wide, so a handful of good measurements
per octave is enough to define the shape across all 88 keys.

**Deriving the stretch.** With a smoothed `B` at every key, pianito computes
the same beatless-octave condition worked out above - now for real, weighted
across the 2:1, 4:2, and 6:3 partial pairs - and integrates it octave by
octave outward from A4 (anchored at 0 cents) to the top and bottom of the
keyboard. The result is 88 per-key cents offsets: this piano's actual
stretch curve, derived from what its strings actually do rather than
assumed from a population average.

That curve is exposed as `StretchMode::Profile` (`--stretch profile` or
`stretch = "profile"` in the config file). It's used automatically once a
profile with enough usable partials exists; if the loaded profile predates
partial recording, or too few notes fit, pianito falls back to the built-in
Railsback default rather than silently reverting to unstretched ET.

## Further reading

- O.L. Railsback, "Scale Temperament as Applied to Piano Tuning," *Journal
  of the Acoustical Society of America* 9(3), 274 (1938) - the original
  measurements behind the curve's name.
- Harold A. Conklin Jr., "Generation of partials due to nonlinear mixing in
  a stringed instrument," *Journal of the Acoustical Society of America*
  105(1), 536-545 (1999) - the physics of string stiffness driving
  inharmonicity.
