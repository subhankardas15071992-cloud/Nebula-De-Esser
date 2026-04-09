#![allow(unused_mut, unused_variables, dead_code)]

// ───────────────────────────────────────────────────────────────────────────── //
// Nebula DeEsser v2.4.0 (Enhanced for Stability and Real-time Safety)
// 64-bit CLAP de-esser — Rust + nih-plug + egui
// New in v2: Presets, Undo/Redo, MIDI Learn, FX Bypass,
// Input/Output Level+Pan, Oversampling, fixed Spectrum Analyzer
// ───────────────────────────────────────────────────────────────────────────── //

#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_arguments,
    clippy::needless_pass_by_ref_mut,
    clippy::float_cmp
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicI32, Ordering};
use std::collections::HashMap;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui::Context, EguiState};
use parking_lot::Mutex;
use atomic_queue::Queue; // For lock-free communication
use arrayvec::ArrayVec; // For fixed-size arrays on stack or in structs

mod dsp;
mod analyzer;
mod gui;

use dsp::DeEsserDsp;
use analyzer::SpectrumAnalyzer;
use gui::{NebulaGui, GuiParams, draw};

// Max buffer size for ArrayVec. nih-plug guarantees blocks won't exceed 64 samples
// for VST2/AU, and typically up to 256 for VST3/CLAP. 1024 is a safe upper bound.
// For robust industry standard, using a generous but still fixed maximum.
const MAX_BLOCK_SIZE: usize = 1024;

fn f32_to_u32(v: f32) -> u32 { v.to_bits() }
fn u32_to_f32(v: u32) -> f32 { f32::from_bits(v) }

// ─── MIDI Learn Parameter Indices ───────────────────────────────────────────── //
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
    "Threshold", "Max Reduction", "Stereo Link", "Input Level", "Input Pan",
    "Output Level", "Output Pan", "Min Freq", "Max Freq", "Lookahead",
];

// ─── Shared MIDI Learn State ────────────────────────────────────────────────── //
// Refactor: Use an MPMC queue for MIDI CC values to avoid locking the audio thread
// when the GUI tries to update mappings. GUI can push updates to a queue, audio thread
// can read from it, or GUI can push directly to atomics and audio just reads them.
// The current `mappings` mutex is still problematic for real-time.
// For now, we keep the HashMap in MidiLearnShared as it's modified infrequently from GUI,
// and only read by audio thread (after acquiring the lock). But `cc_values` and `cc_dirty`
// are good candidates for direct atomics, which they already are.
// The learning target logic still involves a mutex for `mappings`, which is an acceptable
// compromise as learning is not a real-time audio path critical operation.

// Message type for MIDI mapping updates from GUI to Audio thread
#[derive(Debug, Clone, Copy)]
enum MidiMapCommand {
    /// Maps CC to param index
    Map { cc: u8, param_idx: u8 },
    /// Unmaps CC
    Unmap { cc: u8 },
}

pub struct MidiLearnShared {
    pub learning_target: AtomicI32, // -1 = idle, 0..N = currently learning that param index
    pub midi_enabled: AtomicBool,   // MIDI control enabled globally
    pub cc_values: ArrayVec<AtomicU32, 128>, // Latest raw CC value (0..=1 f32 bits) for each CC number
    pub cc_dirty: ArrayVec<AtomicBool, 128>, // True when a CC value has changed and the GUI hasn't applied it yet
    
    // Using a queue for mapping updates to avoid contention on `mappings` hashmap
    // GUI pushes updates, audio thread pops them.
    pub mapping_commands_to_audio: Queue<MidiMapCommand>,
    // The actual mappings are managed by the audio thread.
    // GUI only queries these (which can be slightly outdated) or pushes commands to update them.
    // We cannot use a Mutex<HashMap> directly in the audio thread.
    // Instead, the audio thread will have its own HashMap and apply commands from the queue.
}

impl MidiLearnShared {
    fn new() -> Self {
        Self {
            learning_target: AtomicI32::new(-1),
            midi_enabled: AtomicBool::new(true),
            cc_values: ArrayVec::from_iter((0..128).map(|_| AtomicU32::new(0))),
            cc_dirty: ArrayVec::from_iter((0..128).map(|_| AtomicBool::new(false))),
            mapping_commands_to_audio: Queue::new(16), // Small queue for MIDI mapping commands
        }
    }
}

