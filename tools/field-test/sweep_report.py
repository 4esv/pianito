"""Score pianito against an independent fit over a sweep's takes.

Usage: sweep_report.py <session_dir>/p1_sweep
Reads the latest take per key (takes/nMMM_<note>.tN.wav), runs truth.py (with
the setup pass's silence take as the noise profile) and replay, prints a table
and writes report.json. Cents are against 440 ET; err_* = pianito - truth.
"""
import json, re, subprocess, sys
from pathlib import Path

HERE = Path(__file__).parent
REPLAY = HERE / "replay/target/release/replay"
d = Path(sys.argv[1])
latest = {}
for p in sorted((d / "takes").glob("n*.wav")):
    m = re.match(r"n(\d{3})_\w+\.t(\d+)\.wav", p.name)
    midi, take = int(m.group(1)), int(m.group(2))
    if midi not in latest or take > latest[midi][1]:
        latest[midi] = (p, take)
args = [x for midi, (p, _) in sorted(latest.items()) for x in (str(p), str(midi))]
noise = d.parent / "p0_setup/takes/silence.t1.wav"


def jsonl(cmd):
    out = subprocess.run(cmd, capture_output=True, text=True, check=True).stdout
    return {j["midi"]: j for j in map(json.loads, out.splitlines())}


truth = jsonl(["uv", "run", "-q", "--with", "numpy", "--with", "soundfile", "python",
               str(HERE / "truth.py"), "--noise", str(noise), *args])
rep = jsonl([str(REPLAY), *args])

NAMES = ["A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#"]
med = lambda o: None if o is None else o["med"]
pct = lambda o, frames: 100 * (o["n"] if o else 0) / frames
fmt = lambda v, w=7: f"{v:>{w}.1f}" if v is not None else " " * (w - 1) + "-"
rows = []
print(f"{'note':5} {'truth':>7} {'B':>8} {'uni':>5} | {'guided':>7} {'ok%':>4} | {'full':>8} {'ok%':>4} | {'err_g':>6}")
for m in sorted(latest):
    t, r = truth[m], rep[m]
    note = f"{NAMES[(m - 21) % 12]}{(m - 21 + 9) // 12}"
    g, f = med(r["guided"]), med(r["full"])
    row = dict(midi=m, note=note, truth=t["cents"], B=t["B"], uni=t["unison_spread_c"],
               guided=g, g_ok=pct(r["guided"], r["frames"]), full=f, f_ok=pct(r["full"], r["frames"]),
               fit_log10b=med(r["fit_log10b"]), err_g=None if g is None else g - t["cents"])
    rows.append(row)
    print(f"{note:5} {fmt(t['cents'])} {t['B']:8.5f} {fmt(t['unison_spread_c'], 5)} | {fmt(g)} {row['g_ok']:4.0f} | "
          f"{fmt(f, 8)} {row['f_ok']:4.0f} | {fmt(row['err_g'], 6)}")
json.dump(rows, open(d / "report.json", "w"), indent=1)
