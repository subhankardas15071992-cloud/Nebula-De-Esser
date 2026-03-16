// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser — Spectrum Analyzer
// Lock-free ring buffer FFT analyzer with Hann windowing
// ─────────────────────────────────────────────────────────────────────────────

use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;

const FFT_SIZE: usize = 2048;
const FFT_HOP: usize = 512;
const NUM_BINS: usize = FFT_SIZE / 2 + 1;

/// Shared spectrum data (lock-protected for GUI thread access)
pub struct SpectrumData {
    pub magnitudes: Vec<f32>,
    pub sample_rate: f64,
    pub fresh: bool,
}

impl Default for SpectrumData {
    fn default() -> Self {
        Self {
            magnitudes: vec![f32::NEG_INFINITY; NUM_BINS],
            sample_rate: 44100.0,
            fresh: false,
        }
    }
}

/// Hann window precomputed for FFT_SIZE
fn make_hann_window() -> Vec<f64> {
    (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (FFT_SIZE - 1) as f64).cos()))
        .collect()
}

/// Lock-free spectrum analyzer — audio thread writes, GUI reads
pub struct SpectrumAnalyzer {
    ring_buffer: Vec<f64>,
    write_pos: usize,
    hop_counter: usize,
    window: Vec<f64>,
    fft_input: Vec<f64>,
    fft_output: Vec<rustfft::num_complex::Complex<f64>>,
    planner: rustfft::FftPlanner<f64>,
    pub shared: Arc<Mutex<SpectrumData>>,
    pub dirty: Arc<AtomicBool>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = rustfft::FftPlanner::new();
        // Pre-plan FFT
        let _ = planner.plan_fft_forward(FFT_SIZE);

        Self {
            ring_buffer: vec![0.0; FFT_SIZE],
            write_pos: 0,
            hop_counter: 0,
            window: make_hann_window(),
            fft_input: vec![0.0; FFT_SIZE],
            fft_output: vec![rustfft::num_complex::Complex::new(0.0, 0.0); FFT_SIZE],
            planner,
            shared: Arc::new(Mutex::new(SpectrumData::default())),
            dirty: Arc::new(AtomicBool::new(false)),
        }
    }

    /// Feed mono sample (mix of both channels) to analyzer
    #[inline(always)]
    pub fn push(&mut self, sample: f64) {
        self.ring_buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % FFT_SIZE;
        self.hop_counter += 1;

        if self.hop_counter >= FFT_HOP {
            self.hop_counter = 0;
            self.compute_fft();
        }
    }

    fn compute_fft(&mut self) {
        // Linearize ring buffer into fft_input with Hann window
        for i in 0..FFT_SIZE {
            let idx = (self.write_pos + i) % FFT_SIZE;
            self.fft_input[i] = self.ring_buffer[idx] * self.window[i];
        }

        // Copy to complex buffer
        for i in 0..FFT_SIZE {
            self.fft_output[i] =
                rustfft::num_complex::Complex::new(self.fft_input[i], 0.0);
        }

        let fft = self.planner.plan_fft_forward(FFT_SIZE);
        fft.process(&mut self.fft_output);

        // Compute magnitudes in dBFS
        let scale = 2.0 / FFT_SIZE as f64;
        let mut mags = vec![f32::NEG_INFINITY; NUM_BINS];
        for i in 0..NUM_BINS {
            let mag = self.fft_output[i].norm() * scale;
            mags[i] = if mag < 1e-12 {
                -120.0_f32
            } else {
                (20.0 * mag.log10()) as f32
            };
        }

        // Write to shared (non-blocking try_lock, skip frame if locked)
        if let Some(mut guard) = self.shared.try_lock() {
            guard.magnitudes = mags;
            guard.fresh = true;
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn get_shared(&self) -> Arc<Mutex<SpectrumData>> {
        Arc::clone(&self.shared)
    }

    pub fn reset(&mut self) {
        self.ring_buffer.fill(0.0);
        self.write_pos = 0;
        self.hop_counter = 0;
    }

    pub fn num_bins() -> usize {
        NUM_BINS
    }

    pub fn fft_size() -> usize {
        FFT_SIZE
    }
}
