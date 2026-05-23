#![allow(unexpected_cfgs)]

use std::any::Any;
use std::ffi::c_void;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, OnceLock};

use cocoa::appkit::{NSButton, NSTextField, NSView, NSViewHeightSizable, NSViewWidthSizable};
use cocoa::base::{id, nil, NO, YES};
use cocoa::foundation::{NSPoint, NSRect, NSSize, NSString};
use nih_plug::prelude::{Editor, FloatParam, GuiContext, ParamSetter, ParentWindowHandle};
use objc::declare::ClassDecl;
use objc::runtime::{Class, Object, Sel};
use objc::{class, msg_send, sel, sel_impl, Encode, Encoding};
use parking_lot::Mutex;

use super::analyzer::SpectrumData;
use super::{
    apply_midi_mapping, clamp_max_frequency, clamp_min_frequency, u32_to_f32, Meters,
    MidiLearnShared, NebulaParams, PersistentStore, StoredEditorSize,
};

const BASE_W: f32 = 860.0;
const BASE_H: f32 = 640.0;
const MIN_WINDOW_SCALE: f32 = 0.65;
const MAX_WINDOW_SCALE: f32 = 3.0;
const TIMER_INTERVAL_SECONDS: f64 = 1.0 / 30.0;

const MTL_PIXEL_FORMAT_BGRA8_UNORM: u64 = 80;
const MTL_LOAD_ACTION_CLEAR: u64 = 2;
const MTL_STORE_ACTION_STORE: u64 = 1;

const TAG_THRESHOLD: isize = 1;
const TAG_MAX_REDUCTION: isize = 2;
const TAG_MIN_FREQ: isize = 3;
const TAG_MAX_FREQ: isize = 4;
const TAG_STEREO_LINK: isize = 5;
const TAG_LOOKAHEAD_MS: isize = 6;
const TAG_INPUT_LEVEL: isize = 7;
const TAG_INPUT_PAN: isize = 8;
const TAG_OUTPUT_LEVEL: isize = 9;
const TAG_OUTPUT_PAN: isize = 10;
const TAG_CUT_WIDTH: isize = 11;
const TAG_CUT_DEPTH: isize = 12;
const TAG_CUT_SLOPE: isize = 13;
const TAG_MIX: isize = 14;
const TAG_BYPASS: isize = 20;
const TAG_MODE_RELATIVE: isize = 21;
const TAG_WIDE_RANGE: isize = 22;
const TAG_FILTER_SOLO: isize = 23;
const TAG_LOOKAHEAD_ENABLED: isize = 24;
const TAG_TRIGGER_HEAR: isize = 25;
const TAG_VOCAL_MODE: isize = 26;
const TAG_BASIS_MODE: isize = 30;
const TAG_STEREO_MODE: isize = 31;
const TAG_SIDECHAIN_MODE: isize = 32;
const TAG_OVERSAMPLING: isize = 33;
const TAG_GUI_SCALE: isize = 40;

#[repr(C)]
#[derive(Clone, Copy)]
struct MTLClearColor {
    red: f64,
    green: f64,
    blue: f64,
    alpha: f64,
}

unsafe impl Encode for MTLClearColor {
    fn encode() -> Encoding {
        unsafe { Encoding::from_str("{MTLClearColor=dddd}") }
    }
}

#[link(name = "Metal", kind = "framework")]
extern "C" {
    fn MTLCreateSystemDefaultDevice() -> id;
}

#[link(name = "QuartzCore", kind = "framework")]
extern "C" {}

pub(super) fn create_editor(
    params: Arc<NebulaParams>,
    _spectrum: Arc<Mutex<SpectrumData>>,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    storage: Arc<PersistentStore>,
) -> Option<Box<dyn Editor>> {
    let editor_size = storage.editor_size();
    Some(Box::new(MetalEditor {
        params,
        meters,
        midi_learn,
        storage,
        scale_bits: AtomicU32::new(1.0_f32.to_bits()),
        window_width_bits: Arc::new(AtomicU32::new(editor_size.width.to_bits())),
        window_height_bits: Arc::new(AtomicU32::new(editor_size.height.to_bits())),
    }))
}

struct MetalEditor {
    params: Arc<NebulaParams>,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    storage: Arc<PersistentStore>,
    scale_bits: AtomicU32,
    window_width_bits: Arc<AtomicU32>,
    window_height_bits: Arc<AtomicU32>,
}

impl Editor for MetalEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn Any + Send> {
        let ParentWindowHandle::AppKitNsView(parent_view) = parent else {
            return Box::new(());
        };
        if parent_view.is_null() {
            return Box::new(());
        }

