use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui::Context, EguiState};
use parking_lot::Mutex;

pub mod analyzer;
pub mod dsp;
mod gui;

use analyzer::SpectrumAnalyzer;
use dsp::{db_to_lin, DeEsserDsp, ProcessFrame, ProcessSettings};
use gui::{draw, GuiParams, NebulaGui};

const UNMAPPED_CC: i32 = -1;

#[inline]
fn f32_to_u32(value: f32) -> u32 {
    value.to_bits()
}

#[inline]
fn u32_to_f32(value: u32) -> f32 {
    f32::from_bits(value)
}

pub const MIDI_THRESHOLD: u8 = 0;
pub const MIDI_MAX_RED: u8 = 1;
pub const MIDI_STEREO_LINK: u8 = 2;
pub const MIDI_INPUT_LEVEL: u8 = 3;
pub const MIDI_INPUT_PAN: u8 = 4;
pub const MIDI_OUTPUT_LEVEL: u8 = 5;
pub const MIDI_OUTPUT_PAN: u8 = 6;
pub const MIDI_MIN_FREQ: u8 = 7;
pub const MIDI_MAX_FREQ: u8 = 8;
pub const MIDI_LOOKAHEAD: u8 = 9;
pub const MIDI_PARAM_COUNT: usize = 10;

pub const MIDI_PARAM_NAMES: &[&str] = &[
    "TKEO Sharpness",
    "Max Reduction",
    "Stereo Link",
    "Input Level",
    "Input Pan",
    "Output Level",
    "Output Pan",
    "Min Frequency",
    "Max Frequency",
    "Lookahead",
];

pub struct MidiLearnShared {
    pub learning_target: AtomicI32,
    pub mappings: Mutex<HashMap<u8, u8>>,
    pub saved_mappings: Mutex<HashMap<u8, u8>>,
    pub midi_enabled: AtomicBool,
    pub cc_values: Vec<AtomicU32>,
    pub cc_dirty: Vec<AtomicBool>,
    cc_bindings: Vec<AtomicI32>,
    bindings_dirty: AtomicBool,
}

impl MidiLearnShared {
    fn new() -> Self {
        Self {
            learning_target: AtomicI32::new(UNMAPPED_CC),
            mappings: Mutex::new(HashMap::new()),
            saved_mappings: Mutex::new(HashMap::new()),
            midi_enabled: AtomicBool::new(true),
            cc_values: (0..128).map(|_| AtomicU32::new(0)).collect(),
            cc_dirty: (0..128).map(|_| AtomicBool::new(false)).collect(),
            cc_bindings: (0..128).map(|_| AtomicI32::new(UNMAPPED_CC)).collect(),
            bindings_dirty: AtomicBool::new(false),
        }
    }

    fn binding_for_cc(&self, cc: usize) -> Option<u8> {
        let binding = self.cc_bindings[cc.min(127)].load(Ordering::Acquire);
        (binding >= 0).then_some(binding as u8)
    }

    fn learn_cc(&self, cc: u8, parameter_index: u8) {
        self.cc_bindings[cc as usize].store(parameter_index as i32, Ordering::Release);
        self.bindings_dirty.store(true, Ordering::Release);
    }

    fn sync_mutex_from_atomic_if_needed(&self) {
        if !self.bindings_dirty.swap(false, Ordering::AcqRel) {
            return;
        }

        let mut mappings = self.mappings.lock();
        mappings.clear();
        for (cc, binding) in self.cc_bindings.iter().enumerate() {
            let value = binding.load(Ordering::Acquire);
            if value >= 0 {
                mappings.insert(cc as u8, value as u8);
            }
        }
    }

    fn sync_atomic_from_mutex(&self) {
        for binding in &self.cc_bindings {
            binding.store(UNMAPPED_CC, Ordering::Release);
        }

        let mappings = self.mappings.lock().clone();
        for (cc, parameter_index) in mappings {
            self.cc_bindings[cc as usize].store(parameter_index as i32, Ordering::Release);
        }
    }
}

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
    #[id = "input_level"]
    pub input_level: FloatParam,
    #[id = "input_pan"]
    pub input_pan: FloatParam,
    #[id = "output_level"]
    pub output_level: FloatParam,
    #[id = "output_pan"]
    pub output_pan: FloatParam,
    #[id = "bypass"]
    pub bypass: FloatParam,
    #[id = "oversampling"]
    pub oversampling: FloatParam,
    #[id = "cut_width"]
    pub cut_width: FloatParam,
    #[id = "cut_depth"]
    pub cut_depth: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
    #[id = "cut_slope"]
    pub cut_slope: FloatParam,
}

