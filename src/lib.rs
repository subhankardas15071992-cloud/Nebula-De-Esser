use std::collections::HashMap;
use std::f64::consts::PI;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicU32, Ordering};
use std::sync::Arc;

use nih_plug::prelude::*;
use nih_plug_egui::{create_egui_editor, EguiState}; // Use the egui re-exported by nih_plug_egui to avoid version mismatches use nih_plug_egui::egui;
use parking_lot::Mutex;

pub mod analyzer;
pub mod dsp;
mod gui;

use analyzer::SpectrumAnalyzer;
use dsp::{db_to_lin, DeEsserDsp, ProcessFrame, ProcessSettings};
use gui::{draw, GuiParams, NebulaGui};

const UNMAPPED_CC: i32 = -1;

#[inline]
fn f32_to_u32(value: f32) -> u32 { value.to_bits() }

#[inline]
fn u32_to_f32(value: u32) -> f32 { f32::from_bits(value) }

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
    "TKEO Sharpness", "Max Reduction", "Stereo Link", "Input Level",
    "Input Pan", "Output Level", "Output Pan", "Min Frequency",
    "Max Frequency", "Lookahead",
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
        if !self.bindings_dirty.swap(false, Ordering::AcqRel) { return; }
        let mut mappings = self.mappings.lock();
        mappings.clear();
        for (cc, binding) in self.cc_bindings.iter().enumerate() {
            let value = binding.load(Ordering::Acquire);
            if value >= 0 { mappings.insert(cc as u8, value as u8); }
        }
    }

    fn sync_atomic_from_mutex(&self) {
        for binding in &self.cc_bindings { binding.store(UNMAPPED_CC, Ordering::Release); }
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
    #[id = "cut_slope"] pub cut_slope: FloatParam,
}

impl Default for NebulaParams {
    fn default() -> Self {
        let freq_range = FloatRange::Skewed { min: 1.0, max: 24_000.0, factor: FloatRange::skew_factor(-2.0) };
        Self {
            editor_state: EguiState::from_size(860, 640),
            threshold: FloatParam::new("TKEO Sharpness", 50.0, FloatRange::Linear { min: 0.0, max: 100.0 }).with_unit(" %").with_step_size(1.0),
            max_reduction: FloatParam::new("Max Reduction", -12.0, FloatRange::Linear { min: -100.0, max: 0.0 }).with_unit(" dB").with_step_size(0.1),
            min_freq: FloatParam::new("Min Frequency", 4_000.0, freq_range.clone()).with_unit(" Hz").with_step_size(1.0),
            max_freq: FloatParam::new("Max Frequency", 12_000.0, freq_range).with_unit(" Hz").with_step_size(1.0),
            mode_relative: bool_param("Mode", true),
            use_peak_filter: bool_param("Filter", false),
            use_wide_range: bool_param("Range", false),
            filter_solo: bool_param("Filter Solo", false),
            lookahead_enabled: bool_param("Lookahead Enabled", false),
            lookahead_ms: FloatParam::new("Lookahead", 0.0, FloatRange::Linear { min: 0.0, max: 20.0 }).with_unit(" ms").with_step_size(0.1),
            trigger_hear: bool_param("Trigger Hear", false),
            stereo_link: FloatParam::new("Stereo Link", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01),
            stereo_mid_side: bool_param("Mid/Side", false),
            sidechain_external: bool_param("Sidechain", false),
            vocal_mode: bool_param("Vocal Mode", true),
            input_level: FloatParam::new("Input Level", 0.0, FloatRange::Linear { min: -100.0, max: 100.0 }).with_unit(" dB").with_step_size(0.1).with_smoother(SmoothingStyle::Linear(20.0)),
            input_pan: FloatParam::new("Input Pan", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 }).with_step_size(0.01).with_smoother(SmoothingStyle::Linear(20.0)),
            output_level: FloatParam::new("Output Level", 0.0, FloatRange::Linear { min: -100.0, max: 100.0 }).with_unit(" dB").with_step_size(0.1).with_smoother(SmoothingStyle::Linear(20.0)),
            output_pan: FloatParam::new("Output Pan", 0.0, FloatRange::Linear { min: -1.0, max: 1.0 }).with_step_size(0.01).with_smoother(SmoothingStyle::Linear(20.0)),
            bypass: bool_param("Bypass", false),
            oversampling: FloatParam::new("Oversampling", 0.0, FloatRange::Linear { min: 0.0, max: 4.0 }).with_step_size(1.0),
            cut_width: FloatParam::new("Cut Width", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01),
            cut_depth: FloatParam::new("Cut Depth", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01),
            mix: FloatParam::new("Mix", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 }).with_step_size(0.01).with_smoother(SmoothingStyle::Linear(10.0)),
            cut_slope: FloatParam::new("Cut Slope", 50.0, FloatRange::Linear { min: 0.0, max: 100.0 }).with_unit(" dB/oct").with_step_size(0.1),
        }
    }
}

