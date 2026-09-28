"""Score a real pianito tuning session from a recording made alongside it.

Usage: live_result.py <record_dir> <session.json> <a4_hz>
  record_dir    conductor output of the `record` plan (session_full.wav + log.jsonl)
  session.json  the pianito session saved under the data dir's sessions/

For each completed note, replays guided detection second by second over the
span between the previous confirm and this one, and reports the median of the
last run of >=3 readings within +-60 c of pianito's target (A4 + Railsback).

WARNING: do not score the last strike before Space. Players often move on and
strike the next key before confirming, which reads as ~+100 c. The plateau
is what the note settled at. This measures with pianito's own detector; pair it
with a fresh `recheck` pass for independent evidence.
"""
import json, math, statistics as st, subprocess, sys, tempfile
from datetime import datetime
from pathlib import Path

import soundfile as sf

HERE = Path(__file__).parent
REPLAY = str(HERE / "replay/target/release/replay")
rec, sess_path, a4 = Path(sys.argv[1]), Path(sys.argv[2]), float(sys.argv[3])
sess = json.load(open(sess_path))
wav = rec / "session_full.wav"
# The recorder starts ~2 s before its first logged event (noise-floor pass)
t0 = json.loads(open(rec / "log.jsonl").readline())["t"] - 2.0
sr = sf.info(wav).samplerate
ts = lambda s: datetime.fromisoformat(s.replace("Z", "+00:00")).timestamp()
rb = dict(map(lambda l: (int(l.split()[0]), float(l.split()[1])),
              subprocess.run([REPLAY, "--railsback"], capture_output=True, text=True).stdout.splitlines()))
off = 1200 * math.log2(a4 / 440)

prev = ts(sess["created_at"])
print(f"{'note':5} {'saved':>6} {'final':>7} {'secs':>5}")
finals = []
with tempfile.TemporaryDirectory() as tmp:
    for c in sess["completed_notes"]:
        a, b, m = prev, ts(c["timestamp"]), c["midi"]
        prev = b
        x, _ = sf.read(wav, start=int((a - t0) * sr), stop=int((b - t0) * sr), dtype="float32")
        args = []
        for k in range(int(len(x) / sr) - 1):
            p = f"{tmp}/w{k}.wav"
            sf.write(p, x[k * sr:(k + 1) * sr + sr // 2], sr, subtype="FLOAT")
            args += [p, str(m)]
        out = subprocess.run([REPLAY, *args], capture_output=True, text=True,
                             env={"T0": "0.4", "GUIDED_ONLY": "1"}).stdout.splitlines()
        target = off + rb[m]
        seq = [(g["med"] - target) if (g := json.loads(l)["guided"]) else None for l in out]
        runs, cur = [], []
        for v in seq + [None]:
            if v is not None and abs(v) < 60:
                cur.append(v)
            else:
                if len(cur) >= 3: runs.append(cur)
                cur = []
        final = st.median(runs[-1][-5:]) if runs else None
        if final is not None: finals.append(final)
        print(f"{c['note']:5} {c['final_cents']:6.1f} {final if final is None else f'{final:+7.1f}':>7} {len(seq):5d}")
if finals:
    print(f"final |c| median {st.median(map(abs, finals)):.1f}, max {max(map(abs, finals)):.1f}, "
          f"within +-5: {sum(abs(v) <= 5 for v in finals)}/{len(finals)}")