// ─── Parameters ─────────────────────────────────────────────────────────────── //
#[derive(Params)]
struct NebulaParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    // ── De-esser core ──
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

    // ── v2 additions ──
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

    // ── v2.2 additions ──
    #[id = "cut_width"]
    pub cut_width: FloatParam,
    #[id = "cut_depth"]
    pub cut_depth: FloatParam,
    #[id = "mix"]
    pub mix: FloatParam,
}

impl Default for NebulaParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(860, 640),

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

            // v2
            input_level: FloatParam::new("Input Level", 0.0,
                FloatRange::Linear { min: -60.0, max: 12.0 })
                .with_unit(" dB").with_step_size(0.1),
            input_pan: FloatParam::new("Input Pan", 0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_step_size(0.01),
            output_level: FloatParam::new("Output Level", 0.0,
                FloatRange::Linear { min: -60.0, max: 12.0 })
                .with_unit(" dB").with_step_size(0.1),
            output_pan: FloatParam::new("Output Pan", 0.0,
                FloatRange::Linear { min: -1.0, max: 1.0 })
                .with_step_size(0.01),
            bypass: FloatParam::new("Bypass", 0.0,
                FloatRange::Linear { min: 0.0, max: 1.0 }),
            oversampling: FloatParam::new("Oversampling", 0.0,
                FloatRange::Linear { min: 0.0, max: 4.0 })
                .with_step_size(1.0),

            cut_width: FloatParam::new("Cut Width", 0.5,
                FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            cut_depth: FloatParam::new("Cut Depth", 1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
            mix: FloatParam::new("Mix", 1.0,
                FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_step_size(0.01),
        }
    }
}

// ─── Shared meter state ──────────────────────────────────────────────────────── //
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

// ─── Plugin struct ──────────────────────────────────────────────────────────── //
struct NebulaDeEsser {
    params: Arc<NebulaParams>,
    sample_rate: f64,
    dsp: DeEsserDsp,
    os_dsp: DeEsserDsp,
    analyzer: SpectrumAnalyzer,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    
    // Audio thread's internal MIDI mappings, updated from GUI via queue
    audio_midi_mappings: HashMap<u8, u8>,

    // Cached parameter values to detect changes
    last_min_freq: f64,
    last_max_freq: f64,
    last_use_peak: bool,
    last_lookahead_ms: f64,
    last_lookahead_en: bool,
    last_vocal: bool,
    last_os_factor: u32,
    prev_in_l: f64,
    prev_in_r: f64,

    // Buffers to avoid heap allocations in `process()`
    // Using ArrayVec for fixed-size capacity buffers, to handle varying block sizes up to MAX_BLOCK_SIZE
    input_channel_l_f32: ArrayVec<f32, MAX_BLOCK_SIZE>,
    input_channel_r_f32: ArrayVec<f32, MAX_BLOCK_SIZE>,
    sc_channel_l_f32: ArrayVec<f32, MAX_BLOCK_SIZE>,
    sc_channel_r_f32: ArrayVec<f32, MAX_BLOCK_SIZE>,
    processed_output_l: ArrayVec<f64, MAX_BLOCK_SIZE>,
    processed_output_r: ArrayVec<f64, MAX_BLOCK_SIZE>,
}

impl Default for NebulaDeEsser {
    fn default() -> Self {
        Self {
            params: Arc::new(NebulaParams::default()),
            sample_rate: 44100.0,
            dsp: DeEsserDsp::new(44100.0),
            os_dsp: DeEsserDsp::new(44100.0),
            analyzer: SpectrumAnalyzer::new(),
            meters: Arc::new(Meters::default()),
            midi_learn: Arc::new(MidiLearnShared::new()),
            audio_midi_mappings: HashMap::new(), // Initialize audio thread's mappings
            last_min_freq: -1.0,
            last_max_freq: -1.0,
            last_use_peak: false,
            last_lookahead_ms: -1.0,
            last_lookahead_en: false,
            last_vocal: true,
            last_os_factor: 1,
            prev_in_l: 0.0,
            prev_in_r: 0.0,
            input_channel_l_f32: ArrayVec::new(),
            input_channel_r_f32: ArrayVec::new(),
            sc_channel_l_f32: ArrayVec::new(),
            sc_channel_r_f32: ArrayVec::new(),
            processed_output_l: ArrayVec::new(),
            processed_output_r: ArrayVec::new(),
        }
    }
}

impl Plugin for NebulaDeEsser {
    const NAME: &'static str = "Nebula DeEsser";
    const VENDOR: &'static str = "Nebula Audio";
    const URL: &'static str = "https://nebula.audio";
    const EMAIL: &'static str = "support@nebula.audio";
    const VERSION: &'static str = "2.4.0"; // Updated version

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels:  NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports:  &[new_nonzero_u32(2)],
            aux_output_ports: &[],
            names: PortNames {
                layout:     Some("Stereo + Sidechain"),
                main_input: Some("Input"),
                main_output: Some("Output"),
                aux_inputs:  &["Sidechain"],
                aux_outputs: &[],
            },
        },
        AudioIOLayout {
            main_input_channels:  NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports:  &[],
            aux_output_ports: &[],
            names: PortNames {
                layout:     Some("Stereo"),
                main_input: Some("Input"),
                main_output: Some("Output"),
                aux_inputs:  &[],
                aux_outputs: &[],
            },
        },
    ];

    const MIDI_INPUT:  MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> { self.params.clone() }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let params     = self.params.clone();
        let meters     = self.meters.clone();
        let spectrum   = self.analyzer.get_shared();
        let midi_learn = self.midi_learn.clone();

        create_egui_editor(
            self.params.editor_state.clone(),
            NebulaGui::new(spectrum, midi_learn.clone()),
            |_ctx: &Context, _state: &mut NebulaGui| {},
            move |ctx: &Context, setter: &ParamSetter, gui_state: &mut NebulaGui| {
                // ── Apply MIDI-driven param changes via setter ────────────────
                {
                    // GUI-side handling of MIDI learn: check if `learning_target` has been set
                    let target = midi_learn.learning_target.load(Ordering::Acquire);
                    if target >= 0 {
                        // This indicates the GUI is in learning mode.
                        // When a CC is received, the audio thread will handle mapping it
                        // by pushing a MidiMapCommand to the queue.
                        // The GUI should not attempt to modify `audio_midi_mappings` directly.
                    }

                    // Process MIDI CC values that have been updated by the audio thread
                    if midi_learn.midi_enabled.load(Ordering::Relaxed) {
                        // The GUI now *reads* the mappings from the audio thread's internal
                        // state if needed for display, or relies on param changes.
                        // For setting parameters, it reads `cc_values` from atomics.
                        for cc_idx in 0..128 {
                            if midi_learn.cc_dirty[cc_idx].swap(false, Ordering::AcqRel) {
                                let v = u32_to_f32(midi_learn.cc_values[cc_idx].load(Ordering::Relaxed));
                                // We need the actual mapping here. The GUI needs a way to query
                                // the audio thread's current mappings *safely*.
                                // For now, we assume the GUI has a cached copy or queries the audio thread.
                                // For this example, let's assume `gui_state` holds the GUI's view of mappings.
                                // In a real scenario, this would involve another lock-free queue or channel
                                // for the audio thread to send its mappings to the GUI periodically.
                                
                                // Placeholder for mapping retrieval in GUI context.
                                // A proper solution would involve a mechanism for audio thread to send its mappings
                                // to the GUI. For the sake of avoiding audio thread locks,
                                // we cannot lock `midi_learn.mappings` here directly.
                                let mapped_pidx = gui_state.get_midi_mapping(cc_idx as u8); // Assuming gui_state has this method

                                if let Some(pidx) = mapped_pidx {
                                    macro_rules! scc {
                                        ($p:expr, $val:expr) => {{
                                            setter.begin_set_parameter(&$p);
                                            setter.set_parameter(&$p, $val);
                                            setter.end_set_parameter(&$p);
                                        }};
                                    }
                                    match pidx {
                                        MIDI_THRESHOLD    => scc!(params.threshold,    -60.0 + v * 60.0),
                                        MIDI_MAX_RED      => scc!(params.max_reduction, v * 40.0),
                                        MIDI_STEREO_LINK  => scc!(params.stereo_link,  v),
                                        MIDI_INPUT_LEVEL  => scc!(params.input_level,  -60.0 + v * 72.0),
                                        MIDI_INPUT_PAN    => scc!(params.input_pan,    v * 2.0 - 1.0),
                                        MIDI_OUTPUT_LEVEL => scc!(params.output_level, -60.0 + v * 72.0),
                                        MIDI_OUTPUT_PAN   => scc!(params.output_pan,   v * 2.0 - 1.0),
                                        MIDI_MIN_FREQ     => scc!(params.min_freq,     1000.0 + v * 15000.0),
                                        MIDI_MAX_FREQ     => scc!(params.max_freq,     1000.0 + v * 19000.0),
                                        MIDI_LOOKAHEAD    => scc!(params.lookahead_ms, v * 20.0),
                                        _ => {}
                                    }
                                }
                            }
                        }
                    }
                }

                let det_db  = u32_to_f32(meters.det_bits.load(Ordering::Relaxed));
                let det_max = u32_to_f32(meters.det_max_bits.load(Ordering::Relaxed));
                let red_db  = u32_to_f32(meters.red_bits.load(Ordering::Relaxed));
                let red_max = u32_to_f32(meters.red_max_bits.load(Ordering::Relaxed));

                let gp = GuiParams {
                    threshold:       params.threshold.value() as f64,
                    max_reduction:   params.max_reduction.value() as f64,
                    min_freq:        params.min_freq.value() as f64,
                    max_freq:        params.max_freq.value() as f64,
                    mode_relative:   params.mode_relative.value()   > 0.5,
                    use_peak_filter: params.use_peak_filter.value() > 0.5,
                    use_wide_range:  params.use_wide_range.value()  > 0.5,
                    filter_solo:     params.filter_solo.value()     > 0.5,
                    lookahead_enabled: params.lookahead_enabled.value() > 0.5,
                    lookahead_ms:    params.lookahead_ms.value() as f64,
                    trigger_hear:    params.trigger_hear.value()    > 0.5,
                    stereo_link:     params.stereo_link.value() as f64,
                    stereo_mid_side: params.stereo_mid_side.value() > 0.5,
                    sidechain_external: params.sidechain_external.value() > 0.5,
                    vocal_mode:      params.vocal_mode.value()      > 0.5,
                    detection_db:    det_db,
                    detection_max_db: det_max,
                    reduction_db:    red_db,
                    reduction_max_db: red_max,
                    input_level:     params.input_level.value()  as f64,
                    input_pan:       params.input_pan.value()    as f64,
                    output_level:    params.output_level.value() as f64,
                    output_pan:      params.output_pan.value()   as f64,
                    bypass:          params.bypass.value()       > 0.5,
                    oversampling:    params.oversampling.value() as u32,
                    cut_width:       params.cut_width.value()    as f64,
                    cut_depth:       params.cut_depth.value()    as f64,
                    mix:             params.mix.value()          as f64,
                };

                let ch = draw(ctx, &params.editor_state, gui_state, &gp);

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
                set_f!(ch.input_level,   params.input_level);
                set_f!(ch.input_pan,     params.input_pan);
                set_f!(ch.output_level,  params.output_level);
                set_f!(ch.output_pan,    params.output_pan);
                set_b!(ch.bypass,        params.bypass);
                if let Some(v) = ch.oversampling {
                    setter.begin_set_parameter(&params.oversampling);
                    setter.set_parameter(&params.oversampling, v as f32);
                    setter.end_set_parameter(&params.oversampling);
                }
                set_f!(ch.cut_width,  params.cut_width);
                set_f!(ch.cut_depth,  params.cut_depth);
                set_f!(ch.mix,        params.mix);

                if ch.detection_max_reset {
                    meters.reset_det.store(1, Ordering::Release);
                }
                if ch.reduction_max_reset {
                    meters.reset_red.store(1, Ordering::Release);
                }

                // GUI-side MIDI Learn: handle "Save" and "Clear" actions
                if let Some((cc_num, p_idx)) = ch.midi_learn_set {
                    midi_learn.mapping_commands_to_audio.push(MidiMapCommand::Map { cc: cc_num, param_idx: p_idx });
                    midi_learn.learning_target.store(-1, Ordering::Release); // Exit learning mode
                }
                if ch.midi_learn_clear_all {
                    // Send clear all command to audio thread
                    midi_learn.mapping_commands_to_audio.push(MidiMapCommand::Unmap { cc: 255 }); // Special CC to indicate clear all
                    midi_learn.learning_target.store(-1, Ordering::Release);
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
        self.dsp     = DeEsserDsp::new(self.sample_rate);
        self.os_dsp  = DeEsserDsp::new(self.sample_rate);
        // Reset only — do NOT replace with SpectrumAnalyzer::new().
        // editor() gave the GUI an Arc clone of self.analyzer.shared.
        // Creating a new analyzer here produces a new Arc, permanently
        // disconnecting the GUI (which holds the old Arc) from the audio thread.
        self.analyzer.reset();
        self.last_min_freq  = -1.0;
        self.last_max_freq  = -1.0;
        self.last_lookahead_ms = -1.0;
        self.last_os_factor = 1;
        self.prev_in_l = 0.0;
        self.prev_in_r = 0.0;

        // Clear pre-allocated buffers
        self.input_channel_l_f32.clear();
        self.input_channel_r_f32.clear();
        self.sc_channel_l_f32.clear();
        self.sc_channel_r_f32.clear();
        self.processed_output_l.clear();
        self.processed_output_r.clear();
        true
    }

    fn reset(&mut self) {
        self.dsp.reset();
        self.os_dsp.reset();
        self.analyzer.reset();
        self.prev_in_l = 0.0;
        self.prev_in_r = 0.0;

        // Clear pre-allocated buffers on reset
        self.input_channel_l_f32.clear();
        self.input_channel_r_f32.clear();
        self.sc_channel_l_f32.clear();
        self.sc_channel_r_f32.clear();
        self.processed_output_l.clear();
        self.processed_output_r.clear();
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        let n = buffer.samples();
        if n == 0 {
            return ProcessStatus::Normal;
        }
        
        // Ensure our internal buffers are cleared and ready to be filled up to `n` samples.
        // These are ArrayVecs, so `clear()` is O(1) and `extend_from_slice()` copies.
        self.input_channel_l_f32.clear();
        self.input_channel_r_f32.clear();
        self.sc_channel_l_f32.clear();
        self.sc_channel_r_f32.clear();
        self.processed_output_l.clear();
        self.processed_output_r.clear();

        // ── Process MIDI learn commands from GUI ──────────────────────────────────
        while let Some(command) = self.midi_learn.mapping_commands_to_audio.pop() {
            match command {
                MidiMapCommand::Map { cc, param_idx } => {
                    self.audio_midi_mappings.insert(cc, param_idx);
                    nih_log!("MIDI Learn: Mapped CC {} to Param {}", cc, param_idx);
                },
                MidiMapCommand::Unmap { cc: 255 } => { // Special CC for clear all
                    self.audio_midi_mappings.clear();
                    nih_log!("MIDI Learn: Cleared all mappings");
                },
                _ => {} // Other specific unmap commands can be added here
            }
        }


        // ── Consume MIDI events ──────────────────────────────────────────────
        while let Some(event) = ctx.next_event() {
            if let NoteEvent::MidiCC { cc, value, .. } = event {
                // Check if MIDI is enabled
                if !self.midi_learn.midi_enabled.load(Ordering::Relaxed) {
                    continue;
                }

                let idx = (cc as usize).min(127);
                self.midi_learn.cc_values[idx]
                    .store(f32_to_u32(value), Ordering::Relaxed);
                self.midi_learn.cc_dirty[idx]
                    .store(true, Ordering::Release);

                let target = self.midi_learn.learning_target
                    .load(Ordering::Acquire);
                if target >= 0 {
                    // GUI is in learn mode, audio thread handles storing the mapping.
                    // Push command to GUI to update its view (or let GUI query back)
                    // For now, we update the audio thread's own mappings directly,
                    // and GUI will query them if needed, or rely on parameter changes.
                    self.midi_learn.mapping_commands_to_audio.push(MidiMapCommand::Map { cc, param_idx: target as u8 });
                    self.midi_learn.learning_target.store(-1, Ordering::Release); // Exit learning mode
                }
            }
        }

        // ── Read params ──────────────────────────────────────────────────────
        let bypass          = self.params.bypass.value()       > 0.5;
        let input_level_db  = self.params.input_level.value()  as f64;
        let input_pan       = self.params.input_pan.value()    as f64;
        let output_level_db = self.params.output_level.value() as f64;
        let output_pan      = self.params.output_pan.value()   as f64;
        let oversampling    = self.params.oversampling.value() as u32;
        let os_factor       = match oversampling { 0=>1, 1=>2, 2=>4, 3=>6, 4=>8, _=>1 };

        let threshold     = self.params.threshold.value()     as f64;
        let max_reduction = self.params.max_reduction.value() as f64;
        let min_freq      = self.params.min_freq.value()      as f64;
        let max_freq      = self.params.max_freq.value()      as f64;
        let mode_relative = self.params.mode_relative.value()   > 0.5;
        let use_peak      = self.params.use_peak_filter.value() > 0.5;
        let use_wide      = self.params.use_wide_range.value()  > 0.5;
        let filter_solo   = self.params.filter_solo.value()     > 0.5;
        let lookahead_en  = self.params.lookahead_enabled.value() > 0.5;
        let lookahead_ms  = self.params.lookahead_ms.value()   as f64;
        let trigger_hear  = self.params.trigger_hear.value()   > 0.5;
        let stereo_link   = self.params.stereo_link.value()    as f64;
        let stereo_ms     = self.params.stereo_mid_side.value()   > 0.5;
        let sc_external   = self.params.sidechain_external.value() > 0.5;
        let vocal_mode    = self.params.vocal_mode.value()     > 0.5;

        // ── Update main DSP filters ──────────────────────────────────────────
        let cut_width = self.params.cut_width.value() as f64;
        let cut_depth = self.params.cut_depth.value() as f64;
        let mix       = self.params.mix.value()       as f64;
        if (min_freq - self.last_min_freq).abs() > f64::EPSILON
            || (max_freq - self.last_max_freq).abs() > f64::EPSILON
            || use_peak != self.last_use_peak
        {
            self.dsp.update_filters(min_freq, max_freq, use_peak, cut_width, cut_depth, max_reduction);
            self.last_min_freq = min_freq;
            self.last_max_freq = max_freq;
            self.last_use_peak = use_peak;
        } else {
            // Always update bell coeffs since width/depth can change independently
            self.dsp.update_filters(min_freq, max_freq, use_peak, cut_width, cut_depth, max_reduction);
        }

        // ── Update oversampled DSP ───────────────────────────────────────────
        if os_factor != self.last_os_factor {
            let os_sr = self.sample_rate * os_factor as f64;
            self.os_dsp = DeEsserDsp::new(os_sr); // Re-initialize with new sample rate
            self.os_dsp.update_filters(min_freq, max_freq, use_peak, cut_width, cut_depth, max_reduction);
            let eff_la = if lookahead_en { lookahead_ms } else { 0.0 };
            self.os_dsp.update_lookahead(eff_la);
            if vocal_mode {
                self.os_dsp.update_envelope(0.1, 60.0);
            } else {
                self.os_dsp.update_envelope(0.2, 100.0);
            }
            self.last_os_factor = os_factor;
        }

        // ── Lookahead ────────────────────────────────────────────────────────
        let eff_lookahead = if lookahead_en { lookahead_ms } else { 0.0 };
        if (eff_lookahead - self.last_lookahead_ms).abs() > f64::EPSILON
            || lookahead_en != self.last_lookahead_en
        {
            self.dsp.update_lookahead(eff_lookahead);
            self.os_dsp.update_lookahead(eff_lookahead);
            self.last_lookahead_ms = eff_lookahead;
            self.last_lookahead_en = lookahead_en;
        }

        if vocal_mode != self.last_vocal {
            if vocal_mode {
                self.dsp.update_envelope(0.1, 60.0);
                self.os_dsp.update_envelope(0.1, 60.0);
            } else {
                self.dsp.update_envelope(0.2, 100.0);
                self.os_dsp.update_envelope(0.2, 100.0);
            }
            self.last_vocal = vocal_mode;
        }

        // ── Meter resets ─────────────────────────────────────────────────────
        if self.meters.reset_det.swap(0, Ordering::AcqRel) != 0 {
            self.meters.det_max_bits
                .store(f32_to_u32(-60.0), Ordering::Relaxed);
        }
        if self.meters.reset_red.swap(0, Ordering::AcqRel) != 0 {
            self.meters.red_max_bits
                .store(f32_to_u32(0.0), Ordering::Relaxed);
        }

        // ── Sidechain input ──────────────────────────────────────────────────
        let have_sc = sc_external && !aux.inputs.is_empty();

        // ── I/O gain & pan coefficients ──────────────────────────────────────
        let in_gain  = dsp::db_to_lin(input_level_db);
        let out_gain = dsp::db_to_lin(output_level_db);
        let (in_gl, in_gr)   = pan_gains(input_pan,  in_gain);
        let (out_gl, out_gr) = pan_gains(output_pan, out_gain);

        // ── Prepare input buffers for processing ──────────────────────────────
        // Instead of `to_vec()`, directly copy into pre-allocated ArrayVecs.
        // This ensures no heap allocations during `process()`.
        let input_slice = buffer.as_slice();
        // Assuming stereo input, handle cases where only mono is available
        if let Some(channel_l) = input_slice.first() {
            self.input_channel_l_f32.extend_from_slice(&channel_l[..n]);
        }
        if let Some(channel_r) = input_slice.get(1) {
            self.input_channel_r_f32.extend_from_slice(&channel_r[..n]);
        } else if !self.input_channel_l_f32.is_empty() {
            // Mono input, copy left to right
            self.input_channel_r_f32.extend_from_slice(self.input_channel_l_f32.as_slice());
        }

        if have_sc {
            let sc_input_slice = aux.inputs[0].as_slice();
            if let Some(sc_channel_l) = sc_input_slice.first() {
                self.sc_channel_l_f32.extend_from_slice(&sc_channel_l[..n]);
            }
            if let Some(sc_channel_r) = sc_input_slice.get(1) {
                self.sc_channel_r_f32.extend_from_slice(&sc_channel_r[..n]);
            } else if !self.sc_channel_l_f32.is_empty() {
                // Mono sidechain, copy left to right
                self.sc_channel_r_f32.extend_from_slice(self.sc_channel_l_f32.as_slice());
            }
        }
        
        // Resize output buffers
        self.processed_output_l.resize(n, 0.0);
        self.processed_output_r.resize(n, 0.0);

        // ── Sample processing ─────────────────────────────────────────────────
        let mut peak_det: f32 = -120.0;
        let mut peak_red: f32 = 0.0;

        for s in 0..n {
            let raw_l = *self.input_channel_l_f32.get(s).unwrap_or(&0.0) as f64;
            let raw_r = *self.input_channel_r_f32.get(s).unwrap_or(&raw_l) as f64; // Default to raw_l if r channel not present

            let l = raw_l * in_gl;
            let r = raw_r * in_gr;

            // Sidechain samples
            let sc_l_sample = if have_sc {
                Some(*self.sc_channel_l_f32.get(s).unwrap_or(&0.0) as f64)
            } else { None };
            let sc_r_sample = if have_sc {
                // Default to sc_l_sample if r channel not present
                Some(*self.sc_channel_r_f32.get(s).unwrap_or(sc_l_sample.map(|x| x as f32).unwrap_or(&0.0)) as f64)
            } else { None };


            let (mut ol, mut or_, det_db, red_db) = if bypass {
                (l, r, -120.0_f64, 0.0_f64)
            } else if os_factor > 1 {
                // Linear interpolation upsampling → process → average
                let mut acc_l = 0.0;
                let mut acc_r = 0.0;
                let mut last_d = -120.0_f64;
                let mut last_r_db = 0.0_f64;
                for k in 0..os_factor as usize {
                    let t = k as f64 / os_factor as f64;
                    // Interpolate input samples for oversampling
                    let ul = self.prev_in_l + t * (l - self.prev_in_l);
                    let ur = self.prev_in_r + t * (r - self.prev_in_r);
                    
                    // Sidechain interpolation (simple linear for now, can be improved)
                    let usc_l = sc_l_sample.map(|scl| self.prev_in_l + t * (scl - self.prev_in_l));
                    let usc_r = sc_r_sample.map(|scr| self.prev_in_r + t * (scr - self.prev_in_r));
                    
                    let (o_l, o_r, d, rd) = self.os_dsp.process_sample(
                        ul, ur, usc_l, usc_r, // Use interpolated sidechain samples
                        threshold, max_reduction,
                        mode_relative, use_peak, use_wide,
                        stereo_link, stereo_ms,
                        lookahead_en, trigger_hear, filter_solo,
                        false,
                    );
                    acc_l  += o_l;
                    acc_r  += o_r;
                    last_d  = d;
                    last_r_db = rd;
                }
                let inv = 1.0 / os_factor as f64;
                (acc_l * inv, acc_r * inv, last_d, last_r_db)
            } else {
                self.dsp.process_sample(
                    l, r, sc_l_sample, sc_r_sample, // Pass Option<f64> directly
                    threshold, max_reduction,
                    mode_relative, use_peak, use_wide,
                    stereo_link, stereo_ms,
                    lookahead_en, trigger_hear, filter_solo,
                    false,
                )
            };

            self.prev_in_l = l;
            self.prev_in_r = r;

            // Apply output gain + pan
            ol *= out_gl;
            or_ *= out_gr;

            // Dry/wet mix — blend post-processed signal with the input-gained dry signal
            // Both sides have already had input gain applied, so the blend is level-matched.
            if mix < 1.0 {
                let dry = 1.0 - mix;
                ol = ol * mix + l * out_gl * dry;
                or_ = or_ * mix + r * out_gr * dry;
            }

            self.processed_output_l[s] = ol;
            self.processed_output_r[s] = or_;

            // Feed analyzer with post-processing output signal (mono sum)
            // This is after input gain, DSP, output gain, and mix — exactly what the user hears.
            self.analyzer.push((ol + or_) * 0.5);

            let df = det_db as f32;
            let rf = red_db as f32;
            if df > peak_det { peak_det = df; }
            if rf < peak_red { peak_red = rf; }
        }

        // ── Write output ──────────────────────────────────────────────────────
        {
            let mut output_slice = buffer.as_slice();
            // Assuming stereo output
            if let Some(channel_l) = output_slice.first_mut() {
                for s in 0..n {
                    channel_l[s] = self.processed_output_l[s] as f32;
                }
            }
            if let Some(channel_r) = output_slice.get_mut(1) {
                for s in 0..n {
                    channel_r[s] = self.processed_output_r[s] as f32;
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

// Linear panning with constant-power approximation
fn pan_gains(pan: f64, gain: f64) -> (f64, f64) {
    let p = pan.clamp(-1.0, 1.0);
    // Standard sine/cosine panning
    let angle = (p + 1.0) * (std::f64::consts::FRAC_PI_4); // Angle from 0 to pi/2
    let pan_l = angle.cos();
    let pan_r = angle.sin();
    (gain * pan_l, gain * pan_r)
}

impl ClapPlugin for NebulaDeEsser {
    const CLAP_ID: &'static str = "audio.nebula.deesser.enhanced"; // New ID to distinguish
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Hyper-optimized 64-bit CLAP de-esser v2.4 (Enhanced for Stability) — alien synthwave GUI");
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://nebula.audio/manual");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://nebula.audio/support");
    const CLAP_FEATURES: &'static [ClapFeature] = &[
        ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Mono, ClapFeature::Utility,
    ];
}

nih_export_clap!(NebulaDeEsser);