        let host_scale = f32::from_bits(self.scale_bits.load(Ordering::Acquire)).clamp(0.5, 3.0);
        let user_w = f32::from_bits(self.window_width_bits.load(Ordering::Acquire))
            .clamp(BASE_W * MIN_WINDOW_SCALE, BASE_W * MAX_WINDOW_SCALE);
        let user_h = f32::from_bits(self.window_height_bits.load(Ordering::Acquire))
            .clamp(BASE_H * MIN_WINDOW_SCALE, BASE_H * MAX_WINDOW_SCALE);
        let width = (user_w * host_scale).round().max(1.0);
        let height = (user_h * host_scale).round().max(1.0);

        unsafe {
            let device = MTLCreateSystemDefaultDevice();
            if device == nil {
                return Box::new(());
            }
            let command_queue: id = msg_send![device, newCommandQueue];
            if command_queue == nil {
                let _: () = msg_send![device, release];
                return Box::new(());
            }

            let frame = NSRect::new(
                NSPoint::new(0.0, 0.0),
                NSSize::new(width as f64, height as f64),
            );
            let view = NSView::initWithFrame_(NSView::alloc(nil), frame);
            view.setAutoresizingMask_(NSViewWidthSizable | NSViewHeightSizable);
            let _: () = msg_send![view, setBounds: base_bounds()];
            view.setWantsLayer(YES);

            let layer: id = msg_send![class!(CAMetalLayer), layer];
            let _: () = msg_send![layer, setDevice: device];
            let _: () = msg_send![layer, setPixelFormat: MTL_PIXEL_FORMAT_BGRA8_UNORM];
            let _: () = msg_send![layer, setFramebufferOnly: YES];
            let _: id = msg_send![layer, retain];
            view.setLayer(layer);

            let parent_view = parent_view as id;
            parent_view.addSubview_(view);

            let target: id = msg_send![target_class(), new];
            let mut state = Box::new(MetalWindowState {
                params: self.params.clone(),
                meters: self.meters.clone(),
                midi_learn: self.midi_learn.clone(),
                storage: self.storage.clone(),
                context,
                view,
                layer,
                device,
                command_queue,
                target,
                timer: nil,
                controls: Vec::new(),
                det_meter_label: nil,
                red_meter_label: nil,
                window_width_bits: self.window_width_bits.clone(),
                window_height_bits: self.window_height_bits.clone(),
            });

            let state_ptr = state.as_mut() as *mut MetalWindowState;
            (*target).set_ivar("state", state_ptr.cast::<c_void>());
            state.build_controls();
            state.sync_from_params();
            state.render();

            let timer: id = msg_send![
                class!(NSTimer),
                scheduledTimerWithTimeInterval: TIMER_INTERVAL_SECONDS
                target: target
                selector: sel!(timerFired:)
                userInfo: nil
                repeats: YES
            ];
            if timer != nil {
                let _: id = msg_send![timer, retain];
                state.timer = timer;
            }

            Box::new(MetalWindowHandle {
                state: Box::into_raw(state),
            })
        }
    }

    fn size(&self) -> (u32, u32) {
        let host_scale = f32::from_bits(self.scale_bits.load(Ordering::Acquire)).clamp(0.5, 3.0);
        let user_w = f32::from_bits(self.window_width_bits.load(Ordering::Acquire))
            .clamp(BASE_W * MIN_WINDOW_SCALE, BASE_W * MAX_WINDOW_SCALE);
        let user_h = f32::from_bits(self.window_height_bits.load(Ordering::Acquire))
            .clamp(BASE_H * MIN_WINDOW_SCALE, BASE_H * MAX_WINDOW_SCALE);
        (
            (user_w * host_scale).round() as u32,
            (user_h * host_scale).round() as u32,
        )
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        self.scale_bits
            .store(factor.max(0.5).to_bits(), Ordering::Release);
        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {}

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {}

    fn param_values_changed(&self) {}
}

struct MetalWindowHandle {
    state: *mut MetalWindowState,
}

unsafe impl Send for MetalWindowHandle {}

impl Drop for MetalWindowHandle {
    fn drop(&mut self) {
        if !self.state.is_null() {
            unsafe {
                drop(Box::from_raw(self.state));
            }
            self.state = std::ptr::null_mut();
        }
    }
}

struct MetalWindowState {
    params: Arc<NebulaParams>,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    storage: Arc<PersistentStore>,
    context: Arc<dyn GuiContext>,
    view: id,
    layer: id,
    device: id,
    command_queue: id,
    target: id,
    timer: id,
    controls: Vec<ControlBinding>,
    det_meter_label: id,
    red_meter_label: id,
    window_width_bits: Arc<AtomicU32>,
    window_height_bits: Arc<AtomicU32>,
}

