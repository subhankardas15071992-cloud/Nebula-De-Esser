// Nebula DeEsser DSP Engine — TKEO + Orthogonal Subspace Projection
// Implements: Teager-Kaiser Energy Operator for sibilance detection
//             Adaptive N-dimensional subspace analysis in Relative mode

use std::f64::consts::{FRAC_1_SQRT_2, PI};

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

// ============================================================================
// TKEO-BASED SIBILANCE DETECTION
// ============================================================================

/// Teager-Kaiser Energy Operator: ψ[x(n)] = x²[n] - x[n-1]·x[n+1]
/// Measures instantaneous energy to detect sharp transients (sibilance)
#[inline]
pub fn teager_kaiser_energy(current: f64, prev_1: f64, prev_2: f64) -> f64 {
    (prev_1 * prev_1 - current * prev_2).abs()
}

/// TKEO spike detection threshold based on sensitivity setting
/// 
/// # Arguments
/// * `tkeo_value` - Current TKEO energy measurement
/// * `sensitivity` - 0.0 (aggressive) to 1.0 (selective)
/// * `baseline_features` - [short, mid, long] TKEO envelopes for context
/// 
/// # Returns
/// Normalized detection confidence [0.0, 1.0]
#[inline]
pub fn tkeo_detection_threshold(
    tkeo_value: f64,
    sensitivity: f64,
    baseline_features: [f64; 3],
) -> f64 {
    // Higher sensitivity = requires sharper spike to trigger classification
    let baseline = baseline_features[2]; // Long-term envelope = vocal cord vibration baseline
    let spike_ratio = tkeo_value / (baseline + 1.0e-12);
    
    // Sensitivity mapping:
    // 0.0 → trigger at 1.2x baseline (aggressive, catches mild sibilance)
    // 1.0 → trigger at 3.0x baseline (conservative, only sharp transients)
    let threshold_multiplier = 1.2 + sensitivity * 1.8;
    
    // Return normalized detection confidence [0, 1]
    ((spike_ratio - 1.0) / (threshold_multiplier - 1.0)).clamp(0.0, 1.0)
}

// ============================================================================
// ADAPTIVE SUBSPACE TRACKER
// ============================================================================

#[derive(Clone, Copy, Debug)]
pub struct AdaptiveSubspaceTracker {
    eigenvector: [f64; 3],
    update_rate: f64,
}

impl AdaptiveSubspaceTracker {
    pub fn new() -> Self {
        Self {
            eigenvector: [0.577_350_269_2; 3], // Normalized [1,1,1]/√3
            update_rate: 0.00035,
        }
    }

    #[inline]
    pub fn update(&mut self, features: [f64; 3]) {
        let norm = features.iter().map(|v| v * v).sum::<f64>().sqrt();
        if norm <= 1.0e-12 {
            return;
        }
        let normalized = features.map(|v| v / norm);
        let dot = self.eigenvector
            .iter()
            .zip(normalized.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>();

        for (index, component) in self.eigenvector.iter_mut().enumerate() {
            *component += self.update_rate * dot * (normalized[index] - dot * *component);
        }

        let vec_norm = self.eigenvector.iter().map(|v| v * v).sum::<f64>().sqrt();
        if vec_norm > 1.0e-12 {
            for component in &mut self.eigenvector {
                *component /= vec_norm;
            }
        }
    }

    #[inline]
    pub fn orthogonal_ratio(&self, features: [f64; 3]) -> f64 {
        let norm_sq = features.iter().map(|v| v * v).sum::<f64>().max(1.0e-12);
        let projection = self.eigenvector
            .iter()
            .zip(features.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>();
        ((norm_sq - projection * projection).max(0.0) / norm_sq).clamp(0.0, 1.0)
    }
}

// ============================================================================
// CHANNEL STATE
// ============================================================================

#[derive(Clone, Debug)]
pub struct ChannelState {
    // TKEO multi-resolution envelopes
    pub tkeo_env_short: EnvelopeFollower,
    pub tkeo_env_mid: EnvelopeFollower,
    pub tkeo_env_long: EnvelopeFollower,
    
    // Subspace tracking
    pub subspace_tracker: AdaptiveSubspaceTracker,
    
    // Formant tracking for vowel classification
    pub formant_trackers: [Kalman1D; 3],
    
    // State for TKEO calculation
    pub prev_input_1: f64,
    pub prev_input_2: f64,
    
    // Vowel classification state
    pub vowel_probs: [f64; 5],
    pub dominant_vowel: VowelClass,
}

impl ChannelState {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            tkeo_env_short: EnvelopeFollower::new(0.25, 8.0, sample_rate),
            tkeo_env_mid: EnvelopeFollower::new(0.8, 25.0, sample_rate),
            tkeo_env_long: EnvelopeFollower::new(2.5, 80.0, sample_rate),
            subspace_tracker: AdaptiveSubspaceTracker::new(),
            formant_trackers: default_formant_trackers(),
            prev_input_1: 0.0,
            prev_input_2: 0.0,
            vowel_probs: [0.2; 5],
            dominant_vowel: VowelClass::A,
        }
    }

