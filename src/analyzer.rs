#![allow(dead_code, unused_variables, clippy::cast_precision_loss)]
// ─────────────────────────────────────────────────────────────────────────────
// Nebula De Esser v2.3.0 — Post-effects Spectrum Analyzer (FIXED for Real-Time)
// FIX: Removed heap allocations inside process loop.
// FIX: Changed shared data to fixed arrays to prevent GUI allocations.
// ─────────────────────────────────────────────────────────────────────────────

use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use rustfft::Fft; // Import trait for .process

pub const FFT_SIZE: usize = 2048;
pub const FFT_HOP:  usize = 512;       // 75% overlap
pub const NUM_BINS: usize = FFT_SIZE / 2 + 1;

// Hann window coherent gain correction: 4.0 / FFT_SIZE
const MAG_SCALE: f64 = 4.0 / FFT_SIZE as f64;

/// Shared spectrum data read by the GUI thread
pub struct SpectrumData {
    /// Fixed array prevents allocation in the GUI thread
    pub magnitudes:  [f32; NUM_BINS],
    pub sample_rate: f64,
    pub fft_size:    usize,
    pub fresh:       bool,
}

impl Default for SpectrumData {
    fn default() -> Self {
        Self {
            magnitudes:  [-120.0_f32; NUM_BINS], // Fixed array initialization
            sample_rate: 44100.0,
            fft_size:    FFT_SIZE,
            fresh:       false,
        }
    }
}

/// Precompute Hann window coefficients
fn make_hann_window() -> Vec {
    (0..FFT_SIZE)
        .map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (FFT_SIZE - 1) as f64).cos()))
        .collect()
}

/// Lock-free post-effects spectrum analyzer.
pub struct SpectrumAnalyzer {
    /// Circular sample buffer
    ring_buffer: Vec,
    write_pos:   usize,
    hop_counter: usize,
    window:      Vec,
    /// Reused FFT scratch buffer
    fft_scratch: Vec&lt;rustfft::num_complex::Complex>,
    /// Cached FFT plan to avoid lookups
    fft_plan:    Arc<dyn rustfft::Fft<f64>>,
    /// Pre-allocated buffer for magnitude calculation (FIX: prevents allocation in compute_fft)
    mags_buffer: [f32; NUM_BINS],
    
    pub shared:  Arc&lt;Mutex>,
    pub dirty:   Arc,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = rustfft::FftPlanner::new();
        let plan = planner.plan_fft_forward(FFT_SIZE);

        Self {
            ring_buffer: vec![0.0; FFT_SIZE],
            write_pos:   0,
            hop_counter: 0,
            window:      make_hann_window(),
            fft_scratch: vec![rustfft::num_complex::Complex::new(0.0, 0.0); FFT_SIZE],
            fft_plan:    plan, // Cache the plan
            mags_buffer: [-120.0; NUM_BINS], // Pre-allocate calculation buffer
            shared:      Arc::new(Mutex::new(SpectrumData::default())),
            dirty:       Arc::new(AtomicBool::new(false)),
        }
    }

    /// Push one post-effects mono sample into the analyzer.
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
        // 1. Copy samples to scratch with windowing
        for i in 0..FFT_SIZE {
            let ring_idx = (self.write_pos + i) % FFT_SIZE;
            self.fft_scratch[i] = rustfft::num_complex::Complex::new(
                self.ring_buffer[ring_idx] * self.window[i],
                0.0,
            );
        }

        // 2. Run FFT (using cached plan)
        self.fft_plan.process(&mut self.fft_scratch);

        // 3. Compute magnitudes into pre-allocated buffer
        // FIX: No longer creating `vec!` here.
        
        // DC — suppress
        self.mags_buffer[0] = -120.0;

        // Bins 1 .. NUM_BINS-2 — doubled (single-sided)
        for i in 1..NUM_BINS - 1 {
            let mag = self.fft_scratch[i].norm() * MAG_SCALE;
            self.mags_buffer[i] = if mag &lt; 1e-12 {
                -120.0_f32
            } else {
                (20.0 * mag.log10()) as f32
            };
        }

        // Nyquist bin — not doubled
        {
            let i = NUM_BINS - 1;
            let mag = self.fft_scratch[i].norm() * (MAG_SCALE * 0.5);
            self.mags_buffer[i] = if mag < 1e-12 { -120.0_f32 } else { (20.0 * mag.log10()) as f32 };
        }

        // 4. Non-blocking write to shared data
        if let Some(mut guard) = self.shared.try_lock() {
            // Copy from our local buffer to the shared struct
            guard.magnitudes.copy_from_slice(&self.mags_buffer);
            guard.fft_size   = FFT_SIZE;
            guard.fresh      = true;
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn get_shared(&self) -> Arc&lt;Mutex> {
        Arc::clone(&self.shared)
    }

    pub fn reset(&mut self) {
        self.ring_buffer.fill(0.0);
        self.mags_buffer.fill(-120.0); // Reset local buffer too
        self.write_pos   = 0;
        self.hop_counter = 0;
    }

    pub fn num_bins() -> usize  { NUM_BINS }
    pub fn fft_size() -> usize  { FFT_SIZE }
}
