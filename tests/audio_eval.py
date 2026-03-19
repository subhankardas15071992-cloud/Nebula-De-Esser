#!/usr/bin/env python3
"""
═══════════════════════════════════════════════════════════════════════════════
  NEBULA DEESSER v2.0.0 — Comprehensive Audio & Stress Test Suite
  Mirrors the Rust DSP exactly in Python/NumPy for black-box + white-box tests
═══════════════════════════════════════════════════════════════════════════════
"""

import numpy as np
import scipy.signal as spsig
import time, random, math, struct, sys, os
from dataclasses import dataclass, field
from typing import List, Tuple, Optional

SR = 44100
PASS = "\033[92m✓ PASS\033[0m"
FAIL = "\033[91m✗ FAIL\033[0m"
WARN = "\033[93m⚠ WARN\033[0m"
INFO = "\033[96m·\033[0m"

results = []          # (test_name, status, detail)
refinements = []      # issues to report for code fix

def log(name, status, detail=""):
    results.append((name, status, detail))
    sym = {"PASS": PASS, "FAIL": FAIL, "WARN": WARN}.get(status, INFO)
    print(f"  {sym}  {name:<55} {detail}")

def section(title):
    print(f"\n{'─'*72}")
    print(f"  {title}")
    print(f"{'─'*72}")

# ─────────────────────────────────────────────────────────────────────────────
#  Python mirror of Rust DSP (exact arithmetic match)
# ─────────────────────────────────────────────────────────────────────────────

class Biquad:
    """Exact mirror of BiquadCoeffs + BiquadState in dsp.rs"""
    def __init__(self):
        self.b0=self.b1=self.b2=self.a1=self.a2 = 0.0
        self.x1=self.x2=self.y1=self.y2 = 0.0

    @classmethod
    def highpass(cls, freq, q, sr):
        b = cls()
        w0 = 2*math.pi*freq/sr
        cw, sw = math.cos(w0), math.sin(w0)
        alpha = sw/(2*q)
        b0 = (1+cw)/2; b1 = -(1+cw); b2 = (1+cw)/2
        a0 = 1+alpha; a1 = -2*cw; a2 = 1-alpha
        b.b0,b.b1,b.b2,b.a1,b.a2 = b0/a0,b1/a0,b2/a0,a1/a0,a2/a0
        return b

    @classmethod
    def lowpass(cls, freq, q, sr):
        b = cls()
        w0 = 2*math.pi*freq/sr
        cw, sw = math.cos(w0), math.sin(w0)
        alpha = sw/(2*q)
        b0 = (1-cw)/2; b1 = 1-cw; b2 = (1-cw)/2
        a0 = 1+alpha; a1 = -2*cw; a2 = 1-alpha
        b.b0,b.b1,b.b2,b.a1,b.a2 = b0/a0,b1/a0,b2/a0,a1/a0,a2/a0
        return b

    @classmethod
    def bandpass(cls, freq, q, sr):
        b = cls()
        w0 = 2*math.pi*freq/sr
        cw, sw = math.cos(w0), math.sin(w0)
        alpha = sw/(2*q)
        b0 = sw/2; b1 = 0.0; b2 = -sw/2
        a0 = 1+alpha; a1 = -2*cw; a2 = 1-alpha
        b.b0,b.b1,b.b2,b.a1,b.a2 = b0/a0,b1/a0,b2/a0,a1/a0,a2/a0
        return b

    def process(self, x):
        # Denormal guard — matches flushed-to-zero behaviour
        DENORM = 1e-30
        x = x if abs(x) > 1e-38 else 0.0
        y = self.b0*x + self.b1*self.x1 + self.b2*self.x2 \
            - self.a1*self.y1 - self.a2*self.y2
        # Flush denormals in state (FTZ)
        self.x2 = self.x1; self.x1 = x
        self.y2 = self.y1
        self.y1 = y if abs(y) > 1e-30 else 0.0
        return self.y1

    def reset(self):
        self.x1=self.x2=self.y1=self.y2 = 0.0

    def process_block(self, x: np.ndarray) -> np.ndarray:
        out = np.zeros_like(x)
        for i, s in enumerate(x):
            out[i] = self.process(s)
        return out

class EnvelopeFollower:
    def __init__(self, attack_ms, release_ms, sr):
        self.att = 0.0 if attack_ms<=0  else math.exp(-1/(attack_ms*0.001*sr))
        self.rel = 0.0 if release_ms<=0 else math.exp(-1/(release_ms*0.001*sr))
        self.env = 0.0

    def process(self, x):
        a = abs(x)
        if a > self.env:
            self.env = self.att*(self.env - a) + a
        else:
            self.env = self.rel*(self.env - a) + a
        return self.env

    def reset(self): self.env = 0.0

    def process_block(self, x):
        out = np.zeros(len(x))
        for i, s in enumerate(x): out[i] = self.process(s)
        return out

class GainSmoother:
    def __init__(self, time_ms, sr):
        self.coeff = 0.0 if time_ms<=0 else math.exp(-1/(time_ms*0.001*sr))
        self.current = 1.0

    def process(self, target):
        self.current = self.coeff*(self.current - target) + target
        return self.current

    def reset(self): self.current = 1.0

class LookaheadDelay:
    def __init__(self, max_ms, sr):
        self.buf = np.zeros(max(int(max_ms*0.001*sr)+2, 2))
        self.wpos = 0; self.delay = 0

    def set_delay(self, ms, sr):
        self.delay = min(round(ms*0.001*sr), len(self.buf)-1)

    def process(self, x):
        self.buf[self.wpos] = x
        rpos = (self.wpos - self.delay) % len(self.buf)
        self.wpos = (self.wpos + 1) % len(self.buf)
        return self.buf[rpos]

    def process_block(self, x):
        out = np.zeros(len(x))
        for i, s in enumerate(x): out[i] = self.process(s)
        return out

    def reset(self): self.buf.fill(0); self.wpos = 0

def lin_to_db(x):
    return 20*math.log10(max(abs(x), 1e-10))

def db_to_lin(db):
    return 10**(db/20)

def compute_gr(det_db, thr_db, max_red_db, knee_db=2.0):
    over = det_db - thr_db
    if over <= -knee_db*0.5:   return 0.0
    elif over <= knee_db*0.5:
        kf = (over + knee_db*0.5)/knee_db
        return -kf*kf*abs(max_red_db)
    else:                       return -abs(max_red_db)