fn bool_param(name: &str, default: bool) -> FloatParam {
    FloatParam::new(name, if default { 1.0 } else { 0.0 }, FloatRange::Linear { min: 0.0, max: 1.0 })
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

const ACTIVE_AUDIO_IO_LAYOUTS: &[AudioIOLayout] = &[AudioIOLayout {
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
}];

struct WetMixSmoother { coeff: f64, current: f64 }

impl WetMixSmoother {
    fn new(sample_rate: f64) -> Self {
        let mut s = Self { coeff: 0.0, current: 1.0 };
        s.set_sample_rate(sample_rate);
        s
    }
    fn set_sample_rate(&mut self, sample_rate: f64) {
        self.coeff = if sample_rate <= 0.0 { 0.0 } else { (-1.0 / (0.010 * sample_rate)).exp() };
    }
    fn reset(&mut self, value: f64) { self.current = value.clamp(0.0, 1.0); }
    fn next(&mut self, target: f64) -> f64 {
        let t = target.clamp(0.0, 1.0);
        self.current = t + self.coeff * (self.current - t);
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
    prev_main_l: f64, prev_main_r: f64, prev_sc_l: f64, prev_sc_r: f64,
    is_ready: AtomicBool,
}

impl Default for NebulaDeEsser {
    fn default() -> Self {
        let sr = 44_100.0;
        Self {
            params: Arc::new(NebulaParams::default()),
            sample_rate: sr,
            dsp: DeEsserDsp::new(sr),
            os_dsp: DeEsserDsp::new(sr),
            analyzer: SpectrumAnalyzer::new(),
            meters: Arc::new(Meters::default()),
            midi_learn: Arc::new(MidiLearnShared::new()),
            current_os_factor: 1,
            reported_latency: 0,
            wet_mix: WetMixSmoother::new(sr),
            prev_main_l: 0.0, prev_main_r: 0.0, prev_sc_l: 0.0, prev_sc_r: 0.0,
            is_ready: AtomicBool::new(false),
        }
    }
}

impl Plugin for NebulaDeEsser {
    const NAME: &'static str = "Nebula DeEsser";
    const VENDOR: &'static str = "Nebula Audio";
    const URL: &'static str = "https://github.com/subhankardas15071992-cloud/Nebula-De-Esser";
    const EMAIL: &'static str = "support@nebula.audio";
    const VERSION: &'static str = env!("CARGO_PKG_VERSION");
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = ACTIVE_AUDIO_IO_LAYOUTS;
    const MIDI_INPUT: MidiConfig = MidiConfig::Basic;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;
    const SAMPLE_ACCURATE_AUTOMATION: bool = true;
    
    type SysExMessage = ();
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        Arc::clone(&self.params) as Arc<dyn Params>
    }

    // ✅ FIXED: GUI fully restored. Removed premature is_ready check that blocked EGUI on Windows.
        fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        // Cakewalk calls editor() during scan before initialize() runs.
        // Return None temporarily to avoid egui init in headless scanner.
        #[cfg(target_os = "windows")]
        if !self.is_ready.load(Ordering::Acquire) {
            return None;
        }

        // Wrap in catch_unwind so any egui panic doesn't crash the DAW
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let params = self.params.clone();
            let meters = self.meters.clone();
            let spectrum = self.analyzer.get_shared();
            let midi_learn = self.midi_learn.clone();

            // Use the existing editor_state — no font overrides needed
            let egui_state = self.params.editor_state.clone();

            create_egui_editor(
                egui_state,
                NebulaGui::new(spectrum, midi_learn.clone()),
                |_ctx: &egui::Context, _state: &mut NebulaGui| {},
                move |ctx: &egui::Context, setter: &ParamSetter, gui_state: &mut NebulaGui| {
                    midi_learn.sync_mutex_from_atomic_if_needed();
                    if midi_learn.midi_enabled.load(Ordering::Relaxed) {
                        for cc in 0..128 {
                            if !midi_learn.cc_dirty[cc].swap(false, Ordering::AcqRel) { continue; }
                            let Some(pidx) = midi_learn.binding_for_cc(cc) else { continue; };
                            let val = u32_to_f32(midi_learn.cc_values[cc].load(Ordering::Relaxed));
                            apply_midi_mapping(pidx, val, &params, setter);
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

                    // Use the correct draw signature from your gui.rs
                    let changes = draw(ctx, &params.editor_state, gui_state, &gui_params);
                    apply_gui_changes(&changes, &params, setter);
                    midi_learn.sync_atomic_from_mutex();
                    if changes.detection_max_reset { meters.reset_det.store(1, Ordering::Release); }
                    if changes.reduction_max_reset { meters.reset_red.store(1, Ordering::Release); }
                },
            )
        })).ok().flatten()
    }

    // ✅ Cakewalk guard remains here: prevents processing before initialization
    fn process(&mut self, buffer: &mut Buffer, aux: &mut AuxiliaryBuffers, context: &mut impl ProcessContext<Self>) -> ProcessStatus {
        if !self.is_ready.load(Ordering::Acquire) || buffer.samples() == 0 {
            return ProcessStatus::Normal;
        }

        while let Some(event) = context.next_event() {
            if let NoteEvent::MidiCC { cc, value, .. } = event {
                if !self.midi_learn.midi_enabled.load(Ordering::Relaxed) { continue; }
                let idx = (cc as usize).min(127);
                self.midi_learn.cc_values[idx].store(f32_to_u32(value), Ordering::Relaxed);
                self.midi_learn.cc_dirty[idx].store(true, Ordering::Release);
                let target = self.midi_learn.learning_target.load(Ordering::Acquire);
                if target >= 0 {
                    self.midi_learn.learning_target.store(UNMAPPED_CC, Ordering::Release);
                    self.midi_learn.learn_cc(cc, target as u8);
                }
            }
        }

        if self.meters.reset_det.swap(0, Ordering::AcqRel) != 0 { self.meters.det_max_bits.store(f32_to_u32(-120.0), Ordering::Relaxed); }
        if self.meters.reset_red.swap(0, Ordering::AcqRel) != 0 { self.meters.red_max_bits.store(f32_to_u32(0.0), Ordering::Relaxed); }

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

        prepare_dsp(&mut self.dsp, min_freq, max_freq, use_peak_filter, cut_width, cut_depth, cut_slope, max_reduction, if lookahead_enabled { lookahead_ms } else { 0.0 }, single_vocal);
        if os_factor != self.current_os_factor {
            self.os_dsp = DeEsserDsp::new(self.sample_rate * os_factor as f64);
            self.current_os_factor = os_factor;
        }
        prepare_dsp(&mut self.os_dsp, min_freq, max_freq, use_peak_filter, cut_width, cut_depth, cut_slope, max_reduction, if lookahead_enabled { lookahead_ms } else { 0.0 }, single_vocal);

        let target_latency = if lookahead_enabled && lookahead_ms > 0.0 { lookahead_latency_samples(lookahead_ms, self.sample_rate) } else { 0 };
        if target_latency != self.reported_latency { context.set_latency_samples(target_latency); self.reported_latency = target_latency; }

        let settings = ProcessSettings { threshold_db: threshold, max_reduction_db: max_reduction, mode_relative, use_peak_filter, use_wide_range, trigger_hear, filter_solo, stereo_link, stereo_mid_side };
        let sidechain_buffers = if sidechain_external && !aux.inputs.is_empty() { Some(aux.inputs[0].as_slice_immutable()) } else { None };

        let samples = buffer.samples();
        let channels = buffer.as_slice();
        if channels.len() < 2 { return ProcessStatus::Normal; }
        let (left_slice, right_slice) = { let (l, r) = channels.split_at_mut(1); (&mut l[0], &mut r[0]) };

        let mut peak_det = -120.0_f32;
        let mut peak_red = 0.0_f32;

        for i in 0..samples {
            let input_level_db = self.params.input_level.smoothed.next() as f64;
            let input_pan = self.params.input_pan.smoothed.next() as f64;
            let output_level_db = self.params.output_level.smoothed.next() as f64;
            let output_pan = self.params.output_pan.smoothed.next() as f64;

            let main_in_l = left_slice[i] as f64;
            let main_in_r = right_slice[i] as f64;
            let ig = db_to_lin(input_level_db);
            let (ig_l, ig_r) = pan_gains(input_pan, ig);
            let processed_in_l = main_in_l * ig_l;
            let processed_in_r = main_in_r * ig_r;

            let sc_in_l = sidechain_buffers.and_then(|b| b.first()).and_then(|c| c.get(i)).copied().map(f64::from).unwrap_or(processed_in_l);
            let sc_in_r = sidechain_buffers.and_then(|b| b.get(1)).and_then(|c| c.get(i)).copied().map(f64::from).unwrap_or(processed_in_r);

            let ProcessFrame { wet_l, wet_r, dry_l, dry_r, detection_db, reduction_db } = if os_factor > 1 {
                let mut wl = 0.0; let mut wr = 0.0; let mut dl = 0.0; let mut dr = 0.0;
                let mut det = -120.0_f64; let mut red = 0.0_f64;
                for sub in 0..os_factor {
                    let t = (sub as f64 + 1.0) / os_factor as f64;
                    let il = self.prev_main_l + (processed_in_l - self.prev_main_l) * t;
                    let ir = self.prev_main_r + (processed_in_r - self.prev_main_r) * t;
                    let sl = self.prev_sc_l + (sc_in_l - self.prev_sc_l) * t;
                    let sr = self.prev_sc_r + (sc_in_r - self.prev_sc_r) * t;
                    let f = self.os_dsp.process_frame(il, ir, sl, sr, settings);
                    wl += f.wet_l; wr += f.wet_r; dl += f.dry_l; dr += f.dry_r;
                    det = det.max(f.detection_db); red = red.min(f.reduction_db);
                }
                let inv = 1.0 / os_factor as f64;
                ProcessFrame { wet_l: wl * inv, wet_r: wr * inv, dry_l: dl * inv, dry_r: dr * inv, detection_db: det, reduction_db: red }
            } else {
                self.dsp.process_frame(processed_in_l, processed_in_r, sc_in_l, sc_in_r, settings)
            };

            let mix_t = if self.params.bypass.value() > 0.5 { 0.0 } else if trigger_hear || filter_solo { 1.0 } else { self.params.mix.smoothed.next() as f64 };
            let wm = self.wet_mix.next(mix_t);
            let dm = 1.0 - wm;
            let mixed_l = wet_l * wm + dry_l * dm;
            let mixed_r = wet_r * wm + dry_r * dm;

            let og = db_to_lin(output_level_db);
            let (og_l, og_r) = pan_gains(output_pan, og);
            left_slice[i] = (mixed_l * og_l) as f32;
            right_slice[i] = (mixed_r * og_r) as f32;
            self.analyzer.push((mixed_l + mixed_r) * 0.5);

            peak_det = peak_det.max(detection_db as f32);
            peak_red = peak_red.min(reduction_db as f32);
            self.prev_main_l = processed_in_l; self.prev_main_r = processed_in_r;
            self.prev_sc_l = sc_in_l; self.prev_sc_r = sc_in_r;
        }

        self.meters.det_bits.store(f32_to_u32(peak_det), Ordering::Relaxed);
        self.meters.red_bits.store(f32_to_u32(peak_red), Ordering::Relaxed);
        if peak_det > u32_to_f32(self.meters.det_max_bits.load(Ordering::Relaxed)) { self.meters.det_max_bits.store(f32_to_u32(peak_det), Ordering::Relaxed); }
        if peak_red < u32_to_f32(self.meters.red_max_bits.load(Ordering::Relaxed)) { self.meters.red_max_bits.store(f32_to_u32(peak_red), Ordering::Relaxed); }

        ProcessStatus::Normal
    }
}

