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

// ─── CHANGED: "Threshold" → "TKEO Sensitivity" for v2.5.0 algorithm ───
pub const MIDI_PARAM_NAMES: &[&str] = &[
    "TKEO Sensitivity",
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
                "Threshold",
                -20.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_step_size(0.1),
            max_reduction: FloatParam::new(
                "Max Reduction",
                12.0,
                FloatRange::Linear { min: 0.0, max: 40.0 },
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
                FloatRange::Linear { min: 0.0, max: 20.0 },
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
                FloatRange::Linear { min: -100.0, max: 100.0 },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_smoother(SmoothingStyle::Linear(20.0)),
            input_pan: FloatParam::new(
                "Input Pan",
                0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
            )
            .with_step_size(0.01)
            .with_smoother(SmoothingStyle::Linear(20.0)),
            output_level: FloatParam::new(
                "Output Level",
                0.0,
                FloatRange::Linear { min: -100.0, max: 100.0 },
            )
            .with_unit(" dB")
            .with_step_size(0.1)
            .with_smoother(SmoothingStyle::Linear(20.0)),
            output_pan: FloatParam::new(
                "Output Pan",
                0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 },
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
                FloatRange::Linear { min: 0.0, max: 100.0 },
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

    fn params(&self) -> Arc<NebulaParams> {
        self.params.clone()
    }

    fn editor(&mut self, _async_executor: AsyncExecutor) -> Option<Box<dyn Editor>> {
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
        context: &mut impl InitContext,
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
        context: &mut impl ProcessContext,
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
            if ($param.value() - $value).abs() > 0.001 {
                $param.set_value($value);
                setter.set_parameter(&*$param);
            }
        }};
    }

    match parameter_index {
        MIDI_THRESHOLD => set_param!(params.threshold, value),
        MIDI_MAX_RED => set_param!(params.max_reduction, value),
        MIDI_STEREO_LINK => set_param!(params.stereo_link, value),
        MIDI_INPUT_LEVEL => set_param!(params.input_level, value),
        MIDI_INPUT_PAN => set_param!(params.input_pan, value),
        MIDI_OUTPUT_LEVEL => set_param!(params.output_level, value),
        MIDI_OUTPUT_PAN => set_param!(params.output_pan, value),
        MIDI_MIN_FREQ => set_param!(params.min_freq, value),
        MIDI_MAX_FREQ => set_param!(params.max_freq, value),
        MIDI_LOOKAHEAD => set_param!(params.lookahead_ms, value),
        _ => {}
    }
}

fn apply_gui_changes(changes: &gui::GuiChanges, params: &NebulaParams, setter: &ParamSetter) {
    macro_rules! set_if_some {
        ($field:expr, $param:expr) => {
            if let Some(v) = $field {
                if ($param.value() as f64 - v).abs() > 0.001 {
                    $param.set_value(v as f32);
                    setter.set_parameter(&*$param);
                }
            }
        };
        ($field:expr, $param:expr, bool) => {
            if let Some(v) = $field {
                let current = $param.value();
                let target = if v { 1.0 } else { 0.0 };
                if (current - target).abs() > 0.001 {
                    $param.set_value(target);
                    setter.set_parameter(&*$param);
                }
            }
        };
    }

    set_if_some!(changes.threshold, params.threshold);
    set_if_some!(changes.max_reduction, params.max_reduction);
    set_if_some!(changes.min_freq, params.min_freq);
    set_if_some!(changes.max_freq, params.max_freq);
    set_if_some!(changes.mode_relative, params.mode_relative, bool);
    set_if_some!(changes.use_peak_filter, params.use_peak_filter, bool);
    set_if_some!(changes.use_wide_range, params.use_wide_range, bool);
    set_if_some!(changes.filter_solo, params.filter_solo, bool);
    set_if_some!(changes.lookahead_enabled, params.lookahead_enabled, bool);
    set_if_some!(changes.lookahead_ms, params.lookahead_ms);
    set_if_some!(changes.trigger_hear, params.trigger_hear, bool);
    set_if_some!(changes.stereo_link, params.stereo_link);
    set_if_some!(changes.stereo_mid_side, params.stereo_mid_side, bool);
    set_if_some!(changes.sidechain_external, params.sidechain_external, bool);
    set_if_some!(changes.vocal_mode, params.vocal_mode, bool);
    set_if_some!(changes.input_level, params.input_level);
    set_if_some!(changes.input_pan, params.input_pan);
    set_if_some!(changes.output_level, params.output_level);
    set_if_some!(changes.output_pan, params.output_pan);
    set_if_some!(changes.bypass, params.bypass, bool);
    set_if_some!(changes.oversampling, params.oversampling);
    set_if_some!(changes.cut_width, params.cut_width);
    set_if_some!(changes.cut_depth, params.cut_depth);
    set_if_some!(changes.cut_slope, params.cut_slope);
    set_if_some!(changes.mix, params.mix);
}

fn prepare_dsp(
    dsp: &mut DeEsserDsp,
    min_freq: f64,
    max_freq: f64,
    use_peak: bool,
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
        use_peak,
        cut_width,
        cut_depth,
        cut_slope,
        max_reduction,
    );
    dsp.update_lookahead(lookahead_ms);
    dsp.set_single_vocal(single_vocal);
}

fn oversampling_factor(os: u32) -> u32 {
    match os {
        1 => 2,
        2 => 4,
        3 => 6,
        4 => 8,
        _ => 1,
    }
}

fn lookahead_latency_samples(lookahead_ms: f64, sample_rate: f64) -> u32 {
    ((lookahead_ms * sample_rate) / 1000.0).round() as u32
}

fn db_to_lin(db: f64) -> f64 {
    10.0_f64.powf(db / 20.0)
}

fn lin_to_db(lin: f64) -> f64 {
    if lin <= 0.0 {
        -120.0
    } else {
        20.0 * lin.log10()
    }
}

fn pan_gains(pan: f64, gain: f64) -> (f64, f64) {
    let pan_clamped = pan.clamp(-1.0, 1.0);
    let (l, r) = if pan_clamped >= 0.0 {
        (1.0, 1.0 - pan_clamped)
    } else {
        (1.0 + pan_clamped, 1.0)
    };
    (l * gain, r * gain)
}