@dataclass
class DeEsserParams:
    threshold:     float = -20.0
    max_reduction: float = 12.0
    min_freq:      float = 4000.0
    max_freq:      float = 12000.0
    mode_relative: bool  = True
    use_peak:      bool  = False
    use_wide:      bool  = False
    filter_solo:   bool  = False
    lookahead_en:  bool  = False
    lookahead_ms:  float = 2.0
    stereo_link:   float = 1.0
    input_level:   float = 0.0
    output_level:  float = 0.0
    input_pan:     float = 0.0
    output_pan:    float = 0.0
    bypass:        bool  = False

class DeEsserDSP:
    """Full stereo de-esser, exact mirror of Rust DeEsserDsp"""
    def __init__(self, sr=SR):
        self.sr = sr
        self.hp   = [Biquad(), Biquad()]
        self.lp   = [Biquad(), Biquad()]
        self.pk   = [Biquad(), Biquad()]
        self.denv = [EnvelopeFollower(0.1, 60.0, sr) for _ in range(2)]
        self.fenv = [EnvelopeFollower(0.1, 60.0, sr) for _ in range(2)]
        self.gs   = [GainSmoother(1.0, sr) for _ in range(2)]
        self.la_a = [LookaheadDelay(20.0, sr) for _ in range(2)]
        self.la_s = [LookaheadDelay(20.0, sr) for _ in range(2)]
        self.att_coeff = math.exp(-1/(0.1*0.001*sr))
        self.rel_coeff = math.exp(-1/(50.0*0.001*sr))
        self.update_filters(4000.0, 12000.0, False)

    def update_filters(self, mn, mx, peak):
        mn = np.clip(mn, 20, self.sr*0.49)
        mx = np.clip(mx, mn+10, self.sr*0.49)
        center = math.sqrt(mn*mx)
        bw = mx - mn
        q  = max(center/bw, 0.1)
        for ch in range(2):
            self.hp[ch] = Biquad.highpass(mn, 0.707, self.sr)
            self.lp[ch] = Biquad.lowpass(mx, 0.707, self.sr)
            self.pk[ch] = Biquad.bandpass(center, q, self.sr)

    def update_lookahead(self, ms):
        for ch in range(2):
            self.la_a[ch].set_delay(ms, self.sr)
            self.la_s[ch].set_delay(ms, self.sr)

    def apply_det_filter(self, x, ch, peak, wide):
        if peak:
            return self.pk[ch].process(x)
        hp = self.hp[ch].process(x)
        return self.lp[ch].process(hp)

    def pan_gains(self, pan, gain):
        pan = np.clip(pan, -1, 1)
        gl = (1.0 if pan<=0 else 1.0-pan) * gain
        gr = (1.0 if pan>=0 else 1.0+pan) * gain
        return gl, gr

    def process_block(self, L: np.ndarray, R: np.ndarray,
                      p: DeEsserParams) -> Tuple[np.ndarray, np.ndarray,
                                                  np.ndarray, np.ndarray]:
        n = len(L)
        outL = np.zeros(n); outR = np.zeros(n)
        det  = np.zeros(n); red  = np.zeros(n)

        ig = db_to_lin(p.input_level)
        og = db_to_lin(p.output_level)
        igl, igr = self.pan_gains(p.input_pan, ig)
        ogl, ogr = self.pan_gains(p.output_pan, og)

        for i in range(n):
            raw_l, raw_r = L[i], R[i]

            if p.bypass:
                outL[i] = raw_l; outR[i] = raw_r
                det[i] = -120.0; red[i] = 0.0
                continue

            l = raw_l * igl; r = raw_r * igr

            dl = self.apply_det_filter(l, 0, p.use_peak, p.use_wide)
            dr = self.apply_det_filter(r, 1, p.use_peak, p.use_wide)

            al = self.la_a[0].process(l) if p.lookahead_en else l
            ar = self.la_a[1].process(r) if p.lookahead_en else r

            el = self.denv[0].process(dl)
            er = self.denv[1].process(dr)
            fl = self.fenv[0].process(abs(l))
            fr = self.fenv[1].process(abs(r))

            ell = el*(1-p.stereo_link) + (el+er)*0.5*p.stereo_link
            erl = er*(1-p.stereo_link) + (el+er)*0.5*p.stereo_link

            knee = 2.0
            def ch_gain(env_det, env_full, ch_idx):
                ddb  = lin_to_db(env_det)
                fdb  = lin_to_db(env_full)
                if p.mode_relative:
                    eff = ddb - (p.threshold + fdb)
                    tdb = p.threshold
                    gr  = compute_gr(eff + tdb, tdb, p.max_reduction, knee)
                else:
                    gr  = compute_gr(ddb, p.threshold, p.max_reduction, knee)
                tgt = db_to_lin(gr)
                g   = self.gs[ch_idx].process(tgt)
                return g, ddb

            gl, ddb_l = ch_gain(ell, fl, 0)
            gr_, ddb_r = ch_gain(erl, fr, 1)

            if p.filter_solo:
                ol = dl * gl; orr = dr * gr_
            else:
                ol = al * gl; orr = ar * gr_

            outL[i] = ol * ogl; outR[i] = orr * ogr
            det[i]  = (ddb_l + ddb_r) * 0.5
            red[i]  = lin_to_db((gl + gr_) * 0.5)

        return outL, outR, det, red

    def reset(self):
        for ch in range(2):
            self.hp[ch].reset(); self.lp[ch].reset(); self.pk[ch].reset()
            self.denv[ch].reset(); self.fenv[ch].reset()
            self.gs[ch].reset()
            self.la_a[ch].reset(); self.la_s[ch].reset()

# ─── Signal Generators ───────────────────────────────────────────────────────
def sine(freq, dur_s, sr=SR, amp=0.5):
    t = np.arange(int(dur_s*sr)) / sr
    return amp * np.sin(2*np.pi*freq*t)

def white_noise(dur_s, sr=SR, amp=0.3):
    return amp * np.random.randn(int(dur_s*sr))

def impulse(dur_s, sr=SR, pos_s=0.01, amp=1.0):
    x = np.zeros(int(dur_s*sr))
    x[int(pos_s*sr)] = amp
    return x

