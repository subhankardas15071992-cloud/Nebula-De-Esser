// Nebula DeEsser v2.6.0 — TKEO Sensitivity & Multi-Vector Subspace Mode
// Updated: Threshold controls now map to TKEO spike detection sensitivity
//          Relative mode enables adaptive N-dimensional contextual analysis

use nih_plug::prelude::*;
use std::sync::Arc;
use parking_lot::Mutex;

mod dsp;
mod gui;
mod param;

use dsp::{DeEsserDsp, ProcessFrame, ProcessSettings};
use gui::{NebulaGui, GuiParams, GuiChanges, draw};
use param::ParamSnapshot;

pub struct NebulaDeEsser {
    params: Arc<NebulaDeEsserParams>,
    dsp: DeEsserDsp,
    gui_state: EguiState,
    gui: NebulaGui,
    sample_rate: f32,
}

#[derive(Params)]
pub struct NebulaDeEsserParams {
    #[id = "bypass"]
    pub bypass: BoolParam,

    // === TKEO SENSITIVITY (replaces old compression threshold) ===
    #[id = "tkeo_sensitivity"]
    pub tkeo_sensitivity: FloatParam,  // 0.0-1.0: how sharp a spike must be to trigger

    #[id = "max_reduction"]
    pub max_reduction: FloatParam,

    #[id = "min_freq"]
    pub min_freq: FloatParam,

    #[id = "max_freq"]
    pub max_freq: FloatParam,

    // === SUBSPACE MODE (replaces old mode_relative) ===
    #[id = "subspace_mode"]
    pub subspace_mode: EnumParam<SubspaceMode>,  // Absolute (3D) vs Relative (N-D adaptive)

    #[id = "use_peak_filter"]
    pub use_peak_filter: BoolParam,

    #[id = "use_wide_range"]
    pub use_wide_range: BoolParam,

    #[id = "filter_solo"]
    pub filter_solo: BoolParam,

    #[id = "lookahead_enabled"]
    pub lookahead_enabled: BoolParam,

    #[id = "lookahead_ms"]
    pub lookahead_ms: FloatParam,

    #[id = "trigger_hear"]
    pub trigger_hear: BoolParam,

    #[id = "stereo_link"]
    pub stereo_link: FloatParam,

    #[id = "stereo_mid_side"]
    pub stereo_mid_side: BoolParam,

    #[id = "sidechain_external"]
    pub sidechain_external: BoolParam,

    #[id = "vocal_mode"]
    pub vocal_mode: BoolParam,

    #[id = "input_level"]
    pub input_level: FloatParam,

    #[id = "input_pan"]
    pub input_pan: FloatParam,

    #[id = "output_level"]
    pub output_level: FloatParam,

    #[id = "output_pan"]
    pub output_pan: FloatParam,

    #[id = "cut_width"]
    pub cut_width: FloatParam,

    #[id = "cut_depth"]
    pub cut_depth: FloatParam,

    #[id = "cut_slope"]
    pub cut_slope: FloatParam,

    #[id = "mix"]
    pub mix: FloatParam,

    #[id = "oversampling"]
    pub oversampling: IntParam,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, ParamEnum)]
pub enum SubspaceMode {
    #[name = "Absolute"]
    Absolute,  // Strict 3-vector: voiced/unvoiced/residual
    #[name = "Relative"]
    Relative,  // Adaptive N-dimensional contextual analysis
}