impl Default for NebulaParams {
    fn default() -> Self {
        let freq_range = FloatRange::Skewed {
            min: 1.0,
            max: 24_000.0,
            factor: FloatRange::skew_factor(-2.0),
        };

        Self {
            editor_state: EguiState::from_size(860, 640),
            threshold: FloatParam::new(
                "TKEO Sharpness",
                50.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" %")
            .with_step_size(1.0),
            max_reduction: FloatParam::new(
                "Max Reduction",
                -12.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 0.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1),
            min_freq: FloatParam::new("Min Frequency", 4_000.0, freq_range.clone())
                .with_unit(" Hz")
                .with_step_size(1.0),
            max_freq: FloatParam::new("Max Frequency", 12_000.0, freq_range)
                .with_unit(" Hz")
                .with_step_size(1.0),
            mode_relative: bool_param("Mode", true),
            use_peak_filter: bool_param("Filter", false),
            use_wide_range: bool_param("Range", false),
            filter_solo: bool_param("Filter Solo", false),
            lookahead_enabled: bool_param("Lookahead Enabled", false),
            lookahead_ms: FloatParam::new(
                "Lookahead",
                0.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 20.0,
                },
            )
            .with_unit(" ms")
            .with_step_size(0.1),
            trigger_hear: bool_param("Trigger Hear", false),
            stereo_link: FloatParam::new(
                "Stereo Link",
                1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 },
            )
            .with_step_size(0.01),
            stereo_mid_side: bool_param("Mid/Side", false),
            sidechain_external: bool_param("Sidechain", false),
            vocal_mode: bool_param("Vocal Mode", true),
            input_level: FloatParam::new(
                "Input Level",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_smoother(SmoothingStyle::Linear(20.0)),
            input_pan: FloatParam::new(
                "Input Pan",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_step_size(0.01)
            .with_smoother(SmoothingStyle::Linear(20.0)),
            output_level: FloatParam::new(
                "Output Level",
                0.0,
                FloatRange::Linear {
                    min: -100.0,
                    max: 100.0,
                },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_smoother(SmoothingStyle::Linear(20.0)),
            output_pan: FloatParam::new(
                "Output Pan",
                0.0,
                FloatRange::Linear {
                    min: -1.0,
                    max: 1.0,
                },
            )
            .with_step_size(0.01)
            .with_smoother(SmoothingStyle::Linear(20.0)),
            bypass: bool_param("Bypass", false),
            oversampling: FloatParam::new(
                "Oversampling",
                0.0,
                FloatRange::Linear { min: 0.0, max: 4.0 },
            )
            .with_step_size(1.0),
            cut_width: FloatParam::new("Cut Width", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            cut_depth: FloatParam::new("Cut Depth", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01)
                .with_smoother(SmoothingStyle::Linear(10.0)),
            cut_slope: FloatParam::new(
                "Cut Slope",
                50.0,
                FloatRange::Linear {
                    min: 0.0,
                    max: 100.0,
                },
            )
            .with_unit(" dB/oct")
            .with_step_size(0.1),
        }
    }
}

fn bool_param(name: &str, default: bool) -> FloatParam {
    FloatParam::new(
        name,
        if default { 1.0 } else { 0.0 },
        FloatRange::Linear { min: 0.0, max: 1.0 },
    )
}

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
            det_bits: AtomicU32::new(f32_to_u32(-120.0)),
            det_max_bits: AtomicU32::new(f32_to_u32(-120.0)),
            red_bits: AtomicU32::new(f32_to_u32(0.0)),
            red_max_bits: AtomicU32::new(f32_to_u32(0.0)),
            reset_det: AtomicI32::new(0),
            reset_red: AtomicI32::new(0),
        }
    }
}

struct WetMixSmoother {
    coeff: f64,
    current: f64,
}

impl WetMixSmoother {
    fn new(sample_rate: f64) -> Self {
        let mut smoother = Self {
            coeff: 0.0,
            current: 1.0,
        };
        smoother.set_sample_rate(sample_rate);
        smoother
    }

    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.coeff = if sample_rate <= 0.0 {
            0.0
        } else {
            (-1.0 / (0.010 * sample_rate)).exp()
        };
    }

    fn reset(&mut self, value: f64) {
        self.current = value.clamp(0.0, 1.0);
    }

    fn next(&mut self, target: f64) -> f64 {
        let target = target.clamp(0.0, 1.0);
        self.current = target + self.coeff * (self.current - target);
        self.current.clamp(0.0, 1.0)
    }
}

struct NebulaDeEsser {
    params: Arc<NebulaParams>,
    sample_rate: f64,
    dsp: DeEsserDsp,
    os_dsp: DeEsserDsp,
    analyzer: SpectrumAnalyzer,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    current_os_factor: u32,
    reported_latency: u32,
    wet_mix: WetMixSmoother,
    prev_main_l: f64,
    prev_main_r: f64,
    prev_sc_l: f64,
    prev_sc_r: f64,
}