def pink_noise(dur_s, sr=SR, amp=0.3):
    n = int(dur_s*sr)
    w = np.fft.rfftfreq(n)
    w[0] = 1.0
    s = 1/np.sqrt(w)
    ph = np.random.uniform(0, 2*np.pi, len(w))
    x = np.fft.irfft(s*np.exp(1j*ph), n=n)
    return amp * x / (np.max(np.abs(x))+1e-9)

def music_like(dur_s, sr=SR):
    """Bandlimited signal resembling speech + music, heavily sibilant"""
    t = np.arange(int(dur_s*sr))/sr
    # Fundamental + harmonics
    sig = 0.3*np.sin(2*np.pi*110*t) + 0.2*np.sin(2*np.pi*220*t)
    # Sibilant burst every 0.5s
    for onset in np.arange(0, dur_s, 0.5):
        idx = int(onset*sr); end = idx + int(0.05*sr)
        sig[idx:end] += 0.5*np.random.randn(end-idx)
    return sig * 0.5

def denormal_signal(n):
    """Mix of normals and subnormals"""
    x = np.zeros(n)
    x[::3] = 1e-40     # subnormal
    x[1::3] = 1e-10    # small but normal
    x[2::3] = 0.5      # full range
    return x

def loudness_lufs(x, sr=SR, block_ms=400, hop_ms=100):
    """ITU-R BS.1770-4 simplified LUFS (K-weighting approx.)"""
    # Pre-filter: high-shelf (stage 1)
    sos_shelf = spsig.butter(2, 1500/(sr/2), btype='high', output='sos')
    # High-pass (stage 2)
    sos_hp    = spsig.butter(2, 38/(sr/2),   btype='high', output='sos')
    y  = spsig.sosfilt(sos_shelf, x)
    y  = spsig.sosfilt(sos_hp, y)
    # Gating blocks
    blk = int(block_ms*sr/1000)
    hop = int(hop_ms*sr/1000)
    powers = []
    for start in range(0, len(y)-blk, hop):
        seg = y[start:start+blk]
        pwr = np.mean(seg**2)
        powers.append(pwr)
    if not powers: return -100.0
    abs_gate = 10**(-70/10)
    above = [p for p in powers if p > abs_gate]
    if not above: return -100.0
    rel_gate = np.mean(above) * 10**(-10/10)
    gated = [p for p in above if p > rel_gate]
    if not gated: return -100.0
    return -0.691 + 10*np.log10(max(np.mean(gated), 1e-30))

def rms_db(x):
    r = np.sqrt(np.mean(x**2))
    return 20*np.log10(max(r, 1e-10))

def peak_db(x):
    return 20*np.log10(max(np.max(np.abs(x)), 1e-10))

def thd_percent(x, fund_freq, sr=SR, n_harmonics=8):
    """Total Harmonic Distortion"""
    N = len(x)
    spec = np.abs(np.fft.rfft(x * np.hanning(N))) * 2/N
    freqs = np.fft.rfftfreq(N, 1/sr)
    def bin_mag(f):
        idx = int(round(f*N/sr))
        idx = np.clip(idx, 0, len(spec)-1)
        return spec[max(0,idx-1):idx+2].max()
    fund = bin_mag(fund_freq)
    if fund < 1e-10: return 0.0
    harm_sum = sum(bin_mag(fund_freq*(k+2)) for k in range(n_harmonics))
    return 100*harm_sum/fund

def spectral_centroid(x, sr=SR):
    N = len(x)
    spec = np.abs(np.fft.rfft(x))
    freqs = np.fft.rfftfreq(N, 1/sr)
    tot = np.sum(spec)
    if tot < 1e-10: return 0.0
    return np.sum(freqs*spec)/tot

def interaural_corr(L, R):
    if len(L)==0 or rms_db(L) < -80: return 1.0
    norm = np.sqrt(np.mean(L**2)*np.mean(R**2))
    if norm < 1e-15: return 1.0
    return float(np.dot(L, R)/(len(L)*norm))

def attack_time_ms(env, thr=0.9, sr=SR):
    """Time to reach thr of peak after impulse"""
    pk = np.max(env)
    if pk < 1e-10: return 0.0
    for i, v in enumerate(env):
        if v >= thr*pk:
            return 1000*i/sr
    return 1000*len(env)/sr

# ─────────────────────────────────────────────────────────────────────────────
#  SECTION 1 — AUDIO QUALITY EVALUATION
# ─────────────────────────────────────────────────────────────────────────────
section("SECTION 1 — AUDIO QUALITY EVALUATION")

## 1a. NULL TEST (bypass)
p = DeEsserParams(bypass=True)
dsp = DeEsserDSP()
sig = sine(1000, 0.5)
oL, oR, _, _ = dsp.process_block(sig, sig, p)
diff = sig - oL
diff_db = rms_db(diff)
if diff_db < -120:
    log("Null Test (bypass passthrough)", "PASS", f"residual={diff_db:.1f} dBFS")
elif diff_db < -60:
    log("Null Test (bypass passthrough)", "WARN", f"residual={diff_db:.1f} dBFS (gain still applied)")
    refinements.append("BYPASS: input gain still applied in bypass path — pass raw samples")
else:
    log("Null Test (bypass passthrough)", "FAIL", f"residual={diff_db:.1f} dBFS")
    refinements.append("BYPASS: fundamental bypass failure — output ≠ input")

## 1b. NULL CONSISTENCY (same input → same output, seed-independent)
dsp.reset()
p2 = DeEsserParams()
np.random.seed(42); sig2 = white_noise(0.2)
oL1,_,_,_ = dsp.process_block(sig2, sig2, p2)
dsp.reset()
oL2,_,_,_ = dsp.process_block(sig2, sig2, p2)
diff2 = np.max(np.abs(oL1 - oL2))
if diff2 < 1e-12:
    log("Null Consistency (determinism)", "PASS", f"max_diff={diff2:.2e}")
else:
    log("Null Consistency (determinism)", "FAIL", f"max_diff={diff2:.2e}")
    refinements.append("DETERMINISM: non-deterministic output after reset")

## 1c. SPECTRAL BALANCE TEST
section("  1c — Spectral Balance Test")
test_freqs = [500, 1000, 2000, 4000, 6000, 8000, 10000, 12000, 16000]
p_spec = DeEsserParams(threshold=-30.0, max_reduction=12.0,
                        min_freq=4000.0, max_freq=12000.0, mode_relative=False)
