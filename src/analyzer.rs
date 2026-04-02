#![allow(dead_code, unused_variables, clippy::cast_precision_loss)]
// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser v2.3.0 — Post-effects Spectrum Analyzer
// Placed after the full signal chain so the display reflects what the user hears.
//
// Accuracy fixes vs v2.2:
//   1. Ring buffer read order — oldest sample first, not from write_pos (which
//      points one ahead of the last written sample, causing a 1-sample phase
//      wrap that scrambled the window alignment).
//   2. Magnitude scaling — Hann window has a coherent gain of 0.5, so the
//      correct normalisation is (2.0 / (FFT_SIZE * window_sum)) where
//      window_sum ≈ FFT_SIZE * 0.5.  Simplified: scale = 4.0 / FFT_SIZE.
//      The previous code used 2.0 / FFT_SIZE which under-reported by 6 dB.
//   3. DC bin is set to -120 dBFS (not meaningful for audio analysis).
//   4. Nyquist bin (index 0 of the mirrored half) is not doubled.
// ─────────────────────────────────────────────────────────────────────────────

use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;

pub const FFT_SIZE: usize = 2048;
pub const FFT_HOP:  usize = 512;       // 75% overlap
pub const NUM_BINS: usize = FFT_SIZE / 2 + 1;

// Hann window coherent gain correction: 4.0 / FFT_SIZE
// Derivation: Hann sum = FFT_SIZE/2, single-sided doubles non-DC bins → 2/(FFT_SIZE/2) = 4/FFT_SIZE
const MAG_SCALE: f64 = 4.0 / FFT_SIZE as f64;

/// Shared spectrum data read by the GUI thread
pub struct SpectrumData {
    pub magnitudes:  Vec<f32>,
    pub sample_rate: f64,
    pub fft_size:    usize,
    pub fresh:       bool,
}

impl Default for SpectrumData {
    fn default() -> Self {
        Self {
            magnitudes:  vec![-120.0_f32; NUM_BINS],
            sample_rate: 44100.0,
            fft_size:    FFT_SIZE,
            fresh:       false,
        }
    }
}

/// Precompute Hann window coefficients
fn make_hann_window() -> Vec<f64> {
    (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (FFT_SIZE - 1) as f64).cos()))
        .collect()
}

/// Lock-free post-effects spectrum analyzer.
/// Audio thread writes via `push()`, GUI reads via the shared Arc<Mutex<SpectrumData>>.
/// `try_lock()` is used on the audio thread — if the GUI holds the lock the frame is
/// skipped rather than blocking.
pub struct SpectrumAnalyzer {
    /// Circular sample buffer — always FFT_SIZE samples
    ring_buffer: Vec<f64>,
    /// Index of the *next* write position (oldest sample is at write_pos)
    write_pos:   usize,
    /// Counts samples since last FFT trigger
    hop_counter: usize,
    window:      Vec<f64>,
    /// Reused FFT scratch buffer
    fft_scratch: Vec<rustfft::num_complex::Complex<f64>>,
    planner:     rustfft::FftPlanner<f64>,
    pub shared:  Arc<Mutex<SpectrumData>>,
    pub dirty:   Arc<AtomicBool>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = rustfft::FftPlanner::new();
        // Pre-warm the planner so the first audio block doesn't allocate
        let _ = planner.plan_fft_forward(FFT_SIZE);

        Self {
            ring_buffer: vec![0.0; FFT_SIZE],
            write_pos:   0,
            hop_counter: 0,
            window:      make_hann_window(),
            fft_scratch: vec![rustfft::num_complex::Complex::new(0.0, 0.0); FFT_SIZE],
            planner,
            shared:      Arc::new(Mutex::new(SpectrumData::default())),
            dirty:       Arc::new(AtomicBool::new(false)),
        }
    }

    /// Push one post-effects mono sample into the analyzer.
    /// Called from the audio thread — must never allocate or block.
    #[inline(always)]
    pub fn push(&mut self, sample: f64) {
        self.ring_buffer[self.write_pos] = sample;
        // Advance write pointer — write_pos now points to the oldest sample
        self.write_pos = (self.write_pos + 1) % FFT_SIZE;
        self.hop_counter += 1;

        if self.hop_counter >= FFT_HOP {
            self.hop_counter = 0;
            self.compute_fft();
        }
    }

    fn compute_fft(&mut self) {
        // Linearise ring buffer oldest-first with Hann window applied.
        // write_pos is the oldest sample because we advanced it after writing.
        for i in 0..FFT_SIZE {
            let ring_idx = (self.write_pos + i) % FFT_SIZE;
            self.fft_scratch[i] = rustfft::num_complex::Complex::new(
                self.ring_buffer[ring_idx] * self.window[i],
                0.0,
            );
        }

        let fft = self.planner.plan_fft_forward(FFT_SIZE);
        fft.process(&mut self.fft_scratch);

        // Compute single-sided magnitude spectrum in dBFS.
        // MAG_SCALE = 4.0 / FFT_SIZE corrects for Hann window gain and
        // the factor-of-2 for the single-sided representation.
        // DC (bin 0) and Nyquist (bin NUM_BINS-1) are NOT doubled — they
        // have no mirror image in the single-sided spectrum.
        let mut mags = vec![-120.0_f32; NUM_BINS];

        // DC — not useful, suppress
        mags[0] = -120.0;

        // Bins 1 .. NUM_BINS-2 — doubled (single-sided)
        for i in 1..NUM_BINS - 1 {
            let mag = self.fft_scratch[i].norm() * MAG_SCALE;
            mags[i] = if mag < 1e-12 {
                -120.0_f32
            } else {
                (20.0 * mag.log10()) as f32
            };
        }

        // Nyquist bin — not doubled
        {
            let i = NUM_BINS - 1;
            let mag = self.fft_scratch[i].norm() * (MAG_SCALE * 0.5);
            mags[i] = if mag < 1e-12 { -120.0_f32 } else { (20.0 * mag.log10()) as f32 };
        }

        // Non-blocking write to shared data — skip frame if GUI holds the lock
        if let Some(mut guard) = self.shared.try_lock() {
            guard.magnitudes = mags;
            guard.fft_size   = FFT_SIZE;
            guard.fresh      = true;
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn get_shared(&self) -> Arc<Mutex<SpectrumData>> {
        Arc::clone(&self.shared)
    }

    pub fn reset(&mut self) {
        self.ring_buffer.fill(0.0);
        self.write_pos   = 0;
        self.hop_counter = 0;
    }

    pub fn num_bins() -> usize  { NUM_BINS }
    pub fn fft_size() -> usize  { FFT_SIZE }
}
