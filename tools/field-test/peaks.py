import sys, numpy as np, soundfile as sf
def peaks(path, t0=0.3, secs=1.5, fmin=20, fmax=6000, k=10):
    x, sr = sf.read(path); seg = x[int(t0*sr):int((t0+secs)*sr)]
    n = 1 << 19
    s = np.abs(np.fft.rfft(seg*np.hanning(len(seg)), n)); f = np.fft.rfftfreq(n, 1/sr)
    m = (f > fmin) & (f < fmax); s2 = s.copy(); s2[~m] = 0
    out = []
    for _ in range(k):
        i = np.argmax(s2); a = s2[i]
        if a <= 0: break
        y0,y1,y2 = np.log(s[i-1:i+2]+1e-20); d = 0.5*(y0-y2)/(y0-2*y1+y2)
        out.append((f[i]+d*(f[1]-f[0]), 20*np.log10(a/s[m].max())))
        w = int(3/(f[1]-f[0])) + 5; s2[max(i-w,0):i+w] = 0
    return sorted(out)
for p in sys.argv[1:]:
    print(p.split('/')[-1], ' '.join(f'{fr:.2f}({db:.0f})' for fr, db in peaks(p)))
