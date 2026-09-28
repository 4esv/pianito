# /// script
# requires-python = ">=3.11"
# dependencies = ["sounddevice", "numpy", "soundfile"]
# ///
"""Hands-free recording conductor for pianito field testing.

Speaks each step aloud (macOS `say`), waits for the player's strike, records a
fixed window from onset, saves one WAV per step plus a continuous session WAV,
and logs JSONL events. Live control via a ctl file (one command per write):
  pause | resume | redo | skip | back | goto <id> | say <text> | next | quit
"""
import json, os, queue, subprocess, sys, threading, time
from pathlib import Path

import numpy as np
import sounddevice as sd
import soundfile as sf

SR = 48000
BLOCK = 480  # 10 ms
NAMES = ["A", "A#", "B", "C", "C#", "D", "D#", "E", "F", "F#", "G", "G#"]


def note_name(midi):
    i = midi - 21
    return f"{NAMES[i % 12]}{(i + 9) // 12}"


def spoken(name):
    base, octv = name[:-1], name[-1]
    return base.replace("#", " sharp") + " " + octv


def freq(midi, a4=440.0):
    return a4 * 2 ** ((midi - 69) / 12)


class Recorder:
    def __init__(self, outdir):
        self.q = queue.Queue()
        self.ring = np.zeros(SR * 30, dtype=np.float32)  # 30 s history
        self.pos = 0  # total samples written
        self.lock = threading.Lock()
        self.full = sf.SoundFile(outdir / "session_full.wav", "w", SR, 1, "FLOAT")
        self.stream = sd.InputStream(samplerate=SR, channels=1, blocksize=BLOCK,
                                     dtype="float32", callback=self._cb)
        self.stream.start()
        threading.Thread(target=self._writer, daemon=True).start()

    def _cb(self, indata, frames, t, status):
        x = indata[:, 0].copy()
        with self.lock:
            n = len(x)
            idx = (self.pos + np.arange(n)) % len(self.ring)
            self.ring[idx] = x
            self.pos += n
        self.q.put(x)

    def _writer(self):
        while True:
            self.full.write(self.q.get())

    def now(self):
        with self.lock:
            return self.pos

    def get(self, start, end):
        with self.lock:
            idx = np.arange(start, end) % len(self.ring)
            return self.ring[idx].copy()

    def rms_db(self, start, end):
        x = self.get(start, end)
        return 20 * np.log10(np.sqrt(np.mean(x * x)) + 1e-12)


