// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser — DSP Engine
// Hyper-optimized 64-bit de-essing signal processing
// ─────────────────────────────────────────────────────────────────────────────

use std::f64::consts::PI;

/// Biquad filter coefficients stored as f64 for maximum precision
#[derive(Clone, Copy, Debug)]
pub struct BiquadCoeffs {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

/// Biquad filter state (two channels)
#[derive(Clone, Copy, Debug, Default)]
pub struct BiquadState {
    pub x1: f64,
    pub x2: f64,
    pub y1: f64,
    pub y2: f64,
}

impl BiquadCoeffs {
    /// Lowpass shelving filter (second-order)
    #[inline(always)]
    pub fn lowpass_shelf(freq: f64, gain_db: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let a_lin = 10.0_f64.powf(gain_db / 40.0);
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / 2.0 * (a_lin + 1.0 / a_lin).sqrt();

        let b0 = a_lin * ((a_lin + 1.0) - (a_lin - 1.0) * cos_w0 + 2.0 * a_lin.sqrt() * alpha);
        let b1 = 2.0 * a_lin * ((a_lin - 1.0) - (a_lin + 1.0) * cos_w0);
        let b2 = a_lin * ((a_lin + 1.0) - (a_lin - 1.0) * cos_w0 - 2.0 * a_lin.sqrt() * alpha);
        let a0 = (a_lin + 1.0) + (a_lin - 1.0) * cos_w0 + 2.0 * a_lin.sqrt() * alpha;
        let a1 = -2.0 * ((a_lin - 1.0) + (a_lin + 1.0) * cos_w0);
        let a2 = (a_lin + 1.0) + (a_lin - 1.0) * cos_w0 - 2.0 * a_lin.sqrt() * alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Bandpass / peak EQ filter for detection
    #[inline(always)]
    pub fn bandpass_peak(freq: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = sin_w0 / 2.0;
        let b1 = 0.0;
        let b2 = -sin_w0 / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Highpass filter for detection chain
    #[inline(always)]
    pub fn highpass(freq: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 + cos_w0) / 2.0;
        let b1 = -(1.0 + cos_w0);
        let b2 = (1.0 + cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Lowpass filter for sidechan smoothing
    #[inline(always)]
    pub fn lowpass(freq: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * freq / sample_rate;
        let cos_w0 = w0.cos();
        let sin_w0 = w0.sin();
        let alpha = sin_w0 / (2.0 * q);

        let b0 = (1.0 - cos_w0) / 2.0;
        let b1 = 1.0 - cos_w0;
        let b2 = (1.0 - cos_w0) / 2.0;
        let a0 = 1.0 + alpha;
        let a1 = -2.0 * cos_w0;
        let a2 = 1.0 - alpha;

        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// Process one sample through biquad
    #[inline(always)]
    pub fn process(&self, state: &mut BiquadState, x: f64) -> f64 {
        let y = self.b0 * x + self.b1 * state.x1 + self.b2 * state.x2
            - self.a1 * state.y1
            - self.a2 * state.y2;
        state.x2 = state.x1;
        state.x1 = x;
        state.y2 = state.y1;
        state.y1 = y;
        y
    }
}

/// Ballistic envelope follower with attack/release
#[derive(Clone, Debug)]
pub struct EnvelopeFollower {
    pub attack_coeff: f64,
    pub release_coeff: f64,
    pub envelope: f64,
}

impl EnvelopeFollower {
    pub fn new(attack_ms: f64, release_ms: f64, sample_rate: f64) -> Self {
        let attack_coeff = if attack_ms <= 0.0 {
            0.0
        } else {
            (-1.0 / (attack_ms * 0.001 * sample_rate)).exp()
        };
        let release_coeff = if release_ms <= 0.0 {
            0.0
        } else {
            (-1.0 / (release_ms * 0.001 * sample_rate)).exp()
        };
        Self {
            attack_coeff,
            release_coeff,
            envelope: 0.0,
        }
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        let abs_in = input.abs();
        if abs_in > self.envelope {
            self.envelope = self.attack_coeff * (self.envelope - abs_in) + abs_in;
        } else {
            self.envelope = self.release_coeff * (self.envelope - abs_in) + abs_in;
        }
        self.envelope
    }

    pub fn reset(&mut self) {
        self.envelope = 0.0;
    }
}

/// Lookahead delay line — ring buffer approach for zero-overhead lookahead
pub struct LookaheadDelay {
    buffer: Vec<f64>,
    write_pos: usize,
    delay_samples: usize,
}

impl LookaheadDelay {
    pub fn new(max_delay_ms: f64, sample_rate: f64) -> Self {
        let max_samples = (max_delay_ms * 0.001 * sample_rate).ceil() as usize + 1;
        Self {
            buffer: vec![0.0; max_samples.max(1)],
            write_pos: 0,
            delay_samples: 0,
        }
    }

    pub fn set_delay(&mut self, delay_ms: f64, sample_rate: f64) {
        let samples = (delay_ms * 0.001 * sample_rate).round() as usize;
        self.delay_samples = samples.min(self.buffer.len().saturating_sub(1));
    }

    #[inline(always)]
    pub fn process(&mut self, input: f64) -> f64 {
        self.buffer[self.write_pos] = input;
        let read_pos = if self.write_pos >= self.delay_samples {
            self.write_pos - self.delay_samples
        } else {
            self.buffer.len() - self.delay_samples + self.write_pos
        };
        self.write_pos = (self.write_pos + 1) % self.buffer.len();
        self.buffer[read_pos]
    }

    pub fn reset(&mut self) {
        self.buffer.fill(0.0);
        self.write_pos = 0;
    }
}

/// Gain computer — converts envelope to gain reduction
#[inline(always)]
pub fn compute_gain_reduction(
    detection_level_db: f64,
    threshold_db: f64,
    max_reduction_db: f64,
    knee_db: f64,
) -> f64 {
    let over = detection_level_db - threshold_db;
    if over <= -knee_db * 0.5 {
        0.0
    } else if over <= knee_db * 0.5 {
        // Soft knee region
        let knee_factor = (over + knee_db * 0.5) / knee_db;
        -knee_factor * knee_factor * max_reduction_db.abs()
    } else {
        // Hard limiting zone
        -max_reduction_db.abs()
    }
}

/// Safe dB conversion
#[inline(always)]
pub fn lin_to_db(lin: f64) -> f64 {
    if lin <= 1e-10 {
        -200.0
    } else {
        20.0 * lin.log10()
    }
}

#[inline(always)]
pub fn db_to_lin(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

/// Smoothed gain coefficient (single-pole IIR)
pub struct GainSmoother {
    pub coeff: f64,
    pub current: f64,
}

impl GainSmoother {
    pub fn new(time_ms: f64, sample_rate: f64) -> Self {
        let coeff = if time_ms <= 0.0 {
            0.0
        } else {
            (-1.0 / (time_ms * 0.001 * sample_rate)).exp()
        };
        Self { coeff, current: 1.0 }
    }

    #[inline(always)]
    pub fn process(&mut self, target: f64) -> f64 {
        self.current = self.coeff * (self.current - target) + target;
        self.current
    }
}

/// Per-channel DSP state
pub struct ChannelDsp {
    // Detection filters
    pub detect_hp: BiquadState,
    pub detect_lp: BiquadState,
    pub detect_peak: BiquadState,
    // Envelope followers
    pub detect_env: EnvelopeFollower,
    pub full_env: EnvelopeFollower,
    // Gain smoothing
    pub gain_smoother: GainSmoother,
    // Lookahead delays (audio path)
    pub lookahead_audio: LookaheadDelay,
    pub lookahead_sidechain: LookaheadDelay,
}

impl ChannelDsp {
    pub fn new(sample_rate: f64) -> Self {
        Self {
            detect_hp: BiquadState::default(),
            detect_lp: BiquadState::default(),
            detect_peak: BiquadState::default(),
            detect_env: EnvelopeFollower::new(0.1, 50.0, sample_rate),
            full_env: EnvelopeFollower::new(0.1, 50.0, sample_rate),
            gain_smoother: GainSmoother::new(1.0, sample_rate),
            lookahead_audio: LookaheadDelay::new(20.0, sample_rate),
            lookahead_sidechain: LookaheadDelay::new(20.0, sample_rate),
        }
    }

    pub fn reset(&mut self) {
        self.detect_hp = BiquadState::default();
        self.detect_lp = BiquadState::default();
        self.detect_peak = BiquadState::default();
        self.detect_env.reset();
        self.full_env.reset();
        self.gain_smoother.current = 1.0;
        self.lookahead_audio.reset();
        self.lookahead_sidechain.reset();
    }
}

/// Main DSP processor — 2-channel (stereo)
pub struct DeEsserDsp {
    pub channels: [ChannelDsp; 2],
    pub sample_rate: f64,
    // Shared filter coefficients (rebuilt on param change)
    pub hp_coeffs: BiquadCoeffs,
    pub lp_coeffs: BiquadCoeffs,
    pub peak_coeffs: BiquadCoeffs,
    // Gain smoothing coefficients
    pub attack_coeff: f64,
    pub release_coeff: f64,
}

impl DeEsserDsp {
    pub fn new(sample_rate: f64) -> Self {
        let dummy_hp = BiquadCoeffs::highpass(6000.0, 0.707, sample_rate);
        let dummy_lp = BiquadCoeffs::lowpass(12000.0, 0.707, sample_rate);
        let dummy_peak = BiquadCoeffs::bandpass_peak(8000.0, 2.0, sample_rate);
        Self {
            channels: [ChannelDsp::new(sample_rate), ChannelDsp::new(sample_rate)],
            sample_rate,
            hp_coeffs: dummy_hp,
            lp_coeffs: dummy_lp,
            peak_coeffs: dummy_peak,
            attack_coeff: (-1.0_f64 / (0.1_f64 * 0.001 * sample_rate)).exp(),
            release_coeff: (-1.0_f64 / (50.0_f64 * 0.001 * sample_rate)).exp(),
        }
    }

    pub fn reset(&mut self) {
        for ch in &mut self.channels {
            ch.reset();
        }
    }

    pub fn update_filters(&mut self, min_freq: f64, max_freq: f64, use_peak: bool) {
        let sr = self.sample_rate;
        let min_f = min_freq.clamp(20.0, sr * 0.49);
        let max_f = max_freq.clamp(min_f + 10.0, sr * 0.49);
        let center = (min_f * max_f).sqrt();
        let bw = max_f - min_f;
        let q = (center / bw).max(0.1);
        self.hp_coeffs = BiquadCoeffs::highpass(min_f, 0.707, sr);
        self.lp_coeffs = BiquadCoeffs::lowpass(max_f, 0.707, sr);
        self.peak_coeffs = BiquadCoeffs::bandpass_peak(center, q, sr);
        let _ = use_peak;
    }

    pub fn update_lookahead(&mut self, lookahead_ms: f64) {
        let sr = self.sample_rate;
        for ch in &mut self.channels {
            ch.lookahead_audio.set_delay(lookahead_ms, sr);
            ch.lookahead_sidechain.set_delay(lookahead_ms, sr);
        }
    }

    pub fn update_envelope(&mut self, attack_ms: f64, release_ms: f64) {
        let sr = self.sample_rate;
        for ch in &mut self.channels {
            ch.detect_env = EnvelopeFollower::new(attack_ms, release_ms, sr);
            ch.full_env = EnvelopeFollower::new(attack_ms, release_ms, sr);
        }
    }

    /// Process a single stereo sample pair
    /// Returns (left_out, right_out, detection_level_db, reduction_db)
    #[inline(always)]
    pub fn process_sample(
        &mut self,
        left_in: f64,
        right_in: f64,
        ext_left: Option<f64>,
        ext_right: Option<f64>,
        threshold_db: f64,
        max_reduction_db: f64,
        mode_relative: bool,
        use_peak_filter: bool,
        use_wide_range: bool,
        stereo_link: f64,      // 0.0 = full independent, 1.0 = full linked
        stereo_mid_side: bool, // true = mid/side stereo linking
        lookahead_enabled: bool,
        trigger_hear: bool,
        filter_solo: bool,
    ) -> (f64, f64, f64, f64) {
        // ── Mid/Side encode if needed ─────────────────────────────────────────
        let (mut l, mut r) = if stereo_mid_side {
            (
                (left_in + right_in) * std::f64::consts::FRAC_1_SQRT_2,
                (left_in - right_in) * std::f64::consts::FRAC_1_SQRT_2,
            )
        } else {
            (left_in, right_in)
        };

        // ── Sidechain source ─────────────────────────────────────────────────
        let sc_l = ext_left.unwrap_or(l);
        let sc_r = ext_right.unwrap_or(r);

        // ── Detection filtering ──────────────────────────────────────────────
        let mut det_l = self.apply_detection_filter(sc_l, 0, use_peak_filter, use_wide_range);
        let mut det_r = self.apply_detection_filter(sc_r, 1, use_peak_filter, use_wide_range);

        // ── Lookahead on audio path ──────────────────────────────────────────
        let (audio_l, audio_r) = if lookahead_enabled {
            (
                self.channels[0].lookahead_audio.process(l),
                self.channels[1].lookahead_audio.process(r),
            )
        } else {
            (l, r)
        };

        // ── Envelope detection ───────────────────────────────────────────────
        let env_det_l = self.channels[0].detect_env.process(det_l);
        let env_det_r = self.channels[1].detect_env.process(det_r);
        let env_full_l = self.channels[0].full_env.process(l.abs());
        let env_full_r = self.channels[1].full_env.process(r.abs());

        // ── Stereo link ──────────────────────────────────────────────────────
        let env_det_linked_l =
            env_det_l * (1.0 - stereo_link) + (env_det_l + env_det_r) * 0.5 * stereo_link;
        let env_det_linked_r =
            env_det_r * (1.0 - stereo_link) + (env_det_l + env_det_r) * 0.5 * stereo_link;

        // ── Gain computation ─────────────────────────────────────────────────
        let knee = 2.0;
        let (gain_l, det_db_l) = self.compute_channel_gain(
            env_det_linked_l,
            env_full_l,
            threshold_db,
            max_reduction_db,
            mode_relative,
            knee,
            0,
        );
        let (gain_r, det_db_r) = self.compute_channel_gain(
            env_det_linked_r,
            env_full_r,
            threshold_db,
            max_reduction_db,
            mode_relative,
            knee,
            1,
        );

        // ── Output routing ───────────────────────────────────────────────────
        let (out_l, out_r) = if trigger_hear {
            (det_l, det_r)
        } else if filter_solo {
            (det_l * gain_l, det_r * gain_r)
        } else {
            (audio_l * gain_l, audio_r * gain_r)
        };

        // ── Mid/Side decode ──────────────────────────────────────────────────
        let (final_l, final_r) = if stereo_mid_side {
            (
                (out_l + out_r) * std::f64::consts::FRAC_1_SQRT_2,
                (out_l - out_r) * std::f64::consts::FRAC_1_SQRT_2,
            )
        } else {
            (out_l, out_r)
        };

        // Average detection for meter display
        let avg_det_db = (det_db_l + det_db_r) * 0.5;
        let avg_reduction_db = lin_to_db((gain_l + gain_r) * 0.5);

        (final_l, final_r, avg_det_db, avg_reduction_db)
    }

    #[inline(always)]
    fn apply_detection_filter(
        &mut self,
        sample: f64,
        ch: usize,
        use_peak: bool,
        use_wide: bool,
    ) -> f64 {
        if use_wide {
            if use_peak {
                self.peak_coeffs.process(&mut self.channels[ch].detect_peak, sample)
            } else {
                let hp = self.hp_coeffs.process(&mut self.channels[ch].detect_hp, sample);
                self.lp_coeffs.process(&mut self.channels[ch].detect_lp, hp)
            }
        } else {
            // Split mode: bandpass between min and max freq
            if use_peak {
                self.peak_coeffs.process(&mut self.channels[ch].detect_peak, sample)
            } else {
                let hp = self.hp_coeffs.process(&mut self.channels[ch].detect_hp, sample);
                self.lp_coeffs.process(&mut self.channels[ch].detect_lp, hp)
            }
        }
    }

    #[inline(always)]
    fn compute_channel_gain(
        &mut self,
        env_det: f64,
        env_full: f64,
        threshold_db: f64,
        max_reduction_db: f64,
        mode_relative: bool,
        knee: f64,
        ch: usize,
    ) -> (f64, f64) {
        let det_level_db = lin_to_db(env_det);
        let full_level_db = lin_to_db(env_full);

        let effective_det_db = if mode_relative {
            // Relative: compare filtered signal vs full bandwidth
            let threshold_adj = threshold_db + full_level_db;
            det_level_db - threshold_adj
        } else {
            det_level_db - threshold_db
        };

        let gr_db = compute_gain_reduction(
            if mode_relative { effective_det_db + threshold_db } else { det_level_db },
            threshold_db,
            max_reduction_db,
            knee,
        );

        let target_gain = db_to_lin(gr_db);
        let smoothed_gain = self.channels[ch].gain_smoother.process(target_gain);

        (smoothed_gain, det_level_db)
    }
}