impl Drop for MetalWindowState {
    fn drop(&mut self) {
        unsafe {
            if self.timer != nil {
                let _: () = msg_send![self.timer, invalidate];
                let _: () = msg_send![self.timer, release];
                self.timer = nil;
            }
            if self.view != nil {
                self.view.removeFromSuperview();
                let _: () = msg_send![self.view, release];
                self.view = nil;
            }
            if self.layer != nil {
                let _: () = msg_send![self.layer, release];
                self.layer = nil;
            }
            if self.command_queue != nil {
                let _: () = msg_send![self.command_queue, release];
                self.command_queue = nil;
            }
            if self.device != nil {
                let _: () = msg_send![self.device, release];
                self.device = nil;
            }
            if self.target != nil {
                let _: () = msg_send![self.target, release];
                self.target = nil;
            }
        }
    }
}

#[derive(Clone, Copy)]
struct ControlBinding {
    tag: isize,
    control: id,
    value_label: id,
}

impl MetalWindowState {
    unsafe fn build_controls(&mut self) {
        self.add_panel(18.0, 76.0, 268.0, 290.0);
        self.add_panel(298.0, 76.0, 268.0, 290.0);
        self.add_panel(578.0, 76.0, 264.0, 290.0);
        self.add_panel(18.0, 382.0, 548.0, 142.0);
        self.add_panel(578.0, 382.0, 264.0, 142.0);

        let title = format!(
            "Nebula De-Esser   |   v{}.{}",
            env!("CARGO_PKG_VERSION_MAJOR"),
            env!("CARGO_PKG_VERSION_MINOR")
        );
        self.add_label(24.0, 18.0, 390.0, 24.0, &title, 17.0, true);
        self.add_label(
            24.0,
            43.0,
            540.0,
            18.0,
            "Sibilance Processor   |   64-bit",
            11.0,
            false,
        );

        self.add_button(TAG_BYPASS, 690.0, 22.0, 140.0, 28.0, "Bypass");

        self.add_section(34.0, 92.0, "Detection");
        self.add_knob(TAG_THRESHOLD, 34.0, 122.0, 66.0, "TKEO Sharp", 0.0, 100.0);
        self.add_knob(TAG_MIN_FREQ, 112.0, 122.0, 66.0, "Min Freq", 1.0, 24_000.0);
        self.add_knob(TAG_MAX_FREQ, 190.0, 122.0, 66.0, "Max Freq", 1.0, 24_000.0);
        self.add_popup(
            TAG_BASIS_MODE,
            34.0,
            232.0,
            104.0,
            "Basis",
            &["Odd", "Even", "Both"],
        );
        self.add_button(TAG_MODE_RELATIVE, 150.0, 254.0, 104.0, 24.0, "Relative");
        self.add_button(TAG_WIDE_RANGE, 34.0, 304.0, 104.0, 24.0, "Wide");
        self.add_button(TAG_FILTER_SOLO, 150.0, 304.0, 104.0, 24.0, "Filter Solo");
        self.det_meter_label = self.add_label(34.0, 336.0, 220.0, 16.0, "Detect --", 10.0, false);

        self.add_section(314.0, 92.0, "Reduction");
        self.add_knob(
            TAG_MAX_REDUCTION,
            314.0,
            122.0,
            66.0,
            "Max Reduction",
            -100.0,
            0.0,
        );
        self.add_knob(TAG_CUT_WIDTH, 392.0, 122.0, 66.0, "Cut Width", 0.0, 1.0);
        self.add_knob(TAG_CUT_DEPTH, 470.0, 122.0, 66.0, "Cut Depth", 0.0, 1.0);
        self.add_knob(TAG_CUT_SLOPE, 354.0, 232.0, 66.0, "Cut Slope", 0.0, 100.0);
        self.add_knob(TAG_MIX, 432.0, 232.0, 66.0, "Mix", 0.0, 1.0);
        self.red_meter_label =
            self.add_label(314.0, 336.0, 220.0, 16.0, "Reduction --", 10.0, false);

        self.add_section(596.0, 92.0, "Stereo + Trigger");
        self.add_knob(TAG_STEREO_LINK, 596.0, 122.0, 66.0, "Stereo Link", 0.0, 1.0);
        self.add_knob(TAG_LOOKAHEAD_MS, 674.0, 122.0, 66.0, "Lookahead", 0.0, 20.0);
        self.add_popup(
            TAG_STEREO_MODE,
            596.0,
            232.0,
            104.0,
            "Stereo Mode",
            &["Stereo", "Mid", "Side"],
        );
        self.add_popup(
            TAG_SIDECHAIN_MODE,
            714.0,
            232.0,
            104.0,
            "Sidechain",
            &["Internal", "External", "MIDI"],
        );
        self.add_button(TAG_TRIGGER_HEAR, 596.0, 304.0, 104.0, 24.0, "Trigger Hear");
        self.add_button(TAG_VOCAL_MODE, 714.0, 304.0, 104.0, 24.0, "Vocal");
        self.add_button(
            TAG_LOOKAHEAD_ENABLED,
            596.0,
            336.0,
            104.0,
            24.0,
            "Lookahead",
        );

        self.add_section(34.0, 398.0, "I/O");
        self.add_knob(
            TAG_INPUT_LEVEL,
            34.0,
            426.0,
            60.0,
            "Input Level",
            -100.0,
            100.0,
        );
        self.add_knob(TAG_INPUT_PAN, 126.0, 426.0, 60.0, "Input Pan", -1.0, 1.0);
        self.add_knob(
            TAG_OUTPUT_LEVEL,
            314.0,
            426.0,
            60.0,
            "Output Level",
            -100.0,
            100.0,
        );
        self.add_knob(TAG_OUTPUT_PAN, 406.0, 426.0, 60.0, "Output Pan", -1.0, 1.0);
        self.add_popup(
            TAG_OVERSAMPLING,
            596.0,
            410.0,
            104.0,
            "Oversampling",
            &["Off", "2x", "4x", "6x", "8x"],
        );
        self.add_slider(
            TAG_GUI_SCALE,
            596.0,
            472.0,
            220.0,
            "GUI Size",
            MIN_WINDOW_SCALE as f64,
            MAX_WINDOW_SCALE as f64,
        );
    }