    pub fn reset(&mut self) {
        self.tkeo_env_short.reset();
        self.tkeo_env_mid.reset();
        self.tkeo_env_long.reset();
        self.subspace_tracker = AdaptiveSubspaceTracker::new();
        self.formant_trackers = default_formant_trackers();
        self.prev_input_1 = 0.0;
        self.prev_input_2 = 0.0;
        self.vowel_probs = [0.2; 5];
        self.dominant_vowel = VowelClass::A;
    }
}

// ============================================================================
// PROCESS SETTINGS (updated for TKEO)
// ============================================================================

#[derive(Clone, Copy, Debug, Default)]
pub struct ProcessSettings {
    pub tkeo_sensitivity: f64,      // NEW: TKEO spike detection threshold (0-1)
    pub max_reduction_db: f64,
    pub mode_relative: bool,         // true = Relative (N-D), false = Absolute (3-D)
    pub use_peak_filter: bool,
    pub use_wide_range: bool,
    pub trigger_hear: bool,
    pub filter_solo: bool,
    pub stereo_link: f64,
    pub stereo_mid_side: bool,
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

// ============================================================================
// MAIN DSP PROCESSOR
// ============================================================================

pub struct DeEsserDsp {
    sample_rate: f64,
    channels: [ChannelState; 2],
    // ... other filter coefficients would go here
}

impl DeEsserDsp {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            sample_rate,
            channels: [
                ChannelState::new(sample_rate),
                ChannelState::new(sample_rate),
            ],
        }
    }

    pub fn reset(&mut self) {
        for channel in &mut self.channels {
            channel.reset();
        }
    }

    pub fn update_filters(
        &mut self,
        _min_freq: f64,
        _max_freq: f64,
        _use_peak: bool,
        _cut_width: f64,
        _cut_depth: f64,
        _cut_slope: f64,
        _max_reduction: f64,
    ) {
        // Filter update logic would go here
    }

    pub fn update_lookahead(&mut self, _delay_ms: f64) {
        // Lookahead buffer management
    }

    pub fn update_vocal_mode(&mut self, _single_vocal: bool) {
        // Adjust envelope times based on vocal mode
    }

    /// Core processing: TKEO detection + subspace projection
    #[inline]
    pub fn process_frame(
        &mut self,
        input_l: f64,
        input_r: f64,
        sidechain_l: f64,
        sidechain_r: f64,
        settings: ProcessSettings,
    ) -> ProcessFrame {
        // Process each channel
        let (wet_l, dry_l, det_db_l, red_db_l) = 
            self.process_channel(input_l, sidechain_l, settings, 0);
        let (wet_r, dry_r, det_db_r, red_db_r) = 
            self.process_channel(input_r, sidechain_r, settings, 1);

        // Stereo linking
        let stereo_link = settings.stereo_link.clamp(0.0, 1.0);
        let linked_det = (det_db_l + det_db_r) * 0.5;
        let linked_red = (red_db_l + red_db_r) * 0.5;
        
        let detection_db = det_db_l * (1.0 - stereo_link) + linked_det * stereo_link;
        let reduction_db = red_db_l * (1.0 - stereo_link) + linked_red * stereo_link;

        ProcessFrame {
            wet_l,
            wet_r,
            dry_l,
            dry_r,
            detection_db,
            reduction_db,
        }
    }

    #[inline]
    fn process_channel(
        &mut self,
        input: f64,
        sidechain: f64,
        settings: ProcessSettings,
        channel_idx: usize,
    ) -> (f64, f64, f64, f64) {
        let channel = &mut self.channels[channel_idx];
        
        // === TKEO CALCULATION ===
        let tkeo = teager_kaiser_energy(input, channel.prev_input_1, channel.prev_input_2);
        channel.prev_input_2 = channel.prev_input_1;
        channel.prev_input_1 = input;

        // Multi-resolution TKEO envelopes for context
        let f_short = channel.tkeo_env_short.process(tkeo);
        let f_mid = channel.tkeo_env_mid.process(tkeo);
        let f_long = channel.tkeo_env_long.process(tkeo);
        let features = [f_short, f_mid, f_long];

        // Update adaptive subspace tracker
        channel.subspace_tracker.update(features);
        let orth_ratio = channel.subspace_tracker.orthogonal_ratio(features);

        // === SUBSPACE METRICS ===
        let subspace_factor = self.subspace_metrics(
            input,
            f_short, f_mid, f_long,
            settings.mode_relative,
            settings.use_wide_range,
            channel_idx,
            settings.tkeo_sensitivity,
            orth_ratio,
        );

        // === TKEO GATING ===
        let tkeo_gate = tkeo_detection_threshold(tkeo, settings.tkeo_sensitivity, features);
        
        // Combined detection: subspace analysis × TKEO sensitivity gating
        let detection_confidence = (subspace_factor * tkeo_gate).clamp(0.0, 1.0);
        
        // Apply gain reduction based on detection
        let reduction_amount = if detection_confidence > 0.1 {
            let excess = (detection_confidence - 0.1) / 0.9;
            (excess * settings.max_reduction_db.abs()).clamp(0.0, settings.max_reduction_db.abs())
        } else {
            0.0
        };

        let reduction_db = -reduction_amount;
        let gain = db_to_lin(reduction_db);
        
        let wet = input * gain;
        let dry = input;
        let det_db = lin_to_db(f_short); // Report short-window TKEO as detection level
        let red_db = reduction_db;

        (wet, dry, det_db, red_db)
    }

    /// Subspace metrics: 3-vector (Absolute) or N-dimensional (Relative) analysis
    #[inline]
    fn subspace_metrics(
        &mut self,
        input: f64,
        f_short: f64,
        f_mid: f64,
        f_long: f64,
        mode_relative: bool,
        _use_wide_range: bool,
        channel_idx: usize,
        tkeo_sensitivity: f64,
        orth_ratio: f64,
    ) -> f64 {
        let channel = &mut self.channels[channel_idx];
        
        // === BASE 3-VECTOR DECOMPOSITION ===
        let sum = (f_short + f_mid + f_long).max(1.0e-12);
        let voiced_axis = (f_long / sum).clamp(0.0, 1.0);      // Harmonics (vocal cords)
        let unvoiced_axis = (f_short / sum).clamp(0.0, 1.0);    // Sibilance (TKEO-detected noise)
        let residual_axis = (f_mid / sum).clamp(0.0, 1.0);      // "Math dust" (uncategorized)

        // Orthogonal energy gating: remove what doesn't fit voiced subspace
        let base_3vector = (unvoiced_axis * 0.55 + residual_axis * 0.45)
            * (0.55 + 0.45 * orth_ratio)
            * (1.0 - 0.25 * voiced_axis);

        if !mode_relative {
            // === ABSOLUTE MODE: Strict 3-vector ===
            return base_3vector.clamp(0.2, 1.1);
        }

        // === RELATIVE MODE: Adaptive N-dimensional contextual analysis ===
        
        // 1. Higher-Order Correlation: Cross-frequency relationships
        //    e.g., how 12kHz air correlates with 300Hz chest resonance
        let high_freq_energy = f_short;
        let low_freq_energy = f_long;
        let cross_correlation = (high_freq_energy * low_freq_energy).sqrt() / 
                               ((high_freq_energy.powi(2) + low_freq_energy.powi(2)) / 2.0 + 1.0e-12);
        
        // 2. Contextual Intelligence: Detect signal complexity
        let flux = ((f_short - f_mid).abs() + (f_mid - f_long).abs()) / 
                   (f_short + f_mid + f_long + 1.0e-12);
        let spectral_entropy = estimate_spectral_entropy(input, channel_idx);
        
        // 3. Dynamic dimension expansion decision
        let complexity_score = (flux * 0.4 + (1.0 - cross_correlation) * 0.3 + spectral_entropy * 0.3)
            .clamp(0.0, 1.0);
        
        // Expand subspace dimensions when signal is complex (breathy, textured vocals)
        let dimension_expansion = if complexity_score > 0.6 {
            // Add contextual vectors: breath texture, formant transition rate, etc.
            let breath_indicator = (f_short / (f_mid + 1.0e-9)).clamp(0.0, 2.0) - 1.0;
            let formant_stability = channel.formant_trackers.iter()
                .map(|k| (k.estimate - k.covariance).abs())
                .sum::<f64>() / 3.0;
            
            // Weight extra vectors by complexity
            (breath_indicator.abs() * 0.5 + (1.0 - formant_stability) * 0.5) 
                * (complexity_score - 0.6) / 0.4
        } else {
            0.0
        }.clamp(0.0, 1.0);
        
        // 4. Combine: base 3-vector + adaptive expansion + TKEO sensitivity gating
        let extended_vector_gain = base_3vector 
            * (1.0 + dimension_expansion * 0.4)  // Expand when contextually appropriate
            * (0.8 + 0.2 * tkeo_sensitivity);    // Sensitivity modulates overall gain
        
        extended_vector_gain.clamp(0.25, 1.2)
    }
}