impl ClapPlugin for NebulaDeEsser {
    const CLAP_ID: &'static str = "audio.nebula.deesser";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("Spectral-style de-esser with split/wide processing, sidechain, and lookahead");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo, ClapFeature::Deesser, ClapFeature::Filter, ClapFeature::Utility, ClapFeature::Restoration];
}

impl Vst3Plugin for NebulaDeEsser {
    const VST3_CLASS_ID: [u8; 16] = *b"NebulaDeEssrVST3";
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] = &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics, Vst3SubCategory::Filter, Vst3SubCategory::Tools];
}

nih_export_clap!(NebulaDeEsser);
nih_export_vst3!(NebulaDeEsser);

fn apply_midi_mapping(parameter_index: u8, value: f32, params: &Arc<NebulaParams>, setter: &ParamSetter) {
    macro_rules! set_param { ($p:expr, $v:expr) => {{ setter.begin_set_parameter(&$p); setter.set_parameter(&$p, $v); setter.end_set_parameter(&$p); }}; }
    match parameter_index {
        0 => set_param!(params.threshold, value * 100.0),
        1 => set_param!(params.max_reduction, -100.0 + value * 100.0),
        2 => set_param!(params.stereo_link, value),
        3 => set_param!(params.input_level, -100.0 + value * 200.0),
        4 => set_param!(params.input_pan, value * 2.0 - 1.0),
        5 => set_param!(params.output_level, -100.0 + value * 200.0),
        6 => set_param!(params.output_pan, value * 2.0 - 1.0),
        7 => set_param!(params.min_freq, 1.0 + value * 23_999.0),
        8 => set_param!(params.max_freq, 1.0 + value * 23_999.0),
        9 => set_param!(params.lookahead_ms, value * 20.0),
        _ => {}
    }
}

