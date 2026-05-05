use std::f64::consts::PI;

use rustfft::{num_complex::Complex, FftPlanner};

use crate::dsp::{lin_to_db, DeEsserDsp, ProcessSettings};

#[derive(Clone, Copy, Debug)]
pub struct ObjectiveAudioMetrics {
    pub target_band_attenuation_db: f64,
    pub low_band_leakage_db: f64,
    pub residual_target_focus_ratio: f64,
    pub bypass_transparency_snr_db: f64,
    pub reported_latency_samples: u32,
    pub measured_latency_samples: usize,
    pub peak_output_dbfs: f64,
    pub all_samples_finite: bool,
}

pub fn measure_deesser_objective_metrics(sample_rate: f64) -> ObjectiveAudioMetrics {
    let settings = ProcessSettings {
        threshold_db: 24.0,
        max_reduction_db: 12.0,
        mode_relative: false,
        ..ProcessSettings::default()
    };
    let (dry, wet, all_samples_finite, peak_output_dbfs, latency) =
        render_sibilant_fixture(sample_rate, settings, 1.0);

    let target_band_attenuation_db = band_power_db(&dry, sample_rate, 5_000.0, 10_000.0)
        - band_power_db(&wet, sample_rate, 5_000.0, 10_000.0);
    let low_band_leakage_db = (band_power_db(&dry, sample_rate, 120.0, 2_500.0)
        - band_power_db(&wet, sample_rate, 120.0, 2_500.0))
    .abs();

    let residual = dry
        .iter()
        .zip(wet.iter())
        .map(|(dry, wet)| dry - wet)
        .collect::<Vec<_>>();
    let residual_target_focus_ratio = band_energy(&residual, sample_rate, 5_000.0, 10_000.0)
        / band_energy(&residual, sample_rate, 0.0, sample_rate * 0.5).max(1.0e-18);

    let (_, transparent_wet, transparent_finite, _, _) =
        render_sibilant_fixture(sample_rate, settings, 0.0);
    let bypass_transparency_snr_db = snr_db(&dry, &transparent_wet);

    ObjectiveAudioMetrics {
        target_band_attenuation_db,
        low_band_leakage_db,
        residual_target_focus_ratio,
        bypass_transparency_snr_db,
        reported_latency_samples: latency,
        measured_latency_samples: measure_latency_samples(sample_rate),
        peak_output_dbfs,
        all_samples_finite: all_samples_finite && transparent_finite,
    }
}

fn render_sibilant_fixture(
    sample_rate: f64,
    settings: ProcessSettings,
    cut_depth: f64,
) -> (Vec<f64>, Vec<f64>, bool, f64, u32) {
    let mut dsp = DeEsserDsp::new(sample_rate);
    dsp.update_filters(4_000.0, 12_000.0, 0.55, cut_depth, 60.0, 12.0);
    dsp.update_vocal_mode(true);

    let latency = dsp.latency_samples();
    let frames = 24_576;
    let mut dry = Vec::with_capacity(frames);
    let mut wet = Vec::with_capacity(frames);
    let mut finite = true;
    let mut peak = 0.0_f64;

    for sample_idx in 0..frames {
        let t = sample_idx as f64 / sample_rate;
        let voiced = 0.38 * (2.0 * PI * 180.0 * t).sin()
            + 0.18 * (2.0 * PI * 720.0 * t).sin()
            + 0.10 * (2.0 * PI * 1_350.0 * t).sin();
        let burst_gate = if (sample_idx / 80) % 4 == 0 { 1.0 } else { 0.0 };
        let deterministic_noise = hash_noise(sample_idx);
        let high_noise = 0.5 * deterministic_noise
            - 0.35 * hash_noise(sample_idx.wrapping_sub(1))
            - 0.15 * hash_noise(sample_idx.wrapping_sub(2));
        let sibilant = burst_gate * 0.42 * high_noise;
        let input = voiced + sibilant;
        let frame = dsp.process_frame(input, input, input, input, settings);
        let output = frame.wet_l;

        finite &= frame.wet_l.is_finite()
            && frame.wet_r.is_finite()
            && frame.dry_l.is_finite()
            && frame.reduction_db.is_finite();
        peak = peak.max(output.abs());
        dry.push(frame.dry_l);
        wet.push(output);
    }

    (dry, wet, finite, lin_to_db(peak), latency)
}

fn hash_noise(index: usize) -> f64 {
    let mut value = index as u64;
    value ^= value >> 33;
    value = value.wrapping_mul(0xff51afd7ed558ccd);
    value ^= value >> 33;
    value = value.wrapping_mul(0xc4ceb9fe1a85ec53);
    value ^= value >> 33;
    (value as f64 / u64::MAX as f64) * 2.0 - 1.0
}

fn measure_latency_samples(sample_rate: f64) -> usize {
    let mut dsp = DeEsserDsp::new(sample_rate);
    dsp.update_filters(4_000.0, 12_000.0, 0.5, 0.0, 50.0, 12.0);
    dsp.update_lookahead(0.0);
    let search = dsp.latency_samples() as usize + 8;

    let mut best_index = 0;
    let mut best_abs = 0.0;
    for sample_idx in 0..search {
        let input = if sample_idx == 0 { 1.0 } else { 0.0 };
        let frame = dsp.process_frame(
            input,
            input,
            0.0,
            0.0,
            ProcessSettings {
                threshold_db: 100.0,
                max_reduction_db: 12.0,
                ..ProcessSettings::default()
            },
        );
        if frame.dry_l.abs() > best_abs {
            best_abs = frame.dry_l.abs();
            best_index = sample_idx;
        }
    }

    best_index
}

pub fn snr_db(reference: &[f64], test: &[f64]) -> f64 {
    let signal = reference.iter().map(|sample| sample * sample).sum::<f64>();
    let noise = reference
        .iter()
        .zip(test.iter())
        .map(|(reference, test)| {
            let error = reference - test;
            error * error
        })
        .sum::<f64>();

    10.0 * (signal / noise.max(1.0e-24)).max(1.0e-24).log10()
}

pub fn band_power_db(samples: &[f64], sample_rate: f64, low_hz: f64, high_hz: f64) -> f64 {
    10.0 * band_energy(samples, sample_rate, low_hz, high_hz)
        .max(1.0e-24)
        .log10()
}

pub fn band_energy(samples: &[f64], sample_rate: f64, low_hz: f64, high_hz: f64) -> f64 {
    if samples.is_empty() {
        return 0.0;
    }

    let fft_size = samples.len().next_power_of_two();
    let mut planner = FftPlanner::<f64>::new();
    let fft = planner.plan_fft_forward(fft_size);
    let mut frame = vec![Complex::new(0.0, 0.0); fft_size];
    let input_len = samples.len() as f64;
    for (idx, sample) in samples.iter().enumerate() {
        let window = 0.5 - 0.5 * (2.0 * PI * (idx as f64 + 0.5) / input_len).cos();
        frame[idx] = Complex::new(sample * window, 0.0);
    }
    fft.process(&mut frame);

    let nyquist_bin = fft_size / 2;
    let low_bin =
        ((low_hz.max(0.0) * fft_size as f64 / sample_rate).floor() as usize).clamp(0, nyquist_bin);
    let high_bin = ((high_hz.max(low_hz) * fft_size as f64 / sample_rate).ceil() as usize)
        .clamp(low_bin, nyquist_bin);

    frame[low_bin..=high_bin]
        .iter()
        .map(|bin| bin.norm_sqr())
        .sum::<f64>()
}

pub fn total_energy(samples: &[f64]) -> f64 {
    samples.iter().map(|sample| sample * sample).sum()
}
