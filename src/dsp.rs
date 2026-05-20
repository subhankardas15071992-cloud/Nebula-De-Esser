use std::f64::consts::{FRAC_1_SQRT_2, PI};
use std::sync::Arc;

use rustfft::{num_complex::Complex, Fft, FftPlanner};

const SPECTRAL_OSP_FFT_SIZE: usize = 512;
const SPECTRAL_OSP_HOP: usize = 128;
const SPECTRAL_OSP_BINS: usize = SPECTRAL_OSP_FFT_SIZE / 2 + 1;
const SPECTRAL_OSP_BASIS: usize = 3;
const SPECTRAL_OSP_COV_SIZE: usize = SPECTRAL_OSP_BINS * SPECTRAL_OSP_BINS;
const SPECTRAL_OSP_POWER_ITERS: usize = 2;

#[inline]
pub fn db_to_lin(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

#[inline]
pub fn lin_to_db(value: f64) -> f64 {
    if value <= 1.0e-12 {
        -120.0
    } else {
        20.0 * value.log10()
    }
}

#[inline]
fn ftz(value: f64) -> f64 {
    if value.abs() < 1.0e-30 {
        0.0
    } else {
        value
    }
}

#[derive(Clone, Copy, Debug)]
enum VowelClass {
    A,
    E,
    I,
    O,
    U,
}

impl VowelClass {
    #[inline]
    fn idx(self) -> usize {
        match self {
            Self::A => 0,
            Self::E => 1,
            Self::I => 2,
            Self::O => 3,
            Self::U => 4,
        }
    }

    #[inline]
    fn all() -> [Self; 5] {
        [Self::A, Self::E, Self::I, Self::O, Self::U]
    }
}

#[inline]
fn default_formant_trackers() -> [Kalman1D; 3] {
    [
        Kalman1D::new(730.0, 0.004, 0.05),
        Kalman1D::new(1090.0, 0.003, 0.04),
        Kalman1D::new(2440.0, 0.005, 0.06),
    ]
}

#[derive(Clone, Copy, Debug)]
struct Kalman1D {
    estimate: f64,
    covariance: f64,
    process_noise: f64,
    measurement_noise: f64,
}

impl Kalman1D {
    fn new(initial: f64, process_noise: f64, measurement_noise: f64) -> Self {
        Self {
            estimate: initial,
            covariance: 1.0,
            process_noise,
            measurement_noise,
        }
    }

    #[inline]
    fn update(&mut self, measurement: f64) -> f64 {
        self.covariance += self.process_noise;
        let k = self.covariance / (self.covariance + self.measurement_noise);
        self.estimate += k * (measurement - self.estimate);
        self.covariance *= 1.0 - k;
        self.estimate
    }
}

#[derive(Clone, Debug)]
struct AdaptiveSubspaceTracker {
    eigenvector: [f64; 3],
    update_rate: f64,
}

impl AdaptiveSubspaceTracker {
    fn new() -> Self {
        Self {
            eigenvector: [0.577_350_269_2; 3],
            update_rate: 0.00035,
        }
    }

    #[inline]
    fn update(&mut self, features: [f64; 3], rate_scale: f64) {
        let norm = (features.iter().map(|v| v * v).sum::<f64>()).sqrt();
        if norm <= 1.0e-12 {
            return;
        }
        let normalized = features.map(|v| v / norm);
        let update_rate = self.update_rate * rate_scale.clamp(0.0, 1.0);
        if update_rate <= 0.0 {
            return;
        }
        let dot = self
            .eigenvector
            .iter()
            .zip(normalized.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>();

        for (index, component) in self.eigenvector.iter_mut().enumerate() {
            *component += update_rate * dot * (normalized[index] - dot * *component);
        }

        let vec_norm = (self.eigenvector.iter().map(|v| v * v).sum::<f64>()).sqrt();
        if vec_norm > 1.0e-12 {
            for component in &mut self.eigenvector {
                *component /= vec_norm;
            }
        }
    }

    #[inline]
    fn orthogonal_ratio(&self, features: [f64; 3]) -> f64 {
        let norm_sq = features.iter().map(|v| v * v).sum::<f64>().max(1.0e-12);
        let projection = self
            .eigenvector
            .iter()
            .zip(features.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>();
        ((norm_sq - projection * projection).max(0.0) / norm_sq).clamp(0.0, 1.0)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct SubspaceMetrics {
    confidence: f64,
    orthogonal_ratio: f64,
}

#[derive(Clone, Copy, Debug, Default)]
struct SpectralTrainingGate {
    voiced_confidence: f64,
    sibilant_confidence: f64,
    flatness: f64,
    flux: f64,
    centroid: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BasisMode {
    Odd,
    Even,
    Both,
}

impl Default for BasisMode {
    fn default() -> Self {
        Self::Both
    }
}

impl BasisMode {
    pub fn from_selection(selection: u32) -> Self {
        match selection {
            0 => Self::Odd,
            1 => Self::Even,
            _ => Self::Both,
        }
    }

    #[inline]
    fn should_learn(self, frame_index: usize) -> bool {
        match self {
            Self::Odd => frame_index % 2 == 1,
            Self::Even => frame_index % 2 == 0,
            Self::Both => true,
        }
    }
}

#[derive(Clone)]
struct SpectralOspChannel {
    input_ring: Vec<f64>,
    wet_ring: Vec<f64>,
    dry_ring: Vec<f64>,
    norm_ring: Vec<f64>,
    window: Vec<f64>,
    frame: Vec<Complex<f64>>,
    analysis_magnitudes: Vec<f64>,
    spectral_vector: Vec<f64>,
    prev_spectral_vector: Vec<f64>,
    basis: Vec<Vec<f64>>,
    basis_work: Vec<Vec<f64>>,
    covariance: Vec<f64>,
    write_pos: usize,
    hop_counter: usize,
    filled: usize,
    basis_frame_counter: usize,
    fft: Arc<dyn Fft<f64>>,
    ifft: Arc<dyn Fft<f64>>,
}

impl SpectralOspChannel {
    fn new() -> Self {
        let mut planner = FftPlanner::<f64>::new();
        Self {
            input_ring: vec![0.0; SPECTRAL_OSP_FFT_SIZE],
            wet_ring: vec![0.0; SPECTRAL_OSP_FFT_SIZE],
            dry_ring: vec![0.0; SPECTRAL_OSP_FFT_SIZE],
            norm_ring: vec![0.0; SPECTRAL_OSP_FFT_SIZE],
            window: sqrt_hann_window(SPECTRAL_OSP_FFT_SIZE),
            frame: vec![Complex::new(0.0, 0.0); SPECTRAL_OSP_FFT_SIZE],
            analysis_magnitudes: vec![0.0; SPECTRAL_OSP_BINS],
            spectral_vector: vec![0.0; SPECTRAL_OSP_BINS],
            prev_spectral_vector: vec![0.0; SPECTRAL_OSP_BINS],
            basis: vec![vec![0.0; SPECTRAL_OSP_BINS]; SPECTRAL_OSP_BASIS],
            basis_work: vec![vec![0.0; SPECTRAL_OSP_BINS]; SPECTRAL_OSP_BASIS],
            covariance: vec![0.0; SPECTRAL_OSP_COV_SIZE],
            write_pos: 0,
            hop_counter: 0,
            filled: 0,
            basis_frame_counter: 0,
            fft: planner.plan_fft_forward(SPECTRAL_OSP_FFT_SIZE),
            ifft: planner.plan_fft_inverse(SPECTRAL_OSP_FFT_SIZE),
        }
    }

    fn latency_samples(&self) -> usize {
        SPECTRAL_OSP_FFT_SIZE
    }

    fn reset(&mut self) {
        self.input_ring.fill(0.0);
        self.wet_ring.fill(0.0);
        self.dry_ring.fill(0.0);
        self.norm_ring.fill(0.0);
        self.frame.fill(Complex::new(0.0, 0.0));
        self.analysis_magnitudes.fill(0.0);
        self.spectral_vector.fill(0.0);
        self.prev_spectral_vector.fill(0.0);
        for basis in &mut self.basis {
            basis.fill(0.0);
        }
        for basis in &mut self.basis_work {
            basis.fill(0.0);
        }
        self.covariance.fill(0.0);
        self.write_pos = 0;
        self.hop_counter = 0;
        self.filled = 0;
        self.basis_frame_counter = 0;
    }

    #[allow(clippy::too_many_arguments)]
    fn process(
        &mut self,
        input: f64,
        amount: f64,
        min_freq_hz: f64,
        max_freq_hz: f64,
        max_reduction_db: f64,
        cut_width: f64,
        cut_slope: f64,
        basis_mode: BasisMode,
        sample_rate: f64,
    ) -> (f64, f64) {
        let norm = self.norm_ring[self.write_pos].max(1.0e-9);
        let wet = self.wet_ring[self.write_pos] / norm;
        let dry = self.dry_ring[self.write_pos] / norm;
        self.wet_ring[self.write_pos] = 0.0;
        self.dry_ring[self.write_pos] = 0.0;
        self.norm_ring[self.write_pos] = 0.0;

        self.input_ring[self.write_pos] = input;
        self.write_pos = (self.write_pos + 1) % SPECTRAL_OSP_FFT_SIZE;
        self.filled = (self.filled + 1).min(SPECTRAL_OSP_FFT_SIZE);
        self.hop_counter += 1;

        if self.filled >= SPECTRAL_OSP_FFT_SIZE && self.hop_counter >= SPECTRAL_OSP_HOP {
            self.hop_counter = 0;
            self.compute_frame(
                amount,
                min_freq_hz,
                max_freq_hz,
                max_reduction_db,
                cut_width,
                cut_slope,
                basis_mode,
                sample_rate,
            );
        }

        (wet, dry)
    }

    #[allow(clippy::too_many_arguments)]
    fn compute_frame(
        &mut self,
        amount: f64,
        min_freq_hz: f64,
        max_freq_hz: f64,
        max_reduction_db: f64,
        cut_width: f64,
        cut_slope: f64,
        basis_mode: BasisMode,
        sample_rate: f64,
    ) {
        for idx in 0..SPECTRAL_OSP_FFT_SIZE {
            let ring_idx = (self.write_pos + idx) % SPECTRAL_OSP_FFT_SIZE;
            self.frame[idx] = Complex::new(self.input_ring[ring_idx] * self.window[idx], 0.0);
        }

        self.fft.process(&mut self.frame);

        let nyquist = sample_rate * 0.5;
        let min_freq = min_freq_hz.clamp(20.0, nyquist.max(20.0));
        let max_freq = max_freq_hz.clamp(min_freq + 1.0, nyquist.max(min_freq + 1.0));
        let bin_min = ((min_freq * SPECTRAL_OSP_FFT_SIZE as f64 / sample_rate).floor() as usize)
            .clamp(1, SPECTRAL_OSP_BINS - 1);
        let bin_max = ((max_freq * SPECTRAL_OSP_FFT_SIZE as f64 / sample_rate).ceil() as usize)
            .clamp(bin_min, SPECTRAL_OSP_BINS - 1);

        self.analysis_magnitudes.fill(0.0);
        self.spectral_vector.fill(0.0);
        let mut band_energy = 0.0;
        for bin in bin_min..=bin_max {
            let mag = self.frame[bin].norm();
            self.analysis_magnitudes[bin] = mag;
            band_energy += mag * mag;
        }
        band_energy = band_energy.sqrt().max(1.0e-12);
        for bin in bin_min..=bin_max {
            self.spectral_vector[bin] = self.analysis_magnitudes[bin] / band_energy;
        }

        let amount = amount.clamp(0.0, 1.0);
        let training_gate = self.training_gate(amount, bin_min, bin_max);
        let diagnostic_veto = (1.0
            - 0.10 * training_gate.sibilant_confidence
            - 0.04 * training_gate.flatness
            - 0.04 * training_gate.flux
            - 0.02 * training_gate.centroid)
            .clamp(0.75, 1.0);
        let learn_rate = if basis_mode.should_learn(self.basis_frame_counter) {
            0.0035 * training_gate.voiced_confidence * diagnostic_veto
        } else {
            0.0
        };
        self.basis_frame_counter = self.basis_frame_counter.wrapping_add(1);
        if learn_rate > 1.0e-6 {
            self.update_covariance_basis(learn_rate, bin_min, bin_max);
        }
        self.prev_spectral_vector
            .copy_from_slice(&self.spectral_vector);

        let mut projection_coeffs = [0.0; SPECTRAL_OSP_BASIS];
        for (basis_idx, basis) in self.basis.iter().enumerate() {
            projection_coeffs[basis_idx] = dot(basis, &self.spectral_vector);
        }

        let floor_gain = db_to_lin(-max_reduction_db.abs());
        let center_bin = (bin_min + bin_max) as f64 * 0.5;
        let half_width = ((bin_max - bin_min).max(1) as f64) * 0.5;
        let surgical_focus = cut_width.clamp(0.0, 1.0);
        let edge_power = 1.0 + (cut_slope / 100.0).clamp(0.0, 1.0) * 3.0;

        for bin in bin_min..=bin_max {
            let mut projection = 0.0;
            for basis_idx in 0..SPECTRAL_OSP_BASIS {
                projection += projection_coeffs[basis_idx] * self.basis[basis_idx][bin];
            }
            let residual = (self.spectral_vector[bin] - projection).abs();
            let orthogonal_ratio =
                (residual / (self.spectral_vector[bin].abs() + 1.0e-9)).clamp(0.0, 1.0);
            let distance = ((bin as f64 - center_bin).abs() / half_width).clamp(0.0, 1.0);
            let broad_taper = (1.0 - distance.powf(edge_power)).clamp(0.0, 1.0);
            let narrow_taper = (1.0 - distance.powf(0.55 + edge_power)).clamp(0.0, 1.0);
            let taper = broad_taper * (1.0 - surgical_focus) + narrow_taper * surgical_focus;
            let removal = amount * orthogonal_ratio * taper;
            let gain = (1.0 - removal * (1.0 - floor_gain)).clamp(floor_gain, 1.0);
            self.frame[bin] *= gain;
            let mirror = SPECTRAL_OSP_FFT_SIZE - bin;
            if mirror < SPECTRAL_OSP_FFT_SIZE && mirror != bin {
                self.frame[mirror] *= gain;
            }
        }

        self.ifft.process(&mut self.frame);
        let inv_fft = 1.0 / SPECTRAL_OSP_FFT_SIZE as f64;
        for idx in 0..SPECTRAL_OSP_FFT_SIZE {
            let ring_idx = (self.write_pos + idx) % SPECTRAL_OSP_FFT_SIZE;
            let window = self.window[idx];
            let input = self.input_ring[ring_idx];
            self.wet_ring[ring_idx] += self.frame[idx].re * inv_fft * window;
            self.dry_ring[ring_idx] += input * window * window;
            self.norm_ring[ring_idx] += window * window;
        }
    }

    fn update_covariance_basis(&mut self, learn_rate: f64, bin_min: usize, bin_max: usize) {
        if self.spectral_vector.iter().all(|v| v.abs() < 1.0e-12) {
            return;
        }

        let alpha = learn_rate.clamp(0.0, 0.05);
        let decay = 1.0 - alpha;
        for value in &mut self.covariance {
            *value *= decay;
        }

        for row in bin_min..=bin_max {
            let row_feature = self.spectral_vector[row];
            if row_feature.abs() <= 1.0e-12 {
                continue;
            }
            let row_offset = row * SPECTRAL_OSP_BINS;
            for col in bin_min..=bin_max {
                self.covariance[row_offset + col] +=
                    alpha * row_feature * self.spectral_vector[col];
            }
        }

        self.seed_missing_basis_vectors(bin_min, bin_max);

        for _ in 0..SPECTRAL_OSP_POWER_ITERS {
            for basis_idx in 0..SPECTRAL_OSP_BASIS {
                self.basis_work[basis_idx].fill(0.0);
                for row in bin_min..=bin_max {
                    let row_offset = row * SPECTRAL_OSP_BINS;
                    self.basis_work[basis_idx][row] = dot(
                        &self.covariance[row_offset + bin_min..=row_offset + bin_max],
                        &self.basis[basis_idx][bin_min..=bin_max],
                    );
                }
            }

            orthonormalize(&mut self.basis_work);
            for basis_idx in 0..SPECTRAL_OSP_BASIS {
                self.basis[basis_idx].copy_from_slice(&self.basis_work[basis_idx]);
            }
        }
    }

    fn seed_missing_basis_vectors(&mut self, bin_min: usize, bin_max: usize) {
        if self.basis.iter().all(|basis| dot(basis, basis) > 1.0e-9) {
            return;
        }

        let center = (bin_min + bin_max) as f64 * 0.5;
        let half_width = ((bin_max - bin_min).max(1) as f64) * 0.5;
        for basis_idx in 0..SPECTRAL_OSP_BASIS {
            if dot(&self.basis[basis_idx], &self.basis[basis_idx]) > 1.0e-9 {
                continue;
            }

            self.basis[basis_idx].fill(0.0);
            for bin in bin_min..=bin_max {
                let feature = self.spectral_vector[bin];
                let x = ((bin as f64 - center) / half_width).clamp(-1.0, 1.0);
                self.basis[basis_idx][bin] = match basis_idx {
                    0 => feature,
                    1 => feature * x,
                    _ => feature * (x * x - 1.0 / 3.0),
                };
            }
        }

        orthonormalize(&mut self.basis);
    }

    fn training_gate(&self, amount: f64, bin_min: usize, bin_max: usize) -> SpectralTrainingGate {
        let bin_count = (bin_max - bin_min + 1).max(1) as f64;
        let mut mag_sum = 0.0;
        let mut log_sum = 0.0;
        let mut energy_sum = 0.0;
        let mut weighted_bin_sum = 0.0;
        let mut upper_energy = 0.0;
        let mut peak_mag = 0.0_f64;
        let upper_start = bin_min + ((bin_max - bin_min) * 2) / 3;

        for bin in bin_min..=bin_max {
            let mag = self.analysis_magnitudes[bin].max(0.0);
            let energy = mag * mag;
            mag_sum += mag;
            log_sum += (mag + 1.0e-12).ln();
            energy_sum += energy;
            weighted_bin_sum += energy * bin as f64;
            if bin >= upper_start {
                upper_energy += energy;
            }
            peak_mag = peak_mag.max(mag);
        }

        if mag_sum <= 1.0e-12 || energy_sum <= 1.0e-18 {
            return SpectralTrainingGate::default();
        }

        let arithmetic_mean = mag_sum / bin_count;
        let geometric_mean = (log_sum / bin_count).exp();
        let flatness = (geometric_mean / (arithmetic_mean + 1.0e-12)).clamp(0.0, 1.0);
        let peak_concentration =
            ((peak_mag / (mag_sum + 1.0e-12)) * bin_count).clamp(0.0, bin_count) / bin_count;
        let centroid_bin = weighted_bin_sum / energy_sum;
        let centroid =
            ((centroid_bin - bin_min as f64) / (bin_max - bin_min).max(1) as f64).clamp(0.0, 1.0);
        let upper_ratio = (upper_energy / energy_sum).clamp(0.0, 1.0);

        let prev_energy = dot(
            &self.prev_spectral_vector[bin_min..=bin_max],
            &self.prev_spectral_vector[bin_min..=bin_max],
        );
        let flux = if prev_energy <= 1.0e-12 {
            0.0
        } else {
            let diff_energy = self.spectral_vector[bin_min..=bin_max]
                .iter()
                .zip(self.prev_spectral_vector[bin_min..=bin_max].iter())
                .map(|(current, previous)| {
                    let diff = current - previous;
                    diff * diff
                })
                .sum::<f64>();
            (diff_energy.sqrt() * FRAC_1_SQRT_2).clamp(0.0, 1.0)
        };

        let noise_like = ((flatness - 0.18) / 0.58).clamp(0.0, 1.0);
        let unstable = ((flux - 0.16) / 0.68).clamp(0.0, 1.0);
        let upper_bias = ((upper_ratio - 0.42) / 0.38).clamp(0.0, 1.0);
        let centroid_bias = ((centroid - 0.55) / 0.35).clamp(0.0, 1.0);
        let sibilant_confidence = (amount * 0.52
            + noise_like * 0.28
            + unstable * 0.12
            + upper_bias * 0.05
            + centroid_bias * 0.03)
            .clamp(0.0, 1.0);

        let tonal_confidence =
            ((1.0 - noise_like) * 0.72 + peak_concentration * 0.28).clamp(0.0, 1.0);
        let stability = (1.0 - unstable).clamp(0.0, 1.0);
        let clean_confidence = (1.0 - amount).powi(2);
        let voiced_confidence =
            (tonal_confidence * stability * clean_confidence * (1.0 - sibilant_confidence).powi(2))
                .clamp(0.0, 1.0);

        SpectralTrainingGate {
            voiced_confidence,
            sibilant_confidence,
            flatness,
            flux,
            centroid,
        }
    }
}

fn sqrt_hann_window(size: usize) -> Vec<f64> {
    (0..size)
        .map(|idx| {
            let phase = 2.0 * PI * (idx as f64 + 0.5) / size as f64;
            let hann = 0.5 * (1.0 - phase.cos());
            hann.max(0.0).sqrt()
        })
        .collect()
}

#[inline]
fn dot(a: &[f64], b: &[f64]) -> f64 {
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

fn orthonormalize(basis: &mut [Vec<f64>]) {
    for idx in 0..basis.len() {
        for prev in 0..idx {
            let projection = dot(&basis[idx], &basis[prev]);
            let (left, right) = basis.split_at_mut(idx);
            let previous = &left[prev];
            let current = &mut right[0];
            for (component, previous_component) in current.iter_mut().zip(previous.iter()) {
                *component -= projection * *previous_component;
            }
        }

        let norm = dot(&basis[idx], &basis[idx]).sqrt();
        if norm > 1.0e-9 {
            for component in &mut basis[idx] {
                *component /= norm;
            }
        } else {
            basis[idx].fill(0.0);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct BiquadState {
    x1: f64,
    x2: f64,
    y1: f64,
    y2: f64,
}

impl BiquadCoeffs {
    #[inline]
    fn process(self, state: &mut BiquadState, input: f64) -> f64 {
        let output = self.b0 * input + self.b1 * state.x1 + self.b2 * state.x2
            - self.a1 * state.y1
            - self.a2 * state.y2;

        state.x2 = ftz(state.x1);
        state.x1 = ftz(input);
        state.y2 = ftz(state.y1);
        state.y1 = ftz(output);
        state.y1
    }

    #[inline]
    pub fn lowpass(freq_hz: f64, q: f64, sample_rate: f64) -> Self {
        let omega = 2.0 * PI * freq_hz / sample_rate;
        let sin = omega.sin();
        let cos = omega.cos();
        let alpha = sin / (2.0 * q.max(1.0e-6));
        let a0 = 1.0 + alpha;
        let b0 = (1.0 - cos) * 0.5;
        let b1 = 1.0 - cos;
        let b2 = b0;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    #[inline]
    pub fn highpass(freq_hz: f64, q: f64, sample_rate: f64) -> Self {
        let omega = 2.0 * PI * freq_hz / sample_rate;
        let sin = omega.sin();
        let cos = omega.cos();
        let alpha = sin / (2.0 * q.max(1.0e-6));
        let a0 = 1.0 + alpha;
        let b0 = (1.0 + cos) * 0.5;
        let b1 = -(1.0 + cos);
        let b2 = b0;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    #[inline]
    pub fn bandpass_peak(freq_hz: f64, q: f64, sample_rate: f64) -> Self {
        let omega = 2.0 * PI * freq_hz / sample_rate;
        let sin = omega.sin();
        let cos = omega.cos();
        let alpha = sin / (2.0 * q.max(1.0e-6));
        let a0 = 1.0 + alpha;

        Self {
            b0: (sin * 0.5) / a0,
            b1: 0.0,
            b2: (-sin * 0.5) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha) / a0,
        }
    }

    #[inline]
    pub fn bell(freq_hz: f64, q: f64, gain_db: f64, sample_rate: f64) -> Self {
        let omega = 2.0 * PI * freq_hz / sample_rate;
        let sin = omega.sin();
        let cos = omega.cos();
        let a = 10.0_f64.powf(gain_db / 40.0);
        let alpha = sin / (2.0 * q.max(1.0e-6));
        let a0 = 1.0 + alpha / a;

        Self {
            b0: (1.0 + alpha * a) / a0,
            b1: (-2.0 * cos) / a0,
            b2: (1.0 - alpha * a) / a0,
            a1: (-2.0 * cos) / a0,
            a2: (1.0 - alpha / a) / a0,
        }
    }
}

#[derive(Clone, Debug)]
struct EnvelopeFollower {
    attack_coeff: f64,
    release_coeff: f64,
    envelope: f64,
}

impl EnvelopeFollower {
    fn new(attack_ms: f64, release_ms: f64, sample_rate: f64) -> Self {
        let mut follower = Self {
            attack_coeff: 0.0,
            release_coeff: 0.0,
            envelope: 0.0,
        };
        follower.set_times(attack_ms, release_ms, sample_rate);
        follower
    }

    fn set_times(&mut self, attack_ms: f64, release_ms: f64, sample_rate: f64) {
        self.attack_coeff = smoothing_coeff(attack_ms, sample_rate);
        self.release_coeff = smoothing_coeff(release_ms, sample_rate);
    }

    #[inline]
    fn process(&mut self, input: f64) -> f64 {
        let target = input.abs();
        let coeff = if target > self.envelope {
            self.attack_coeff
        } else {
            self.release_coeff
        };

        self.envelope = target + coeff * (self.envelope - target);
        self.envelope = ftz(self.envelope);
        self.envelope
    }

    fn reset(&mut self) {
        self.envelope = 0.0;
    }
}

#[derive(Clone, Debug)]
struct ReductionSmoother {
    attack_coeff: f64,
    release_coeff: f64,
    hold_samples: usize,
    hold_counter: usize,
    stages: [f64; 3],
}

impl ReductionSmoother {
    fn new(attack_ms: f64, hold_ms: f64, release_ms: f64, sample_rate: f64) -> Self {
        let mut smoother = Self {
            attack_coeff: 0.0,
            release_coeff: 0.0,
            hold_samples: 0,
            hold_counter: 0,
            stages: [0.0; 3],
        };
        smoother.set_times(attack_ms, hold_ms, release_ms, sample_rate);
        smoother
    }

    fn set_times(&mut self, attack_ms: f64, hold_ms: f64, release_ms: f64, sample_rate: f64) {
        self.attack_coeff = smoothing_coeff(attack_ms, sample_rate);
        self.release_coeff = smoothing_coeff(release_ms, sample_rate);
        self.hold_samples = ((hold_ms.max(0.0) * sample_rate) / 1000.0).round() as usize;
    }

    #[inline]
    fn process(&mut self, target: f64) -> f64 {
        let target = target.clamp(0.0, 1.0);
        let current = self.stages[2];
        let mut stage_target = target;
        let coeff = if target > current {
            self.hold_counter = self.hold_samples;
            self.attack_coeff
        } else if self.hold_counter > 0 {
            self.hold_counter -= 1;
            stage_target = current;
            0.0
        } else {
            self.release_coeff
        };

        for stage in &mut self.stages {
            *stage = stage_target + coeff * (*stage - stage_target);
            *stage = ftz((*stage).clamp(0.0, 1.0));
        }

        self.stages[2]
    }

    fn reset(&mut self) {
        self.hold_counter = 0;
        self.stages = [0.0; 3];
    }
}

#[derive(Clone, Debug)]
struct LookaheadDelay {
    buffer: Vec<f64>,
    write_pos: usize,
    delay_samples: usize,
}

impl LookaheadDelay {
    fn new(max_delay_ms: f64, sample_rate: f64) -> Self {
        let capacity = ((max_delay_ms * sample_rate) / 1000.0).ceil() as usize + 2;
        Self {
            buffer: vec![0.0; capacity.max(2)],
            write_pos: 0,
            delay_samples: 0,
        }
    }

    fn set_delay(&mut self, delay_ms: f64, sample_rate: f64) {
        let samples = ((delay_ms.max(0.0) * sample_rate) / 1000.0).round() as usize;
        self.delay_samples = samples.min(self.buffer.len().saturating_sub(1));
    }

    #[inline]
    fn process(&mut self, input: f64) -> f64 {
        self.buffer[self.write_pos] = input;
        let read_pos = if self.write_pos >= self.delay_samples {
            self.write_pos - self.delay_samples
        } else {
            self.buffer.len() + self.write_pos - self.delay_samples
        };
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        self.buffer[read_pos]
    }

    fn latency_samples(&self) -> usize {
        self.delay_samples
    }

    fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessSettings {
    pub threshold_db: f64,
    pub max_reduction_db: f64,
    pub mode_relative: bool,
    pub basis_mode: BasisMode,
    pub use_wide_range: bool,
    pub trigger_hear: bool,
    pub filter_solo: bool,
    pub stereo_link: f64,
    pub stereo_mid_side: bool,
    pub midi_trigger: f64,
}

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessFrame {
    pub wet_l: f64,
    pub wet_r: f64,
    pub dry_l: f64,
    pub dry_r: f64,
    pub detection_db: f64,
    pub reduction_db: f64,
}

#[derive(Clone)]
struct ChannelState {
    detect_hp: [BiquadState; 3],
    detect_lp: [BiquadState; 3],
    split_lp: [BiquadState; 3],
    bell: [BiquadState; 2],
    detect_env: EnvelopeFollower,
    full_env: EnvelopeFollower,
    reduction: ReductionSmoother,
    audio_delay: LookaheadDelay,
    prev_input_1: f64,
    prev_input_2: f64,
    tkeo_env_short: EnvelopeFollower,
    tkeo_env_mid: EnvelopeFollower,
    tkeo_env_long: EnvelopeFollower,
    subspace_tracker: AdaptiveSubspaceTracker,
    formant_filters: [BiquadState; 3],
    formant_trackers: [Kalman1D; 3],
    vowel_probs: [f64; 5],
    dominant_vowel: VowelClass,
    spectral_osp: SpectralOspChannel,
}

impl ChannelState {
    fn new(sample_rate: f64) -> Self {
        Self {
            detect_hp: [BiquadState::default(); 3],
            detect_lp: [BiquadState::default(); 3],
            split_lp: [BiquadState::default(); 3],
            bell: [BiquadState::default(); 2],
            detect_env: EnvelopeFollower::new(0.2, 70.0, sample_rate),
            full_env: EnvelopeFollower::new(0.5, 120.0, sample_rate),
            reduction: ReductionSmoother::new(0.2, 6.0, 85.0, sample_rate),
            audio_delay: LookaheadDelay::new(20.0, sample_rate),
            prev_input_1: 0.0,
            prev_input_2: 0.0,
            tkeo_env_short: EnvelopeFollower::new(0.25, 8.0, sample_rate),
            tkeo_env_mid: EnvelopeFollower::new(0.8, 25.0, sample_rate),
            tkeo_env_long: EnvelopeFollower::new(2.5, 80.0, sample_rate),
            subspace_tracker: AdaptiveSubspaceTracker::new(),
            formant_filters: [BiquadState::default(); 3],
            formant_trackers: default_formant_trackers(),
            vowel_probs: [0.2; 5],
            dominant_vowel: VowelClass::A,
            spectral_osp: SpectralOspChannel::new(),
        }
    }

    fn reset(&mut self) {
        self.detect_hp = [BiquadState::default(); 3];
        self.detect_lp = [BiquadState::default(); 3];
        self.split_lp = [BiquadState::default(); 3];
        self.bell = [BiquadState::default(); 2];
        self.detect_env.reset();
        self.full_env.reset();
        self.reduction.reset();
        self.audio_delay.reset();
        self.prev_input_1 = 0.0;
        self.prev_input_2 = 0.0;
        self.tkeo_env_short.reset();
        self.tkeo_env_mid.reset();
        self.tkeo_env_long.reset();
        self.subspace_tracker = AdaptiveSubspaceTracker::new();
        self.formant_filters = [BiquadState::default(); 3];
        self.formant_trackers = default_formant_trackers();
        self.vowel_probs = [0.2; 5];
        self.dominant_vowel = VowelClass::A;
        self.spectral_osp.reset();
    }
}

pub struct DeEsserDsp {
    sample_rate: f64,
    detect_hp: [BiquadCoeffs; 3],
    detect_lp: [BiquadCoeffs; 3],
    split_lp: [BiquadCoeffs; 3],
    bell: [BiquadCoeffs; 2],
    formant_coeffs: [BiquadCoeffs; 3],
    detection_center_hz: f64,
    spectral_min_hz: f64,
    spectral_max_hz: f64,
    spectral_cut_width: f64,
    spectral_cut_slope: f64,
    full_cut_depth_db: f64,
    channels: [ChannelState; 2],
}

impl DeEsserDsp {
    const BUTTERWORTH_QS: [f64; 3] = [0.517_638_090_205, 0.707_106_781_187, 1.931_851_652_58];

    pub fn new(sample_rate: f64) -> Self {
        let mut dsp = Self {
            sample_rate,
            detect_hp: [BiquadCoeffs::default(); 3],
            detect_lp: [BiquadCoeffs::default(); 3],
            split_lp: [BiquadCoeffs::default(); 3],
            bell: [BiquadCoeffs::default(); 2],
            formant_coeffs: [BiquadCoeffs::default(); 3],
            detection_center_hz: 6_900.0,
            spectral_min_hz: 4_000.0,
            spectral_max_hz: 12_000.0,
            spectral_cut_width: 0.5,
            spectral_cut_slope: 50.0,
            full_cut_depth_db: 0.0,
            channels: [
                ChannelState::new(sample_rate),
                ChannelState::new(sample_rate),
            ],
        };

        dsp.update_filters(4_000.0, 12_000.0, 0.5, 1.0, 50.0, 12.0);
        dsp.update_lookahead(0.0);
        dsp.update_vocal_mode(true);
        dsp
    }

    pub fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.reset();
        }
    }

    pub fn latency_samples(&self) -> u32 {
        (self.channels[0].audio_delay.latency_samples()
            + self.channels[0].spectral_osp.latency_samples()) as u32
    }

    pub fn update_lookahead(&mut self, delay_ms: f64) {
        for channel in &mut self.channels {
            channel.audio_delay.set_delay(delay_ms, self.sample_rate);
        }
    }

    pub fn update_vocal_mode(&mut self, single_vocal: bool) {
        let (attack_ms, _hold_ms, release_ms) = if single_vocal {
            (0.15, 6.0, 65.0)
        } else {
            (0.35, 10.0, 95.0)
        };

        for channel in &mut self.channels {
            channel
                .detect_env
                .set_times(attack_ms, release_ms, self.sample_rate);
            channel
                .full_env
                .set_times(attack_ms * 1.5, release_ms * 1.35, self.sample_rate);
            channel.reduction.set_times(0.0, 0.0, 0.0, self.sample_rate);
            channel
                .tkeo_env_short
                .set_times(attack_ms * 0.8, release_ms * 0.12, self.sample_rate);
            channel
                .tkeo_env_mid
                .set_times(attack_ms * 2.5, release_ms * 0.35, self.sample_rate);
            channel
                .tkeo_env_long
                .set_times(attack_ms * 8.0, release_ms, self.sample_rate);
        }
    }

    pub fn update_filters(
        &mut self,
        min_freq_hz: f64,
        max_freq_hz: f64,
        cut_width: f64,
        cut_depth: f64,
        cut_slope: f64,
        max_reduction_db: f64,
    ) {
        let nyquist_guard = (self.sample_rate * 0.49).min(24_000.0).max(2.0);
        let mut min_freq = min_freq_hz.clamp(1.0, nyquist_guard);
        let mut max_freq = max_freq_hz.clamp(1.0, nyquist_guard);
        if min_freq > max_freq {
            std::mem::swap(&mut min_freq, &mut max_freq);
        }
        if (max_freq - min_freq).abs() < 1.0 {
            max_freq = (min_freq + 1.0).min(nyquist_guard);
        }

        let center_freq = (min_freq * max_freq).sqrt().clamp(20.0, nyquist_guard);
        let bell_q = (0.4 + cut_width.clamp(0.0, 1.0) * 11.6).clamp(0.4, 12.0);
        let slope = (cut_slope / 100.0).clamp(0.0, 1.0);

        self.detect_hp = Self::make_hp(min_freq, self.sample_rate);
        self.detect_lp = Self::make_lp(max_freq, self.sample_rate);
        self.split_lp = Self::make_lp(center_freq, self.sample_rate);
        self.detection_center_hz = center_freq;
        self.spectral_min_hz = min_freq;
        self.spectral_max_hz = max_freq;
        self.spectral_cut_width = cut_width.clamp(0.0, 1.0);
        self.spectral_cut_slope = cut_slope.clamp(0.0, 100.0);

        self.full_cut_depth_db = max_reduction_db.abs() * cut_depth.clamp(0.0, 1.0);
        let stage_1_depth = -(self.full_cut_depth_db * (0.65 + 0.2 * (1.0 - slope)));
        let stage_2_depth = -(self.full_cut_depth_db - stage_1_depth.abs());
        let bell_q_2 = (bell_q * (1.0 + slope * 1.75)).clamp(0.4, 16.0);

        self.bell = [
            BiquadCoeffs::bell(center_freq, bell_q, stage_1_depth, self.sample_rate),
            BiquadCoeffs::bell(center_freq, bell_q_2, stage_2_depth, self.sample_rate),
        ];
        self.formant_coeffs = [
            BiquadCoeffs::bandpass_peak(730.0, 2.4, self.sample_rate),
            BiquadCoeffs::bandpass_peak(1090.0, 2.8, self.sample_rate),
            BiquadCoeffs::bandpass_peak(2440.0, 3.2, self.sample_rate),
        ];
    }

    #[inline]
    pub fn process_frame(
        &mut self,
        input_l: f64,
        input_r: f64,
        sidechain_l: f64,
        sidechain_r: f64,
        settings: ProcessSettings,
    ) -> ProcessFrame {
        let stereo_link_raw = settings.stereo_link.clamp(0.0, 2.0);
        let stereo_link = stereo_link_raw.min(1.0);
        let ms_focus = (stereo_link_raw - 1.0).clamp(0.0, 1.0);
        let process_ms_focus = ms_focus > 0.0;
        let process_side_focus = settings.stereo_mid_side;
        let max_reduction_db = settings.max_reduction_db.abs().max(1.0e-6);

        let (audio_l, audio_r) = if process_ms_focus {
            lr_to_ms(input_l, input_r)
        } else {
            (input_l, input_r)
        };
        let (sc_l, sc_r) = if process_ms_focus {
            lr_to_ms(sidechain_l, sidechain_r)
        } else {
            (sidechain_l, sidechain_r)
        };

        let delayed_l = self.channels[0].audio_delay.process(audio_l);
        let delayed_r = self.channels[1].audio_delay.process(audio_r);

        let band_detect_l = self.detect_signal(sc_l, 0);
        let band_detect_r = self.detect_signal(sc_r, 1);
        let tkeo_sensitivity = threshold_to_tkeo_sensitivity(settings.threshold_db);
        let detected_l = if settings.use_wide_range {
            sc_l * 0.65 + band_detect_l * 0.35
        } else {
            band_detect_l
        };
        let detected_r = if settings.use_wide_range {
            sc_r * 0.65 + band_detect_r * 0.35
        } else {
            band_detect_r
        };

        let detected_env_l = self.channels[0].detect_env.process(detected_l);
        let detected_env_r = self.channels[1].detect_env.process(detected_r);
        let full_env_l = self.channels[0].full_env.process(sc_l);
        let full_env_r = self.channels[1].full_env.process(sc_r);

        let subspace_l = self.subspace_metrics(
            detected_l,
            detected_env_l,
            full_env_l,
            tkeo_sensitivity,
            settings.mode_relative,
            settings.use_wide_range,
            0,
        );
        let subspace_r = self.subspace_metrics(
            detected_r,
            detected_env_r,
            full_env_r,
            tkeo_sensitivity,
            settings.mode_relative,
            settings.use_wide_range,
            1,
        );
        let psycho_l = self.psychoacoustic_weight(sc_l, detected_l, 0);
        let psycho_r = self.psychoacoustic_weight(sc_r, detected_r, 1);
        let formant_lock_l = self.formant_preservation_lock(sc_l, detected_l, 0);
        let formant_lock_r = self.formant_preservation_lock(sc_r, detected_r, 1);

        let linked_detect = (detected_env_l + detected_env_r) * 0.5;
        let linked_full = (full_env_l + full_env_r) * 0.5;
        let detect_env_l = detected_env_l * (1.0 - stereo_link) + linked_detect * stereo_link;
        let detect_env_r = detected_env_r * (1.0 - stereo_link) + linked_detect * stereo_link;
        let full_env_l = full_env_l * (1.0 - stereo_link) + linked_full * stereo_link;
        let full_env_r = full_env_r * (1.0 - stereo_link) + linked_full * stereo_link;

        let comparison_l = if settings.use_wide_range {
            // Wide = full-signal analysis. Decide using overall voice behavior.
            let behavior_energy = full_env_l * (0.7 + 0.3 * subspace_l.confidence);
            lin_to_db(behavior_energy)
        } else if settings.mode_relative {
            // Split = harsh-band analysis relative to full signal.
            lin_to_db(detect_env_l) - lin_to_db(full_env_l)
        } else {
            // Split absolute = harsh-band absolute energy.
            lin_to_db(detect_env_l)
        };
        let comparison_r = if settings.use_wide_range {
            let behavior_energy = full_env_r * (0.7 + 0.3 * subspace_r.confidence);
            lin_to_db(behavior_energy)
        } else if settings.mode_relative {
            lin_to_db(detect_env_r) - lin_to_db(full_env_r)
        } else {
            lin_to_db(detect_env_r)
        };

        let fixed_trigger_db = if settings.use_wide_range {
            -24.0
        } else {
            -30.0
        };
        let base_target_l = reduction_amount(comparison_l, fixed_trigger_db, max_reduction_db);
        let base_target_r = reduction_amount(comparison_r, fixed_trigger_db, max_reduction_db);

        // OSP confidence is the main reduction controller. The envelope detector remains as a
        // stabilizing energy gate so the processor does not chase TKEO noise during silence.
        let base_support_l = base_target_l * (0.25 + 0.75 * subspace_l.confidence);
        let base_support_r = base_target_r * (0.25 + 0.75 * subspace_r.confidence);
        let osp_target_l = (subspace_l.confidence * 0.85 + base_support_l * 0.15).clamp(0.0, 1.0);
        let osp_target_r = (subspace_r.confidence * 0.85 + base_support_r * 0.15).clamp(0.0, 1.0);
        let transparency_l =
            transparency_shaping(subspace_l.orthogonal_ratio, psycho_l, formant_lock_l);
        let transparency_r =
            transparency_shaping(subspace_r.orthogonal_ratio, psycho_r, formant_lock_r);
        let midi_trigger = settings.midi_trigger.clamp(0.0, 1.0);
        let mut reduction_target_l = if midi_trigger > 0.0 {
            midi_trigger
        } else {
            osp_target_l * transparency_l
        }
        .clamp(0.0, 1.0);
        let mut reduction_target_r = if midi_trigger > 0.0 {
            midi_trigger
        } else {
            osp_target_r * transparency_r
        }
        .clamp(0.0, 1.0);

        if process_ms_focus {
            if process_side_focus {
                reduction_target_l *= 1.0 - ms_focus;
            } else {
                reduction_target_r *= 1.0 - ms_focus;
            }
        }

        let amount_l = self.channels[0].reduction.process(reduction_target_l);
        let amount_r = self.channels[1].reduction.process(reduction_target_r);
        let reduction_db_l = -(self.full_cut_depth_db * amount_l);
        let reduction_db_r = -(self.full_cut_depth_db * amount_r);

        let (spectral_wet_l, spectral_dry_l) = self.channels[0].spectral_osp.process(
            delayed_l,
            amount_l,
            self.spectral_min_hz,
            self.spectral_max_hz,
            self.full_cut_depth_db,
            self.spectral_cut_width,
            self.spectral_cut_slope,
            settings.basis_mode,
            self.sample_rate,
        );
        let (spectral_wet_r, spectral_dry_r) = self.channels[1].spectral_osp.process(
            delayed_r,
            amount_r,
            self.spectral_min_hz,
            self.spectral_max_hz,
            self.full_cut_depth_db,
            self.spectral_cut_width,
            self.spectral_cut_slope,
            settings.basis_mode,
            self.sample_rate,
        );

        let wet_l = if settings.trigger_hear {
            detected_l
        } else if settings.filter_solo {
            detected_l * db_to_lin(reduction_db_l)
        } else {
            spectral_wet_l
        };

        let wet_r = if settings.trigger_hear {
            detected_r
        } else if settings.filter_solo {
            detected_r * db_to_lin(reduction_db_r)
        } else {
            spectral_wet_r
        };

        let (wet_l, wet_r) = if process_ms_focus {
            ms_to_lr(wet_l, wet_r)
        } else {
            (wet_l, wet_r)
        };
        let (dry_l, dry_r) = if process_ms_focus {
            ms_to_lr(spectral_dry_l, spectral_dry_r)
        } else {
            (spectral_dry_l, spectral_dry_r)
        };

        ProcessFrame {
            wet_l,
            wet_r,
            dry_l,
            dry_r,
            detection_db: (lin_to_db(detect_env_l) + lin_to_db(detect_env_r)) * 0.5,
            reduction_db: (reduction_db_l + reduction_db_r) * 0.5,
        }
    }

    fn make_hp(freq_hz: f64, sample_rate: f64) -> [BiquadCoeffs; 3] {
        Self::BUTTERWORTH_QS.map(|q| BiquadCoeffs::highpass(freq_hz, q, sample_rate))
    }

    fn make_lp(freq_hz: f64, sample_rate: f64) -> [BiquadCoeffs; 3] {
        Self::BUTTERWORTH_QS.map(|q| BiquadCoeffs::lowpass(freq_hz, q, sample_rate))
    }

    #[inline]
    fn detect_signal(&mut self, input: f64, channel_idx: usize) -> f64 {
        let channel = &mut self.channels[channel_idx];
        let mut stage = input;
        for (coeffs, state) in self.detect_hp.iter().zip(channel.detect_hp.iter_mut()) {
            stage = coeffs.process(state, stage);
        }
        for (coeffs, state) in self.detect_lp.iter().zip(channel.detect_lp.iter_mut()) {
            stage = coeffs.process(state, stage);
        }
        stage
    }

    #[inline]
    fn subspace_metrics(
        &mut self,
        input: f64,
        detected_env: f64,
        full_env: f64,
        tkeo_sensitivity: f64,
        mode_relative: bool,
        use_wide_range: bool,
        channel_idx: usize,
    ) -> SubspaceMetrics {
        let channel = &mut self.channels[channel_idx];
        let tkeo = teager_kaiser_energy(input, channel.prev_input_1, channel.prev_input_2);
        channel.prev_input_2 = channel.prev_input_1;
        channel.prev_input_1 = input;

        let f_short = channel.tkeo_env_short.process(tkeo);
        let f_mid = channel.tkeo_env_mid.process(tkeo);
        let f_long = channel.tkeo_env_long.process(tkeo);
        let sum = (f_short + f_mid + f_long).max(1.0e-12);

        // Absolute mode = strict 3-vector decomposition:
        // voiced axis (harmonic), unvoiced axis (sibilant), residual axis.
        let voiced_axis = (f_long / sum).clamp(0.0, 1.0);
        let unvoiced_axis = (f_short / sum).clamp(0.0, 1.0);
        let residual_axis = (f_mid / sum).clamp(0.0, 1.0);
        let features = [voiced_axis, residual_axis, unvoiced_axis];
        let orth_ratio = channel.subspace_tracker.orthogonal_ratio(features);
        let sharpness_score = unvoiced_axis * 0.6 + residual_axis * 0.4;
        let sharpness_requirement = 0.15 + 0.7 * tkeo_sensitivity.clamp(0.0, 1.0);
        let spike_classification = ((sharpness_score - sharpness_requirement)
            / (1.0 - sharpness_requirement).max(1.0e-6))
        .clamp(0.0, 1.0);
        let learn_rate = 1.0 - spike_classification * 0.92;
        channel.subspace_tracker.update(features, learn_rate);
        let detected_ratio = (detected_env / (full_env + 1.0e-9)).clamp(0.0, 4.0);
        let detected_gate = if use_wide_range {
            ((lin_to_db(full_env + 1.0e-12) + 72.0) / 42.0).clamp(0.0, 1.0)
        } else {
            (detected_ratio / 0.55).clamp(0.0, 1.0)
        };

        let strict_three_vector = spike_classification
            * (unvoiced_axis * 0.55 + residual_axis * 0.45)
            * (0.55 + 0.45 * orth_ratio)
            * (1.0 - 0.25 * voiced_axis)
            * detected_gate;

        if !mode_relative {
            return SubspaceMetrics {
                confidence: strict_three_vector.clamp(0.0, 1.0),
                orthogonal_ratio: orth_ratio,
            };
        }

        // Relative mode = adaptive multi-vector behavior (beyond 3D when needed).
        // The decision to expand weighting uses signal context + detector relation.
        let flux = ((f_short - f_mid).abs() + (f_mid - f_long).abs()) / sum;
        let multi_vector_enable = if use_wide_range {
            (detected_ratio * 0.45 + flux * 1.6 + orth_ratio * 0.5).clamp(0.0, 1.0)
        } else {
            (detected_ratio * 0.55 + flux * 1.2 + orth_ratio * 0.6).clamp(0.0, 1.0)
        };
        let extended_vector_gain = (strict_three_vector
            + multi_vector_enable * (flux * 0.55 + orth_ratio * 0.45)
            + spike_classification * (0.12 + 0.08 * (1.0 - tkeo_sensitivity)))
            * detected_gate;

        SubspaceMetrics {
            confidence: extended_vector_gain.clamp(0.0, 1.0),
            orthogonal_ratio: orth_ratio,
        }
    }

    #[inline]
    fn psychoacoustic_weight(&self, fullband: f64, detected: f64, channel_idx: usize) -> f64 {
        let channel = &self.channels[channel_idx];
        let harmonicity = (channel.tkeo_env_long.envelope
            / (channel.tkeo_env_short.envelope + 1.0e-9))
            .clamp(0.0, 3.0);
        let voicedness = (fullband.abs() / (detected.abs() + 1.0e-9)).clamp(0.0, 8.0);
        let harmonic_mask = (harmonicity * 0.25 + voicedness * 0.08).clamp(0.0, 1.0);

        (1.0 - 0.48 * harmonic_mask).clamp(0.52, 1.0)
    }

    #[inline]
    fn formant_preservation_lock(&mut self, input: f64, detected: f64, channel_idx: usize) -> f64 {
        let channel = &mut self.channels[channel_idx];
        let mut energies = [0.0; 3];
        for (index, (coeffs, state)) in self
            .formant_coeffs
            .iter()
            .zip(channel.formant_filters.iter_mut())
            .enumerate()
        {
            let sample = coeffs.process(state, input);
            energies[index] = sample * sample;
        }

        let sum_energy = energies.iter().sum::<f64>() + 1.0e-12;
        let norm = energies.map(|v| v / sum_energy);
        let measured_f1 = 450.0 + 600.0 * norm[0];
        let measured_f2 = 800.0 + 1800.0 * norm[1];
        let measured_f3 = 2000.0 + 1700.0 * norm[2];

        let f1 = channel.formant_trackers[0].update(measured_f1);
        let f2 = channel.formant_trackers[1].update(measured_f2);
        let f3 = channel.formant_trackers[2].update(measured_f3);
        let vowel = classify_vowel(f1, f2);
        channel.dominant_vowel = vowel;
        let target = vowel_targets(vowel);

        let formant_distance = (((f1 - target[0]).abs() / 500.0)
            + ((f2 - target[1]).abs() / 1300.0)
            + ((f3 - target[2]).abs() / 1800.0))
            / 3.0;
        let lock_strength = (1.0 - formant_distance).clamp(0.0, 1.0);

        for (idx, cls) in VowelClass::all().iter().enumerate() {
            let goal = if cls.idx() == vowel.idx() { 1.0 } else { 0.0 };
            channel.vowel_probs[idx] = channel.vowel_probs[idx] * 0.98 + goal * 0.02;
        }

        let vowel_confidence = channel.vowel_probs[vowel.idx()].clamp(0.0, 1.0);
        let protected_formant = target
            .iter()
            .map(|f| (self.detection_center_hz - *f).abs())
            .fold(f64::MAX, f64::min);
        let band_overlap = (1.0 - (protected_formant / 4500.0)).clamp(0.0, 1.0);
        let sibilant_bias = (detected.abs() / (input.abs() + 1.0e-9)).clamp(0.0, 1.0);
        let effective_lock =
            lock_strength * vowel_confidence * band_overlap * (1.0 - 0.35 * sibilant_bias);

        (1.0 - 0.55 * effective_lock).clamp(0.45, 1.0)
    }

    #[inline]
    #[cfg(test)]
    fn split_complement(&mut self, input: f64, channel_idx: usize) -> (f64, f64) {
        let channel = &mut self.channels[channel_idx];
        let mut low = input;
        for (coeffs, state) in self.split_lp.iter().zip(channel.split_lp.iter_mut()) {
            low = coeffs.process(state, low);
        }

        (low, input - low)
    }
}

#[inline]
fn lr_to_ms(left: f64, right: f64) -> (f64, f64) {
    (
        (left + right) * FRAC_1_SQRT_2,
        (left - right) * FRAC_1_SQRT_2,
    )
}

#[inline]
fn ms_to_lr(mid: f64, side: f64) -> (f64, f64) {
    ((mid + side) * FRAC_1_SQRT_2, (mid - side) * FRAC_1_SQRT_2)
}

#[inline]
fn smoothing_coeff(time_ms: f64, sample_rate: f64) -> f64 {
    if time_ms <= 0.0 {
        0.0
    } else {
        (-1.0 / ((time_ms * sample_rate) / 1000.0)).exp()
    }
}

#[inline]
fn reduction_amount(detected_db: f64, threshold_db: f64, max_reduction_db: f64) -> f64 {
    let excess_db = detected_db - threshold_db;
    if excess_db <= -3.0 {
        0.0
    } else if excess_db < 3.0 {
        let t = (excess_db + 3.0) / 6.0;
        let eased = t * t * (3.0 - 2.0 * t);
        eased * (excess_db.max(0.0) / max_reduction_db).clamp(0.0, 1.0)
    } else {
        (excess_db / max_reduction_db).clamp(0.0, 1.0)
    }
}

#[inline]
fn threshold_to_tkeo_sensitivity(threshold_value: f64) -> f64 {
    (threshold_value / 100.0).clamp(0.0, 1.0)
}

#[inline]
fn transparency_shaping(subspace: f64, psycho: f64, formant_lock: f64) -> f64 {
    let subspace_weight = 0.85 + 0.15 * subspace.clamp(0.0, 1.0);
    let psycho_weight = 0.85 + 0.15 * psycho.clamp(0.0, 1.0);
    let formant_weight = 0.85 + 0.15 * formant_lock.clamp(0.0, 1.0);
    (subspace_weight * psycho_weight * formant_weight).clamp(0.65, 1.0)
}

#[inline]
fn teager_kaiser_energy(current: f64, prev_1: f64, prev_2: f64) -> f64 {
    (prev_1 * prev_1 - current * prev_2).abs()
}

#[inline]
fn classify_vowel(f1: f64, f2: f64) -> VowelClass {
    let references = [
        (VowelClass::A, [730.0, 1090.0]),
        (VowelClass::E, [530.0, 1840.0]),
        (VowelClass::I, [270.0, 2290.0]),
        (VowelClass::O, [570.0, 840.0]),
        (VowelClass::U, [300.0, 870.0]),
    ];

    let mut best = VowelClass::A;
    let mut best_distance = f64::MAX;
    for (vowel, [ref_f1, ref_f2]) in references {
        let d1 = (f1 - ref_f1) / 700.0;
        let d2 = (f2 - ref_f2) / 2200.0;
        let distance = d1 * d1 + d2 * d2;
        if distance < best_distance {
            best_distance = distance;
            best = vowel;
        }
    }

    best
}

#[inline]
fn vowel_targets(vowel: VowelClass) -> [f64; 3] {
    match vowel {
        VowelClass::A => [730.0, 1090.0, 2440.0],
        VowelClass::E => [530.0, 1840.0, 2480.0],
        VowelClass::I => [270.0, 2290.0, 3010.0],
        VowelClass::O => [570.0, 840.0, 2410.0],
        VowelClass::U => [300.0, 870.0, 2240.0],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn complementary_split_recombines_exactly() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        dsp.update_filters(4_000.0, 10_000.0, 0.5, 1.0, 50.0, 12.0);

        for sample in [0.0, 0.1, -0.25, 0.75, -0.5, 0.33] {
            let (low, high) = dsp.split_complement(sample, 0);
            assert!(((low + high) - sample).abs() < 1.0e-12);
        }
    }

    #[test]
    fn lookahead_latency_matches_requested_delay() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        dsp.update_lookahead(5.0);
        assert_eq!(dsp.latency_samples(), 752);
    }

    #[test]
    fn basis_mode_controls_which_spectral_frames_learn() {
        fn first_frame_basis_energy(mode: BasisMode) -> f64 {
            let mut channel = SpectralOspChannel::new();
            let sample_rate = 48_000.0;
            for sample_idx in 0..SPECTRAL_OSP_FFT_SIZE {
                let phase = 2.0 * PI * 7_000.0 * sample_idx as f64 / sample_rate;
                channel.process(
                    0.5 * phase.sin(),
                    0.0,
                    4_000.0,
                    12_000.0,
                    12.0,
                    0.5,
                    50.0,
                    mode,
                    sample_rate,
                );
            }

            channel
                .basis
                .iter()
                .flat_map(|basis| basis.iter())
                .map(|component| component * component)
                .sum()
        }

        assert!(first_frame_basis_energy(BasisMode::Even) > 0.5);
        assert!(first_frame_basis_energy(BasisMode::Both) > 0.5);
        assert_eq!(first_frame_basis_energy(BasisMode::Odd), 0.0);
    }

    #[test]
    fn spectral_basis_tracks_multiple_covariance_frames() {
        let mut channel = SpectralOspChannel::new();
        channel.spectral_vector[48] = 1.0;
        channel.update_covariance_basis(0.05, 48, 96);
        channel.spectral_vector.fill(0.0);
        channel.spectral_vector[96] = 1.0;
        channel.update_covariance_basis(0.05, 48, 96);

        let first_bin_power = channel
            .basis
            .iter()
            .map(|basis| basis[48].abs())
            .sum::<f64>();
        let second_bin_power = channel
            .basis
            .iter()
            .map(|basis| basis[96].abs())
            .sum::<f64>();
        let first_cov = channel.covariance[48 * SPECTRAL_OSP_BINS + 48];
        let second_cov = channel.covariance[96 * SPECTRAL_OSP_BINS + 96];

        assert!(first_cov > 0.0);
        assert!(second_cov > 0.0);
        assert!(first_bin_power > 0.5);
        assert!(second_bin_power > 0.5);
    }

    #[test]
    fn spectral_training_gate_prefers_stable_tonal_voiced_frames() {
        let mut channel = SpectralOspChannel::new();
        channel.analysis_magnitudes[72] = 1.0;
        channel.spectral_vector[72] = 1.0;

        let gate = channel.training_gate(0.0, 48, 96);

        assert!(gate.voiced_confidence > 0.7);
        assert!(gate.sibilant_confidence < 0.2);
        assert!(gate.flatness < 0.2);
    }

    #[test]
    fn spectral_training_gate_rejects_noisy_or_sibilant_frames() {
        let mut noisy = SpectralOspChannel::new();
        let norm = (49.0_f64).sqrt();
        for bin in 48..=96 {
            noisy.analysis_magnitudes[bin] = 1.0;
            noisy.spectral_vector[bin] = 1.0 / norm;
        }
        let noise_gate = noisy.training_gate(0.0, 48, 96);

        let mut sibilant = SpectralOspChannel::new();
        sibilant.analysis_magnitudes[92] = 1.0;
        sibilant.spectral_vector[92] = 1.0;
        let sibilant_gate = sibilant.training_gate(0.85, 48, 96);

        assert!(noise_gate.voiced_confidence < 0.15);
        assert!(noise_gate.sibilant_confidence > 0.25);
        assert!(sibilant_gate.voiced_confidence < 0.05);
        assert!(sibilant_gate.sibilant_confidence > 0.4);
    }

    #[test]
    fn under_threshold_signal_does_not_reduce() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        let frame = dsp.process_frame(
            0.001,
            0.001,
            0.001,
            0.001,
            ProcessSettings {
                threshold_db: 50.0,
                max_reduction_db: -12.0,
                mode_relative: false,
                ..ProcessSettings::default()
            },
        );

        assert!(frame.reduction_db > -0.1);
    }

    #[test]
    fn midi_trigger_can_drive_reduction_without_detector_audio() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        dsp.update_filters(4_000.0, 12_000.0, 0.5, 1.0, 50.0, 12.0);
        let settings = ProcessSettings {
            threshold_db: 100.0,
            max_reduction_db: -12.0,
            midi_trigger: 1.0,
            ..ProcessSettings::default()
        };

        let mut frame = ProcessFrame::default();
        for _ in 0..1024 {
            frame = dsp.process_frame(0.5, 0.5, 0.0, 0.0, settings);
        }

        assert!(frame.reduction_db < -1.0);
        assert!(frame.wet_l.is_finite());
        assert!(frame.wet_r.is_finite());
    }

    #[test]
    fn threshold_control_changes_reduction_amount() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        let settings_loose = ProcessSettings {
            threshold_db: 0.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            ..ProcessSettings::default()
        };
        let settings_strict = ProcessSettings {
            threshold_db: 100.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            ..ProcessSettings::default()
        };

        let mut loose_reduction = 0.0;
        let mut strict_reduction = 0.0;
        for sample_idx in 0..512 {
            let sample = if sample_idx % 24 == 0 { 0.85 } else { 0.0 };
            loose_reduction = dsp
                .process_frame(sample, sample, sample, sample, settings_loose)
                .reduction_db;
        }
        dsp.reset();
        for sample_idx in 0..512 {
            let sample = if sample_idx % 24 == 0 { 0.85 } else { 0.0 };
            strict_reduction = dsp
                .process_frame(sample, sample, sample, sample, settings_strict)
                .reduction_db;
        }

        assert!(loose_reduction < strict_reduction - 0.25);
    }

    #[test]
    fn split_and_wide_modes_produce_different_reduction_behavior() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        let split_settings = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            use_wide_range: false,
            ..ProcessSettings::default()
        };
        let wide_settings = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            use_wide_range: true,
            ..ProcessSettings::default()
        };

        let mut split_reduction = 0.0;
        let mut wide_reduction = 0.0;
        for _ in 0..256 {
            split_reduction = dsp
                .process_frame(0.7, 0.7, 0.7, 0.7, split_settings)
                .reduction_db;
        }
        dsp.reset();
        for _ in 0..256 {
            wide_reduction = dsp
                .process_frame(0.7, 0.7, 0.7, 0.7, wide_settings)
                .reduction_db;
        }

        assert!((split_reduction - wide_reduction).abs() > 0.1);
    }

    #[test]
    fn relative_mode_engages_adaptive_multi_vector_weighting() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        let absolute = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            ..ProcessSettings::default()
        };
        let relative = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: true,
            ..ProcessSettings::default()
        };

        let mut abs_reduction = 0.0;
        let mut rel_reduction = 0.0;
        for _ in 0..256 {
            abs_reduction = dsp
                .process_frame(0.75, 0.75, 0.75, 0.75, absolute)
                .reduction_db;
        }
        dsp.reset();
        for _ in 0..256 {
            rel_reduction = dsp
                .process_frame(0.75, 0.75, 0.75, 0.75, relative)
                .reduction_db;
        }

        assert!((abs_reduction - rel_reduction).abs() > 0.1);
    }
}