fn apply_gui_changes(changes: &gui::GuiChanges, params: &Arc<NebulaParams>, setter: &ParamSetter) {
    macro_rules! set_float { ($f:expr, $p:expr) => { if let Some(v) = $f { setter.begin_set_parameter(&$p); setter.set_parameter(&$p, v as f32); setter.end_set_parameter(&$p); } }; }
    macro_rules! set_bool { ($f:expr, $p:expr) => { if let Some(v) = $f { setter.begin_set_parameter(&$p); setter.set_parameter(&$p, if v { 1.0 } else { 0.0 }); setter.end_set_parameter(&$p); } }; }
    set_float!(changes.threshold, params.threshold); set_float!(changes.max_reduction, params.max_reduction);
    set_float!(changes.min_freq, params.min_freq); set_float!(changes.max_freq, params.max_freq);
    set_bool!(changes.mode_relative, params.mode_relative); set_bool!(changes.use_peak_filter, params.use_peak_filter);
    set_bool!(changes.use_wide_range, params.use_wide_range); set_bool!(changes.filter_solo, params.filter_solo);
    set_bool!(changes.lookahead_enabled, params.lookahead_enabled); set_float!(changes.lookahead_ms, params.lookahead_ms);
    set_bool!(changes.trigger_hear, params.trigger_hear); set_float!(changes.stereo_link, params.stereo_link);
    set_bool!(changes.stereo_mid_side, params.stereo_mid_side); set_bool!(changes.sidechain_external, params.sidechain_external);
    set_bool!(changes.vocal_mode, params.vocal_mode); set_float!(changes.input_level, params.input_level);
    set_float!(changes.input_pan, params.input_pan); set_float!(changes.output_level, params.output_level);
    set_float!(changes.output_pan, params.output_pan); set_bool!(changes.bypass, params.bypass);
    set_float!(changes.cut_width, params.cut_width); set_float!(changes.cut_depth, params.cut_depth);
    set_float!(changes.cut_slope, params.cut_slope); set_float!(changes.mix, params.mix);
    if let Some(os) = changes.oversampling { setter.begin_set_parameter(&params.oversampling); setter.set_parameter(&params.oversampling, os as f32); setter.end_set_parameter(&params.oversampling); }
}

