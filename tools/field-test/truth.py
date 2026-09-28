"""Independent ground-truth analyzer (not pianito code).

For a take of a known key: grid-search f0 (+-250 cents of ET) and B to maximize
peak energy at predicted partials f_n = n f0 sqrt(1 + B n^2), then locate each
partial peak sub-bin and least-squares refit (f0, B). Also reports the spread of
the strongest cluster near partials 1-2 (mistuned unison strings show as split
peaks). Usage: truth.py <wav> <midi> [...] -> JSON lines.
"""
import json, sys
import numpy as np
import soundfile as sf


def et(m): return 440.0 * 2 ** ((m - 69) / 12)
def cents(f, t): return 1200 * np.log2(f / t)


def spectrum(x, sr, t0, secs):
    seg = x[int(t0 * sr): int((t0 + secs) * sr)]
    n = 1 << 20
    s = np.abs(np.fft.rfft(seg * np.hanning(len(seg)), n))
    return s, sr / n


def peak_near(s, df, f, tol_c=35):
    lo, hi = int(f * 2 ** (-tol_c / 1200) / df), int(f * 2 ** (tol_c / 1200) / df) + 1
    if hi + 1 >= len(s) or lo < 1: return None
    i = lo + int(np.argmax(s[lo:hi]))
    if i <= lo or i >= hi - 1: return None  # edge = no real peak inside band
    y0, y1, y2 = np.log(s[i - 1:i + 2] + 1e-20)
    d = 0.5 * (y0 - y2) / (y0 - 2 * y1 + y2)
    return (i + d) * df, s[i]


NOISE = None  # path to a silence take; set via --noise


def analyze(path, midi):
    x, sr = sf.read(path)
    target = et(midi)
    # Treble dies fast; bass wants long windows for resolution.
    t0, secs = (0.25, 2.5) if midi < 48 else (0.15, 1.2) if midi < 84 else (0.05, 0.35)
    s, df = spectrum(x, sr, t0, secs)
    if NOISE is not None:
        # Spectral subtraction of the room's stationary hum (same window
        # length), 2x over-subtraction: hum peaks otherwise pose as partials.
        xn, _ = sf.read(NOISE)
        need = int(secs * sr)
        xn = np.tile(xn, need // len(xn) + 2)[: need + int(t0 * sr)]
        sn, _ = spectrum(xn, sr, t0, secs)
        s = np.maximum(s - 2.0 * sn, s * 1e-3)
    nmax = int(min(16, 7000 / target))
    nmax = max(nmax, 1)
    ls = np.log(s + 1e-12)
    best = (-1e9, None, None)
    for c in np.arange(-250, 251, 2.0):
        f0 = target * 2 ** (c / 1200)
        for B in ([0] if nmax < 3 else np.geomspace(1e-5, 2e-2, 40)):
            n = np.arange(1, nmax + 1)
            idx = (n * f0 * np.sqrt(1 + B * n * n) / df).astype(int)
            idx = idx[idx < len(s) - 1]
            score = np.mean([ls[i - 2:i + 3].max() for i in idx])
            if score > best[0]: best = (score, f0, B)
    _, f0, B = best
    parts = []
    for n in range(1, nmax + 1):
        p = peak_near(s, df, n * f0 * np.sqrt(1 + B * n * n))
        if p: parts.append((n, p[0], p[1]))
    amp_max = max((a for _, _, a in parts), default=1)
    parts = [(n, f, a) for n, f, a in parts if a > amp_max * 0.01]
    fit_f0, fit_B = f0, B
    if len(parts) >= 3:
        n = np.array([p[0] for p in parts], float); f = np.array([p[1] for p in parts])
        w = np.array([p[2] for p in parts])
        A = np.vstack([np.ones_like(n), n * n]).T
        y = (f / n) ** 2
        coef, *_ = np.linalg.lstsq(A * w[:, None], y * w, rcond=None)
        if coef[0] > 0 and coef[1] >= 0:
            fit_f0 = np.sqrt(coef[0]); fit_B = coef[1] / coef[0]
    # Unison spread: distinct peaks within +-25 cents of the strongest low partial
    strongest = max(parts, key=lambda p: p[2]) if parts else None
    spread = None
    if strongest and midi >= 35:
        n0, fc, _ = strongest
        lo, hi = int(fc * 2 ** (-25 / 1200) / df), int(fc * 2 ** (25 / 1200) / df)
        seg = s[lo:hi]
        pk = [i for i in range(1, len(seg) - 1) if seg[i] > seg[i - 1] and seg[i] >= seg[i + 1]
              and seg[i] > seg.max() * 0.2]
        if len(pk) >= 2:
            fr = [(lo + i) * df for i in pk]
            spread = round(float(cents(max(fr), min(fr))), 1)
    return dict(path=path, midi=midi, target=target,
                cents=round(float(cents(fit_f0, target)), 2), B=float(fit_B),
                nparts=len(parts), strongest_n=strongest[0] if strongest else None,
                unison_spread_c=spread,
                partial_cents=[(n, round(float(cents(f / n, fit_f0)), 1)) for n, f, _ in parts])


if __name__ == "__main__":
    a = sys.argv[1:]
    if a and a[0] == "--noise":
        NOISE = a[1]; a = a[2:]
    for i in range(0, len(a), 2):
        print(json.dumps(analyze(a[i], int(a[i + 1]))), flush=True)
