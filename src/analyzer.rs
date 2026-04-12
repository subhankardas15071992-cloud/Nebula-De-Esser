use parking_lot::Mutex;
use rustfft::{num_complex::Complex, Fft, FftPlanner};
use std::f64::consts::PI;
use std::sync::Arc;

pub const FFT_SIZE: usize = 2048;
pub const FFT_HOP: usize = 512;
pub const NUM_BINS: usize = FFT_SIZE / 2 + 1;
const MAG_SCALE: f64 = 4.0 / FFT_SIZE as f64;

pub struct SpectrumData {
    pub magnitudes: Vec<f32>,
    pub sample_rate: f64,
}

impl Default for SpectrumData {
    fn default() -> Self {
        Self {
            magnitudes: vec![-120.0; NUM_BINS],
            sample_rate: 44_100.0,
        }
    }
}

pub struct SpectrumAnalyzer {
    ring_buffer: Vec<f64>,
    write_pos: usize,
    hop_counter: usize,
    window: Vec<f64>,
    fft: Arc<dyn Fft<f64>>,
    fft_scratch: Vec<Complex<f64>>,
    magnitude_scratch: Vec<f32>,
    shared: Arc<Mutex<SpectrumData>>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        Self {
            ring_buffer: vec![0.0; FFT_SIZE],
            write_pos: 0,
            hop_counter: 0,
            window: hann_window(),
            fft,
            fft_scratch: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            magnitude_scratch: vec![-120.0; NUM_BINS],
            shared: Arc::new(Mutex::new(SpectrumData::default())),
        }
    }

    pub fn get_shared(&self) -> Arc<Mutex<SpectrumData>> {
        Arc::clone(&self.shared)
    }

    pub fn set_sample_rate(&mut self, sample_rate: f64) {
        self.shared.lock().sample_rate = sample_rate;
    }

    pub fn reset(&mut self) {
        self.ring_buffer.fill(0.0);
        self.write_pos = 0;
        self.hop_counter = 0;
        self.magnitude_scratch.fill(-120.0);
    }

    #[inline]
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
        for (idx, scratch) in self.fft_scratch.iter_mut().enumerate() {
            let ring_idx = (self.write_pos + idx) % FFT_SIZE;
            *scratch = Complex::new(self.ring_buffer[ring_idx] * self.window[idx], 0.0);
        }

        self.fft.process(&mut self.fft_scratch);

        self.magnitude_scratch[0] = -120.0;
        for bin_idx in 1..(NUM_BINS - 1) {
            let magnitude = self.fft_scratch[bin_idx].norm() * MAG_SCALE;
            self.magnitude_scratch[bin_idx] = if magnitude < 1.0e-12 {
                -120.0
            } else {
                (20.0 * magnitude.log10()) as f32
            };
        }

        let nyquist_magnitude = self.fft_scratch[NUM_BINS - 1].norm() * (MAG_SCALE * 0.5);
        self.magnitude_scratch[NUM_BINS - 1] = if nyquist_magnitude < 1.0e-12 {
            -120.0
        } else {
            (20.0 * nyquist_magnitude.log10()) as f32
        };

        if let Some(mut shared) = self.shared.try_lock() {
            shared.magnitudes.clone_from(&self.magnitude_scratch);
        }
    }
}

fn hann_window() -> Vec<f64> {
    (0..FFT_SIZE)
        .map(|idx| 0.5 * (1.0 - (2.0 * PI * idx as f64 / (FFT_SIZE - 1) as f64).cos()))
        .collect()
}