    unsafe fn add_section(&self, x: f64, y: f64, text: &str) {
        self.add_label(x, y, 240.0, 20.0, text, 13.0, true);
    }

    unsafe fn add_panel(&self, x: f64, y: f64, w: f64, h: f64) {
        let panel = NSView::initWithFrame_(NSView::alloc(nil), frame_from_top(x, y, w, h));
        panel.setWantsLayer(YES);
        let layer: id = msg_send![panel, layer];
        let fill_color: id = msg_send![ns_color(0.035, 0.02, 0.095, 0.86), CGColor];
        let border_color: id = msg_send![ns_color(0.0, 0.55, 0.72, 0.48), CGColor];
        let _: () = msg_send![layer, setBackgroundColor: fill_color];
        let _: () = msg_send![layer, setBorderColor: border_color];
        let _: () = msg_send![layer, setBorderWidth: 1.0f64];
        let _: () = msg_send![layer, setCornerRadius: 8.0f64];
        self.add_child(panel);
    }

    unsafe fn add_label(
        &self,
        x: f64,
        y: f64,
        w: f64,
        h: f64,
        text: &str,
        size: f64,
        strong: bool,
    ) -> id {
        let label =
            NSTextField::initWithFrame_(NSTextField::alloc(nil), frame_from_top(x, y, w, h));
        let _: () = msg_send![label, setStringValue: ns_string(text)];
        let _: () = msg_send![label, setEditable: NO];
        let _: () = msg_send![label, setSelectable: NO];
        let _: () = msg_send![label, setBordered: NO];
        let _: () = msg_send![label, setDrawsBackground: NO];
        let color = ns_color(0.78, 0.91, 1.0, if strong { 1.0 } else { 0.82 });
        let _: () = msg_send![label, setTextColor: color];
        let weight = if strong { 0.35 } else { 0.0 };
        let font: id = msg_send![class!(NSFont), systemFontOfSize: size weight: weight];
        let _: () = msg_send![label, setFont: font];
        self.add_child(label);
        label
    }

    unsafe fn add_slider(
        &mut self,
        tag: isize,
        x: f64,
        y: f64,
        w: f64,
        label: &str,
        min: f64,
        max: f64,
    ) {
        self.add_label(x, y, w * 0.55, 18.0, label, 11.0, false);
        let value_label = self.add_label(x + w * 0.58, y, w * 0.42, 18.0, "", 11.0, false);
        let slider: id = msg_send![class!(NSSlider), alloc];
        let slider: id = msg_send![slider, initWithFrame: frame_from_top(x, y + 22.0, w, 24.0)];
        let _: () = msg_send![slider, setMinValue: min];
        let _: () = msg_send![slider, setMaxValue: max];
        let _: () = msg_send![slider, setContinuous: YES];
        let _: () = msg_send![slider, setTag: tag];
        let _: () = msg_send![slider, setTarget: self.target];
        let _: () = msg_send![slider, setAction: sel!(controlChanged:)];
        self.add_child(slider);
        self.controls.push(ControlBinding {
            tag,
            control: slider,
            value_label,
        });
    }

    unsafe fn add_knob(
        &mut self,
        tag: isize,
        x: f64,
        y: f64,
        size: f64,
        label: &str,
        min: f64,
        max: f64,
    ) {
        self.add_label(x - 8.0, y, size + 30.0, 16.0, label, 10.0, false);
        let knob: id = msg_send![class!(NSSlider), alloc];
        let knob: id = msg_send![knob, initWithFrame: frame_from_top(x, y + 19.0, size, size)];
        let _: () = msg_send![knob, setSliderType: 1isize];
        let _: () = msg_send![knob, setMinValue: min];
        let _: () = msg_send![knob, setMaxValue: max];
        let _: () = msg_send![knob, setContinuous: YES];
        let _: () = msg_send![knob, setTag: tag];
        let _: () = msg_send![knob, setTarget: self.target];
        let _: () = msg_send![knob, setAction: sel!(controlChanged:)];
        self.add_child(knob);

        let value_label =
            self.add_label(x - 8.0, y + size + 24.0, size + 24.0, 16.0, "", 10.0, false);
        self.controls.push(ControlBinding {
            tag,
            control: knob,
            value_label,
        });
    }