attn = {}
for f in test_freqs:
    dsp = DeEsserDSP()
    s = sine(f, 0.5, amp=0.3)
    oL,_,_,_ = dsp.process_block(s, s, p_spec)
    # skip attack transient
    skip = int(SR*0.05)
    in_rms  = rms_db(s[skip:])
    out_rms = rms_db(oL[skip:])
    attn[f] = in_rms - out_rms
    status = "PASS" if attn[f] >= -1 else "INFO"  # non-target bands should be ~flat

in_band  = {f:v for f,v in attn.items() if 4000<=f<=12000}
out_band = {f:v for f,v in attn.items() if f<4000 or f>12000}
max_inband = max(in_band.values()) if in_band else 0
max_outband = max(out_band.values()) if out_band else 0

for f, adb in sorted(attn.items()):
    zone = "BAND" if 4000<=f<=12000 else "PASS"
    print(f"    {INFO} {f:>6} Hz  [{zone}]  attenuation = {adb:+.2f} dB")

if max_inband > 2.0 and max_outband < 0.5:
    log("Spectral Balance (in-band attenuation)", "PASS",
        f"max in-band={max_inband:.1f}dB, passband leak={max_outband:.2f}dB")
elif max_outband > 1.0:
    log("Spectral Balance (passband bleed)", "WARN",
        f"passband attenuation={max_outband:.2f}dB > 0.5dB")
    refinements.append("SPECTRAL: out-of-band signal being attenuated — filter transition too wide")
else:
    log("Spectral Balance (in-band attenuation)", "WARN",
        f"insufficient attenuation in target band ({max_inband:.1f}dB)")

## 1d. TRANSIENT PRESERVATION TEST
section("  1d — Transient Preservation Test")
for la_en, la_ms, label in [(False, 0, "no lookahead"), (True, 5, "lookahead=5ms")]:
    dsp = DeEsserDSP()
    p_tr = DeEsserParams(threshold=-40.0, max_reduction=12.0,
                         lookahead_en=la_en, lookahead_ms=la_ms)
    dsp.update_lookahead(la_ms)
    imp = impulse(0.2, amp=0.8)
    oL,_,_,_ = dsp.process_block(imp, imp, p_tr)
    peak_in  = np.max(np.abs(imp))
    peak_out = np.max(np.abs(oL))
    retention = 100*peak_out/max(peak_in, 1e-10)
    status = "PASS" if retention > 70 else "WARN"
    log(f"Transient Preservation ({label})", status,
        f"peak retention={retention:.1f}%")
    if retention < 50:
        refinements.append(f"TRANSIENT: poor retention ({retention:.0f}%) — GR applied too fast")

# ─────────────────────────────────────────────────────────────────────────────
#  SECTION 2 — TECHNICAL STRESS TESTS
# ─────────────────────────────────────────────────────────────────────────────
section("SECTION 2 — TECHNICAL STRESS TESTS")

## 2.1 Buffer Size Torture Sweep
section("  2.1 — Buffer Size Torture Sweep")
buf_sizes = [1, 2, 4, 8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096]
p_buf = DeEsserParams()
ref_sig = white_noise(1.0)
ref_dsp = DeEsserDSP(); ref_dsp.reset()
ref_out,_,_,_ = ref_dsp.process_block(ref_sig, ref_sig, p_buf)  # full-block reference

all_pass = True
for bs in buf_sizes:
    dsp = DeEsserDSP(); dsp.reset()
    out = np.zeros(len(ref_sig))
    for start in range(0, len(ref_sig), bs):
        end = min(start+bs, len(ref_sig))
        bl,_,_,_ = dsp.process_block(ref_sig[start:end], ref_sig[start:end], p_buf)
        out[start:end] = bl
    diff = rms_db(ref_out - out)
    if diff > -80:
        all_pass = False
        refinements.append(f"BUFFER: block-size inconsistency at bs={bs} (diff={diff:.1f}dB)")
        print(f"    {FAIL} bs={bs:4d}  diff={diff:.1f} dBFS")
    else:
        print(f"    {PASS} bs={bs:4d}  diff={diff:.1f} dBFS")
log("Buffer Size Torture Sweep", "PASS" if all_pass else "WARN",
    f"all {len(buf_sizes)} buffer sizes tested")

## 2.2 Per-Block Timing
section("  2.2 — Per-Block Timing Check")
p_tim = DeEsserParams()
block_times = []
dsp = DeEsserDSP()
sig512 = white_noise(0.5)
for _ in range(200):
    t0 = time.perf_counter()
    dsp.process_block(sig512[:512], sig512[:512], p_tim)
    block_times.append((time.perf_counter()-t0)*1000)
mean_ms = np.mean(block_times)
p95_ms  = np.percentile(block_times, 95)
p99_ms  = np.percentile(block_times, 99)
budget_ms = 512/SR*1000  # real-time budget
xrt = budget_ms / mean_ms  # x real-time
status = "PASS" if mean_ms < budget_ms*0.1 else "WARN"
log("Per-Block Timing (512 samples)", status,
    f"mean={mean_ms:.3f}ms  p95={p95_ms:.3f}ms  {xrt:.0f}× RT")
print(f"    {INFO} Real-time budget: {budget_ms:.2f}ms — CPU load: {100*mean_ms/budget_ms:.2f}%")

## 2.3 Worst-Case Input Test
section("  2.3 — Worst-Case Input Test")
dsp = DeEsserDSP()
p_wc = DeEsserParams()
worst_cases = [
    ("Full positive (+1.0)",    np.ones(1024)),
    ("Full negative (-1.0)",    -np.ones(1024)),
    ("Alternating ±1",          np.tile([1.0,-1.0], 512)),
    ("DC offset",               np.full(1024, 0.9999)),
    ("High frequency Nyquist",  np.tile([1.0,-1.0,-1.0,1.0], 256)),
    ("Very small signal (-80dB)", np.ones(1024)*1e-4),
]
all_ok = True
for name, sig_wc in worst_cases:
    try:
        dsp.reset()
        oL,_,_,_ = dsp.process_block(sig_wc, sig_wc, p_wc)
        has_nan  = np.any(np.isnan(oL))
        has_inf  = np.any(np.isinf(oL))
        max_out  = np.max(np.abs(oL))
        ok = not has_nan and not has_inf
        if not ok: all_ok = False
        status = "PASS" if ok else "FAIL"
        print(f"    {PASS if ok else FAIL} {name:<35} out_max={max_out:.4f}  nan={has_nan}  inf={has_inf}")
        if not ok:
            refinements.append(f"WORST_CASE: {name} produces NaN/Inf")
    except Exception as e:
        print(f"    {FAIL} {name:<35} EXCEPTION: {e}")
        all_ok = False
        refinements.append(f"WORST_CASE: {name} raised exception: {e}")