impl Default for NebulaDeEsser {
    fn default() -> Self {
        let sample_rate = 44_100.0;
        Self {
            params: Arc::new(NebulaParams::default()),
            sample_rate,
            dsp: DeEsserDsp::new(sample_rate),
            os_dsp: DeEsserDsp::new(sample_rate),
            analyzer: SpectrumAnalyzer::new(),
            meters: Arc::new(Meters::default()),
            midi_learn: Arc::new(MidiLearnShared::new()),
            current_os_factor: 1,
            reported_latency: 0,
            wet_mix: WetMixSmoother::new(sample_rate),
            prev_main_l: 0.0,
            prev_main_r: 0.0,
            prev_sc_l: 0.0,
            prev_sc_r: 0.0,
        }
    }
}

impl Plugin for NebulaDeEsser {
    const NAME: &'static str = "Nebula DeEsser";
    const VENDOR: &'static str = "Nebula Audio";
    const URL: &'static str = "https://github.com/subhankardas15071992-cloud/Nebula-De-Esser";
    const EMAIL: &'static str = "support@nebula.audio";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

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

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params = self.params.clone();
        let meters = self.meters.clone();
        let spectrum = self.analyzer.get_shared();
        let midi_learn = self.midi_learn.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            NebulaGui::new(spectrum, midi_learn.clone()),
            |_ctx: &Context, _state: &mut NebulaGui| {},
            move |ctx: &Context, setter: &ParamSetter, gui_state: &mut NebulaGui| {
                midi_learn.sync_mutex_from_atomic_if_needed();

                if midi_learn.midi_enabled.load(Ordering::Relaxed) {
                    for cc in 0..128 {
                        if !midi_learn.cc_dirty[cc].swap(false, Ordering::AcqRel) {
                            continue;
                        }

                        let Some(parameter_index) = midi_learn.binding_for_cc(cc) else {
                            continue;
                        };
                        let value = u32_to_f32(midi_learn.cc_values[cc].load(Ordering::Relaxed));
                        apply_midi_mapping(parameter_index, value, &params, setter);
                    }
                }

                let det_db = u32_to_f32(meters.det_bits.load(Ordering::Relaxed));
                let det_max = u32_to_f32(meters.det_max_bits.load(Ordering::Relaxed));
                let red_db = u32_to_f32(meters.red_bits.load(Ordering::Relaxed));
                let red_max = u32_to_f32(meters.red_max_bits.load(Ordering::Relaxed));

                let gui_params = GuiParams {
                    threshold: params.threshold.value() as f64,
                    max_reduction: params.max_reduction.value() as f64,
                    min_freq: params.min_freq.value() as f64,
                    max_freq: params.max_freq.value() as f64,
                    mode_relative: params.mode_relative.value() > 0.5,
                    use_peak_filter: params.use_peak_filter.value() > 0.5,
                    use_wide_range: params.use_wide_range.value() > 0.5,
                    filter_solo: params.filter_solo.value() > 0.5,
                    lookahead_enabled: params.lookahead_enabled.value() > 0.5,
                    lookahead_ms: params.lookahead_ms.value() as f64,
                    trigger_hear: params.trigger_hear.value() > 0.5,
                    stereo_link: params.stereo_link.value() as f64,
                    stereo_mid_side: params.stereo_mid_side.value() > 0.5,
                    sidechain_external: params.sidechain_external.value() > 0.5,
                    vocal_mode: params.vocal_mode.value() > 0.5,
                    detection_db: det_db,
                    detection_max_db: det_max,
                    reduction_db: red_db,
                    reduction_max_db: red_max,
                    input_level: params.input_level.value() as f64,
                    input_pan: params.input_pan.value() as f64,
                    output_level: params.output_level.value() as f64,
                    output_pan: params.output_pan.value() as f64,
                    bypass: params.bypass.value() > 0.5,
                    oversampling: params.oversampling.value() as u32,
                    cut_width: params.cut_width.value() as f64,
                    cut_depth: params.cut_depth.value() as f64,
                    mix: params.mix.value() as f64,
                    cut_slope: params.cut_slope.value() as f64,
                };

                let changes = draw(ctx, &params.editor_state, gui_state, &gui_params);
                apply_gui_changes(&changes, &params, setter);
                midi_learn.sync_atomic_from_mutex();

                if changes.detection_max_reset {
                    meters.reset_det.store(1, Ordering::Release);
                }
                if changes.reduction_max_reset {
                    meters.reset_red.store(1, Ordering::Release);
                }
            },
        )
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.dsp = DeEsserDsp::new(self.sample_rate);
        self.os_dsp = DeEsserDsp::new(self.sample_rate);
        self.current_os_factor = 1;
        self.reported_latency = 0;
        self.analyzer.reset();
        self.analyzer.set_sample_rate(self.sample_rate);
        self.wet_mix.set_sample_rate(self.sample_rate);
        self.wet_mix.reset(self.params.mix.value() as f64);
        self.prev_main_l = 0.0;
        self.prev_main_r = 0.0;
        self.prev_sc_l = 0.0;
        self.prev_sc_r = 0.0;
        context.set_latency_samples(0);
        true
    }

    fn reset(&mut self) {
        self.dsp.reset();
        self.os_dsp.reset();
        self.analyzer.reset();
        self.wet_mix.reset(self.params.mix.value() as f64);
        self.prev_main_l = 0.0;
        self.prev_main_r = 0.0;
        self.prev_sc_l = 0.0;
        self.prev_sc_r = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        while let Some(event) = context.next_event() {
            if let NoteEvent::MidiCC { cc, value, .. } = event {
                if !self.midi_learn.midi_enabled.load(Ordering::Relaxed) {
                    continue;
                }

                let cc_index = (cc as usize).min(127);
                self.midi_learn.cc_values[cc_index].store(f32_to_u32(value), Ordering::Relaxed);
                self.midi_learn.cc_dirty[cc_index].store(true, Ordering::Release);

                let learning_target = self.midi_learn.learning_target.load(Ordering::Acquire);
                if learning_target >= 0 {
                    self.midi_learn
                        .learning_target
                        .store(UNMAPPED_CC, Ordering::Release);
                    self.midi_learn.learn_cc(cc, learning_target as u8);
                }
            }
        }

        if self.meters.reset_det.swap(0, Ordering::AcqRel) != 0 {
            self.meters
                .det_max_bits
                .store(f32_to_u32(-120.0), Ordering::Relaxed);
        }
        if self.meters.reset_red.swap(0, Ordering::AcqRel) != 0 {
            self.meters
                .red_max_bits
                .store(f32_to_u32(0.0), Ordering::Relaxed);
        }

        let threshold = self.params.threshold.value() as f64;
        let max_reduction = self.params.max_reduction.value() as f64;
        let min_freq = self.params.min_freq.value() as f64;
        let max_freq = self.params.max_freq.value() as f64;
        let cut_width = self.params.cut_width.value() as f64;
        let cut_depth = self.params.cut_depth.value() as f64;
        let cut_slope = self.params.cut_slope.value() as f64;
        let mode_relative = self.params.mode_relative.value() > 0.5;
        let use_peak_filter = self.params.use_peak_filter.value() > 0.5;
        let use_wide_range = self.params.use_wide_range.value() > 0.5;
        let filter_solo = self.params.filter_solo.value() > 0.5;
        let trigger_hear = self.params.trigger_hear.value() > 0.5;
        let stereo_link = self.params.stereo_link.value() as f64;
        let stereo_mid_side = self.params.stereo_mid_side.value() > 0.5;
        let sidechain_external = self.params.sidechain_external.value() > 0.5;
        let single_vocal = self.params.vocal_mode.value() > 0.5;
        let lookahead_enabled = self.params.lookahead_enabled.value() > 0.5;
        let lookahead_ms = self.params.lookahead_ms.value() as f64;
        let oversampling = self.params.oversampling.value() as u32;
        let os_factor = oversampling_factor(oversampling);

        prepare_dsp(
            &mut self.dsp,
            min_freq,
            max_freq,
            use_peak_filter,
            cut_width,
            cut_depth,
            cut_slope,
            max_reduction,
            if lookahead_enabled { lookahead_ms } else { 0.0 },
            single_vocal,
        );

        if os_factor != self.current_os_factor {
            self.os_dsp = DeEsserDsp::new(self.sample_rate * os_factor as f64);
            self.current_os_factor = os_factor;
        }
        prepare_dsp(
            &mut self.os_dsp,
            min_freq,
            max_freq,
            use_peak_filter,
            cut_width,
            cut_depth,
            cut_slope,
            max_reduction,
            if lookahead_enabled { lookahead_ms } else { 0.0 },
            single_vocal,
        );

        let target_latency = if lookahead_enabled && lookahead_ms > 0.0 {
            lookahead_latency_samples(lookahead_ms, self.sample_rate)
        } else {
            0
        };
        if target_latency != self.reported_latency {
            context.set_latency_samples(target_latency);
            self.reported_latency = target_latency;
        }

        let settings = ProcessSettings {
            threshold_db: threshold,
            max_reduction_db: max_reduction,
            mode_relative,
            use_peak_filter,
            use_wide_range,
            trigger_hear,
            filter_solo,
            stereo_link,
            stereo_mid_side,
        };

        let sidechain_buffers = if sidechain_external && !aux.inputs.is_empty() {
            Some(aux.inputs[0].as_slice_immutable())
        } else {
            None
        };

        let samples = buffer.samples();
        let channels = buffer.as_slice();
        if channels.len() < 2 {
            return ProcessStatus::Normal;
        }

        let (left_slice, right_slice) = {
            let (left, right) = channels.split_at_mut(1);
            (&mut left[0], &mut right[0])
        };

        let mut peak_det = -120.0_f32;
        let mut peak_red = 0.0_f32;

        for sample_index in 0..samples {
            let input_level_db = self.params.input_level.smoothed.next() as f64;
            let input_pan = self.params.input_pan.smoothed.next() as f64;
            let output_level_db = self.params.output_level.smoothed.next() as f64;
            let output_pan = self.params.output_pan.smoothed.next() as f64;

            let main_in_l = left_slice[sample_index] as f64;
            let main_in_r = right_slice[sample_index] as f64;
            let input_gain = db_to_lin(input_level_db);
            let (input_gain_l, input_gain_r) = pan_gains(input_pan, input_gain);
            let processed_in_l = main_in_l * input_gain_l;
            let processed_in_r = main_in_r * input_gain_r;

            let sc_in_l = sidechain_buffers
                .and_then(|buffers| buffers.first())
                .and_then(|channel| channel.get(sample_index))
                .copied()
                .map(f64::from)
                .unwrap_or(processed_in_l);
            let sc_in_r = sidechain_buffers
                .and_then(|buffers| buffers.get(1))
                .and_then(|channel| channel.get(sample_index))
                .copied()
                .map(f64::from)
                .unwrap_or(processed_in_r);

            let ProcessFrame {
                wet_l,
                wet_r,
                dry_l,
                dry_r,
                detection_db,
                reduction_db,
            } = if os_factor > 1 {
                let mut wet_l_acc = 0.0;
                let mut wet_r_acc = 0.0;
                let mut dry_l_acc = 0.0;
                let mut dry_r_acc = 0.0;
                let mut det_acc = -120.0_f64;
                let mut red_acc = 0.0_f64;

                for substep in 0..os_factor {
                    let t = (substep as f64 + 1.0) / os_factor as f64;
                    let interp_l = self.prev_main_l + (processed_in_l - self.prev_main_l) * t;
                    let interp_r = self.prev_main_r + (processed_in_r - self.prev_main_r) * t;
                    let interp_sc_l = self.prev_sc_l + (sc_in_l - self.prev_sc_l) * t;
                    let interp_sc_r = self.prev_sc_r + (sc_in_r - self.prev_sc_r) * t;

                    let frame = self.os_dsp.process_frame(
                        interp_l,
                        interp_r,
                        interp_sc_l,
                        interp_sc_r,
                        settings,
                    );
                    wet_l_acc += frame.wet_l;
                    wet_r_acc += frame.wet_r;
                    dry_l_acc += frame.dry_l;
                    dry_r_acc += frame.dry_r;
                    det_acc = det_acc.max(frame.detection_db);
                    red_acc = red_acc.min(frame.reduction_db);
                }

                let inv = 1.0 / os_factor as f64;
                ProcessFrame {
                    wet_l: wet_l_acc * inv,
                    wet_r: wet_r_acc * inv,
                    dry_l: dry_l_acc * inv,
                    dry_r: dry_r_acc * inv,
                    detection_db: det_acc,
                    reduction_db: red_acc,
                }
            } else {
                self.dsp
                    .process_frame(processed_in_l, processed_in_r, sc_in_l, sc_in_r, settings)
            };

            let mix_target = if self.params.bypass.value() > 0.5 {
                0.0
            } else if trigger_hear || filter_solo {
                1.0
            } else {
                self.params.mix.smoothed.next() as f64
            };
            let wet_mix = self.wet_mix.next(mix_target);
            let dry_mix = 1.0 - wet_mix;
            let mixed_l = wet_l * wet_mix + dry_l * dry_mix;
            let mixed_r = wet_r * wet_mix + dry_r * dry_mix;

            let output_gain = db_to_lin(output_level_db);
            let (output_gain_l, output_gain_r) = pan_gains(output_pan, output_gain);
            let out_l = mixed_l * output_gain_l;
            let out_r = mixed_r * output_gain_r;

            left_slice[sample_index] = out_l as f32;
            right_slice[sample_index] = out_r as f32;
            self.analyzer.push((out_l + out_r) * 0.5);

            peak_det = peak_det.max(detection_db as f32);
            peak_red = peak_red.min(reduction_db as f32);
            self.prev_main_l = processed_in_l;
            self.prev_main_r = processed_in_r;
            self.prev_sc_l = sc_in_l;
            self.prev_sc_r = sc_in_r;
        }

        self.meters
            .det_bits
            .store(f32_to_u32(peak_det), Ordering::Relaxed);
        self.meters
            .red_bits
            .store(f32_to_u32(peak_red), Ordering::Relaxed);

        let det_max = u32_to_f32(self.meters.det_max_bits.load(Ordering::Relaxed));
        if peak_det > det_max {
            self.meters
                .det_max_bits
                .store(f32_to_u32(peak_det), Ordering::Relaxed);
        }
        let red_max = u32_to_f32(self.meters.red_max_bits.load(Ordering::Relaxed));
        if peak_red < red_max {
            self.meters
                .red_max_bits
                .store(f32_to_u32(peak_red), Ordering::Relaxed);
        }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for NebulaDeEsser {
    const CLAP_ID: &'static str = "audio.nebula.deesser";
    const CLAP_DESCRIPTION: Option<&'static str> =
        Some("Spectral-style de-esser with split/wide processing, sidechain, and lookahead");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect,
        ClapFeature::Stereo,
        ClapFeature::Deesser,
        ClapFeature::Filter,
        ClapFeature::Utility,
        ClapFeature::Restoration,
    ];
}