    unsafe fn add_button(&mut self, tag: isize, x: f64, y: f64, w: f64, h: f64, label: &str) {
        let button = NSButton::initWithFrame_(NSButton::alloc(nil), frame_from_top(x, y, w, h));
        let _: () = msg_send![button, setButtonType: 3isize];
        let _: () = msg_send![button, setBezelStyle: 6isize];
        let _: () = msg_send![button, setTitle: ns_string(label)];
        let _: () = msg_send![button, setTag: tag];
        let _: () = msg_send![button, setTarget: self.target];
        let _: () = msg_send![button, setAction: sel!(controlChanged:)];
        self.add_child(button);
        self.controls.push(ControlBinding {
            tag,
            control: button,
            value_label: nil,
        });
    }

    unsafe fn add_popup(
        &mut self,
        tag: isize,
        x: f64,
        y: f64,
        w: f64,
        label: &str,
        items: &[&str],
    ) {
        self.add_label(x, y, w, 18.0, label, 11.0, false);
        let popup: id = msg_send![class!(NSPopUpButton), alloc];
        let popup: id =
            msg_send![popup, initWithFrame: frame_from_top(x, y + 22.0, w, 26.0) pullsDown: NO];
        for item in items {
            let _: () = msg_send![popup, addItemWithTitle: ns_string(item)];
        }
        let _: () = msg_send![popup, setTag: tag];
        let _: () = msg_send![popup, setTarget: self.target];
        let _: () = msg_send![popup, setAction: sel!(controlChanged:)];
        self.add_child(popup);
        self.controls.push(ControlBinding {
            tag,
            control: popup,
            value_label: nil,
        });
    }

    unsafe fn add_child(&self, child: id) {
        self.view.addSubview_(child);
        let _: () = msg_send![child, release];
    }

    unsafe fn sync_from_params(&self) {
        for binding in &self.controls {
            match binding.tag {
                TAG_THRESHOLD => set_control_f64(binding.control, self.params.threshold.value()),
                TAG_MAX_REDUCTION => {
                    set_control_f64(binding.control, self.params.max_reduction.value())
                }
                TAG_MIN_FREQ => set_control_f64(binding.control, self.params.min_freq.value()),
                TAG_MAX_FREQ => set_control_f64(binding.control, self.params.max_freq.value()),
                TAG_STEREO_LINK => {
                    set_control_f64(binding.control, self.params.stereo_link.value())
                }
                TAG_LOOKAHEAD_MS => {
                    set_control_f64(binding.control, self.params.lookahead_ms.value())
                }
                TAG_INPUT_LEVEL => {
                    set_control_f64(binding.control, self.params.input_level.value())
                }
                TAG_INPUT_PAN => set_control_f64(binding.control, self.params.input_pan.value()),
                TAG_OUTPUT_LEVEL => {
                    set_control_f64(binding.control, self.params.output_level.value())
                }
                TAG_OUTPUT_PAN => set_control_f64(binding.control, self.params.output_pan.value()),
                TAG_CUT_WIDTH => set_control_f64(binding.control, self.params.cut_width.value()),
                TAG_CUT_DEPTH => set_control_f64(binding.control, self.params.cut_depth.value()),
                TAG_CUT_SLOPE => set_control_f64(binding.control, self.params.cut_slope.value()),
                TAG_MIX => set_control_f64(binding.control, self.params.mix.value()),
                TAG_GUI_SCALE => {
                    let width = f32::from_bits(self.window_width_bits.load(Ordering::Acquire));
                    set_control_f64(
                        binding.control,
                        (width / BASE_W).clamp(MIN_WINDOW_SCALE, MAX_WINDOW_SCALE),
                    );
                }
                TAG_BYPASS => set_button_state(binding.control, self.params.bypass.value() > 0.5),
                TAG_MODE_RELATIVE => {
                    set_button_state(binding.control, self.params.mode_relative.value() > 0.5)
                }
                TAG_WIDE_RANGE => {
                    set_button_state(binding.control, self.params.use_wide_range.value() > 0.5)
                }
                TAG_FILTER_SOLO => {
                    set_button_state(binding.control, self.params.filter_solo.value() > 0.5)
                }
                TAG_LOOKAHEAD_ENABLED => {
                    set_button_state(binding.control, self.params.lookahead_enabled.value() > 0.5)
                }
                TAG_TRIGGER_HEAR => {
                    set_button_state(binding.control, self.params.trigger_hear.value() > 0.5)
                }
                TAG_VOCAL_MODE => {
                    set_button_state(binding.control, self.params.vocal_mode.value() > 0.5)
                }
                TAG_BASIS_MODE => {
                    set_popup_index(binding.control, self.params.basis_mode.value() as isize)
                }
                TAG_STEREO_MODE => set_popup_index(
                    binding.control,
                    self.params.stereo_mid_side.value() as isize,
                ),
                TAG_SIDECHAIN_MODE => {
                    set_popup_index(binding.control, self.params.sidechain_mode.value() as isize)
                }
                TAG_OVERSAMPLING => {
                    set_popup_index(binding.control, self.params.oversampling.value() as isize)
                }
                _ => {}
            }

            if binding.value_label != nil {
                let value = if binding.tag == TAG_GUI_SCALE {
                    let width = f32::from_bits(self.window_width_bits.load(Ordering::Acquire));
                    format!(
                        "{:.0} %",
                        (width / BASE_W).clamp(MIN_WINDOW_SCALE, MAX_WINDOW_SCALE) * 100.0
                    )
                } else {
                    format_control_value(binding.tag, &self.params)
                };
                let _: () = msg_send![binding.value_label, setStringValue: ns_string(&value)];
            }
        }

        let detection = u32_to_f32(self.meters.det_bits.load(Ordering::Relaxed));
        let reduction = -u32_to_f32(self.meters.red_bits.load(Ordering::Relaxed));
        if self.det_meter_label != nil {
            let value = format!("Detect  {:.1} dB", detection);
            let _: () = msg_send![self.det_meter_label, setStringValue: ns_string(&value)];
        }
        if self.red_meter_label != nil {
            let value = format!("Reduction  {:.1} dB", reduction);
            let _: () = msg_send![self.red_meter_label, setStringValue: ns_string(&value)];
        }
    }

