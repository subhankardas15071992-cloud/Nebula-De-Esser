use nebula_desser::metrics::{
    band_power_db, measure_deesser_objective_metrics, snr_db, total_energy,
};

#[test]
fn objective_metrics_confirm_targeted_spectral_reduction() {
    let metrics = measure_deesser_objective_metrics(48_000.0);

    assert!(metrics.all_samples_finite);
    assert!(metrics.target_band_attenuation_db > 0.02);
    assert!(metrics.low_band_leakage_db < 1.0);
    assert!(metrics.residual_target_focus_ratio > 0.65);
    assert!(metrics.peak_output_dbfs < 1.0);
}

#[test]
fn objective_metrics_confirm_latency_reporting() {
    let metrics = measure_deesser_objective_metrics(48_000.0);

    assert_eq!(
        metrics.measured_latency_samples,
        metrics.reported_latency_samples as usize
    );
}

#[test]
fn objective_metrics_confirm_bypass_transparency() {
    let metrics = measure_deesser_objective_metrics(48_000.0);

    assert!(metrics.bypass_transparency_snr_db > 90.0);
}

#[test]
fn metric_helpers_are_numerically_stable() {
    let silence = [0.0; 128];
    let impulse = {
        let mut samples = [0.0; 128];
        samples[0] = 1.0;
        samples
    };

    assert_eq!(total_energy(&silence), 0.0);
    assert!(band_power_db(&silence, 48_000.0, 1_000.0, 2_000.0).is_finite());
    assert!(snr_db(&impulse, &impulse) > 200.0);
}