impl Vst3Plugin for NebulaDeEsser {
    const VST3_CLASS_ID: [u8; 16] = *b"NebulaDeEssrVST3";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[
        Vst3SubCategory::Fx,
        Vst3SubCategory::Dynamics,
        Vst3SubCategory::Filter,
        Vst3SubCategory::Tools,
    ];
}

nih_export_clap!(NebulaDeEsser);
nih_export_vst3!(NebulaDeEsser);

fn apply_midi_mapping(
    parameter_index: u8,
    value: f32,
    params: &Arc<NebulaParams>,
    setter: &ParamSetter,
) {
    macro_rules! set_param {
        ($param:expr, $value:expr) => {{
            setter.begin_set_parameter(&$param);
            setter.set_parameter(&$param, $value);
            setter.end_set_parameter(&$param);
        }};
    }

    match parameter_index {
        MIDI_THRESHOLD => set_param!(params.threshold, value * 100.0),
        MIDI_MAX_RED => set_param!(params.max_reduction, -100.0 + value * 100.0),
        MIDI_STEREO_LINK => set_param!(params.stereo_link, value),
        MIDI_INPUT_LEVEL => set_param!(params.input_level, -100.0 + value * 200.0),
        MIDI_INPUT_PAN => set_param!(params.input_pan, value * 2.0 - 1.0),
        MIDI_OUTPUT_LEVEL => set_param!(params.output_level, -100.0 + value * 200.0),
        MIDI_OUTPUT_PAN => set_param!(params.output_pan, value * 2.0 - 1.0),
        MIDI_MIN_FREQ => set_param!(params.min_freq, 1.0 + value * 23_999.0),
        MIDI_MAX_FREQ => set_param!(params.max_freq, 1.0 + value * 23_999.0),
        MIDI_LOOKAHEAD => set_param!(params.lookahead_ms, value * 20.0),
        _ => {}
    }
}