class Conductor:
    def __init__(self, plan_path, outdir):
        self.plan = json.loads(Path(plan_path).read_text())
        self.outdir = Path(outdir)
        (self.outdir / "takes").mkdir(parents=True, exist_ok=True)
        self.log = open(self.outdir / "log.jsonl", "a", buffering=1)
        self.ctl = self.outdir / "ctl"
        self.ctl.write_text("")
        self.rec = Recorder(self.outdir)
        self.floor_db = -60.0
        self.paused = False

    def emit(self, **kw):
        kw["t"] = round(time.time(), 2)
        self.log.write(json.dumps(kw) + "\n")
        print(json.dumps(kw), flush=True)

    def say(self, text):
        self.emit(ev="say", text=text)
        subprocess.run(["say", "-r", "200", text])
        time.sleep(0.25)  # let the speaker ring down before listening

    def poll_ctl(self):
        try:
            cmd = self.ctl.read_text().strip()
        except FileNotFoundError:
            return None
        if cmd:
            self.ctl.write_text("")
            self.emit(ev="ctl", cmd=cmd)
            return cmd
        return None

    def handle_meta(self, cmd):
        """Commands that don't change position. Returns True if consumed."""
        if cmd.startswith("say "):
            self.say(cmd[4:])
            return True
        if cmd == "pause":
            self.paused = True
            self.say("Paused.")
            return True
        if cmd == "resume":
            self.paused = False
            self.say("Resuming.")
            return True
        return False

    def measure_floor(self, secs=2.0):
        start = self.rec.now()
        time.sleep(secs)
        x = self.rec.get(start, self.rec.now())
        frames = x[: len(x) // BLOCK * BLOCK].reshape(-1, BLOCK)
        db = 20 * np.log10(np.sqrt((frames ** 2).mean(1)) + 1e-12)
        # Quiet-ish percentile, clamped: playing during the measurement must
        # not raise the onset threshold out of reach.
        self.floor_db = min(float(np.percentile(db, 20)), -45.0)
        return self.floor_db

    def wait_onset(self, timeout):
        """Returns (onset_sample | None, ctl_cmd | None)."""
        thresh = max(self.floor_db + 15, -55)
        start = self.rec.now()
        last = start
        while True:
            cmd = self.poll_ctl()
            if cmd and not self.handle_meta(cmd):
                return None, cmd
            now = self.rec.now()
            while last + BLOCK <= now:
                # Absolute threshold AND a sharp rise vs 50 ms earlier, so a
                # previous note's ringing tail doesn't count as a new strike.
                cur = self.rec.rms_db(last, last + BLOCK)
                prev = self.rec.rms_db(max(last - 5 * BLOCK, 0), last - 4 * BLOCK) \
                    if last >= 5 * BLOCK else -120
                if cur > thresh and cur - prev > 10:
                    return last, None
                last += BLOCK
            if (now - start) / SR > timeout:
                return None, None
            time.sleep(0.01)

    @staticmethod
    def rough_cents(x, expected_hz):
        """Coarse check the right key was hit: strongest harmonic-sum peak."""
        n = 1 << 16
        seg = x[int(0.15 * SR): int(0.15 * SR) + n]
        if len(seg) < 8192:
            return None
        spec = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n))
        f = np.fft.rfftfreq(n, 1 / SR)
        best, best_s = None, -1
        for c in np.arange(-300, 301, 5):
            f0 = expected_hz * 2 ** (c / 1200)
            s = 0
            for k in range(1, 7):
                i = int(round(k * f0 * n / SR))
                if i + 3 < len(spec):
                    s += spec[max(i - 3, 0): i + 4].max()
            if s > best_s:
                best, best_s = c, s
        return int(best)

    def do_capture(self, step):
        kind = step.get("kind", "note")
        dur = step.get("dur", 5.0)
        sid = step["id"]
        if kind in ("silence", "free"):
            if kind == "silence":
                time.sleep(0.5)
            onset = self.rec.now()
            deadline = time.time() + dur
            while time.time() < deadline:  # noqa: poll ctl during fixed windows
                cmd = self.poll_ctl()
                if cmd and not self.handle_meta(cmd):
                    return "ctl", cmd
                time.sleep(0.05)
        else:
            onset, cmd = self.wait_onset(step.get("timeout", 20))
            if cmd:
                return "ctl", cmd
            if onset is None:
                return "timeout", None
            onset = max(onset - SR // 10, 0)  # 100 ms pre-roll
            while self.rec.now() < onset + int(dur * SR):
                cmd = self.poll_ctl()
                if cmd and not self.handle_meta(cmd):
                    return "ctl", cmd
                time.sleep(0.02)
        x = self.rec.get(onset, onset + int(dur * SR))
        take = 1
        while (self.outdir / "takes" / f"{sid}.t{take}.wav").exists():
            take += 1
        path = self.outdir / "takes" / f"{sid}.t{take}.wav"
        sf.write(path, x, SR, subtype="FLOAT")
        peak = float(np.abs(x).max())
        info = dict(ev="take", id=sid, take=take, path=str(path),
                    peak_db=round(20 * np.log10(peak + 1e-12), 1),
                    floor_db=round(self.floor_db, 1))
        if kind == "note" and "midi" in step:
            info["rough_cents"] = self.rough_cents(x, freq(step["midi"]))
        self.emit(**info)
        return "ok", info

    def run(self, start_id=None):
        steps = self.plan["steps"]
        i = 0
        if start_id:
            i = next(k for k, s in enumerate(steps) if s["id"] == start_id)
        fl = self.measure_floor()
        self.emit(ev="floor", db=round(fl, 1))
        while i < len(steps):
            step = steps[i]
            while self.paused:
                cmd = self.poll_ctl()
                if cmd and not self.handle_meta(cmd) and cmd == "quit":
                    return
                time.sleep(0.1)
            self.emit(ev="step", idx=i, id=step["id"])
            if step.get("say"):
                self.say(step["say"])
            if step.get("kind") == "wait":
                # Block until someone writes "next" (me, after the player confirms)
                while True:
                    cmd = self.poll_ctl()
                    if cmd == "next":
                        break
                    if cmd == "quit":
                        return
                    if cmd:
                        self.handle_meta(cmd)
                    time.sleep(0.1)
                i += 1
                continue
            status, info = self.do_capture(step)
            if status == "ctl":
                cmd = info
                if cmd == "quit":
                    self.say("Stopping.")
                    return
                if cmd in ("redo",):
                    continue
                if cmd in ("skip", "next"):
                    i += 1
                    continue
                if cmd == "back":
                    i = max(i - 1, 0)
                    continue
                if cmd.startswith("goto "):
                    i = next(k for k, s in enumerate(steps) if s["id"] == cmd[5:])
                    continue
                continue
            if status == "timeout":
                self.emit(ev="timeout", id=step["id"])
                self.say("Didn't hear anything. Again.")
                continue
            # Quality gates
            if step.get("kind") == "silence":
                x = self.rec.get(self.rec.now() - int(step["dur"] * SR), self.rec.now())
                fr = x[: len(x) // BLOCK * BLOCK].reshape(-1, BLOCK)
                db = 20 * np.log10(np.sqrt((fr ** 2).mean(1)) + 1e-12)
                self.floor_db = min(float(np.percentile(db, 50)), -45.0)
                self.emit(ev="floor", db=round(float(np.percentile(db, 50)), 1),
                          p90=round(float(np.percentile(db, 90)), 1))
            elif info["peak_db"] > -0.5:
                self.say("That clipped. Again, a bit softer.")
                continue
            rc = info.get("rough_cents")
            if (step.get("check_pitch") and rc is not None and abs(rc) >= 250
                    and step["midi"] >= 48):
                self.say("That doesn't sound like the right key. Again.")
                continue
            if step.get("after"):
                self.say(step["after"])
            i += 1
        self.say(self.plan.get("done", "All done."))
        self.emit(ev="done")


if __name__ == "__main__":
    plan, outdir = sys.argv[1], sys.argv[2]
    start = sys.argv[3] if len(sys.argv) > 3 else None
    Conductor(plan, outdir).run(start)
