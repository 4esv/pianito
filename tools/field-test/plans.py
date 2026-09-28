"""Write the conductor plans used in a field test.

  setup    silence (noise profile) + level checks at A4, A0, C8
  sweep    all 88 keys, bottom to top
  edges    repeats, dynamics, staccato, pedal, intervals, tremolo, talking,
           and A4 single strings via mutes (waits for `echo next > ctl`)
  recheck  E3-D5 (the temperament octave and neighbours) after tuning
  record   silent 3 h capture to run beside the pianito TUI

Usage: plans.py <out_dir>
"""
import json, sys
from pathlib import Path

NAMES = ["A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#"]


def name(m): return f"{NAMES[(m - 21) % 12]}{(m - 21 + 9) // 12}"


def spoken(m):
    n = name(m)
    s = n[:-1].replace("#", " sharp") + " " + n[-1]
    if "#" in n: s += ", black"
    return {21: "A 0, the lowest key", 60: "C 4, middle C", 69: "A 4, the A above middle C"}.get(m, s)


def note(m, dur=None, **kw):
    dur = dur or (8.0 if m < 36 else 6.0 if m < 60 else 4.0 if m < 84 else 3.0)
    return dict(id=f"n{m:03d}_{name(m).replace('#', 's')}", midi=m, dur=dur, say=spoken(m),
                check_pitch=True, **kw)


def intro(text, dur=0.1): return dict(id="intro", kind="silence", dur=dur, say=text)
def take(id, midi, say, dur=4.0): return dict(id=id, midi=midi, say=say, dur=dur)
def wait(id, say): return dict(id=id, kind="wait", say=say)
def free(id, say, dur): return dict(id=id, kind="free", say=say, dur=dur)


HOLD = "Hold the key until you hear the next name."
plans = {
    "setup": [
        dict(id="silence", kind="silence", dur=5.0, say="Setup check. Stay quiet for five seconds."),
        take("level_A4", 69, "Play A 4, the A above middle C, medium loud. Hold it."),
        take("level_A0", 21, "Now A 0, the lowest key. Medium loud. Hold it.", 8.0),
        take("level_C8", 108, "Now C 8, the highest key. Medium loud.", 3.0),
    ],
    "sweep": [intro(f"Full sweep, bottom to top. One strike each, medium loud. {HOLD}")]
             + [note(m) for m in range(21, 109)],
    "edges": [
        intro("Edge cases. Play when I say, hold until I talk again."),
        *[take(f"rep_A4_{i}", 69, f"A 4, medium. Repeat {i} of 3.") for i in (1, 2, 3)],
        take("dyn_A4_pp", 69, "A 4, as soft as you can while still sounding."),
        take("dyn_A4_ff", 69, "A 4, loud."),
        take("dyn_A1_pp", 33, "A 1, soft.", 8.0),
        take("dyn_A1_ff", 33, "A 1, loud.", 8.0),
        take("dyn_C7_pp", 96, "C 7, soft.", 3.0),
        take("dyn_C7_ff", 96, "C 7, loud.", 3.0),
        take("stacc_A4", 69, "A 4, short. Let go immediately.", 3.0),
        take("pedal_A4", 69, "Hold the right pedal down, then play A 4.", 6.0),
        free("pedal_off", "Release the pedal.", 2.0),
        take("oct_A3A4", 57, "Play A 3 and A 4 together."),
        take("third_C4E4", 60, "Play C 4 and E 4 together."),
        free("trem_A4", "Play A 4 over and over, fast, for five seconds. Go.", 6.0),
        free("talk_A4", "Play A 4 and talk while it rings. Go.", 6.0),
        free("scale_C4", "Slow C major scale up from middle C, one note per second. Go.", 10.0),
        wait("mute_center", "On A 4, mute so only the middle string sounds. Tell Claude when ready."),
        take("uni_A4_center", 69, "A 4, center string only.", 5.0),
        wait("mute_left", "Now only the left string. Tell Claude when ready."),
        take("uni_A4_left", 69, "A 4, left string only.", 5.0),
        wait("mute_right", "Now only the right string. Tell Claude when ready."),
        take("uni_A4_right", 69, "A 4, right string only.", 5.0),
        wait("mute_off", "Remove the mutes. Tell Claude when ready."),
        take("uni_A4_all", 69, "A 4, all three strings.", 5.0),
    ],
    "recheck": [intro(f"Checking the tuning. Quiet, then one strike per note. {HOLD}", 3.0)]
               + [note(m, 4.0) for m in range(52, 75)],
    "record": [free("live", "", 10800)],
}

out = Path(sys.argv[1])
out.mkdir(parents=True, exist_ok=True)
for key, steps in plans.items():
    (out / f"{key}.json").write_text(json.dumps({"steps": steps, "done": "Done. Tell Claude."}, indent=1))
    print(out / f"{key}.json", len(steps), "steps")
