use std::f64::consts::PI;

use nebula_desser::dsp::{DeEsserDsp, ProcessSettings};

#[test]
fn sample_rates_up_to_384khz_produce_finite_output() {
    for sample_rate in [44_100.0, 96_000.0, 192_000.0, 384_000.0] {
        let mut dsp = DeEsserDsp::new(sample_rate);
        dsp.update_filters(4_000.0, 12_000.0, false, 0.7, 1.0, 60.0, 16.0);
        dsp.update_vocal_mode(true);

        for sample_idx in 0..4_096 {
            let sample = 0.75 * (2.0 * PI * 7_200.0 * sample_idx as f64 / sample_rate).sin();
            let frame = dsp.process_frame(
                sample,
                sample,
                sample,
                sample,
                ProcessSettings {
                    threshold_db: -28.0,
                    max_reduction_db: 16.0,
                    ..ProcessSettings::default()
                },
            );

            assert!(frame.wet_l.is_finite());
            assert!(frame.wet_r.is_finite());
            assert!(frame.reduction_db.is_finite());
        }
    }
}

#[test]
fn common_host_buffer_sizes_are_stable() {
    let sample_rate = 48_000.0;
    for buffer_size in [32, 64, 128, 256, 512, 1024, 2048] {
        let mut dsp = DeEsserDsp::new(sample_rate);
        dsp.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);

        for sample_idx in 0..buffer_size {
            let low = 0.5 * (2.0 * PI * 900.0 * sample_idx as f64 / sample_rate).sin();
            let high = 0.55 * (2.0 * PI * 6_800.0 * sample_idx as f64 / sample_rate).sin();
            let sample = low + high;
            let frame = dsp.process_frame(
                sample,
                sample,
                sample,
                sample,
                ProcessSettings {
                    threshold_db: -30.0,
                    max_reduction_db: 12.0,
                    ..ProcessSettings::default()
                },
            );

            assert!(frame.wet_l.is_finite());
            assert!(frame.wet_r.is_finite());
        }
    }
}

#[test]
fn external_sidechain_can_drive_gain_reduction() {
    let sample_rate = 48_000.0;
    let mut internal = DeEsserDsp::new(sample_rate);
    let mut external = DeEsserDsp::new(sample_rate);
    internal.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);
    external.update_filters(4_000.0, 12_000.0, false, 0.5, 1.0, 50.0, 12.0);

    let mut internal_min = 0.0_f64;
    let mut external_min = 0.0_f64;
    for sample_idx in 0..8_192 {
        let main_sample = 0.2 * (2.0 * PI * 1_000.0 * sample_idx as f64 / sample_rate).sin();
        let sidechain_sample = 0.8 * (2.0 * PI * 7_000.0 * sample_idx as f64 / sample_rate).sin();

        let internal_frame = internal.process_frame(
            main_sample,
            main_sample,
            main_sample,
            main_sample,
            ProcessSettings {
                threshold_db: -28.0,
                max_reduction_db: 12.0,
                ..ProcessSettings::default()
            },
        );
        let external_frame = external.process_frame(
            main_sample,
            main_sample,
            sidechain_sample,
            sidechain_sample,
            ProcessSettings {
                threshold_db: -28.0,
                max_reduction_db: 12.0,
                ..ProcessSettings::default()
            },
        );

        internal_min = internal_min.min(internal_frame.reduction_db);
        external_min = external_min.min(external_frame.reduction_db);
    }

    assert!(external_min < internal_min - 2.0);
}
