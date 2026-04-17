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
    fn update(&mut self, features: [f64; 3]) {
        let norm = (features.iter().map(|v| v * v).sum::<f64>()).sqrt();
        if norm <= 1.0e-12 {
            return;
        }
        let normalized = features.map(|v| v / norm);
        let dot = self
            .eigenvector
            .iter()
            .zip(normalized.iter())
            .map(|(a, b)| a * b)
            .sum::<f64>();

        for (index, component) in self.eigenvector.iter_mut().enumerate() {
            *component += self.update_rate * dot * (normalized[index] - dot * *component);
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

#[derive(Clone, Debug)]
struct ChannelState {
    detect_hp: [BiquadState; 3],
    detect_lp: [BiquadState; 3],
    detect_peak: BiquadState,
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
}

impl ChannelState {
    fn new(sample_rate: f64) -> Self {
        Self {
            detect_hp: [BiquadState::default(); 3],
            detect_lp: [BiquadState::default(); 3],
            detect_peak: BiquadState::default(),
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
        }
    }

    fn reset(&mut self) {
        self.detect_hp = [BiquadState::default(); 3];
        self.detect_lp = [BiquadState::default(); 3];
        self.detect_peak = BiquadState::default();
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
    }
}

pub struct DeEsserDsp {
    sample_rate: f64,
    detect_hp: [BiquadCoeffs; 3],
    detect_lp: [BiquadCoeffs; 3],
    detect_peak: BiquadCoeffs,
    split_lp: [BiquadCoeffs; 3],
    bell: [BiquadCoeffs; 2],
    formant_coeffs: [BiquadCoeffs; 3],
    detection_center_hz: f64,
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
            detect_peak: BiquadCoeffs::default(),
            split_lp: [BiquadCoeffs::default(); 3],
            bell: [BiquadCoeffs::default(); 2],
            formant_coeffs: [BiquadCoeffs::default(); 3],
            detection_center_hz: 6_900.0,
            full_cut_depth_db: 0.0,
            channels: [
                ChannelState::new(sample_rate),
                ChannelState::new(sample_rate),
            ],
        };

        dsp.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);
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
        self.channels[0].audio_delay.latency_samples() as u32
    }

    pub fn update_lookahead(&mut self, delay_ms: f64) {
        for channel in &mut self.channels {
            channel.audio_delay.set_delay(delay_ms, self.sample_rate);
        }
    }

    pub fn update_vocal_mode(&mut self, single_vocal: bool) {
        let (attack_ms, hold_ms, release_ms) = if single_vocal {
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
            channel
                .reduction
                .set_times(attack_ms, hold_ms, release_ms, self.sample_rate);
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
        _use_peak_filter: bool,
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
        let detection_q = (center_freq / (max_freq - min_freq).max(1.0)).clamp(0.35, 8.0);
        let bell_q = (0.4 + cut_width.clamp(0.0, 1.0) * 11.6).clamp(0.4, 12.0);
        let slope = (cut_slope / 100.0).clamp(0.0, 1.0);

        self.detect_hp = Self::make_hp(min_freq, self.sample_rate);
        self.detect_lp = Self::make_lp(max_freq, self.sample_rate);
        self.detect_peak = BiquadCoeffs::bandpass_peak(center_freq, detection_q, self.sample_rate);
        self.split_lp = Self::make_lp(center_freq, self.sample_rate);
        self.detection_center_hz = center_freq;

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
        let stereo_link = settings.stereo_link.clamp(0.0, 1.0);
        let max_reduction_db = settings.max_reduction_db.abs().max(1.0e-6);

        let (audio_l, audio_r) = if settings.stereo_mid_side {
            lr_to_ms(input_l, input_r)
        } else {
            (input_l, input_r)
        };
        let (sc_l, sc_r) = if settings.stereo_mid_side {
            lr_to_ms(sidechain_l, sidechain_r)
        } else {
            (sidechain_l, sidechain_r)
        };

        let delayed_l = self.channels[0].audio_delay.process(audio_l);
        let delayed_r = self.channels[1].audio_delay.process(audio_r);

        let detected_l = self.detect_signal(sc_l, 0, settings.use_peak_filter);
        let detected_r = self.detect_signal(sc_r, 1, settings.use_peak_filter);

        let detected_env_l = self.channels[0].detect_env.process(detected_l);
        let detected_env_r = self.channels[1].detect_env.process(detected_r);
        let full_env_l = self.channels[0].full_env.process(sc_l);
        let full_env_r = self.channels[1].full_env.process(sc_r);

        let subspace_l = self.subspace_metrics(sc_l, 0);
        let subspace_r = self.subspace_metrics(sc_r, 1);
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

        let comparison_l = if settings.mode_relative {
            lin_to_db(detect_env_l) - lin_to_db(full_env_l)
        } else {
            lin_to_db(detect_env_l)
        };
        let comparison_r = if settings.mode_relative {
            lin_to_db(detect_env_r) - lin_to_db(full_env_r)
        } else {
            lin_to_db(detect_env_r)
        };

        let base_target_l = reduction_amount(comparison_l, settings.threshold_db, max_reduction_db);
        let base_target_r = reduction_amount(comparison_r, settings.threshold_db, max_reduction_db);

        // Keep user controls responsive by treating the transparency stack as a shaping layer
        // around the base control law instead of a hard attenuation cascade.
        let transparency_l = transparency_shaping(subspace_l, psycho_l, formant_lock_l);
        let transparency_r = transparency_shaping(subspace_r, psycho_r, formant_lock_r);
        let reduction_target_l = (base_target_l * transparency_l)
            .max(base_target_l * 0.35)
            .clamp(0.0, 1.0);
        let reduction_target_r = (base_target_r * transparency_r)
            .max(base_target_r * 0.35)
            .clamp(0.0, 1.0);

        let amount_l = self.channels[0].reduction.process(reduction_target_l);
        let amount_r = self.channels[1].reduction.process(reduction_target_r);
        let reduction_db_l = -(self.full_cut_depth_db * amount_l);
        let reduction_db_r = -(self.full_cut_depth_db * amount_r);

        let wet_l = if settings.trigger_hear {
            detected_l
        } else if settings.filter_solo {
            detected_l * db_to_lin(reduction_db_l)
        } else if settings.use_wide_range {
            self.apply_bell(delayed_l, amount_l, 0)
        } else {
            let (low, high) = self.split_complement(delayed_l, 0);
            low + high * db_to_lin(reduction_db_l)
        };

        let wet_r = if settings.trigger_hear {
            detected_r
        } else if settings.filter_solo {
            detected_r * db_to_lin(reduction_db_r)
        } else if settings.use_wide_range {
            self.apply_bell(delayed_r, amount_r, 1)
        } else {
            let (low, high) = self.split_complement(delayed_r, 1);
            low + high * db_to_lin(reduction_db_r)
        };

        let (wet_l, wet_r) = if settings.stereo_mid_side {
            ms_to_lr(wet_l, wet_r)
        } else {
            (wet_l, wet_r)
        };
        let (dry_l, dry_r) = if settings.stereo_mid_side {
            ms_to_lr(delayed_l, delayed_r)
        } else {
            (delayed_l, delayed_r)
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
    fn detect_signal(&mut self, input: f64, channel_idx: usize, use_peak_filter: bool) -> f64 {
        if use_peak_filter {
            return self
                .detect_peak
                .process(&mut self.channels[channel_idx].detect_peak, input);
        }

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
    fn subspace_metrics(&mut self, input: f64, channel_idx: usize) -> f64 {
        let channel = &mut self.channels[channel_idx];
        let tkeo = teager_kaiser_energy(input, channel.prev_input_1, channel.prev_input_2);
        channel.prev_input_2 = channel.prev_input_1;
        channel.prev_input_1 = input;

        let f_short = channel.tkeo_env_short.process(tkeo);
        let f_mid = channel.tkeo_env_mid.process(tkeo);
        let f_long = channel.tkeo_env_long.process(tkeo);
        let features = [f_short, f_mid, f_long];
        channel.subspace_tracker.update(features);
        let orth_ratio = channel.subspace_tracker.orthogonal_ratio(features);

        (0.3 + 0.9 * orth_ratio).clamp(0.25, 1.0)
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
    fn split_complement(&mut self, input: f64, channel_idx: usize) -> (f64, f64) {
        let channel = &mut self.channels[channel_idx];
        let mut low = input;
        for (coeffs, state) in self.split_lp.iter().zip(channel.split_lp.iter_mut()) {
            low = coeffs.process(state, low);
        }

        (low, input - low)
    }

    #[inline]
    fn apply_bell(&mut self, input: f64, amount: f64, channel_idx: usize) -> f64 {
        let channel = &mut self.channels[channel_idx];
        let stage_1 = self.bell[0].process(&mut channel.bell[0], input);
        let stage_2 = self.bell[1].process(&mut channel.bell[1], stage_1);
        let amount = amount.clamp(0.0, 1.0);
        input * (1.0 - amount) + stage_2 * amount
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
fn transparency_shaping(subspace: f64, psycho: f64, formant_lock: f64) -> f64 {
    let subspace_weight = 0.70 + 0.30 * subspace.clamp(0.0, 1.0);
    let psycho_weight = 0.75 + 0.25 * psycho.clamp(0.0, 1.0);
    let formant_weight = 0.72 + 0.28 * formant_lock.clamp(0.0, 1.0);
    (subspace_weight * psycho_weight * formant_weight).clamp(0.45, 1.0)
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
        dsp.update_filters(4_000.0, 10_000.0, false, 0.5, 1.0, 50.0, 12.0);

        for sample in [0.0, 0.1, -0.25, 0.75, -0.5, 0.33] {
            let (low, high) = dsp.split_complement(sample, 0);
            assert!(((low + high) - sample).abs() < 1.0e-12);
        }
    }

    #[test]
    fn lookahead_latency_matches_requested_delay() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        dsp.update_lookahead(5.0);
        assert_eq!(dsp.latency_samples(), 240);
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
                threshold_db: -20.0,
                max_reduction_db: 12.0,
                mode_relative: false,
                ..ProcessSettings::default()
            },
        );

        assert!(frame.reduction_db > -0.1);
    }

    #[test]
    fn threshold_control_changes_reduction_amount() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        let settings_loose = ProcessSettings {
            threshold_db: -35.0,
            max_reduction_db: 12.0,
            mode_relative: false,
            ..ProcessSettings::default()
        };
        let settings_strict = ProcessSettings {
            threshold_db: -6.0,
            max_reduction_db: 12.0,
            mode_relative: false,
            ..ProcessSettings::default()
        };

        let mut loose_reduction = 0.0;
        let mut strict_reduction = 0.0;
        for _ in 0..256 {
            loose_reduction = dsp
                .process_frame(0.85, 0.85, 0.85, 0.85, settings_loose)
                .reduction_db;
        }
        dsp.reset();
        for _ in 0..256 {
            strict_reduction = dsp
                .process_frame(0.85, 0.85, 0.85, 0.85, settings_strict)
                .reduction_db;
        }

        assert!(loose_reduction < strict_reduction - 0.5);
    }
}
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
 
+#[derive(Clone, Copy, Debug)]
+enum VowelClass {
+    A,
+    E,
+    I,
+    O,
+    U,
+}
+
+impl VowelClass {
+    #[inline]
+    fn idx(self) -> usize {
+        match self {
+            Self::A => 0,
+            Self::E => 1,
+            Self::I => 2,
+            Self::O => 3,
+            Self::U => 4,
+        }
+    }
+
+    #[inline]
+    fn all() -> [Self; 5] {
+        [Self::A, Self::E, Self::I, Self::O, Self::U]
+    }
+}
+
+#[inline]
+fn default_formant_trackers() -> [Kalman1D; 3] {
+    [
+        Kalman1D::new(730.0, 0.004, 0.05),
+        Kalman1D::new(1090.0, 0.003, 0.04),
+        Kalman1D::new(2440.0, 0.005, 0.06),
+    ]
+}
+
+#[derive(Clone, Copy, Debug)]
+struct Kalman1D {
+    estimate: f64,
+    covariance: f64,
+    process_noise: f64,
+    measurement_noise: f64,
+}
+
+impl Kalman1D {
+    fn new(initial: f64, process_noise: f64, measurement_noise: f64) -> Self {
+        Self {
+            estimate: initial,
+            covariance: 1.0,
+            process_noise,
+            measurement_noise,
+        }
+    }
+
+    #[inline]
+    fn update(&mut self, measurement: f64) -> f64 {
+        self.covariance += self.process_noise;
+        let k = self.covariance / (self.covariance + self.measurement_noise);
+        self.estimate += k * (measurement - self.estimate);
+        self.covariance *= 1.0 - k;
+        self.estimate
+    }
+}
+
+#[derive(Clone, Debug)]
+struct AdaptiveSubspaceTracker {
+    eigenvector: [f64; 3],
+    update_rate: f64,
+}
+
+impl AdaptiveSubspaceTracker {
+    fn new() -> Self {
+        Self {
+            eigenvector: [0.577_350_269_2; 3],
+            update_rate: 0.00035,
+        }
+    }
+
+    #[inline]
+    fn update(&mut self, features: [f64; 3]) {
+        let norm = (features.iter().map(|v| v * v).sum::<f64>()).sqrt();
+        if norm <= 1.0e-12 {
+            return;
+        }
+        let normalized = features.map(|v| v / norm);
+        let dot = self
+            .eigenvector
+            .iter()
+            .zip(normalized.iter())
+            .map(|(a, b)| a * b)
+            .sum::<f64>();
+
+        for (index, component) in self.eigenvector.iter_mut().enumerate() {
+            *component += self.update_rate * dot * (normalized[index] - dot * *component);
+        }
+
+        let vec_norm = (self.eigenvector.iter().map(|v| v * v).sum::<f64>()).sqrt();
+        if vec_norm > 1.0e-12 {
+            for component in &mut self.eigenvector {
+                *component /= vec_norm;
+            }
+        }
+    }
+
+    #[inline]
+    fn orthogonal_ratio(&self, features: [f64; 3]) -> f64 {
+        let norm_sq = features.iter().map(|v| v * v).sum::<f64>().max(1.0e-12);
+        let projection = self
+            .eigenvector
+            .iter()
+            .zip(features.iter())
+            .map(|(a, b)| a * b)
+            .sum::<f64>();
+        ((norm_sq - projection * projection).max(0.0) / norm_sq).clamp(0.0, 1.0)
+    }
+}
+
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
@@ -286,248 +403,313 @@ pub struct ProcessSettings {
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
 
 #[derive(Clone, Debug)]
 struct ChannelState {
     detect_hp: [BiquadState; 3],
     detect_lp: [BiquadState; 3],
     detect_peak: BiquadState,
     split_lp: [BiquadState; 3],
     bell: [BiquadState; 2],
     detect_env: EnvelopeFollower,
     full_env: EnvelopeFollower,
     reduction: ReductionSmoother,
     audio_delay: LookaheadDelay,
+    prev_input_1: f64,
+    prev_input_2: f64,
+    tkeo_env_short: EnvelopeFollower,
+    tkeo_env_mid: EnvelopeFollower,
+    tkeo_env_long: EnvelopeFollower,
+    subspace_tracker: AdaptiveSubspaceTracker,
+    formant_filters: [BiquadState; 3],
+    formant_trackers: [Kalman1D; 3],
+    vowel_probs: [f64; 5],
+    dominant_vowel: VowelClass,
 }
 
 impl ChannelState {
     fn new(sample_rate: f64) -> Self {
         Self {
             detect_hp: [BiquadState::default(); 3],
             detect_lp: [BiquadState::default(); 3],
             detect_peak: BiquadState::default(),
             split_lp: [BiquadState::default(); 3],
             bell: [BiquadState::default(); 2],
             detect_env: EnvelopeFollower::new(0.2, 70.0, sample_rate),
             full_env: EnvelopeFollower::new(0.5, 120.0, sample_rate),
             reduction: ReductionSmoother::new(0.2, 6.0, 85.0, sample_rate),
             audio_delay: LookaheadDelay::new(20.0, sample_rate),
+            prev_input_1: 0.0,
+            prev_input_2: 0.0,
+            tkeo_env_short: EnvelopeFollower::new(0.25, 8.0, sample_rate),
+            tkeo_env_mid: EnvelopeFollower::new(0.8, 25.0, sample_rate),
+            tkeo_env_long: EnvelopeFollower::new(2.5, 80.0, sample_rate),
+            subspace_tracker: AdaptiveSubspaceTracker::new(),
+            formant_filters: [BiquadState::default(); 3],
+            formant_trackers: default_formant_trackers(),
+            vowel_probs: [0.2; 5],
+            dominant_vowel: VowelClass::A,
         }
     }
 
     fn reset(&mut self) {
         self.detect_hp = [BiquadState::default(); 3];
         self.detect_lp = [BiquadState::default(); 3];
         self.detect_peak = BiquadState::default();
         self.split_lp = [BiquadState::default(); 3];
         self.bell = [BiquadState::default(); 2];
         self.detect_env.reset();
         self.full_env.reset();
         self.reduction.reset();
         self.audio_delay.reset();
+        self.prev_input_1 = 0.0;
+        self.prev_input_2 = 0.0;
+        self.tkeo_env_short.reset();
+        self.tkeo_env_mid.reset();
+        self.tkeo_env_long.reset();
+        self.subspace_tracker = AdaptiveSubspaceTracker::new();
+        self.formant_filters = [BiquadState::default(); 3];
+        self.formant_trackers = default_formant_trackers();
+        self.vowel_probs = [0.2; 5];
+        self.dominant_vowel = VowelClass::A;
     }
 }
 
 pub struct DeEsserDsp {
     sample_rate: f64,
     detect_hp: [BiquadCoeffs; 3],
     detect_lp: [BiquadCoeffs; 3],
     detect_peak: BiquadCoeffs,
     split_lp: [BiquadCoeffs; 3],
     bell: [BiquadCoeffs; 2],
+    formant_coeffs: [BiquadCoeffs; 3],
+    detection_center_hz: f64,
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
             detect_peak: BiquadCoeffs::default(),
             split_lp: [BiquadCoeffs::default(); 3],
             bell: [BiquadCoeffs::default(); 2],
+            formant_coeffs: [BiquadCoeffs::default(); 3],
+            detection_center_hz: 6_900.0,
             full_cut_depth_db: 0.0,
             channels: [
                 ChannelState::new(sample_rate),
                 ChannelState::new(sample_rate),
             ],
         };
 
         dsp.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);
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
         self.channels[0].audio_delay.latency_samples() as u32
     }
 
     pub fn update_lookahead(&mut self, delay_ms: f64) {
         for channel in &mut self.channels {
             channel.audio_delay.set_delay(delay_ms, self.sample_rate);
         }
     }
 
     pub fn update_vocal_mode(&mut self, single_vocal: bool) {
         let (attack_ms, hold_ms, release_ms) = if single_vocal {
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
             channel
                 .reduction
                 .set_times(attack_ms, hold_ms, release_ms, self.sample_rate);
+            channel
+                .tkeo_env_short
+                .set_times(attack_ms * 0.8, release_ms * 0.12, self.sample_rate);
+            channel
+                .tkeo_env_mid
+                .set_times(attack_ms * 2.5, release_ms * 0.35, self.sample_rate);
+            channel
+                .tkeo_env_long
+                .set_times(attack_ms * 8.0, release_ms, self.sample_rate);
         }
     }
 
     pub fn update_filters(
         &mut self,
         min_freq_hz: f64,
         max_freq_hz: f64,
         _use_peak_filter: bool,
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
         let detection_q = (center_freq / (max_freq - min_freq).max(1.0)).clamp(0.35, 8.0);
         let bell_q = (0.4 + cut_width.clamp(0.0, 1.0) * 11.6).clamp(0.4, 12.0);
         let slope = (cut_slope / 100.0).clamp(0.0, 1.0);
 
         self.detect_hp = Self::make_hp(min_freq, self.sample_rate);
         self.detect_lp = Self::make_lp(max_freq, self.sample_rate);
         self.detect_peak = BiquadCoeffs::bandpass_peak(center_freq, detection_q, self.sample_rate);
         self.split_lp = Self::make_lp(center_freq, self.sample_rate);
+        self.detection_center_hz = center_freq;
 
         self.full_cut_depth_db = max_reduction_db.abs() * cut_depth.clamp(0.0, 1.0);
         let stage_1_depth = -(self.full_cut_depth_db * (0.65 + 0.2 * (1.0 - slope)));
         let stage_2_depth = -(self.full_cut_depth_db - stage_1_depth.abs());
         let bell_q_2 = (bell_q * (1.0 + slope * 1.75)).clamp(0.4, 16.0);
 
         self.bell = [
             BiquadCoeffs::bell(center_freq, bell_q, stage_1_depth, self.sample_rate),
             BiquadCoeffs::bell(center_freq, bell_q_2, stage_2_depth, self.sample_rate),
         ];
+        self.formant_coeffs = [
+            BiquadCoeffs::bandpass_peak(730.0, 2.4, self.sample_rate),
+            BiquadCoeffs::bandpass_peak(1090.0, 2.8, self.sample_rate),
+            BiquadCoeffs::bandpass_peak(2440.0, 3.2, self.sample_rate),
+        ];
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
         let stereo_link = settings.stereo_link.clamp(0.0, 1.0);
         let max_reduction_db = settings.max_reduction_db.abs().max(1.0e-6);
 
         let (audio_l, audio_r) = if settings.stereo_mid_side {
             lr_to_ms(input_l, input_r)
         } else {
             (input_l, input_r)
         };
         let (sc_l, sc_r) = if settings.stereo_mid_side {
             lr_to_ms(sidechain_l, sidechain_r)
         } else {
             (sidechain_l, sidechain_r)
         };
 
         let delayed_l = self.channels[0].audio_delay.process(audio_l);
         let delayed_r = self.channels[1].audio_delay.process(audio_r);
 
         let detected_l = self.detect_signal(sc_l, 0, settings.use_peak_filter);
         let detected_r = self.detect_signal(sc_r, 1, settings.use_peak_filter);
 
         let detected_env_l = self.channels[0].detect_env.process(detected_l);
         let detected_env_r = self.channels[1].detect_env.process(detected_r);
         let full_env_l = self.channels[0].full_env.process(sc_l);
         let full_env_r = self.channels[1].full_env.process(sc_r);
 
+        let subspace_l = self.subspace_metrics(sc_l, 0);
+        let subspace_r = self.subspace_metrics(sc_r, 1);
+        let psycho_l = self.psychoacoustic_weight(sc_l, detected_l, 0);
+        let psycho_r = self.psychoacoustic_weight(sc_r, detected_r, 1);
+        let formant_lock_l = self.formant_preservation_lock(sc_l, detected_l, 0);
+        let formant_lock_r = self.formant_preservation_lock(sc_r, detected_r, 1);
+
         let linked_detect = (detected_env_l + detected_env_r) * 0.5;
         let linked_full = (full_env_l + full_env_r) * 0.5;
         let detect_env_l = detected_env_l * (1.0 - stereo_link) + linked_detect * stereo_link;
         let detect_env_r = detected_env_r * (1.0 - stereo_link) + linked_detect * stereo_link;
         let full_env_l = full_env_l * (1.0 - stereo_link) + linked_full * stereo_link;
         let full_env_r = full_env_r * (1.0 - stereo_link) + linked_full * stereo_link;
 
         let comparison_l = if settings.mode_relative {
             lin_to_db(detect_env_l) - lin_to_db(full_env_l)
         } else {
             lin_to_db(detect_env_l)
         };
         let comparison_r = if settings.mode_relative {
             lin_to_db(detect_env_r) - lin_to_db(full_env_r)
         } else {
             lin_to_db(detect_env_r)
         };
 
-        let reduction_target_l =
-            reduction_amount(comparison_l, settings.threshold_db, max_reduction_db);
-        let reduction_target_r =
-            reduction_amount(comparison_r, settings.threshold_db, max_reduction_db);
+        let base_target_l = reduction_amount(comparison_l, settings.threshold_db, max_reduction_db);
+        let base_target_r = reduction_amount(comparison_r, settings.threshold_db, max_reduction_db);
+
+        // Keep user controls responsive by treating the transparency stack as a shaping layer
+        // around the base control law instead of a hard attenuation cascade.
+        let transparency_l = transparency_shaping(subspace_l, psycho_l, formant_lock_l);
+        let transparency_r = transparency_shaping(subspace_r, psycho_r, formant_lock_r);
+        let reduction_target_l = (base_target_l * transparency_l)
+            .max(base_target_l * 0.35)
+            .clamp(0.0, 1.0);
+        let reduction_target_r = (base_target_r * transparency_r)
+            .max(base_target_r * 0.35)
+            .clamp(0.0, 1.0);
 
         let amount_l = self.channels[0].reduction.process(reduction_target_l);
         let amount_r = self.channels[1].reduction.process(reduction_target_r);
         let reduction_db_l = -(self.full_cut_depth_db * amount_l);
         let reduction_db_r = -(self.full_cut_depth_db * amount_r);
 
         let wet_l = if settings.trigger_hear {
             detected_l
         } else if settings.filter_solo {
             detected_l * db_to_lin(reduction_db_l)
         } else if settings.use_wide_range {
             self.apply_bell(delayed_l, amount_l, 0)
         } else {
             let (low, high) = self.split_complement(delayed_l, 0);
             low + high * db_to_lin(reduction_db_l)
         };
 
         let wet_r = if settings.trigger_hear {
             detected_r
         } else if settings.filter_solo {
             detected_r * db_to_lin(reduction_db_r)
         } else if settings.use_wide_range {
             self.apply_bell(delayed_r, amount_r, 1)
         } else {
             let (low, high) = self.split_complement(delayed_r, 1);
@@ -560,50 +742,130 @@ impl DeEsserDsp {
     }
 
     fn make_lp(freq_hz: f64, sample_rate: f64) -> [BiquadCoeffs; 3] {
         Self::BUTTERWORTH_QS.map(|q| BiquadCoeffs::lowpass(freq_hz, q, sample_rate))
     }
 
     #[inline]
     fn detect_signal(&mut self, input: f64, channel_idx: usize, use_peak_filter: bool) -> f64 {
         if use_peak_filter {
             return self
                 .detect_peak
                 .process(&mut self.channels[channel_idx].detect_peak, input);
         }
 
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
 
+    #[inline]
+    fn subspace_metrics(&mut self, input: f64, channel_idx: usize) -> f64 {
+        let channel = &mut self.channels[channel_idx];
+        let tkeo = teager_kaiser_energy(input, channel.prev_input_1, channel.prev_input_2);
+        channel.prev_input_2 = channel.prev_input_1;
+        channel.prev_input_1 = input;
+
+        let f_short = channel.tkeo_env_short.process(tkeo);
+        let f_mid = channel.tkeo_env_mid.process(tkeo);
+        let f_long = channel.tkeo_env_long.process(tkeo);
+        let features = [f_short, f_mid, f_long];
+        channel.subspace_tracker.update(features);
+        let orth_ratio = channel.subspace_tracker.orthogonal_ratio(features);
+
+        (0.3 + 0.9 * orth_ratio).clamp(0.25, 1.0)
+    }
+
+    #[inline]
+    fn psychoacoustic_weight(&self, fullband: f64, detected: f64, channel_idx: usize) -> f64 {
+        let channel = &self.channels[channel_idx];
+        let harmonicity = (channel.tkeo_env_long.envelope
+            / (channel.tkeo_env_short.envelope + 1.0e-9))
+            .clamp(0.0, 3.0);
+        let voicedness = (fullband.abs() / (detected.abs() + 1.0e-9)).clamp(0.0, 8.0);
+        let harmonic_mask = (harmonicity * 0.25 + voicedness * 0.08).clamp(0.0, 1.0);
+
+        (1.0 - 0.48 * harmonic_mask).clamp(0.52, 1.0)
+    }
+
+    #[inline]
+    fn formant_preservation_lock(&mut self, input: f64, detected: f64, channel_idx: usize) -> f64 {
+        let channel = &mut self.channels[channel_idx];
+        let mut energies = [0.0; 3];
+        for (index, (coeffs, state)) in self
+            .formant_coeffs
+            .iter()
+            .zip(channel.formant_filters.iter_mut())
+            .enumerate()
+        {
+            let sample = coeffs.process(state, input);
+            energies[index] = sample * sample;
+        }
+
+        let sum_energy = energies.iter().sum::<f64>() + 1.0e-12;
+        let norm = energies.map(|v| v / sum_energy);
+        let measured_f1 = 450.0 + 600.0 * norm[0];
+        let measured_f2 = 800.0 + 1800.0 * norm[1];
+        let measured_f3 = 2000.0 + 1700.0 * norm[2];
+
+        let f1 = channel.formant_trackers[0].update(measured_f1);
+        let f2 = channel.formant_trackers[1].update(measured_f2);
+        let f3 = channel.formant_trackers[2].update(measured_f3);
+        let vowel = classify_vowel(f1, f2);
+        channel.dominant_vowel = vowel;
+        let target = vowel_targets(vowel);
+
+        let formant_distance = (((f1 - target[0]).abs() / 500.0)
+            + ((f2 - target[1]).abs() / 1300.0)
+            + ((f3 - target[2]).abs() / 1800.0))
+            / 3.0;
+        let lock_strength = (1.0 - formant_distance).clamp(0.0, 1.0);
+
+        for (idx, cls) in VowelClass::all().iter().enumerate() {
+            let goal = if cls.idx() == vowel.idx() { 1.0 } else { 0.0 };
+            channel.vowel_probs[idx] = channel.vowel_probs[idx] * 0.98 + goal * 0.02;
+        }
+
+        let vowel_confidence = channel.vowel_probs[vowel.idx()].clamp(0.0, 1.0);
+        let protected_formant = target
+            .iter()
+            .map(|f| (self.detection_center_hz - *f).abs())
+            .fold(f64::MAX, f64::min);
+        let band_overlap = (1.0 - (protected_formant / 4500.0)).clamp(0.0, 1.0);
+        let sibilant_bias = (detected.abs() / (input.abs() + 1.0e-9)).clamp(0.0, 1.0);
+        let effective_lock =
+            lock_strength * vowel_confidence * band_overlap * (1.0 - 0.35 * sibilant_bias);
+
+        (1.0 - 0.55 * effective_lock).clamp(0.45, 1.0)
+    }
+
     #[inline]
     fn split_complement(&mut self, input: f64, channel_idx: usize) -> (f64, f64) {
         let channel = &mut self.channels[channel_idx];
         let mut low = input;
         for (coeffs, state) in self.split_lp.iter().zip(channel.split_lp.iter_mut()) {
             low = coeffs.process(state, low);
         }
 
         (low, input - low)
     }
 
     #[inline]
     fn apply_bell(&mut self, input: f64, amount: f64, channel_idx: usize) -> f64 {
         let channel = &mut self.channels[channel_idx];
         let stage_1 = self.bell[0].process(&mut channel.bell[0], input);
         let stage_2 = self.bell[1].process(&mut channel.bell[1], stage_1);
         let amount = amount.clamp(0.0, 1.0);
         input * (1.0 - amount) + stage_2 * amount
     }
 }
 
 #[inline]
 fn lr_to_ms(left: f64, right: f64) -> (f64, f64) {
     (
         (left + right) * FRAC_1_SQRT_2,
@@ -617,66 +879,148 @@ fn ms_to_lr(mid: f64, side: f64) -> (f64, f64) {
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
 
+#[inline]
+fn transparency_shaping(subspace: f64, psycho: f64, formant_lock: f64) -> f64 {
+    let subspace_weight = 0.70 + 0.30 * subspace.clamp(0.0, 1.0);
+    let psycho_weight = 0.75 + 0.25 * psycho.clamp(0.0, 1.0);
+    let formant_weight = 0.72 + 0.28 * formant_lock.clamp(0.0, 1.0);
+    (subspace_weight * psycho_weight * formant_weight).clamp(0.45, 1.0)
+}
+
+#[inline]
+fn teager_kaiser_energy(current: f64, prev_1: f64, prev_2: f64) -> f64 {
+    (prev_1 * prev_1 - current * prev_2).abs()
+}
+
+#[inline]
+fn classify_vowel(f1: f64, f2: f64) -> VowelClass {
+    let references = [
+        (VowelClass::A, [730.0, 1090.0]),
+        (VowelClass::E, [530.0, 1840.0]),
+        (VowelClass::I, [270.0, 2290.0]),
+        (VowelClass::O, [570.0, 840.0]),
+        (VowelClass::U, [300.0, 870.0]),
+    ];
+
+    let mut best = VowelClass::A;
+    let mut best_distance = f64::MAX;
+    for (vowel, [ref_f1, ref_f2]) in references {
+        let d1 = (f1 - ref_f1) / 700.0;
+        let d2 = (f2 - ref_f2) / 2200.0;
+        let distance = d1 * d1 + d2 * d2;
+        if distance < best_distance {
+            best_distance = distance;
+            best = vowel;
+        }
+    }
+
+    best
+}
+
+#[inline]
+fn vowel_targets(vowel: VowelClass) -> [f64; 3] {
+    match vowel {
+        VowelClass::A => [730.0, 1090.0, 2440.0],
+        VowelClass::E => [530.0, 1840.0, 2480.0],
+        VowelClass::I => [270.0, 2290.0, 3010.0],
+        VowelClass::O => [570.0, 840.0, 2410.0],
+        VowelClass::U => [300.0, 870.0, 2240.0],
+    }
+}
+
 #[cfg(test)]
 mod tests {
     use super::*;
 
     #[test]
     fn complementary_split_recombines_exactly() {
         let mut dsp = DeEsserDsp::new(48_000.0);
         dsp.update_filters(4_000.0, 10_000.0, false, 0.5, 1.0, 50.0, 12.0);
 
         for sample in [0.0, 0.1, -0.25, 0.75, -0.5, 0.33] {
             let (low, high) = dsp.split_complement(sample, 0);
             assert!(((low + high) - sample).abs() < 1.0e-12);
         }
     }
 
     #[test]
     fn lookahead_latency_matches_requested_delay() {
         let mut dsp = DeEsserDsp::new(48_000.0);
         dsp.update_lookahead(5.0);
         assert_eq!(dsp.latency_samples(), 240);
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
                 threshold_db: -20.0,
                 max_reduction_db: 12.0,
                 mode_relative: false,
                 ..ProcessSettings::default()
             },
         );
 
         assert!(frame.reduction_db > -0.1);
     }
+
+    #[test]
+    fn threshold_control_changes_reduction_amount() {
+        let mut dsp = DeEsserDsp::new(48_000.0);
+        let settings_loose = ProcessSettings {
+            threshold_db: -35.0,
+            max_reduction_db: 12.0,
+            mode_relative: false,
+            ..ProcessSettings::default()
+        };
+        let settings_strict = ProcessSettings {
+            threshold_db: -6.0,
+            max_reduction_db: 12.0,
+            mode_relative: false,
+            ..ProcessSettings::default()
+        };
+
+        let mut loose_reduction = 0.0;
+        let mut strict_reduction = 0.0;
+        for _ in 0..256 {
+            loose_reduction = dsp
+                .process_frame(0.85, 0.85, 0.85, 0.85, settings_loose)
+                .reduction_db;
+        }
+        dsp.reset();
+        for _ in 0..256 {
+            strict_reduction = dsp
+                .process_frame(0.85, 0.85, 0.85, 0.85, settings_strict)
+                .reduction_db;
+        }
+
+        assert!(loose_reduction < strict_reduction - 0.5);
+    }
 }
