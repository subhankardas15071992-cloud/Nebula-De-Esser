#![allow(unused_mut, unused_variables, dead_code)]
#![allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::too_many_arguments,
    clippy::needless_pass_by_ref_mut,
)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicI32, Ordering};
use std::collections::HashMap;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, egui::Context, EguiState};
use parking_lot::Mutex;

mod dsp;
mod analyzer;
mod gui;
mod stable_adapter;

use dsp::DeEsserDsp;
use analyzer::SpectrumAnalyzer;
use gui::{NebulaGui, GuiParams, draw};
use stable_adapter::StableBlockAdapter;

fn f32_to_u32(v: f32) -> u32 { v.to_bits() }
fn u32_to_f32(v: u32) -> f32 { f32::from_bits(v) }

// MIDI Constants
pub const MIDI_THRESHOLD:    u8 = 0;
pub const MIDI_MAX_RED:      u8 = 1;
pub const MIDI_STEREO_LINK:  u8 = 2;
pub const MIDI_INPUT_LEVEL:  u8 = 3;
pub const MIDI_INPUT_PAN:    u8 = 4;
pub const MIDI_OUTPUT_LEVEL: u8 = 5;
pub const MIDI_OUTPUT_PAN:   u8 = 6;
pub const MIDI_MIN_FREQ:     u8 = 7;
pub const MIDI_MAX_FREQ:     u8 = 8;
pub const MIDI_LOOKAHEAD:    u8 = 9;
pub const MIDI_PARAM_COUNT:  usize = 10;

pub const MIDI_PARAM_NAMES: &[&str] = &[
    "Threshold",   "Max Reduction",  "Stereo Link",
    "Input Level", "Input Pan",      "Output Level", "Output Pan",
    "Min Freq",    "Max Freq",       "Lookahead",
];

pub struct MidiLearnShared {
    pub learning_target: AtomicI32,
    pub mappings: Mutex<HashMap<u8, u8>>,
    pub saved_mappings: Mutex<HashMap<u8, u8>>,
    pub midi_enabled: AtomicBool,
    pub cc_values: Vec<AtomicU32>,
    pub cc_dirty: Vec<AtomicBool>,
}

impl MidiLearnShared {
    fn new() -> Self {
        Self {
            learning_target: AtomicI32::new(-1),
            mappings: Mutex::new(HashMap::new()),
            saved_mappings: Mutex::new(HashMap::new()),
            midi_enabled: AtomicBool::new(true),
            cc_values: (0..128).map(|_| AtomicU32::new(0)).collect(),
            cc_dirty: (0..128).map(|_| AtomicBool::new(false)).collect(),
        }
    }
}

#[derive(Params)]
struct NebulaParams {
    #[persist = "editor-state"]
    editor_state: Arc<EguiState>,

    #[id = "threshold"] pub threshold: FloatParam,
    #[id = "max_reduction"] pub max_reduction: FloatParam,
    #[id = "min_freq"] pub min_freq: FloatParam,
    #[id = "max_freq"] pub max_freq: FloatParam,
    #[id = "mode_relative"] pub mode_relative: FloatParam,
    #[id = "use_peak_filter"] pub use_peak_filter: FloatParam,
    #[id = "use_wide_range"] pub use_wide_range: FloatParam,
    #[id = "filter_solo"] pub filter_solo: FloatParam,
    #[id = "lookahead_enabled"] pub lookahead_enabled: FloatParam,
    #[id = "lookahead_ms"] pub lookahead_ms: FloatParam,
    #[id = "trigger_hear"] pub trigger_hear: FloatParam,
    #[id = "stereo_link"] pub stereo_link: FloatParam,
    #[id = "stereo_mid_side"] pub stereo_mid_side: FloatParam,
    #[id = "sidechain_external"] pub sidechain_external: FloatParam,
    #[id = "vocal_mode"] pub vocal_mode: FloatParam,
    #[id = "input_level"] pub input_level: FloatParam,
    #[id = "input_pan"] pub input_pan: FloatParam,
    #[id = "output_level"] pub output_level: FloatParam,
    #[id = "output_pan"] pub output_pan: FloatParam,
    #[id = "bypass"] pub bypass: FloatParam,
    #[id = "oversampling"] pub oversampling: FloatParam,
    #[id = "cut_width"] pub cut_width: FloatParam,
    #[id = "cut_depth"] pub cut_depth: FloatParam,
    #[id = "mix"] pub mix: FloatParam,
}