impl Default for NebulaDeEsserParams {
    fn default() -> Self {
        Self {
            bypass: BoolParam::new("Bypass", false),

            // TKEO Sensitivity: 0.0 = aggressive (triggers on mild spikes)
            //                   1.0 = selective (requires sharp, erratic spikes)
            tkeo_sensitivity: FloatParam::new(
                "TKEO Sensitivity",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("")
            .with_step_size(0.01)
            .with_description(
                "Controls how sharp/erratic an energy spike must be to classify as sibilance. \
                 Lower = more aggressive detection | Higher = requires sharper transients",
            ),

            max_reduction: FloatParam::new(
                "Max Reduction",
                -12.0,
                FloatRange::Linear { min: -48.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_step_size(0.1),

            min_freq: FloatParam::new(
                "Min Freq",
                4000.0,
                FloatRange::Logarithmic { min: 1000.0, max: 20000.0 },
            )
            .with_unit(" Hz")
            .with_step_size(1.0),

            max_freq: FloatParam::new(
                "Max Freq",
                12000.0,
                FloatRange::Logarithmic { min: 1000.0, max: 24000.0 },
            )
            .with_unit(" Hz")
            .with_step_size(1.0),

            subspace_mode: EnumParam::new("Subspace Mode", SubspaceMode::Relative)
                .with_description(
                    "Absolute: 3-vector separation (voiced/unvoiced/residual). \
                     Relative: Adaptive N-dimensional contextual analysis that expands \
                     dimensions based on signal complexity for transparent processing.",
                ),

            use_peak_filter: BoolParam::new("Peak Filter", false),
            use_wide_range: BoolParam::new("Wide Range", false),
            filter_solo: BoolParam::new("Filter Solo", false),
            lookahead_enabled: BoolParam::new("Lookahead", true),
            lookahead_ms: FloatParam::new(
                "Lookahead",
                5.0,
                FloatRange::Linear { min: 0.0, max: 20.0 },
            )
            .with_unit(" ms")
            .with_step_size(0.1),
            trigger_hear: BoolParam::new("Trigger Hear", false),
            stereo_link: FloatParam::new(
                "Stereo Link",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_step_size(0.01),
            stereo_mid_side: BoolParam::new("Mid/Side", false),
            sidechain_external: BoolParam::new("External Sidechain", false),
            vocal_mode: BoolParam::new("Single Vocal", true),
            input_level: FloatParam::new(
                "Input Level",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_step_size(0.1),
            input_pan: FloatParam::new(
                "Input Pan",
                0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
            )
            .with_unit("")
            .with_step_size(0.01),
            output_level: FloatParam::new(
                "Output Level",
                0.0,
                FloatRange::Linear { min: -24.0, max: 24.0 },
            )
            .with_unit(" dB")
            .with_step_size(0.1),
            output_pan: FloatParam::new(
                "Output Pan",
                0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
            )
            .with_unit("")
            .with_step_size(0.01),
            cut_width: FloatParam::new(
                "Cut Width",
                0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("")
            .with_step_size(0.01),
            cut_depth: FloatParam::new(
                "Cut Depth",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit("")
            .with_step_size(0.01),
            cut_slope: FloatParam::new(
                "Cut Slope",
                50.0,
                FloatRange::Linear { min: 0.0, max: 100.0 },
            )
            .with_unit(" %")
            .with_step_size(1.0),
            mix: FloatParam::new(
                "Mix",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_unit(" %")
            .with_step_size(0.01),
            oversampling: IntParam::new(
                "Oversampling",
                0,
                IntRange::Linear { min: 0, max: 4 },
            )
            .with_unit("")
            .with_step_size(1)
            .with_value_to_string_fn(|v| match v {
                0 => "Off".to_string(),
                1 => "2×".to_string(),
                2 => "4×".to_string(),
                3 => "6×".to_string(),
                4 => "8×".to_string(),
                _ => "Off".to_string(),
            }),
        }
    }
}

impl Default for NebulaDeEsser {
    fn default() -> Self {
        Self {
            params: Arc::new(NebulaDeEsserParams::default()),
            dsp: DeEsserDsp::new(48000.0),
            gui_state: EguiState::from_size(860, 640),
            gui: NebulaGui::new(Arc::new(Mutex::new(crate::analyzer::SpectrumData::default()))),
            sample_rate: 48000.0,
        }
    }
}

impl Plugin for NebulaDeEsser {
    const NAME: &'static str = "Nebula De-Esser";
    const VENDOR: &'static str = "Nebula Audio";
    const URL: &'static str = "https://github.com/subhankardas15071992-cloud/Nebula-De-Esser";
    const EMAIL: &'static str = "contact@nebula-audio.dev";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(1),
            main_output_channels: NonZeroU32::new(1),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames::const_default(),
        },
    ];

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate;
        self.dsp = DeEsserDsp::new(self.sample_rate as f64);
        self.gui = NebulaGui::new(Arc::new(Mutex::new(
            crate::analyzer::SpectrumData::default(),
        )));
        true
    }

    fn reset(&mut self) {
        self.dsp.reset();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        if self.params.bypass.value() {
            return ProcessStatus::Normal;
        }

        let settings = ProcessSettings {
            tkeo_sensitivity: self.params.tkeo_sensitivity.value(),
            max_reduction_db: self.params.max_reduction.value(),
            mode_relative: self.params.subspace_mode.value() == SubspaceMode::Relative,
            use_peak_filter: self.params.use_peak_filter.value(),
            use_wide_range: self.params.use_wide_range.value(),
            trigger_hear: self.params.trigger_hear.value(),
            filter_solo: self.params.filter_solo.value(),
            stereo_link: self.params.stereo_link.value(),
            stereo_mid_side: self.params.stereo_mid_side.value(),
        };

        self.dsp.update_filters(
            self.params.min_freq.value(),
            self.params.max_freq.value(),
            self.params.use_peak_filter.value(),
            self.params.cut_width.value(),
            self.params.cut_depth.value(),
            self.params.cut_slope.value(),
            self.params.max_reduction.value().abs(),
        );

        self.dsp.update_lookahead(if self.params.lookahead_enabled.value() {
            self.params.lookahead_ms.value()
        } else {
            0.0
        });

        self.dsp.update_vocal_mode(self.params.vocal_mode.value());

        let channels = buffer.channels();
        let num_samples = buffer.num_samples();

        for sample_idx in 0..num_samples {
            let input_l = channels[0][sample_idx] as f64;
            let input_r = if channels.len() > 1 {
                channels[1][sample_idx] as f64
            } else {
                input_l
            };

            let frame = self.dsp.process_frame(
                input_l,
                input_r,
                input_l,
                input_r,
                settings,
            );

            channels[0][sample_idx] = (frame.wet_l * self.params.mix.value()
                + frame.dry_l * (1.0 - self.params.mix.value())) as f32;

            if channels.len() > 1 {
                channels[1][sample_idx] = (frame.wet_r * self.params.mix.value()
                    + frame.dry_r * (1.0 - self.params.mix.value())) as f32;
            }
        }

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let gui_params = Arc::new(Mutex::new(GuiParams::from_params(&params)));

        GuiEditor::new(
            self.gui_state.clone(),
            move |ctx, changes| {
                let mut gui_params = gui_params.lock();
                let ch = draw(ctx, &self.gui_state, &mut self.gui, &gui_params);
                
                // Apply GUI changes to plugin params
                if let Some(v) = ch.tkeo_sensitivity {
                    params.tkeo_sensitivity.set_value(v);
                }
                if let Some(v) = ch.max_reduction {
                    params.max_reduction.set_value(v);
                }
                if let Some(v) = ch.min_freq {
                    params.min_freq.set_value(v);
                }
                if let Some(v) = ch.max_freq {
                    params.max_freq.set_value(v);
                }
                if let Some(v) = ch.subspace_mode {
                    params.subspace_mode.set_value(v);
                }
                // ... apply other changes as needed
            },
        )
        .with_background_color(nih_plug_egui::egui::Color32::from_rgb(4, 2, 14))
        .with_min_size(860, 640)
    }
}

impl ClapPlugin for NebulaDeEsser {
    const CLAP_ID: &'static str = "nebula.deesser.v2";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Orthogonal Subspace Projection de-esser with TKEO analysis");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Mono,
    ];
}

impl Vst3Plugin for NebulaDeEsser {
    const VST3_CLASS_ID: [u8; 16] = *b"NebulaDeEsserV2";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Dynamics, Vst3SubCategory::Fx];
}

nih_export_clap!(NebulaDeEsser);
nih_export_vst3!(NebulaDeEsser);
