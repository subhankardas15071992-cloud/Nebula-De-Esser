
#![allow(unused_mut, unused_variables, dead_code)]
// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser v1.0.0
// 64-bit CLAP de-esser — Rust + nih-plug + egui
// ─────────────────────────────────────────────────────────────────────────────

#![allow(clippy::cast_precision_loss, clippy::cast_possible_truncation)]

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicI32, Ordering};

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui::Context, EguiState};

mod dsp;
mod analyzer;
mod gui;

use dsp::DeEsserDsp;
use analyzer::SpectrumAnalyzer;
use gui::{NebulaGui, GuiParams, draw};

fn f32_to_u32(v: f32) -> u32 { v.to_bits() }
fn u32_to_f32(v: u32) -> f32 { f32::from_bits(v) }

// ─── Parameters ───────────────────────────────────────────────────────────────

#[derive(Params)]
struct NebulaParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "threshold"]
    pub threshold: FloatParam,
    #[id = "max_reduction"]
    pub max_reduction: FloatParam,
    #[id = "min_freq"]
    pub min_freq: FloatParam,
    #[id = "max_freq"]
    pub max_freq: FloatParam,
    #[id = "mode_relative"]
    pub mode_relative: FloatParam,
    #[id = "use_peak_filter"]
    pub use_peak_filter: FloatParam,
    #[id = "use_wide_range"]
    pub use_wide_range: FloatParam,
    #[id = "filter_solo"]
    pub filter_solo: FloatParam,
    #[id = "lookahead_enabled"]
    pub lookahead_enabled: FloatParam,
    #[id = "lookahead_ms"]
    pub lookahead_ms: FloatParam,
    #[id = "trigger_hear"]
    pub trigger_hear: FloatParam,
    #[id = "stereo_link"]
    pub stereo_link: FloatParam,
    #[id = "stereo_mid_side"]
    pub stereo_mid_side: FloatParam,
    #[id = "sidechain_external"]
    pub sidechain_external: FloatParam,
    #[id = "vocal_mode"]
    pub vocal_mode: FloatParam,
}