    unsafe fn handle_control_change(&mut self, sender: id) {
        let tag: isize = msg_send![sender, tag];
        let setter = ParamSetter::new(self.context.as_ref());
        match tag {
            TAG_THRESHOLD => set_float(&setter, &self.params.threshold, control_f32(sender)),
            TAG_MAX_REDUCTION => {
                set_float(&setter, &self.params.max_reduction, control_f32(sender))
            }
            TAG_MIN_FREQ => {
                let value = clamp_min_frequency(control_f32(sender), self.params.max_freq.value());
                set_float(&setter, &self.params.min_freq, value);
            }
            TAG_MAX_FREQ => {
                let value = clamp_max_frequency(control_f32(sender), self.params.min_freq.value());
                set_float(&setter, &self.params.max_freq, value);
            }
            TAG_STEREO_LINK => set_float(&setter, &self.params.stereo_link, control_f32(sender)),
            TAG_LOOKAHEAD_MS => set_float(&setter, &self.params.lookahead_ms, control_f32(sender)),
            TAG_INPUT_LEVEL => set_float(&setter, &self.params.input_level, control_f32(sender)),
            TAG_INPUT_PAN => set_float(&setter, &self.params.input_pan, control_f32(sender)),
            TAG_OUTPUT_LEVEL => set_float(&setter, &self.params.output_level, control_f32(sender)),
            TAG_OUTPUT_PAN => set_float(&setter, &self.params.output_pan, control_f32(sender)),
            TAG_CUT_WIDTH => set_float(&setter, &self.params.cut_width, control_f32(sender)),
            TAG_CUT_DEPTH => set_float(&setter, &self.params.cut_depth, control_f32(sender)),
            TAG_CUT_SLOPE => set_float(&setter, &self.params.cut_slope, control_f32(sender)),
            TAG_MIX => set_float(&setter, &self.params.mix, control_f32(sender)),
            TAG_GUI_SCALE => {
                let scale = control_f32(sender).clamp(MIN_WINDOW_SCALE, MAX_WINDOW_SCALE);
                let width = BASE_W * scale;
                let height = BASE_H * scale;
                self.window_width_bits
                    .store(width.to_bits(), Ordering::Release);
                self.window_height_bits
                    .store(height.to_bits(), Ordering::Release);
                self.storage
                    .save_editor_size(StoredEditorSize { width, height });
                self.view
                    .setFrameSize(NSSize::new(width as f64, height as f64));
                let _: () = msg_send![self.view, setBounds: base_bounds()];
                let _ = self.context.request_resize();
            }
            TAG_BYPASS => set_float(
                &setter,
                &self.params.bypass,
                if button_state(sender) { 1.0 } else { 0.0 },
            ),
            TAG_MODE_RELATIVE => set_float(
                &setter,
                &self.params.mode_relative,
                if button_state(sender) { 1.0 } else { 0.0 },
            ),
            TAG_WIDE_RANGE => set_float(
                &setter,
                &self.params.use_wide_range,
                if button_state(sender) { 1.0 } else { 0.0 },
            ),
            TAG_FILTER_SOLO => set_float(
                &setter,
                &self.params.filter_solo,
                if button_state(sender) { 1.0 } else { 0.0 },
            ),
            TAG_LOOKAHEAD_ENABLED => set_float(
                &setter,
                &self.params.lookahead_enabled,
                if button_state(sender) { 1.0 } else { 0.0 },
            ),
            TAG_TRIGGER_HEAR => set_float(
                &setter,
                &self.params.trigger_hear,
                if button_state(sender) { 1.0 } else { 0.0 },
            ),
            TAG_VOCAL_MODE => set_float(
                &setter,
                &self.params.vocal_mode,
                if button_state(sender) { 1.0 } else { 0.0 },
            ),
            TAG_BASIS_MODE => set_float(
                &setter,
                &self.params.basis_mode,
                popup_index(sender).clamp(0, 2) as f32,
            ),
            TAG_STEREO_MODE => set_float(
                &setter,
                &self.params.stereo_mid_side,
                popup_index(sender).clamp(0, 2) as f32,
            ),
            TAG_SIDECHAIN_MODE => set_float(
                &setter,
                &self.params.sidechain_mode,
                popup_index(sender).clamp(0, 2) as f32,
            ),
            TAG_OVERSAMPLING => set_float(
                &setter,
                &self.params.oversampling,
                popup_index(sender).clamp(0, 4) as f32,
            ),
            _ => {}
        }
        self.sync_from_params();
        self.render();
    }