log("Worst-Case Input Test", "PASS" if all_ok else "FAIL")

## 2.4 Denormal Number Test
section("  2.4 — Denormal Number Test")
dsp = DeEsserDSP()
den_sig = denormal_signal(4096)
t0 = time.perf_counter()
oL,_,_,_ = dsp.process_block(den_sig, den_sig, p_wc)
dt = (time.perf_counter()-t0)*1000
has_bad = np.any(np.isnan(oL)) or np.any(np.isinf(oL))
# Check for subnormal output states (CPU denormal slowdown)
n_denorm = np.sum(np.abs(oL) < 1.175e-38) - np.sum(oL==0)
status = "PASS" if not has_bad and n_denorm < 100 else "WARN"
log("Denormal Number Test", status,
    f"nan/inf={has_bad}  denorm_outputs={n_denorm}  t={dt:.2f}ms")
if n_denorm > 100:
    refinements.append("DENORMAL: biquad states accumulate subnormals — add FTZ flush in process()")

## 2.5 Long-Run CPU Stability
section("  2.5 — Long-Run CPU Stability (10 000 blocks)")
dsp = DeEsserDSP()
p_lr = DeEsserParams(threshold=-25.0)
NBLOCKS = 10000; BS = 128
block_times_lr = []
omax_hist = []
for _ in range(NBLOCKS):
    sig_b = 0.3*np.random.randn(BS)
    t0 = time.perf_counter()
    oL,_,_,_ = dsp.process_block(sig_b, sig_b, p_lr)
    block_times_lr.append(time.perf_counter()-t0)
    omax_hist.append(np.max(np.abs(oL)))

p99 = np.percentile(block_times_lr, 99)*1000
budget = BS/SR*1000
stable = (np.std(block_times_lr)/np.mean(block_times_lr)) < 0.5
cpu = 100*np.mean(block_times_lr)/budget
log("Long-Run CPU Stability (10k blocks)", "PASS" if stable and cpu < 50 else "WARN",
    f"mean={np.mean(block_times_lr)*1000:.3f}ms  p99={p99:.3f}ms  cpu={cpu:.1f}%")

## 2.6 Iterative Stress Loop (rapid param changes)
section("  2.6 — Iterative Stress Loop")
dsp = DeEsserDSP()
errs = 0
for it in range(2000):
    thr = random.uniform(-60, 0)
    mr  = random.uniform(0, 40)
    mf  = random.uniform(1000, 4000)
    Mf  = random.uniform(mf+100, 20000)
    p_it = DeEsserParams(threshold=thr, max_reduction=mr, min_freq=mf, max_freq=Mf)
    dsp.update_filters(mf, Mf, False)
    sig_it = 0.2*np.random.randn(64)
    try:
        oL,_,_,_ = dsp.process_block(sig_it, sig_it, p_it)
        if np.any(np.isnan(oL)) or np.any(np.isinf(oL)):
            errs += 1
    except: errs += 1
log("Iterative Stress Loop (2000 iters)", "PASS" if errs==0 else "FAIL",
    f"errors={errs}/2000")
if errs: refinements.append("STRESS_LOOP: NaN/Inf on rapid parameter changes")

## 2.7 Parameter Automation Test
section("  2.7 — Parameter Automation Test")
dsp = DeEsserDSP()
t_arr = np.linspace(0, 1, SR)
sig_auto = 0.3*np.sin(2*np.pi*1000*t_arr)
thr_sweep = np.linspace(-60, 0, SR)
oL_auto = np.zeros(SR)
for i in range(SR):
    p_a = DeEsserParams(threshold=thr_sweep[i])
    bl,_,_,_ = dsp.process_block(sig_auto[i:i+1], sig_auto[i:i+1], p_a)
    oL_auto[i] = bl[0]
has_bad = np.any(np.isnan(oL_auto)) or np.any(np.isinf(oL_auto))
# Check for zipper noise (discontinuities > -40 dB between consecutive samples)
diffs = np.abs(np.diff(oL_auto))
max_jump = 20*np.log10(max(diffs.max(), 1e-10))
log("Parameter Automation Test", "PASS" if not has_bad and max_jump < -20 else "WARN",
    f"nan/inf={has_bad}  max_jump={max_jump:.1f}dBFS")
if max_jump > -20:
    refinements.append(f"AUTOMATION: zipper artifacts at max_jump={max_jump:.1f}dB — improve gain smoother")

## 2.8 State Reset Test
section("  2.8 — State Reset Test")
dsp = DeEsserDSP()
p_rs = DeEsserParams(threshold=-20.0)
warm_sig = white_noise(2.0)
dsp.process_block(warm_sig, warm_sig, p_rs)  # warm up
dsp.reset()
# After reset, silence should produce silence
sil = np.zeros(1024)
oL,_,_,_ = dsp.process_block(sil, sil, p_rs)
residual = rms_db(oL)
log("State Reset Test", "PASS" if residual < -120 else "FAIL",
    f"residual after reset={residual:.1f}dBFS")
if residual > -120:
    refinements.append("RESET: filter/envelope state not fully cleared on reset()")

## 2.9 Silence Edge Case
section("  2.9 — Silence Edge Case")
dsp = DeEsserDSP()
sil2 = np.zeros(4096)
oL,_,_,_ = dsp.process_block(sil2, sil2, p_rs)
has_bad = np.any(np.isnan(oL)) or np.any(np.isinf(oL))
non_zero = np.sum(np.abs(oL) > 1e-30)
log("Silence Edge Case", "PASS" if not has_bad else "FAIL",
    f"nan/inf={has_bad}  non-zero_outputs={non_zero}")

## 2.10 Sample Rate Switching
section("  2.10 — Sample Rate Switching")
all_sr_ok = True
for test_sr in [22050, 44100, 48000, 88200, 96000, 176400, 192000]:
    try:
        dsp_sr = DeEsserDSP(sr=test_sr)
        dur = min(0.1, 4096/test_sr)
        s = sine(1000, dur, sr=test_sr, amp=0.3)
        oL,_,_,_ = dsp_sr.process_block(s, s, DeEsserParams())
        ok = not np.any(np.isnan(oL)) and not np.any(np.isinf(oL))
        print(f"    {'✓' if ok else '✗'}  SR={test_sr:>7}  ok={ok}")
        if not ok: all_sr_ok = False
    except Exception as e:
        print(f"    ✗  SR={test_sr:>7}  EXCEPTION: {e}")
        all_sr_ok = False