impl Default for NebulaParams {
    fn default() -> Self {
        Self {
            editor_state: EguiState::from_size(860, 640),
            threshold: FloatParam::new("Threshold", -20.0, FloatRange::Linear { min: -60.0, max: 0.0 }).with_unit(" dB").with_step_size(0.1),
            max_reduction: FloatParam::new("Max Reduction", 12.0, FloatRange::Linear { min: 0.0, max: 40.0 }).with_unit(" dB").with_step_size(0.1),
            min_freq: FloatParam::new("Min Frequency", 4000.0, FloatRange::Skewed { min: 1000.0, max: 16000.0, factor: FloatRange::skew_factor(-1.5) }).with_unit(" Hz").with_step_size(1.0),
            max_freq: FloatParam::new("Max Frequency", 12000.0, FloatRange::Skewed { min: 1000.0, max: 20000.0, factor: FloatRange::skew_factor(-1.5) }).with_unit(" Hz").with_step_size(1.0),
            mode_relative: FloatParam::new("Mode", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            use_peak_filter: FloatParam::new("Filter", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            use_wide_range: FloatParam::new("Range", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            filter_solo: FloatParam::new("Filter Solo", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            lookahead_enabled: FloatParam::new("Lookahead Enable", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            lookahead_ms: FloatParam::new("Lookahead", 2.0, FloatRange::Linear { min: 0.0, max: 20.0 }).with_unit(" ms").with_step_size(0.1),
            trigger_hear: FloatParam::new("Trigger Hear", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            stereo_link: FloatParam::new("Stereo Link", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01),
            stereo_mid_side: FloatParam::new("Stereo Link Mode", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            sidechain_external: FloatParam::new("Sidechain", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            vocal_mode: FloatParam::new("Processing Mode", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            input_level: FloatParam::new("Input Level", 0.0, FloatRange::Linear { min: -60.0, max: 12.0 }).with_unit(" dB").with_step_size(0.1),
            input_pan: FloatParam::new("Input Pan", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 }).with_step_size(0.01),
            output_level: FloatParam::new("Output Level", 0.0, FloatRange::Linear { min: -60.0, max: 12.0 }).with_unit(" dB").with_step_size(0.1),
            output_pan: FloatParam::new("Output Pan", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 }).with_step_size(0.01),
            bypass: FloatParam::new("Bypass", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 }),
            oversampling: FloatParam::new("Oversampling", 0.0, FloatRange::Linear { min: 0.0, max: 4.0 }).with_step_size(1.0),
            cut_width: FloatParam::new("Cut Width", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01),
            cut_depth: FloatParam::new("Cut Depth", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01),
        }
    }
}

struct Meters {
    det_bits: AtomicU32, det_max_bits: AtomicU32,
    red_bits: AtomicU32, red_max_bits: AtomicU32,
    reset_det: AtomicI32, reset_red: AtomicI32,
}

impl Default for Meters {
    fn default() -> Self {
        Self {
            det_bits: AtomicU32::new(f32_to_u32(-60.0)), det_max_bits: AtomicU32::new(f32_to_u32(-60.0)),
            red_bits: AtomicU32::new(f32_to_u32(0.0)), red_max_bits: AtomicU32::new(f32_to_u32(0.0)),
            reset_det: AtomicI32::new(0), reset_red: AtomicI32::new(0),
        }
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
    last_min_freq: f64, last_max_freq: f64,
    last_use_peak: bool, last_lookahead_ms: f64,
    last_lookahead_en: bool, last_vocal: bool,
    last_os_factor: u32,
    prev_in_l: f64, prev_in_r: f64,
    adapter: Option<StableBlockAdapter>,
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
            last_min_freq: -1.0, last_max_freq: -1.0,
            last_use_peak: false, last_lookahead_ms: -1.0,
            last_lookahead_en: false, last_vocal: true,
            last_os_factor: 1, prev_in_l: 0.0, prev_in_r: 0.0,
            adapter: None,
        }
    }
}

impl Plugin for NebulaDeEsser {
    const NAME: &'static str = "Nebula DeEsser";
    const VENDOR: &'static str = "Nebula Audio";
    const URL: &'static str = "https://nebula.audio";
    const EMAIL: &'static str = "support@nebula.audio";
    const VERSION: &'static str = "2.2.0";

    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[new_nonzero_u32(2)],
            aux_output_ports: &[],
            names: PortNames { layout: Some("Stereo + Sidechain"), main_input: Some("Input"), main_output: Some("Output"), aux_inputs: &["Sidechain"], aux_outputs: &[] },
        },
        AudioIOLayout {
            main_input_channels: NonZeroU32::new(2),
            main_output_channels: NonZeroU32::new(2),
            aux_input_ports: &[],
            aux_output_ports: &[],
            names: PortNames { layout: Some("Stereo"), main_input: Some("Input"), main_output: Some("Output"), aux_inputs: &[], aux_outputs: &[] },
        },
    ];

    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> { self.params.clone() }

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
                if midi_learn.midi_enabled.load(Ordering::Relaxed) {
                    let mappings = midi_learn.mappings.lock();
                    for (&cc, &pidx) in mappings.iter() {
                        if midi_learn.cc_dirty[(cc as usize).min(127)].swap(false, Ordering::AcqRel) {
                            let v = u32_to_f32(midi_learn.cc_values[(cc as usize).min(127)].load(Ordering::Relaxed));
                            macro_rules! scc { ($p:expr, $val:expr) => {{ setter.begin_set_parameter(&$p); setter.set_parameter(&$p, $val); setter.end_set_parameter(&$p); }}; }
                            match pidx {
                                MIDI_THRESHOLD => scc!(params.threshold, -60.0 + v * 60.0),
                                MIDI_MAX_RED => scc!(params.max_reduction, v * 40.0),
                                MIDI_STEREO_LINK => scc!(params.stereo_link, v),
                                MIDI_INPUT_LEVEL => scc!(params.input_level, -60.0 + v * 72.0),
                                MIDI_INPUT_PAN => scc!(params.input_pan, v * 2.0 - 1.0),
                                MIDI_OUTPUT_LEVEL => scc!(params.output_level, -60.0 + v * 72.0),
                                MIDI_OUTPUT_PAN => scc!(params.output_pan, v * 2.0 - 1.0),
                                MIDI_MIN_FREQ => scc!(params.min_freq, 1000.0 + v * 15000.0),
                                MIDI_MAX_FREQ => scc!(params.max_freq, 1000.0 + v * 19000.0),
                                MIDI_LOOKAHEAD => scc!(params.lookahead_ms, v * 20.0),
                                _ => {}
                            }
                        }
                    }
                }

                let gp = GuiParams {
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
                    detection_db: u32_to_f32(meters.det_bits.load(Ordering::Relaxed)),
                    detection_max_db: u32_to_f32(meters.det_max_bits.load(Ordering::Relaxed)),
                    reduction_db: u32_to_f32(meters.red_bits.load(Ordering::Relaxed)),
                    reduction_max_db: u32_to_f32(meters.red_max_bits.load(Ordering::Relaxed)),
                    input_level: params.input_level.value() as f64,
                    input_pan: params.input_pan.value() as f64,
                    output_level: params.output_level.value() as f64,
                    output_pan: params.output_pan.value() as f64,
                    bypass: params.bypass.value() > 0.5,
                    oversampling: params.oversampling.value() as u32,
                    cut_width: params.cut_width.value() as f64,
                    cut_depth: params.cut_depth.value() as f64,
                    mix: params.mix.value() as f64,
                };

                let ch = draw(ctx, &params.editor_state, gui_state, &gp);

                macro_rules! set_f { ($opt:expr, $param:expr) => { if let Some(v) = $opt { setter.begin_set_parameter(&$param); setter.set_parameter(&$param, v as f32); setter.end_set_parameter(&$param); } }; }
                macro_rules! set_b { ($opt:expr, $param:expr) => { if let Some(v) = $opt { setter.begin_set_parameter(&$param); setter.set_parameter(&$param, if v { 1.0_f32 } else { 0.0_f32 }); setter.end_set_parameter(&$param); } }; }

                set_f!(ch.threshold, params.threshold); set_f!(ch.max_reduction, params.max_reduction);
                set_f!(ch.min_freq, params.min_freq); set_f!(ch.max_freq, params.max_freq);
                set_f!(ch.stereo_link, params.stereo_link); set_f!(ch.lookahead_ms, params.lookahead_ms);
                set_b!(ch.mode_relative, params.mode_relative); set_b!(ch.use_peak_filter, params.use_peak_filter);
                set_b!(ch.use_wide_range, params.use_wide_range); set_b!(ch.filter_solo, params.filter_solo);
                set_b!(ch.lookahead_enabled, params.lookahead_enabled); set_b!(ch.trigger_hear, params.trigger_hear);
                set_b!(ch.stereo_mid_side, params.stereo_mid_side); set_b!(ch.sidechain_external, params.sidechain_external);
                set_b!(ch.vocal_mode, params.vocal_mode); set_f!(ch.input_level, params.input_level);
                set_f!(ch.input_pan, params.input_pan); set_f!(ch.output_level, params.output_level);
                set_f!(ch.output_pan, params.output_pan); set_b!(ch.bypass, params.bypass);
                if let Some(v) = ch.oversampling { setter.begin_set_parameter(&params.oversampling); setter.set_parameter(&params.oversampling, v as f32); setter.end_set_parameter(&params.oversampling); }
                set_f!(ch.cut_width, params.cut_width); set_f!(ch.cut_depth, params.cut_depth); set_f!(ch.mix, params.mix);

                if ch.detection_max_reset { meters.reset_det.store(1, Ordering::Release); }
                if ch.reduction_max_reset { meters.reset_red.store(1, Ordering::Release); }
            },
        )
    }