    unsafe fn timer_fired(&mut self) {
        self.apply_pending_midi_cc();
        self.sync_from_params();
        self.render();
    }

    fn apply_pending_midi_cc(&self) {
        self.midi_learn.sync_mutex_from_atomic_if_needed();
        if !self.midi_learn.midi_enabled.load(Ordering::Relaxed) {
            return;
        }

        let setter = ParamSetter::new(self.context.as_ref());
        for cc in 0..128 {
            if !self.midi_learn.cc_dirty[cc].swap(false, Ordering::AcqRel) {
                continue;
            }

            let Some(parameter_index) = self.midi_learn.binding_for_cc(cc) else {
                continue;
            };
            let value = u32_to_f32(self.midi_learn.cc_values[cc].load(Ordering::Relaxed));
            apply_midi_mapping(parameter_index, value, &self.params, &setter);
        }
    }

    unsafe fn render(&self) {
        let bounds = self.view.bounds();
        let backing = self.view.convertRectToBacking(bounds);
        let _: () = msg_send![self.layer, setFrame: bounds];
        let _: () = msg_send![self.layer, setDrawableSize: backing.size];

        let drawable: id = msg_send![self.layer, nextDrawable];
        if drawable == nil {
            return;
        }

        let reduction = -u32_to_f32(self.meters.red_bits.load(Ordering::Relaxed));
        let detection = u32_to_f32(self.meters.det_bits.load(Ordering::Relaxed));
        let energy = ((detection + 72.0) / 72.0).clamp(0.0, 1.0) as f64;
        let reduction_glow = (reduction / 24.0).clamp(0.0, 1.0) as f64;
        let bypass = self.params.bypass.value() > 0.5;
        let clear = if bypass {
            MTLClearColor {
                red: 0.025,
                green: 0.028,
                blue: 0.035,
                alpha: 1.0,
            }
        } else {
            MTLClearColor {
                red: 0.018 + 0.02 * reduction_glow,
                green: 0.024 + 0.04 * energy,
                blue: 0.045 + 0.08 * reduction_glow,
                alpha: 1.0,
            }
        };

        let descriptor: id = msg_send![class!(MTLRenderPassDescriptor), renderPassDescriptor];
        let attachments: id = msg_send![descriptor, colorAttachments];
        let attachment: id = msg_send![attachments, objectAtIndexedSubscript: 0usize];
        let texture: id = msg_send![drawable, texture];
        let _: () = msg_send![attachment, setTexture: texture];
        let _: () = msg_send![attachment, setLoadAction: MTL_LOAD_ACTION_CLEAR];
        let _: () = msg_send![attachment, setStoreAction: MTL_STORE_ACTION_STORE];
        let _: () = msg_send![attachment, setClearColor: clear];

        let command_buffer: id = msg_send![self.command_queue, commandBuffer];
        if command_buffer == nil {
            return;
        }
        let encoder: id = msg_send![command_buffer, renderCommandEncoderWithDescriptor: descriptor];
        if encoder != nil {
            let _: () = msg_send![encoder, endEncoding];
        }
        let _: () = msg_send![command_buffer, presentDrawable: drawable];
        let _: () = msg_send![command_buffer, commit];
    }
}

