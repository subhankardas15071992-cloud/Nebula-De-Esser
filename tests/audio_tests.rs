use std::f64::consts::PI;

use nebula_desser::dsp::{DeEsserDsp, ProcessSettings};

fn tone_reduction_db(freq_hz: f64, amplitude: f64, settings: ProcessSettings) -> f64 {
    let sample_rate = 48_000.0;
    let mut dsp = DeEsserDsp::new(sample_rate);
    dsp.update_filters(
        4_000.0,
        12_000.0,
        false,
        0.5,
        1.0,
        50.0,
        settings.max_reduction_db,
    );
    dsp.update_vocal_mode(true);

    let mut minimum_reduction = 0.0_f64;
    for sample_idx in 0..8_192 {
        let phase = 2.0 * PI * freq_hz * sample_idx as f64 / sample_rate;
        let sample = amplitude * phase.sin();
        let frame = dsp.process_frame(sample, sample, sample, sample, settings);
        minimum_reduction = minimum_reduction.min(frame.reduction_db);
    }

    minimum_reduction
}

#[test]
fn high_band_tone_triggers_more_reduction_than_low_band_tone() {
    let settings = ProcessSettings {
        threshold_db: -30.0,
        max_reduction_db: 12.0,
        mode_relative: false,
        ..ProcessSettings::default()
    };

    let low_band_reduction = tone_reduction_db(1_000.0, 0.7, settings);
    let high_band_reduction = tone_reduction_db(7_500.0, 0.7, settings);

    assert!(high_band_reduction < low_band_reduction - 2.0);
}

#[test]
fn zero_lookahead_has_zero_initial_delay() {
    let mut dsp = DeEsserDsp::new(48_000.0);
    dsp.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);
    dsp.update_lookahead(0.0);

    let frame = dsp.process_frame(
        1.0,
        1.0,
        1.0,
        1.0,
        ProcessSettings {
            threshold_db: 0.0,
            max_reduction_db: 12.0,
            ..ProcessSettings::default()
        },
    );

    assert!((frame.dry_l - 1.0).abs() < 1.0e-12);
    assert!((frame.dry_r - 1.0).abs() < 1.0e-12);
}

#[test]
fn lookahead_delay_matches_requested_latency() {
    let mut dsp = DeEsserDsp::new(48_000.0);
    dsp.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);
    dsp.update_lookahead(5.0);

    let latency_samples = 240;
    for index in 0..latency_samples {
        let frame = dsp.process_frame(
            if index == 0 { 1.0 } else { 0.0 },
            if index == 0 { 1.0 } else { 0.0 },
            0.0,
            0.0,
            ProcessSettings {
                threshold_db: 0.0,
                max_reduction_db: 12.0,
                ..ProcessSettings::default()
            },
        );

        if index + 1 < latency_samples {
            assert!(frame.dry_l.abs() < 1.0e-12);
            assert!(frame.dry_r.abs() < 1.0e-12);
        }
    }

    let delayed = dsp.process_frame(
        0.0,
        0.0,
        0.0,
        0.0,
        ProcessSettings {
            threshold_db: 0.0,
            max_reduction_db: 12.0,
            ..ProcessSettings::default()
        },
    );
    assert!(delayed.dry_l.abs() > 0.9);
    assert!(delayed.dry_r.abs() > 0.9);
}

#[test]
fn trigger_hear_outputs_detector_band_not_full_signal() {
    let sample_rate = 48_000.0;
    let mut dsp = DeEsserDsp::new(sample_rate);
    dsp.update_filters(5_000.0, 10_000.0, false, 0.5, 1.0, 50.0, 12.0);

    let mut trigger_energy = 0.0;
    let mut dry_energy = 0.0;
    for sample_idx in 0..2_048 {
        let low = (2.0 * PI * 500.0 * sample_idx as f64 / sample_rate).sin() * 0.8;
        let high = (2.0 * PI * 7_000.0 * sample_idx as f64 / sample_rate).sin() * 0.4;
        let sample = low + high;
        let frame = dsp.process_frame(
            sample,
            sample,
            sample,
            sample,
            ProcessSettings {
                threshold_db: -20.0,
                max_reduction_db: 12.0,
                trigger_hear: true,
                ..ProcessSettings::default()
            },
        );
        trigger_energy += frame.wet_l.abs();
        dry_energy += sample.abs();
    }

    assert!(trigger_energy < dry_energy);
    assert!(trigger_energy > 0.0);
}