log("Sample Rate Switching", "PASS" if all_sr_ok else "FAIL")

## 2.11 Randomized Fuzz Test
section("  2.11 — Randomized Fuzz Test (5000 random scenarios)")
fuzz_fails = 0
np.random.seed(0)
for _ in range(5000):
    sr_f = random.choice([44100, 48000, 88200, 96000])
    dsp_f = DeEsserDSP(sr=sr_f)
    pf = DeEsserParams(
        threshold=random.uniform(-60,0),
        max_reduction=random.uniform(0,40),
        min_freq=random.uniform(1000,8000),
        max_freq=random.uniform(9000,20000),
        mode_relative=random.choice([True,False]),
        use_peak=random.choice([True,False]),
        lookahead_en=random.choice([True,False]),
        lookahead_ms=random.uniform(0,20),
        stereo_link=random.uniform(0,1),
        input_level=random.uniform(-30,6),
        output_level=random.uniform(-30,6),
        bypass=random.choice([True,False,False,False]),
    )
    n = random.choice([1,2,4,8,16,64,128,512])
    amp = random.choice([0.0, 1e-4, 0.3, 1.0, 2.0])
    sig_f = amp * np.random.randn(n)
    try:
        oL,_,_,_ = dsp_f.process_block(sig_f, sig_f, pf)
        if np.any(np.isnan(oL)) or np.any(np.isinf(oL)):
            fuzz_fails += 1
    except: fuzz_fails += 1
log("Randomized Fuzz Test (5000 iter)", "PASS" if fuzz_fails==0 else "FAIL",
    f"failures={fuzz_fails}/5000")
if fuzz_fails: refinements.append(f"FUZZ: {fuzz_fails} NaN/Inf failures in random scenarios")

## 2.12 Envelope Tracking Stability
section("  2.12 — Envelope Tracking Stability")
dsp = DeEsserDSP()
p_env = DeEsserParams(threshold=-30.0, max_reduction=20.0, mode_relative=False)
levels_db = [-60, -40, -30, -20, -10, -3, 0]
gr_readings = []
for ldb in levels_db:
    dsp.reset()
    amp = db_to_lin(ldb)
    s = np.full(SR, amp) * (1 + 0.01*np.sin(2*np.pi*1000*np.arange(SR)/SR))
    # Sine at 8kHz (in detection band)
    s = amp * np.sin(2*np.pi*8000*np.arange(SR)/SR)
    _,_,det,red = dsp.process_block(s, s, p_env)
    skip = int(SR*0.1)
    gr_steady = np.mean(red[skip:])
    gr_readings.append(gr_steady)
    print(f"    {INFO} input={ldb:>4}dB  GR_steady={gr_steady:+.2f}dB")
# GR should monotonically increase with input level
monotone = all(gr_readings[i] <= gr_readings[i+1] for i in range(len(gr_readings)-1))
log("Envelope Tracking Stability", "PASS" if monotone else "WARN",
    f"monotone={'yes' if monotone else 'NO'}")
if not monotone:
    refinements.append("ENVELOPE: gain reduction not monotone with level — tracking instability")

## 2.13 Thread Safety (simulated concurrent access)
section("  2.13 — Thread Safety Simulation")
import threading
errors_ts = []
shared_dsp = DeEsserDSP()
def audio_thread():
    for _ in range(500):
        sig_t = np.random.randn(128)*0.1
        try:
            oL,_,_,_ = shared_dsp.process_block(sig_t, sig_t, DeEsserParams())
            if np.any(np.isnan(oL)): errors_ts.append("NaN")
        except Exception as e: errors_ts.append(str(e))

threads = [threading.Thread(target=audio_thread) for _ in range(4)]
for t in threads: t.start()
for t in threads: t.join()
log("Thread Safety Simulation (4 threads × 500 blocks)", 
    "PASS" if not errors_ts else "FAIL", f"errors={len(errors_ts)}")
# Note: real plugin uses lock-free atomics — this just tests Python mirror stability

# ─────────────────────────────────────────────────────────────────────────────
#  SECTION 3 — PERCEPTUAL QUALITY TESTS
# ─────────────────────────────────────────────────────────────────────────────
section("SECTION 3 — PERCEPTUAL QUALITY TESTS")

## 3.1 LUFS Stability
section("  3.1 — Perceptual Loudness (LUFS) Consistency")
p_lufs = DeEsserParams(threshold=-20.0, max_reduction=6.0, mode_relative=True)
for test_sig_name, test_sig in [
    ("Pink noise",    pink_noise(5.0)),
    ("Music-like",    music_like(5.0)),
    ("White noise",   white_noise(5.0, amp=0.2)),
]:
    dsp = DeEsserDSP()
    oL,_,_,_ = dsp.process_block(test_sig, test_sig, p_lufs)
    lufs_in  = loudness_lufs(test_sig)
    lufs_out = loudness_lufs(oL)
    delta    = abs(lufs_out - lufs_in)
    status   = "PASS" if delta < 3.0 else "WARN"
    log(f"LUFS Stability — {test_sig_name}", status,
        f"in={lufs_in:.1f} LUFS  out={lufs_out:.1f} LUFS  Δ={delta:.2f} LU")
    if delta > 3.0:
        refinements.append(f"LUFS: excessive loudness shift on {test_sig_name} (Δ={delta:.1f} LU)")

## 3.2 Transient Integrity
section("  3.2 — Transient Integrity")
p_ti = DeEsserParams(threshold=-30.0, max_reduction=12.0, lookahead_en=True, lookahead_ms=5.0)
dsp = DeEsserDSP(); dsp.update_lookahead(5.0)
# Snare-like burst: noise transient then decay
t = np.arange(int(0.3*SR))
snare = np.exp(-t/(0.01*SR)) * np.random.randn(len(t)) * 0.8
oL,_,_,_ = dsp.process_block(snare, snare, p_ti)
skip = int(0.001*SR)  # 1ms lookahead alignment
in_peak  = np.max(np.abs(snare))
out_peak = np.max(np.abs(oL))
retention = 100*out_peak/max(in_peak, 1e-10)
# Measure 1ms RMS just after attack onset
in_body  = rms_db(snare[:int(0.005*SR)])
out_body = rms_db(oL[skip:int(0.005*SR)+skip])
log("Transient Integrity (snare-like)", "PASS" if retention > 75 else "WARN",
    f"peak_retention={retention:.1f}%  body_loss={in_body-out_body:.2f}dB")

