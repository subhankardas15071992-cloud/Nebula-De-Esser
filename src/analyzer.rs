#![allow(dead_code, unused_variables, clippy::cast_precision_loss)]
use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use parking_lot::Mutex;
use rustfft::Fft;

pub const FFT_SIZE: usize = 2048;
pub const FFT_HOP:  usize = 512;
pub const NUM_BINS: usize = FFT_SIZE / 2 + 1;
const MAG_SCALE: f64 = 4.0 / FFT_SIZE as f64;

pub struct SpectrumData {
    pub magnitudes:  [f32; NUM_BINS],
    pub sample_rate: f64,
    pub fft_size:    usize,
    pub fresh:       bool,
}
impl Default for SpectrumData {
    fn default() -> Self { Self { magnitudes: [-120.0; NUM_BINS], sample_rate: 44100.0, fft_size: FFT_SIZE, fresh: false } }
}

fn make_hann_window() -> Vec<f64> {
    (0..FFT_SIZE).map(|i| 0.5 * (1.0 - (2.0 * PI * i as f64 / (FFT_SIZE - 1) as f64).cos())).collect()
}

pub struct SpectrumAnalyzer {
    ring_buffer: Vec<f64>,
    write_pos: usize,
    hop_counter: usize,
    window: Vec<f64>,
    fft_scratch: Vec<rustfft::num_complex::Complex<f64>>,
    // FIX: Added generic <f64> to Fft trait
    fft_plan: Arc<dyn Fft<f64>>,
    mags_buffer: [f32; NUM_BINS],
    pub shared: Arc<Mutex<SpectrumData>>,
    pub dirty: Arc<AtomicBool>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = rustfft::FftPlanner::new();
        Self {
            ring_buffer: vec![0.0; FFT_SIZE], write_pos: 0, hop_counter: 0,
            window: make_hann_window(),
            fft_scratch: vec![rustfft::num_complex::Complex::new(0.0, 0.0); FFT_SIZE],
            fft_plan: planner.plan_fft_forward(FFT_SIZE),
            mags_buffer: [-120.0; NUM_BINS],
            shared: Arc::new(Mutex::new(SpectrumData::default())),
            dirty: Arc::new(AtomicBool::new(false)),
        }
    }

    #[inline(always)]
    pub fn push(&mut self, sample: f64) {
        self.ring_buffer[self.write_pos] = sample;
        self.write_pos = (self.write_pos + 1) % FFT_SIZE;
        self.hop_counter += 1;
        if self.hop_counter >= FFT_HOP { self.hop_counter = 0; self.compute_fft(); }
    }

    fn compute_fft(&mut self) {
        for i in 0..FFT_SIZE {
            let ring_idx = (self.write_pos + i) % FFT_SIZE;
            self.fft_scratch[i] = rustfft::num_complex::Complex::new(self.ring_buffer[ring_idx] * self.window[i], 0.0);
        }
        self.fft_plan.process(&mut self.fft_scratch);
        
        self.mags_buffer[0] = -120.0;
        for i in 1..NUM_BINS - 1 {
            let mag = self.fft_scratch[i].norm() * MAG_SCALE;
            self.mags_buffer[i] = if mag < 1e-12 { -120.0 } else { (20.0 * mag.log10()) as f32 };
        }
        let i = NUM_BINS - 1;
        let mag = self.fft_scratch[i].norm() * (MAG_SCALE * 0.5);
        self.mags_buffer[i] = if mag < 1e-12 { -120.0 } else { (20.0 * mag.log10()) as f32 };

        if let Some(mut guard) = self.shared.try_lock() {
            guard.magnitudes.copy_from_slice(&self.mags_buffer);
            guard.fft_size = FFT_SIZE; guard.fresh = true;
            self.dirty.store(true, Ordering::Release);
        }
    }

    pub fn get_shared(&self) -> Arc<Mutex<SpectrumData>> { Arc::clone(&self.shared) }
    pub fn reset(&mut self) { self.ring_buffer.fill(0.0); self.write_pos = 0; self.hop_counter = 0; }
}