extern "C" fn control_changed(this: &mut Object, _cmd: Sel, sender: id) {
    unsafe {
        let state_ptr = *this.get_ivar::<*mut c_void>("state") as *mut MetalWindowState;
        if let Some(state) = state_ptr.as_mut() {
            state.handle_control_change(sender);
        }
    }
}

extern "C" fn timer_fired(this: &mut Object, _cmd: Sel, _timer: id) {
    unsafe {
        let state_ptr = *this.get_ivar::<*mut c_void>("state") as *mut MetalWindowState;
        if let Some(state) = state_ptr.as_mut() {
            state.timer_fired();
        }
    }
}

fn target_class() -> &'static Class {
    static CLASS: OnceLock<&'static Class> = OnceLock::new();
    CLASS.get_or_init(|| {
        let superclass = class!(NSObject);
        let mut decl = ClassDecl::new("NebulaDesserMetalControlTarget", superclass).unwrap();
        decl.add_ivar::<*mut c_void>("state");
        unsafe {
            decl.add_method(
                sel!(controlChanged:),
                control_changed as extern "C" fn(&mut Object, Sel, id),
            );
            decl.add_method(
                sel!(timerFired:),
                timer_fired as extern "C" fn(&mut Object, Sel, id),
            );
        }
        decl.register()
    })
}

fn frame_from_top(x: f64, y: f64, width: f64, height: f64) -> NSRect {
    NSRect::new(
        NSPoint::new(x, BASE_H as f64 - y - height),
        NSSize::new(width, height),
    )
}

fn base_bounds() -> NSRect {
    NSRect::new(
        NSPoint::new(0.0, 0.0),
        NSSize::new(BASE_W as f64, BASE_H as f64),
    )
}

unsafe fn ns_string(value: &str) -> id {
    let string = NSString::alloc(nil).init_str(value);
    let _: id = msg_send![string, autorelease];
    string
}

unsafe fn ns_color(red: f64, green: f64, blue: f64, alpha: f64) -> id {
    msg_send![
        class!(NSColor),
        colorWithCalibratedRed: red
        green: green
        blue: blue
        alpha: alpha
    ]
}

unsafe fn set_control_f64(control: id, value: f32) {
    let _: () = msg_send![control, setDoubleValue: value as f64];
}

unsafe fn control_f32(control: id) -> f32 {
    let value: f64 = msg_send![control, doubleValue];
    value as f32
}

unsafe fn set_button_state(button: id, enabled: bool) {
    let state = if enabled { 1isize } else { 0isize };
    let _: () = msg_send![button, setState: state];
}

unsafe fn button_state(button: id) -> bool {
    let state: isize = msg_send![button, state];
    state != 0
}

unsafe fn set_popup_index(popup: id, index: isize) {
    let _: () = msg_send![popup, selectItemAtIndex: index.max(0)];
}

unsafe fn popup_index(popup: id) -> isize {
    msg_send![popup, indexOfSelectedItem]
}

fn set_float(setter: &ParamSetter, param: &FloatParam, value: f32) {
    setter.begin_set_parameter(param);
    setter.set_parameter(param, value);
    setter.end_set_parameter(param);
}

fn format_control_value(tag: isize, params: &NebulaParams) -> String {
    match tag {
        TAG_THRESHOLD => format!("{:.0} %", params.threshold.value()),
        TAG_MAX_REDUCTION => format!("{:.1} dB", params.max_reduction.value()),
        TAG_MIN_FREQ => format!("{:.0} Hz", params.min_freq.value()),
        TAG_MAX_FREQ => format!("{:.0} Hz", params.max_freq.value()),
        TAG_STEREO_LINK => format!("{:.0} %", params.stereo_link.value() * 100.0),
        TAG_LOOKAHEAD_MS => format!("{:.1} ms", params.lookahead_ms.value()),
        TAG_INPUT_LEVEL => format!("{:.1} dB", params.input_level.value()),
        TAG_INPUT_PAN => format_pan(params.input_pan.value()),
        TAG_OUTPUT_LEVEL => format!("{:.1} dB", params.output_level.value()),
        TAG_OUTPUT_PAN => format_pan(params.output_pan.value()),
        TAG_CUT_WIDTH => format!("{:.0} %", params.cut_width.value() * 100.0),
        TAG_CUT_DEPTH => format!("{:.0} %", params.cut_depth.value() * 100.0),
        TAG_CUT_SLOPE => format!("{:.1} dB/oct", params.cut_slope.value()),
        TAG_MIX => format!("{:.0} %", params.mix.value() * 100.0),
        TAG_GUI_SCALE => String::new(),
        _ => String::new(),
    }
}

fn format_pan(value: f32) -> String {
    if value.abs() < 0.005 {
        "C".to_string()
    } else if value < 0.0 {
        format!("L{:.0}", value.abs() * 100.0)
    } else {
        format!("R{:.0}", value * 100.0)
    }
}
