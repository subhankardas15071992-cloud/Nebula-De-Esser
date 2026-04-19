use std::f64::consts::PI;

use nebula_desser::dsp::{DeEsserDsp, ProcessSettings};

fn process_pair(
    dsp: &mut DeEsserDsp,
    left: f64,
    right: f64,
    settings: ProcessSettings,
) -> (f64, f64, f64) {
    let frame = dsp.process_frame(left, right, left, right, settings);
    (frame.wet_l, frame.wet_r, frame.reduction_db)
}

#[test]
fn split_mode_is_transparent_when_no_reduction_is_requested() {
    let mut dsp = DeEsserDsp::new(48_000.0);
    dsp.update_filters(4_000.0, 12_000.0, false, 0.5, 0.0, 50.0, 12.0);

    for &(left, right) in &[(0.1, -0.2), (0.3, 0.25), (-0.4, 0.5), (0.0, 0.0)] {
        let (wet_l, wet_r, reduction_db) = process_pair(
            &mut dsp,
            left,
            right,
            ProcessSettings {
                tkeo_threshold: 0.4,
                max_reduction_db: 12.0,
                ..ProcessSettings::default()
            },
        );

        assert!((wet_l - left).abs() < 1.0e-9);
        assert!((wet_r - right).abs() < 1.0e-9);
        assert!(reduction_db.abs() < 1.0e-9);
    }
}

#[test]
fn mid_side_path_remains_transparent_below_threshold() {
    let sample_rate = 48_000.0;
    let mut dsp = DeEsserDsp::new(sample_rate);
    dsp.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);

    for sample_idx in 0..4_096 {
        let left = 0.1 * (2.0 * PI * 1_200.0 * sample_idx as f64 / sample_rate).sin();
        let right = 0.08 * (2.0 * PI * 800.0 * sample_idx as f64 / sample_rate).sin();
        let (wet_l, wet_r, reduction_db) = process_pair(
            &mut dsp,
            left,
            right,
            ProcessSettings {
                tkeo_threshold: 1.0,
                max_reduction_db: 12.0,
                stereo_mid_side: true,
                ..ProcessSettings::default()
            },
        );

        assert!((wet_l - left).abs() < 1.0e-6);
        assert!((wet_r - right).abs() < 1.0e-6);
        assert!(reduction_db > -0.1);
    }
}

#[test]
fn wide_and_split_modes_both_stay_finite() {
    let sample_rate = 48_000.0;
    let mut split = DeEsserDsp::new(sample_rate);
    let mut wide = DeEsserDsp::new(sample_rate);
    split.update_filters(4_000.0, 12_000.0, false, 0.9, 1.0, 65.0, 18.0);
    wide.update_filters(4_000.0, 12_000.0, false, 0.9, 1.0, 65.0, 18.0);

    for sample_idx in 0..4_096 {
        let signal = 0.65 * (2.0 * PI * 6_800.0 * sample_idx as f64 / sample_rate).sin();
        let split_frame = split.process_frame(
            signal,
            signal,
            signal,
            signal,
            ProcessSettings {
                tkeo_threshold: 0.22,
                max_reduction_db: 18.0,
                use_wide_range: false,
                ..ProcessSettings::default()
            },
        );
        let wide_frame = wide.process_frame(
            signal,
            signal,
            signal,
            signal,
            ProcessSettings {
                tkeo_threshold: 0.22,
                max_reduction_db: 18.0,
                use_wide_range: true,
                ..ProcessSettings::default()
            },
        );

        assert!(split_frame.wet_l.is_finite());
        assert!(split_frame.wet_r.is_finite());
        assert!(wide_frame.wet_l.is_finite());
        assert!(wide_frame.wet_r.is_finite());
    }
}

#[test]
fn relative_mode_responds_differently_than_absolute_mode() {
    let sample_rate = 48_000.0;
    let mut absolute = DeEsserDsp::new(sample_rate);
    let mut relative = DeEsserDsp::new(sample_rate);
    absolute.update_filters(4_000.0, 12_000.0, false, 0.6, 1.0, 50.0, 12.0);
    relative.update_filters(4_000.0, 12_000.0, false, 0.6, 1.0, 50.0, 12.0);

    let mut absolute_min = 0.0_f64;
    let mut relative_min = 0.0_f64;
    for sample_idx in 0..8_192 {
        let low = 0.6 * (2.0 * PI * 700.0 * sample_idx as f64 / sample_rate).sin();
        let high = 0.45 * (2.0 * PI * 6_500.0 * sample_idx as f64 / sample_rate).sin();
        let sample = low + high;

        let absolute_frame = absolute.process_frame(
            sample,
            sample,
            sample,
            sample,
            ProcessSettings {
                tkeo_threshold: 0.52,
                max_reduction_db: 12.0,
                mode_relative: false,
                ..ProcessSettings::default()
            },
        );
        let relative_frame = relative.process_frame(
            sample,
            sample,
            sample,
            sample,
            ProcessSettings {
                tkeo_threshold: 0.52,
                max_reduction_db: 12.0,
                mode_relative: true,
                ..ProcessSettings::default()
            },
        );

        absolute_min = absolute_min.min(absolute_frame.reduction_db);
        relative_min = relative_min.min(relative_frame.reduction_db);
    }

    assert!((absolute_min - relative_min).abs() > 0.2);
}