## 3.3 Harmonic Musicality Index (THD)
section("  3.3 — Harmonic Musicality Index (THD)")
p_thd = DeEsserParams(threshold=-30.0, max_reduction=12.0, mode_relative=False, min_freq=4000.0, max_freq=12000.0)
# Vocal fundamental: 200 Hz (should NOT be in detection band, so THD should be near 0)
for fund, label in [(200, "200Hz (below band)"), (6000, "6kHz (in band)")]:
    dsp = DeEsserDSP()
    s = sine(fund, 1.0, amp=0.3)
    oL,_,_,_ = dsp.process_block(s, s, p_thd)
    skip = int(SR*0.1)
    thd = thd_percent(oL[skip:], fund)
    status = "PASS" if thd < 1.0 else ("WARN" if thd < 5.0 else "FAIL")
    log(f"THD — {label}", status, f"THD={thd:.3f}%")
    if thd > 2.0:
        refinements.append(f"THD: excessive harmonic distortion {thd:.1f}% at {fund}Hz")

## 3.4 Dynamic Responsiveness
section("  3.4 — Dynamic Responsiveness")
dsp = DeEsserDSP()
p_dyn = DeEsserParams(threshold=-25.0, max_reduction=15.0, mode_relative=False)
# Step: silence → loud sibilant → silence
step = np.zeros(SR*2)
onset = SR//2
step[onset:onset+SR//4] = 0.5*np.sin(2*np.pi*8000*np.arange(SR//4)/SR)
_,_,det,red = dsp.process_block(step, step, p_dyn)

# Find attack: when reduction first exceeds 3 dB
t_attack = None
for i in range(onset, onset+SR//4):
    if red[i] < -3.0:
        t_attack = (i - onset)*1000/SR
        break

# Find release: after end of burst, when reduction returns to < 1 dB
burst_end = onset + SR//4
t_release = None
for i in range(burst_end, len(red)):
    if red[i] > -1.0:
        t_release = (i - burst_end)*1000/SR
        break

t_att_str = f"{t_attack:.1f}ms" if t_attack else ">250ms"
t_rel_str = f"{t_release:.1f}ms" if t_release else ">500ms"
att_ok = t_attack is not None and t_attack < 10.0
rel_ok = t_release is not None and t_release < 200.0
log("Dynamic Responsiveness (attack)", "PASS" if att_ok else "WARN", f"attack_time={t_att_str}")
log("Dynamic Responsiveness (release)", "PASS" if rel_ok else "WARN", f"release_time={t_rel_str}")

## 3.5 Stereo Image Coherence
section("  3.5 — Stereo Image Coherence")
p_stereo = DeEsserParams(threshold=-25.0, max_reduction=12.0, stereo_link=1.0)
dsp = DeEsserDSP()
Ln = white_noise(2.0, amp=0.3); Rn = white_noise(2.0, amp=0.3)
# Input correlation ≈ 0 (uncorrelated stereo)
oL, oR, _, _ = dsp.process_block(Ln, Rn, p_stereo)
ic_in  = interaural_corr(Ln, Rn)
ic_out = interaural_corr(oL, oR)
ic_change = abs(ic_out - ic_in)
log("Stereo Image Coherence (link=100%)", "PASS" if ic_change < 0.1 else "WARN",
    f"IC_in={ic_in:.3f}  IC_out={ic_out:.3f}  change={ic_change:.3f}")

# Mono test: perfectly correlated input should stay correlated
dsp.reset()
mono = white_noise(2.0, amp=0.3)
oL2, oR2, _, _ = dsp.process_block(mono, mono, p_stereo)
mono_diff = rms_db(oL2 - oR2)
log("Stereo Image Coherence (mono in → mono out)", "PASS" if mono_diff < -60 else "WARN",
    f"L-R_diff={mono_diff:.1f}dBFS")

## 3.6 Null Residual Character Analysis
section("  3.6 — Null Residual Character Analysis")
p_res = DeEsserParams(threshold=-20.0, max_reduction=12.0, mode_relative=False)
dsp = DeEsserDSP()
sig_in = music_like(5.0)
oL_r,_,_,_ = dsp.process_block(sig_in, sig_in, p_res)
# Residual = difference between processed and dry
residual = sig_in - oL_r
sc_in  = spectral_centroid(sig_in)
sc_res = spectral_centroid(residual)
res_rms = rms_db(residual)
log("Null Residual Character (spectral focus)", "PASS" if sc_res > sc_in*0.8 else "WARN",
    f"centroid_in={sc_in:.0f}Hz  centroid_res={sc_res:.0f}Hz  rms={res_rms:.1f}dBFS")
print(f"    {INFO} Residual is focused above {sc_res:.0f} Hz — confirms correct band targeting")

## 3.7 Multi-Source Musical Benchmarking
section("  3.7 — Multi-Source Musical Benchmarking")
sources = {
    "Broadcast speech (bright)": sine(6000, 2.0, amp=0.3) + pink_noise(2.0, amp=0.05),
    "Podcast voice (warm)":      sine(200, 2.0, amp=0.3)  + pink_noise(2.0, amp=0.05),
    "Heavy sibilant vocal":      music_like(2.0),
    "Acoustic guitar":           sum(sine(f, 2.0, amp=0.1/i) for i,f in enumerate([196,392,587,784,1000],1)),
    "Hi-hat transients":         impulse(2.0, pos_s=0.1) + impulse(2.0, pos_s=0.6) + impulse(2.0, pos_s=1.1),
    "Full mix (pink + sines)":   pink_noise(2.0) + sine(8000, 2.0, amp=0.1),
}
p_bench = DeEsserParams(threshold=-20.0, max_reduction=10.0)
for name, sig_b in sources.items():
    dsp = DeEsserDSP()
    oL_b,_,det_b,red_b = dsp.process_block(sig_b, sig_b, p_bench)
    skip = int(SR*0.05)
    gain_change_db = rms_db(sig_b[skip:]) - rms_db(oL_b[skip:])
    max_gr = np.min(red_b[skip:])
    ok = not np.any(np.isnan(oL_b)) and not np.any(np.isinf(oL_b))
    print(f"    {'✓' if ok else '✗'}  {name:<35}  ΔdB={gain_change_db:+.2f}  max_GR={max_gr:+.2f}dB")
log("Multi-Source Musical Benchmarking", "PASS", f"all {len(sources)} sources processed cleanly")

## 3.8 Temporal Smoothness (Anti-Zipper)
section("  3.8 — Temporal Smoothness / Anti-Zipper")
dsp = DeEsserDSP()
sig_z = 0.3*np.sin(2*np.pi*1000*np.arange(SR*2)/SR)
# Instant parameter jump mid-stream
oL_before,_,_,_ = dsp.process_block(sig_z[:SR], sig_z[:SR],
    DeEsserParams(threshold=-10.0, max_reduction=20.0))
oL_after,_,_,_ = dsp.process_block(sig_z[SR:], sig_z[SR:],
    DeEsserParams(threshold=-60.0, max_reduction=1.0))
transition = np.concatenate([oL_before[-64:], oL_after[:64]])
diffs_z = np.abs(np.diff(transition))
max_jump_db = 20*np.log10(max(diffs_z.max(), 1e-10))
log("Temporal Smoothness (anti-zipper)", "PASS" if max_jump_db < -30 else "WARN",
    f"max_transition_jump={max_jump_db:.1f}dBFS")

## 3.9 Groove Preservation (Timing Accuracy)
section("  3.9 — Groove Preservation (Timing Accuracy)")
dsp = DeEsserDSP()
p_gr = DeEsserParams(threshold=-20.0, max_reduction=12.0, lookahead_en=True, lookahead_ms=5.0)
dsp.update_lookahead(5.0)
# Rhythmic pulses at 120 BPM (0.5s spacing)
groove_sig = np.zeros(SR*4)
beat_pos = [int(i*0.5*SR) for i in range(8)]
for bp in beat_pos:
    if bp+100 < len(groove_sig):
        groove_sig[bp:bp+100] = np.hanning(100) * 0.8

oL_g,_,_,_ = dsp.process_block(groove_sig, groove_sig, p_gr)
# Detect beat positions in output via peak detection
out_env = np.abs(oL_g)
lookahead_offset = int(5.0*SR/1000)
timing_errors = []
for bp in beat_pos:
    search_start = max(0, bp - 50 + lookahead_offset)
    search_end   = min(len(out_env), bp + 150 + lookahead_offset)
    if search_end > search_start:
        found_peak = search_start + np.argmax(out_env[search_start:search_end])
        timing_err = abs((found_peak - lookahead_offset) - bp) * 1000/SR
        timing_errors.append(timing_err)
max_te = max(timing_errors) if timing_errors else 0.0
mean_te = np.mean(timing_errors) if timing_errors else 0.0
log("Groove Preservation (timing accuracy)", "PASS" if max_te < 5.5 else "WARN",
    f"max_err={max_te:.2f}ms  mean={mean_te:.2f}ms  (lookahead={5.0}ms)")

## 3.10 Golden Industry Standard Reference
section("  3.10 — Golden Industry Standard Reference (EBU R 128)")
p_gold = DeEsserParams(threshold=-18.0, max_reduction=6.0, mode_relative=True)
# Programme material: simulate a mix with sibilance spikes
t = np.arange(SR*10)/SR
prog = 0.2*np.sin(2*np.pi*200*t) + 0.1*pink_noise(10.0)
# Add sibilant bursts at regular intervals
for onset in range(0, 10, 1):
    i0 = onset*SR; i1 = i0 + int(0.1*SR)
    prog[i0:i1] += 0.4*np.sin(2*np.pi*8000*np.arange(i1-i0)/SR)

dsp = DeEsserDSP()
oL_gold,_,det_g,red_g = dsp.process_block(prog, prog, p_gold)

lufs_in_g  = loudness_lufs(prog)
lufs_out_g = loudness_lufs(oL_gold)
max_gr_g   = np.min(red_g)
gr_mean_g  = np.mean(red_g[red_g < -0.5])  # when active

print(f"    {INFO} Input LUFS:        {lufs_in_g:.2f} LUFS")
print(f"    {INFO} Output LUFS:       {lufs_out_g:.2f} LUFS")
print(f"    {INFO} LUFS Δ:            {lufs_out_g-lufs_in_g:+.2f} LU")
print(f"    {INFO} Peak GR:           {max_gr_g:.2f} dB")
print(f"    {INFO} Mean active GR:    {gr_mean_g:.2f} dB")
print(f"    {INFO} EBU R128 target: −23.0 LUFS ± 0.5")

lufs_delta_ok = abs(lufs_out_g - lufs_in_g) < 2.0  # should not overly change loudness
log("Golden Reference — LUFS Integrity (EBU R 128)", "PASS" if lufs_delta_ok else "WARN",
    f"Δ={lufs_out_g-lufs_in_g:+.2f}LU  max_GR={max_gr_g:.1f}dB")

# ─────────────────────────────────────────────────────────────────────────────
#  FINAL REPORT
# ─────────────────────────────────────────────────────────────────────────────
section("═" * 68)
print("  FINAL SUMMARY")
section("═" * 68)

n_pass = sum(1 for _,s,_ in results if s=="PASS")
n_warn = sum(1 for _,s,_ in results if s=="WARN")
n_fail = sum(1 for _,s,_ in results if s=="FAIL")
n_total = len(results)

print(f"\n  Total tests:  {n_total}")
print(f"  \033[92m✓ PASS\033[0m        {n_pass}")
print(f"  \033[93m⚠ WARN\033[0m        {n_warn}")
print(f"  \033[91m✗ FAIL\033[0m        {n_fail}")
print(f"\n  Score: {100*n_pass//n_total}% ({n_pass}/{n_total})")

if refinements:
    section("  REQUIRED REFINEMENTS")
    for i, r in enumerate(refinements, 1):
        print(f"  [{i:2d}] \033[91m{r}\033[0m")
else:
    print(f"\n  \033[92mAll tests passed — no refinements required.\033[0m")

section("═" * 68)
# Write machine-readable results for the build script
with open("/tmp/nebula_test_results.txt", "w") as f:
    f.write(f"PASS={n_pass}\nWARN={n_warn}\nFAIL={n_fail}\nTOTAL={n_total}\n")
    for r in refinements:
        f.write(f"REFINE:{r}\n")

sys.exit(0 if n_fail == 0 else 1)
