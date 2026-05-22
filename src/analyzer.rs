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
    ring_left: Vec<f64>,
    ring_right: Vec<f64>,
    write_pos: usize,
    hop_counter: usize,
    window: Vec<f64>,
    fft: Arc<dyn Fft<f64>>,
    fft_left: Vec<Complex<f64>>,
    fft_right: Vec<Complex<f64>>,
    magnitude_scratch: Vec<f32>,
    shared: Arc<Mutex<SpectrumData>>,
}

impl SpectrumAnalyzer {
    pub fn new() -> Self {
        let mut planner = FftPlanner::<f64>::new();
        let fft = planner.plan_fft_forward(FFT_SIZE);

        Self {
            ring_left: vec![0.0; FFT_SIZE],
            ring_right: vec![0.0; FFT_SIZE],
            write_pos: 0,
            hop_counter: 0,
            window: hann_window(),
            fft,
            fft_left: vec![Complex::new(0.0, 0.0); FFT_SIZE],
            fft_right: vec![Complex::new(0.0, 0.0); FFT_SIZE],
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
        self.ring_left.fill(0.0);
        self.ring_right.fill(0.0);
        self.write_pos = 0;
        self.hop_counter = 0;
        self.magnitude_scratch.fill(-120.0);
    }

    #[inline]
    pub fn push(&mut self, sample: f64) {
        self.push_stereo(sample, sample);
    }

    #[inline]
    pub fn push_stereo(&mut self, left: f64, right: f64) {
        self.ring_left[self.write_pos] = left;
        self.ring_right[self.write_pos] = right;
        self.write_pos = (self.write_pos + 1) % FFT_SIZE;
        self.hop_counter += 1;

        if self.hop_counter >= FFT_HOP {
            self.hop_counter = 0;
            self.compute_fft();
        }
    }

    fn compute_fft(&mut self) {
        for idx in 0..FFT_SIZE {
            let ring_idx = (self.write_pos + idx) % FFT_SIZE;
            self.fft_left[idx] = Complex::new(self.ring_left[ring_idx] * self.window[idx], 0.0);
            self.fft_right[idx] = Complex::new(self.ring_right[ring_idx] * self.window[idx], 0.0);
        }

        self.fft.process(&mut self.fft_left);
        self.fft.process(&mut self.fft_right);

        self.magnitude_scratch[0] = -120.0;
        for bin_idx in 1..(NUM_BINS - 1) {
            let magnitude_l = self.fft_left[bin_idx].norm() * MAG_SCALE;
            let magnitude_r = self.fft_right[bin_idx].norm() * MAG_SCALE;
            let magnitude = ((magnitude_l * magnitude_l + magnitude_r * magnitude_r) * 0.5).sqrt();
            self.magnitude_scratch[bin_idx] = if magnitude < 1.0e-12 {
                -120.0
            } else {
                (20.0 * magnitude.log10()) as f32
            };
        }

        let nyquist_l = self.fft_left[NUM_BINS - 1].norm() * (MAG_SCALE * 0.5);
        let nyquist_r = self.fft_right[NUM_BINS - 1].norm() * (MAG_SCALE * 0.5);
        let nyquist_magnitude = ((nyquist_l * nyquist_l + nyquist_r * nyquist_r) * 0.5).sqrt();
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

#[cfg(test)]
mod tests {
    use super::{SpectrumAnalyzer, FFT_SIZE};

    #[test]
    fn stereo_analysis_preserves_antiphase_side_energy() {
        let mut analyzer = SpectrumAnalyzer::new();
        analyzer.set_sample_rate(48_000.0);

        for sample_idx in 0..(FFT_SIZE * 3) {
            let phase = 2.0 * std::f64::consts::PI * 6_000.0 * sample_idx as f64 / 48_000.0;
            let sample = phase.sin() * 0.5;
            analyzer.push_stereo(sample, -sample);
        }

        let shared = analyzer.get_shared();
        let peak = shared
            .lock()
            .magnitudes
            .iter()
            .copied()
            .fold(-120.0_f32, f32::max);

        assert!(peak > -24.0);
    }
}