impl Default for NebulaParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(860, 580),

            threshold: FloatParam::new("Threshold", -20.0,
                FloatRange::Linear { min: -60.0, max: 0.0 })
                .with_unit(" dB").with_step_size(0.1),

            max_reduction: FloatParam::new("Max Reduction", 12.0,
                FloatRange::Linear { min: 0.0, max: 40.0 })
                .with_unit(" dB").with_step_size(0.1),

            min_freq: FloatParam::new("Min Frequency", 4000.0,
                FloatRange::Skewed { min: 1000.0, max: 16000.0,
                    factor: FloatRange::skew_factor(-1.5) })
                .with_unit(" Hz").with_step_size(1.0),

            max_freq: FloatParam::new("Max Frequency", 12000.0,
                FloatRange::Skewed { min: 1000.0, max: 20000.0,
                    factor: FloatRange::skew_factor(-1.5) })
                .with_unit(" Hz").with_step_size(1.0),

            mode_relative: FloatParam::new("Mode", 1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            use_peak_filter: FloatParam::new("Filter", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            use_wide_range: FloatParam::new("Range", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            filter_solo: FloatParam::new("Filter Solo", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            lookahead_enabled: FloatParam::new("Lookahead Enable", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            lookahead_ms: FloatParam::new("Lookahead", 2.0,
                FloatRange::Linear { min: 0.0, max: 20.0 })
                .with_unit(" ms").with_step_size(0.1),
            trigger_hear: FloatParam::new("Trigger Hear", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            stereo_link: FloatParam::new("Stereo Link", 1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            stereo_mid_side: FloatParam::new("Stereo Link Mode", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            sidechain_external: FloatParam::new("Sidechain", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            vocal_mode: FloatParam::new("Processing Mode", 1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
        }
    }
}

// ─── Shared meter state ────────────────────────────────────────────────────────

struct Meters {
    det_bits: AtomicU32,
    det_max_bits: AtomicU32,
    red_bits: AtomicU32,
    red_max_bits: AtomicU32,
    reset_det: AtomicI32,
    reset_red: AtomicI32,
}

impl Default for Meters {
    fn default() -> Self {
        Self {
            det_bits: AtomicU32::new(f32_to_u32(-60.0)),
            det_max_bits: AtomicU32::new(f32_to_u32(-60.0)),
            red_bits: AtomicU32::new(f32_to_u32(0.0)),
            red_max_bits: AtomicU32::new(f32_to_u32(0.0)),
            reset_det: AtomicI32::new(0),
            reset_red: AtomicI32::new(0),
        }
    }
}

// ─── Plugin ───────────────────────────────────────────────────────────────────

struct NebulaDeEsser {
    params: Arc<NebulaParams>,
    sample_rate: f64,
    dsp: DeEsserDsp,
    analyzer: SpectrumAnalyzer,
    meters: Arc<Meters>,
    last_min_freq: f64,
    last_max_freq: f64,
    last_use_peak: bool,
    last_lookahead_ms: f64,
    last_lookahead_en: bool,
    last_vocal: bool,
}

impl Default for NebulaDeEsser {
    fn default() -> Self {
        Self {
            params: Arc::new(NebulaParams::default()),
            sample_rate: 44100.0,
            dsp: DeEsserDsp::new(44100.0),
            analyzer: SpectrumAnalyzer::new(),
            meters: Arc::new(Meters::default()),
            last_min_freq: -1.0,
            last_max_freq: -1.0,
            last_use_peak: false,
            last_lookahead_ms: -1.0,
            last_lookahead_en: false,
            last_vocal: true,
        }
    }
}

impl Plugin for NebulaDeEsser {
    const NAME: &'static str = "Nebula DeEsser";
    const VENDOR: &'static str = "Nebula Audio";
    const URL: &'static str = "https://nebula.audio";
    const EMAIL: &'static str = "support@nebula.audio";
    const VERSION: &'static str = "1.0.0";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            aux_output_ports: &[],
            names: PortNames {
                layout: Some("Stereo + Sidechain"),
                main_input: Some("Input"),
                main_output: Some("Output"),
                aux_inputs: &["Sidechain"],
                aux_outputs: &[],
            },
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames {
                layout: Some("Stereo"),
                main_input: Some("Input"),
                main_output: Some("Output"),
                aux_inputs: &[],
                aux_outputs: &[],
            },
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> { self.params.clone() }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let meters = self.meters.clone();
        let spectrum = self.analyzer.get_shared();

        create_egui_editor(
            self.params.editor_state.clone(),
            NebulaGui::new(spectrum),
            |_ctx: &Context, _state: &mut NebulaGui| {},
            move |ctx: &Context, setter: &ParamSetter, gui_state: &mut NebulaGui| {
                let det_db  = u32_to_f32(meters.det_bits.load(Ordering::Relaxed));
                let det_max = u32_to_f32(meters.det_max_bits.load(Ordering::Relaxed));
                let red_db  = u32_to_f32(meters.red_bits.load(Ordering::Relaxed));
                let red_max = u32_to_f32(meters.red_max_bits.load(Ordering::Relaxed));

                let gp = GuiParams {
                    threshold:       params.threshold.value() as f64,
                    max_reduction:   params.max_reduction.value() as f64,
                    min_freq:        params.min_freq.value() as f64,
                    max_freq:        params.max_freq.value() as f64,
                    mode_relative:   params.mode_relative.value() > 0.5,
                    use_peak_filter: params.use_peak_filter.value() > 0.5,
                    use_wide_range:  params.use_wide_range.value() > 0.5,
                    filter_solo:     params.filter_solo.value() > 0.5,
                    lookahead_enabled: params.lookahead_enabled.value() > 0.5,
                    lookahead_ms:    params.lookahead_ms.value() as f64,
                    trigger_hear:    params.trigger_hear.value() > 0.5,
                    stereo_link:     params.stereo_link.value() as f64,
                    stereo_mid_side: params.stereo_mid_side.value() > 0.5,
                    sidechain_external: params.sidechain_external.value() > 0.5,
                    vocal_mode:      params.vocal_mode.value() > 0.5,
                    detection_db:    det_db,
                    detection_max_db: det_max,
                    reduction_db:    red_db,
                    reduction_max_db: red_max,
                };

                let ch = draw(ctx, gui_state, &gp);

                macro_rules! set_f {
                    ($opt:expr, $param:expr) => {
                        if let Some(v) = $opt {
                            setter.begin_set_parameter(&$param);
                            setter.set_parameter(&$param, v as f32);
                            setter.end_set_parameter(&$param);
                        }
                    };
                }
                macro_rules! set_b {
                    ($opt:expr, $param:expr) => {
                        if let Some(v) = $opt {
                            setter.begin_set_parameter(&$param);
                            setter.set_parameter(&$param, if v { 1.0_f32 } else { 0.0_f32 });
                            setter.end_set_parameter(&$param);
                        }
                    };
                }

                set_f!(ch.threshold,     params.threshold);
                set_f!(ch.max_reduction, params.max_reduction);
                set_f!(ch.min_freq,      params.min_freq);
                set_f!(ch.max_freq,      params.max_freq);
                set_f!(ch.stereo_link,   params.stereo_link);
                set_f!(ch.lookahead_ms,  params.lookahead_ms);
                set_b!(ch.mode_relative,      params.mode_relative);
                set_b!(ch.use_peak_filter,    params.use_peak_filter);
                set_b!(ch.use_wide_range,     params.use_wide_range);
                set_b!(ch.filter_solo,        params.filter_solo);
                set_b!(ch.lookahead_enabled,  params.lookahead_enabled);
                set_b!(ch.trigger_hear,       params.trigger_hear);
                set_b!(ch.stereo_mid_side,    params.stereo_mid_side);
                set_b!(ch.sidechain_external, params.sidechain_external);
                set_b!(ch.vocal_mode,         params.vocal_mode);

                if ch.detection_max_reset {
                    meters.reset_det.store(1, Ordering::Release);
                }
                if ch.reduction_max_reset {
                    meters.reset_red.store(1, Ordering::Release);
                }
            },
        )
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _ctx: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.dsp = DeEsserDsp::new(self.sample_rate);
        self.analyzer = SpectrumAnalyzer::new();
        self.last_min_freq = -1.0;
        self.last_max_freq = -1.0;
        self.last_lookahead_ms = -1.0;
        true
    }

    fn reset(&mut self) {
        self.dsp.reset();
        self.analyzer.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        _ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // ── Read params ──────────────────────────────────────────────────────
        let threshold     = self.params.threshold.value() as f64;
        let max_reduction = self.params.max_reduction.value() as f64;
        let min_freq      = self.params.min_freq.value() as f64;
        let max_freq      = self.params.max_freq.value() as f64;
        let mode_relative = self.params.mode_relative.value() > 0.5;
        let use_peak      = self.params.use_peak_filter.value() > 0.5;
        let use_wide      = self.params.use_wide_range.value() > 0.5;
        let filter_solo   = self.params.filter_solo.value() > 0.5;
        let lookahead_en  = self.params.lookahead_enabled.value() > 0.5;
        let lookahead_ms  = self.params.lookahead_ms.value() as f64;
        let trigger_hear  = self.params.trigger_hear.value() > 0.5;
        let stereo_link   = self.params.stereo_link.value() as f64;
        let stereo_ms     = self.params.stereo_mid_side.value() > 0.5;
        let sc_external   = self.params.sidechain_external.value() > 0.5;
        let vocal_mode    = self.params.vocal_mode.value() > 0.5;

        // ── Update DSP params ────────────────────────────────────────────────
        if (min_freq - self.last_min_freq).abs() > 0.5
            || (max_freq - self.last_max_freq).abs() > 0.5
            || use_peak != self.last_use_peak
        {
            self.dsp.update_filters(min_freq, max_freq, use_peak);
            self.last_min_freq = min_freq;
            self.last_max_freq = max_freq;
            self.last_use_peak = use_peak;
        }

        let eff_lookahead = if lookahead_en { lookahead_ms } else { 0.0 };
        if (eff_lookahead - self.last_lookahead_ms).abs() > 0.01
            || lookahead_en != self.last_lookahead_en
        {
            self.dsp.update_lookahead(eff_lookahead);
            self.last_lookahead_ms = eff_lookahead;
            self.last_lookahead_en = lookahead_en;
        }

        if vocal_mode != self.last_vocal {
            if vocal_mode {
                self.dsp.update_envelope(0.1, 60.0);
            } else {
                self.dsp.update_envelope(0.2, 100.0);
            }
            self.last_vocal = vocal_mode;
        }

        // ── Meter resets ─────────────────────────────────────────────────────
        if self.meters.reset_det.swap(0, Ordering::AcqRel) != 0 {
            self.meters.det_max_bits.store(f32_to_u32(-60.0), Ordering::Relaxed);
        }
        if self.meters.reset_red.swap(0, Ordering::AcqRel) != 0 {
            self.meters.red_max_bits.store(f32_to_u32(0.0), Ordering::Relaxed);
        }

        // ── Gather sidechain ─────────────────────────────────────────────────
        let n = buffer.samples();
        let have_sc = sc_external && !aux.inputs.is_empty();
        let sc_data: Vec<Vec<f64>> = if have_sc {
            let sc_slice = aux.inputs[0].as_slice();
            sc_slice.iter()
                .map(|ch| ch.iter().map(|&s| s as f64).collect())
                .collect()
        } else {
            vec![]
        };

        // ── Copy input to local f64 buffers ──────────────────────────────────
        let nch = buffer.channels();
        let input_data: Vec<Vec<f64>> = {
            let slice = buffer.as_slice();
            slice.iter()
                .map(|ch| ch.iter().map(|&s| s as f64).collect())
                .collect()
        };

        // ── DSP processing ───────────────────────────────────────────────────
        let mut out_l = vec![0.0_f64; n];
        let mut out_r = vec![0.0_f64; n];
        let mut peak_det: f32 = -120.0;
        let mut peak_red: f32 = 0.0;

        for s in 0..n {
            let l = input_data.get(0).map(|c| c[s]).unwrap_or(0.0);
            let r = input_data.get(1).map(|c| c[s]).unwrap_or(l);
            let sc_l = if have_sc { sc_data.get(0).map(|c| c[s]) } else { None };
            let sc_r = if have_sc { sc_data.get(1).map(|c| c[s]) } else { None };

            let (ol, or_, det_db, red_db) = self.dsp.process_sample(
                l, r, sc_l, sc_r,
                threshold, max_reduction,
                mode_relative, use_peak, use_wide,
                stereo_link, stereo_ms,
                lookahead_en, trigger_hear, filter_solo,
            );

            out_l[s] = ol;
            out_r[s] = or_;
            self.analyzer.push((l + r) * 0.5);

            let df = det_db as f32;
            let rf = red_db as f32;
            if df > peak_det { peak_det = df; }
            if rf < peak_red { peak_red = rf; }
        }

        // ── Write output ─────────────────────────────────────────────────────
        {
            let out_slice = buffer.as_slice();
            for (ch_idx, channel) in out_slice.iter_mut().enumerate() {
                let src = if ch_idx == 0 { &out_l } else { &out_r };
                for (s, smp) in channel.iter_mut().enumerate() {
                    *smp = src[s] as f32;
                }
            }
        }

        // ── Update meters ─────────────────────────────────────────────────────
        self.meters.det_bits.store(f32_to_u32(peak_det), Ordering::Relaxed);
        self.meters.red_bits.store(f32_to_u32(peak_red), Ordering::Relaxed);

        let prev_det = u32_to_f32(self.meters.det_max_bits.load(Ordering::Relaxed));
        if peak_det > prev_det {
            self.meters.det_max_bits.store(f32_to_u32(peak_det), Ordering::Relaxed);
        }
        let prev_red = u32_to_f32(self.meters.red_max_bits.load(Ordering::Relaxed));
        if peak_red < prev_red {
            self.meters.red_max_bits.store(f32_to_u32(peak_red), Ordering::Relaxed);
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for NebulaDeEsser {
    const CLAP_ID: &'static str = "audio.nebula.deesser";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Hyper-optimized 64-bit CLAP de-esser — alien synthwave GUI");
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://nebula.audio/manual");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://nebula.audio/support");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
        ClapFeature::Utility,
    ];
}

nih_export_clap!(NebulaDeEsser);