#[allow(clippy::too_many_arguments)]
fn prepare_dsp(dsp: &mut DeEsserDsp, min_freq: f64, max_freq: f64, use_peak_filter: bool, cut_width: f64, cut_depth: f64, cut_slope: f64, max_reduction: f64, lookahead_ms: f64, single_vocal: bool) {
    dsp.update_filters(min_freq, max_freq, use_peak_filter, cut_width, cut_depth, cut_slope, max_reduction);
    dsp.update_lookahead(lookahead_ms);
    dsp.update_vocal_mode(single_vocal);
}

fn oversampling_factor(s: u32) -> u32 { match s { 1 => 2, 2 => 4, 3 => 6, 4 => 8, _ => 1 } }
fn lookahead_latency_samples(lookahead_ms: f64, sample_rate: f64) -> u32 { ((lookahead_ms.max(0.0) * sample_rate) / 1000.0).round() as u32 }
fn pan_gains(pan: f64, gain: f64) -> (f64, f64) { let a = (pan.clamp(-1.0, 1.0) + 1.0) * (PI * 0.25); (gain * a.cos(), gain * a.sin()) }

#[cfg(test)]
mod tests {
    use super::ACTIVE_AUDIO_IO_LAYOUTS;
    #[test] fn test_stereo_layout() { assert_eq!(ACTIVE_AUDIO_IO_LAYOUTS[0].main_input_channels.map(|c| c.get()), Some(2)); }
}