fn apply_gui_changes(changes: &gui::GuiChanges, params: &Arc<NebulaParams>, setter: &ParamSetter) {
    macro_rules! set_float {
        ($field:expr, $param:expr) => {
            if let Some(value) = $field {
                setter.begin_set_parameter(&$param);
                setter.set_parameter(&$param, value as f32);
                setter.end_set_parameter(&$param);
            }
        };
    }

    macro_rules! set_bool {
        ($field:expr, $param:expr) => {
            if let Some(value) = $field {
                setter.begin_set_parameter(&$param);
                setter.set_parameter(&$param, if value { 1.0 } else { 0.0 });
                setter.end_set_parameter(&$param);
            }
        };
    }

    set_float!(changes.threshold, params.threshold);
    set_float!(changes.max_reduction, params.max_reduction);
    set_float!(changes.min_freq, params.min_freq);
    set_float!(changes.max_freq, params.max_freq);
    set_bool!(changes.mode_relative, params.mode_relative);
    set_bool!(changes.use_peak_filter, params.use_peak_filter);
    set_bool!(changes.use_wide_range, params.use_wide_range);
    set_bool!(changes.filter_solo, params.filter_solo);
    set_bool!(changes.lookahead_enabled, params.lookahead_enabled);
    set_float!(changes.lookahead_ms, params.lookahead_ms);
    set_bool!(changes.trigger_hear, params.trigger_hear);
    set_float!(changes.stereo_link, params.stereo_link);
    set_bool!(changes.stereo_mid_side, params.stereo_mid_side);
    set_bool!(changes.sidechain_external, params.sidechain_external);
    set_bool!(changes.vocal_mode, params.vocal_mode);
    set_float!(changes.input_level, params.input_level);
    set_float!(changes.input_pan, params.input_pan);
    set_float!(changes.output_level, params.output_level);
    set_float!(changes.output_pan, params.output_pan);
    set_bool!(changes.bypass, params.bypass);
    set_float!(changes.cut_width, params.cut_width);
    set_float!(changes.cut_depth, params.cut_depth);
    set_float!(changes.cut_slope, params.cut_slope);
    set_float!(changes.mix, params.mix);

    if let Some(oversampling) = changes.oversampling {
        setter.begin_set_parameter(&params.oversampling);
        setter.set_parameter(&params.oversampling, oversampling as f32);
        setter.end_set_parameter(&params.oversampling);
    }
}