    fn initialize(
        &mut self,
        _layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        context: &mut impl InitContext<Self>,
    ) -> bool {
        self.sample_rate = buffer_config.sample_rate as f64;
        self.dsp = DeEsserDsp::new(self.sample_rate);
        self.os_dsp = DeEsserDsp::new(self.sample_rate);
        
        // Reporting 512 samples of latency to the DAW to shield our buffer
        let internal_block = 512;
        self.adapter = Some(StableBlockAdapter::new(internal_block, 2));
        context.set_latency_samples(internal_block as u32);

        self.analyzer.reset();
        self.last_min_freq = -1.0; self.last_max_freq = -1.0;
        self.last_lookahead_ms = -1.0; self.last_os_factor = 1;
        self.prev_in_l = 0.0; self.prev_in_r = 0.0;
        true
    }

    fn reset(&mut self) {
        self.dsp.reset(); self.os_dsp.reset();
        self.analyzer.reset();
        self.prev_in_l = 0.0; self.prev_in_r = 0.0;
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        aux: &mut AuxiliaryBuffers,
        ctx: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // 1. MIDI Handling
        while let Some(event) = ctx.next_event() {
            if let NoteEvent::MidiCC { cc, value, .. } = event {
                if !self.midi_learn.midi_enabled.load(Ordering::Relaxed) { continue; }
                let idx = (cc as usize).min(127);
                self.midi_learn.cc_values[idx].store(f32_to_u32(value), Ordering::Relaxed);
                self.midi_learn.cc_dirty[idx].store(true, Ordering::Release);
                let target = self.midi_learn.learning_target.load(Ordering::Acquire);
                if target >= 0 {
                    self.midi_learn.learning_target.store(-1, Ordering::Release);
                    self.midi_learn.mappings.lock().insert(cc, target as u8);
                }
            }
        }

        // 2. Read Parameters
        let threshold = self.params.threshold.value() as f64;
        let max_reduction = self.params.max_reduction.value() as f64;
        let min_freq = self.params.min_freq.value() as f64;
        let max_freq = self.params.max_freq.value() as f64;
        let mode_relative = self.params.mode_relative.value() > 0.5;
        let use_peak = self.params.use_peak_filter.value() > 0.5;
        let use_wide = self.params.use_wide_range.value() > 0.5;
        let filter_solo = self.params.filter_solo.value() > 0.5;
        let lookahead_en = self.params.lookahead_enabled.value() > 0.5;
        let lookahead_ms = self.params.lookahead_ms.value() as f64;
        let trigger_hear = self.params.trigger_hear.value() > 0.5;
        let stereo_link = self.params.stereo_link.value() as f64;
        let stereo_ms = self.params.stereo_mid_side.value() > 0.5;
        let sc_external = self.params.sidechain_external.value() > 0.5;
        let vocal_mode = self.params.vocal_mode.value() > 0.5;
        let oversampling = self.params.oversampling.value() as u32;
        let os_factor = match oversampling { 0=>1, 1=>2, 2=>4, 3=>6, 4=>8, _=>1 };
        let input_level_db = self.params.input_level.value() as f64;
        let input_pan = self.params.input_pan.value() as f64;
        let output_level_db = self.params.output_level.value() as f64;
        let output_pan = self.params.output_pan.value() as f64;
        let cut_width = self.params.cut_width.value() as f64;
        let cut_depth = self.params.cut_depth.value() as f64;
        let mix = self.params.mix.value() as f64;
        let bypass = self.params.bypass.value() > 0.5;

        // 3. Update DSP State
        self.dsp.update_filters(min_freq, max_freq, use_peak, cut_width, cut_depth, max_reduction);
        if os_factor != self.last_os_factor {
            self.os_dsp = DeEsserDsp::new(self.sample_rate * os_factor as f64);
            self.last_os_factor = os_factor;
        }
        self.os_dsp.update_filters(min_freq, max_freq, use_peak, cut_width, cut_depth, max_reduction);
        let eff_la = if lookahead_en { lookahead_ms } else { 0.0 };
        self.dsp.update_lookahead(eff_la);
        self.os_dsp.update_lookahead(eff_la);

        // 4. Run Shielded Processing
        if let Some(adapter) = &mut self.adapter {
            adapter.process_shielded(buffer, aux, |in_l, in_r, sc_l_buf, sc_r_buf, out_l, out_r| {
                let mut peak_det: f32 = -120.0;
                let mut peak_red: f32 = 0.0;
                let in_gain = dsp::db_to_lin(input_level_db);
                let out_gain = dsp::db_to_lin(output_level_db);
                let (in_gl, in_gr) = pan_gains(input_pan, in_gain);
                let (out_gl, out_gr) = pan_gains(output_pan, out_gain);

                for s in 0..adapter.internal_block_size {
                    let l = in_l[s] * in_gl;
                    let r = in_r[s] * in_gr;
                    let sc_l = if sc_external { Some(sc_l_buf[s]) } else { None };
                    let sc_r = if sc_external { Some(sc_r_buf[s]) } else { None };

                    let (mut ol, mut or_, det_db, red_db) = if bypass {
                        (l, r, -120.0, 0.0)
                    } else if os_factor > 1 {
                        let mut acc_l = 0.0; let mut acc_r = 0.0;
                        let mut last_d = -120.0; let mut last_rd = 0.0;
                        for k in 0..os_factor as usize {
                            let t = k as f64 / os_factor as f64;
                            let ul = self.prev_in_l + t * (l - self.prev_in_l);
                            let ur = self.prev_in_r + t * (r - self.prev_in_r);
                            let (o_l, o_r, d, rd) = self.os_dsp.process_sample(ul, ur, sc_l, sc_r, threshold, max_reduction, mode_relative, use_peak, use_wide, stereo_link, stereo_ms, lookahead_en, trigger_hear, filter_solo, false);
                            acc_l += o_l; acc_r += o_r; last_d = d; last_rd = rd;
                        }
                        (acc_l / os_factor as f64, acc_r / os_factor as f64, last_d, last_rd)
                    } else {
                        self.dsp.process_sample(l, r, sc_l, sc_r, threshold, max_reduction, mode_relative, use_peak, use_wide, stereo_link, stereo_ms, lookahead_en, trigger_hear, filter_solo, false)
                    };

                    self.prev_in_l = l; self.prev_in_r = r;
                    ol *= out_gl; or_ *= out_gr;
                    if mix < 1.0 {
                        ol = ol * mix + l * out_gl * (1.0 - mix);
                        or_ = or_ * mix + r * out_gr * (1.0 - mix);
                    }

                    out_l[s] = ol; out_r[s] = or_;
                    self.analyzer.push((ol + or_) * 0.5);
                    if det_db as f32 > peak_det { peak_det = det_db as f32; }
                    if red_db as f32 < peak_red { peak_red = red_db as f32; }
                }

                // Update meters from within the block
                self.meters.det_bits.store(f32_to_u32(peak_det), Ordering::Relaxed);
                self.meters.red_bits.store(f32_to_u32(peak_red), Ordering::Relaxed);
            });
        }

        ProcessStatus::Normal
    }
}

fn pan_gains(pan: f64, gain: f64) -> (f64, f64) {
    let p = pan.clamp(-1.0, 1.0);
    let pan_l = if p <= 0.0 { 1.0 } else { 1.0 - p };
    let pan_r = if p >= 0.0 { 1.0 } else { 1.0 + p };
    (gain * pan_l, gain * pan_r)
}

impl ClapPlugin for NebulaDeEsser {
    const CLAP_ID: &'static str = "audio.nebula.deesser";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Hyper-optimized 64-bit CLAP de-esser v2.2 — alien synthwave GUI");
    const CLAP_MANUAL_URL: Option<&'static str> = Some("https://nebula.audio/manual");
    const CLAP_SUPPORT_URL: Option<&'static str> = Some("https://nebula.audio/support");
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Mono, ClapFeature::Utility];
}

nih_export_clap!(NebulaDeEsser);