// ============================================================================
// HELPER STRUCTURES
// ============================================================================

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VowelClass {
    A, E, I, O, U,
}

impl VowelClass {
    #[inline]
    fn idx(self) -> usize {
        match self {
            Self::A => 0, Self::E => 1, Self::I => 2, Self::O => 3, Self::U => 4,
        }
    }
    #[inline]
    fn all() -> [Self; 5] {
        [Self::A, Self::E, Self::I, Self::O, Self::U]
    }
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

#[inline]
fn default_formant_trackers() -> [Kalman1D; 3] {
    [
        Kalman1D::new(730.0, 0.004, 0.05),
        Kalman1D::new(1090.0, 0.003, 0.04),
        Kalman1D::new(2440.0, 0.005, 0.06),
    ]
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

#[inline]
fn smoothing_coeff(time_ms: f64, sample_rate: f64) -> f64 {
    if time_ms <= 0.0 {
        0.0
    } else {
        (-1.0 / ((time_ms * sample_rate) / 1000.0)).exp()
    }
}

#[inline]
fn estimate_spectral_entropy(input: f64, _channel_idx: usize) -> f64 {
    // Simplified proxy: track recent input variance as spectral complexity indicator
    // In production: would use actual short-term FFT entropy calculation
    use std::sync::atomic::{AtomicF64, Ordering};
    static RECENT_INPUT: [AtomicF64; 2] = [
        AtomicF64::new(0.0),
        AtomicF64::new(0.0),
    ];
    
    let prev = RECENT_INPUT[_channel_idx].load(Ordering::Relaxed);
    let current = input.abs();
    let variance = (current - prev).abs();
    RECENT_INPUT[_channel_idx].store(current, Ordering::Relaxed);
    
    (variance * 10.0).clamp(0.0, 1.0)
}