#[allow(clippy::too_many_arguments)]
fn prepare_dsp(
    dsp: &mut DeEsserDsp,
    min_freq: f64,
    max_freq: f64,
    use_peak_filter: bool,
    cut_width: f64,
    cut_depth: f64,
    cut_slope: f64,
    max_reduction: f64,
    lookahead_ms: f64,
    single_vocal: bool,
) {
    dsp.update_filters(
        min_freq,
        max_freq,
        use_peak_filter,
        cut_width,
        cut_depth,
        cut_slope,
        max_reduction,
    );
    dsp.update_lookahead(lookahead_ms);
    dsp.update_vocal_mode(single_vocal);
}

fn oversampling_factor(selection: u32) -> u32 {
    match selection {
        1 => 2,
        2 => 4,
        3 => 6,
        4 => 8,
        _ => 1,
    }
}

fn lookahead_latency_samples(lookahead_ms: f64, sample_rate: f64) -> u32 {
    ((lookahead_ms.max(0.0) * sample_rate) / 1000.0).round() as u32
}

fn pan_gains(pan: f64, gain: f64) -> (f64, f64) {
    let angle = (pan.clamp(-1.0, 1.0) + 1.0) * (PI * 0.25);
    (gain * angle.cos(), gain * angle.sin())
}
```

## `src/dsp.rs`

```rust
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
            channel.reduction.set_times(0.0, 0.0, 0.0, self.sample_rate);
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

        let band_detect_l = self.detect_signal(sc_l, 0, settings.use_peak_filter);
        let band_detect_r = self.detect_signal(sc_r, 1, settings.use_peak_filter);
        let tkeo_sensitivity = threshold_to_tkeo_sensitivity(settings.threshold_db);
        let detected_l = if settings.use_wide_range {
            sc_l * 0.65 + band_detect_l * 0.35
        } else {
            band_detect_l
        };
        let detected_r = if settings.use_wide_range {
            sc_r * 0.65 + band_detect_r * 0.35
        } else {
            band_detect_r
        };

        let detected_env_l = self.channels[0].detect_env.process(detected_l);
        let detected_env_r = self.channels[1].detect_env.process(detected_r);
        let full_env_l = self.channels[0].full_env.process(sc_l);
        let full_env_r = self.channels[1].full_env.process(sc_r);

        let subspace_l = self.subspace_metrics(
            sc_l,
            detected_env_l,
            full_env_l,
            tkeo_sensitivity,
            settings.mode_relative,
            settings.use_wide_range,
            0,
        );
        let subspace_r = self.subspace_metrics(
            sc_r,
            detected_env_r,
            full_env_r,
            tkeo_sensitivity,
            settings.mode_relative,
            settings.use_wide_range,
            1,
        );
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

        let comparison_l = if settings.use_wide_range {
            // Wide = full-signal analysis. Decide using overall voice behavior.
            let behavior_energy = full_env_l * (0.7 + 0.3 * subspace_l);
            lin_to_db(behavior_energy)
        } else if settings.mode_relative {
            // Split = harsh-band analysis relative to full signal.
            lin_to_db(detect_env_l) - lin_to_db(full_env_l)
        } else {
            // Split absolute = harsh-band absolute energy.
            lin_to_db(detect_env_l)
        };
        let comparison_r = if settings.use_wide_range {
            let behavior_energy = full_env_r * (0.7 + 0.3 * subspace_r);
            lin_to_db(behavior_energy)
        } else if settings.mode_relative {
            lin_to_db(detect_env_r) - lin_to_db(full_env_r)
        } else {
            lin_to_db(detect_env_r)
        };

        let fixed_trigger_db = if settings.use_wide_range {
            -24.0
        } else {
            -30.0
        };
        let base_target_l = reduction_amount(comparison_l, fixed_trigger_db, max_reduction_db);
        let base_target_r = reduction_amount(comparison_r, fixed_trigger_db, max_reduction_db);

        // Keep user controls responsive by treating the transparency stack as a shaping layer
        // around the base control law instead of a hard attenuation cascade.
        let transparency_l = transparency_shaping(subspace_l, psycho_l, formant_lock_l);
        let transparency_r = transparency_shaping(subspace_r, psycho_r, formant_lock_r);
        // Keep user controls as the dominant driver.
        // The transparency layer gently nudges behavior instead of suppressing it.
        let reduction_target_l = (base_target_l * (0.75 + 0.25 * transparency_l)).clamp(0.0, 1.0);
        let reduction_target_r = (base_target_r * (0.75 + 0.25 * transparency_r)).clamp(0.0, 1.0);

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
    fn subspace_metrics(
        &mut self,
        input: f64,
        detected_env: f64,
        full_env: f64,
        tkeo_sensitivity: f64,
        mode_relative: bool,
        use_wide_range: bool,
        channel_idx: usize,
    ) -> f64 {
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
        let sum = (f_short + f_mid + f_long).max(1.0e-12);

        // Absolute mode = strict 3-vector decomposition:
        // voiced axis (harmonic), unvoiced axis (sibilant), residual axis.
        let voiced_axis = (f_long / sum).clamp(0.0, 1.0);
        let unvoiced_axis = (f_short / sum).clamp(0.0, 1.0);
        let residual_axis = (f_mid / sum).clamp(0.0, 1.0);
        let sharpness_score = unvoiced_axis * 0.6 + residual_axis * 0.4;
        let sharpness_requirement = 0.15 + 0.7 * tkeo_sensitivity.clamp(0.0, 1.0);
        let spike_classification = ((sharpness_score - sharpness_requirement)
            / (1.0 - sharpness_requirement).max(1.0e-6))
        .clamp(0.0, 1.0);

        let strict_three_vector = spike_classification
            * (unvoiced_axis * 0.55 + residual_axis * 0.45)
            * (0.55 + 0.45 * orth_ratio)
            * (1.0 - 0.25 * voiced_axis);

        if !mode_relative {
            return strict_three_vector.clamp(0.2, 1.1);
        }

        // Relative mode = adaptive multi-vector behavior (beyond 3D when needed).
        // The decision to expand weighting uses signal context + detector relation.
        let detected_ratio = (detected_env / (full_env + 1.0e-9)).clamp(0.0, 4.0);
        let flux = ((f_short - f_mid).abs() + (f_mid - f_long).abs()) / sum;
        let multi_vector_enable = if use_wide_range {
            (detected_ratio * 0.45 + flux * 1.6 + orth_ratio * 0.5).clamp(0.0, 1.0)
        } else {
            (detected_ratio * 0.55 + flux * 1.2 + orth_ratio * 0.6).clamp(0.0, 1.0)
        };
        let extended_vector_gain = strict_three_vector
            + multi_vector_enable * (flux * 0.55 + orth_ratio * 0.45)
            + spike_classification * (0.12 + 0.08 * (1.0 - tkeo_sensitivity));

        extended_vector_gain.clamp(0.25, 1.2)
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
fn threshold_to_tkeo_sensitivity(threshold_value: f64) -> f64 {
    (threshold_value / 100.0).clamp(0.0, 1.0)
}

#[inline]
fn transparency_shaping(subspace: f64, psycho: f64, formant_lock: f64) -> f64 {
    let subspace_weight = 0.85 + 0.15 * subspace.clamp(0.0, 1.0);
    let psycho_weight = 0.85 + 0.15 * psycho.clamp(0.0, 1.0);
    let formant_weight = 0.85 + 0.15 * formant_lock.clamp(0.0, 1.0);
    (subspace_weight * psycho_weight * formant_weight).clamp(0.65, 1.0)
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
                threshold_db: 50.0,
                max_reduction_db: -12.0,
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
            threshold_db: 10.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            ..ProcessSettings::default()
        };
        let settings_strict = ProcessSettings {
            threshold_db: 90.0,
            max_reduction_db: -12.0,
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

    #[test]
    fn split_and_wide_modes_produce_different_reduction_behavior() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        let split_settings = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            use_wide_range: false,
            ..ProcessSettings::default()
        };
        let wide_settings = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            use_wide_range: true,
            ..ProcessSettings::default()
        };

        let mut split_reduction = 0.0;
        let mut wide_reduction = 0.0;
        for _ in 0..256 {
            split_reduction = dsp
                .process_frame(0.7, 0.7, 0.7, 0.7, split_settings)
                .reduction_db;
        }
        dsp.reset();
        for _ in 0..256 {
            wide_reduction = dsp
                .process_frame(0.7, 0.7, 0.7, 0.7, wide_settings)
                .reduction_db;
        }

        assert!((split_reduction - wide_reduction).abs() > 0.1);
    }

    #[test]
    fn relative_mode_engages_adaptive_multi_vector_weighting() {
        let mut dsp = DeEsserDsp::new(48_000.0);
        let absolute = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: false,
            ..ProcessSettings::default()
        };
        let relative = ProcessSettings {
            threshold_db: 50.0,
            max_reduction_db: -12.0,
            mode_relative: true,
            ..ProcessSettings::default()
        };

        let mut abs_reduction = 0.0;
        let mut rel_reduction = 0.0;
        for _ in 0..256 {
            abs_reduction = dsp
                .process_frame(0.75, 0.75, 0.75, 0.75, absolute)
                .reduction_db;
        }
        dsp.reset();
        for _ in 0..256 {
            rel_reduction = dsp
                .process_frame(0.75, 0.75, 0.75, 0.75, relative)
                .reduction_db;
        }

        assert!((abs_reduction - rel_reduction).abs() > 0.1);
    }
}
