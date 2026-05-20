use std::any::Any;
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::{Arc, Once};

use nih_plug::prelude::{Editor, FloatParam, GuiContext, ParamSetter, ParentWindowHandle};
use parking_lot::Mutex;
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{
    GetLastError, ERROR_CLASS_ALREADY_EXISTS, HINSTANCE, HWND, LPARAM, LRESULT, RECT, WPARAM,
};
use windows::Win32::Graphics::Direct2D::Common::{
    D2D1_ALPHA_MODE_UNKNOWN, D2D1_COLOR_F, D2D1_PIXEL_FORMAT, D2D_RECT_F, D2D_SIZE_U,
};
use windows::Win32::Graphics::Direct2D::{
    D2D1CreateFactory, ID2D1Factory, ID2D1HwndRenderTarget, ID2D1SolidColorBrush,
    D2D1_DRAW_TEXT_OPTIONS_NONE, D2D1_ELLIPSE, D2D1_FACTORY_TYPE_SINGLE_THREADED,
    D2D1_FEATURE_LEVEL_DEFAULT, D2D1_HWND_RENDER_TARGET_PROPERTIES, D2D1_PRESENT_OPTIONS_NONE,
    D2D1_RENDER_TARGET_PROPERTIES, D2D1_RENDER_TARGET_TYPE_DEFAULT, D2D1_RENDER_TARGET_USAGE_NONE,
    D2D1_ROUNDED_RECT,
};
use windows::Win32::Graphics::DirectWrite::{
    DWriteCreateFactory, IDWriteFactory, IDWriteFontCollection, IDWriteTextFormat,
    DWRITE_FACTORY_TYPE_SHARED, DWRITE_FONT_STRETCH_NORMAL, DWRITE_FONT_STYLE_NORMAL,
    DWRITE_FONT_WEIGHT_DEMI_BOLD, DWRITE_FONT_WEIGHT_NORMAL, DWRITE_MEASURING_MODE_NATURAL,
    DWRITE_PARAGRAPH_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_CENTER, DWRITE_TEXT_ALIGNMENT_LEADING,
    DWRITE_TEXT_ALIGNMENT_TRAILING,
};
use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_UNKNOWN;
use windows::Win32::Graphics::Gdi::{
    BeginPaint, EndPaint, InvalidateRect, UpdateWindow, HBRUSH, PAINTSTRUCT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::HiDpi::{
    GetDpiForSystem, GetDpiForWindow, SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE, VK_RETURN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW, KillTimer,
    LoadCursorW, RegisterClassW, SetTimer, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    CREATESTRUCTW, CS_HREDRAW, CS_VREDRAW, DLGC_WANTALLKEYS, DLGC_WANTCHARS, GWLP_USERDATA, HMENU,
    IDC_ARROW, SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE, WM_CHAR, WM_DPICHANGED,
    WM_DPICHANGED_AFTERPARENT, WM_DPICHANGED_BEFOREPARENT, WM_ERASEBKGND, WM_GETDLGCODE,
    WM_KEYDOWN, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_RBUTTONDOWN, WM_SIZE, WM_TIMER, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS,
    WS_VISIBLE,
};
use windows_numerics::Vector2;

use super::analyzer::SpectrumData;
use super::{
    u32_to_f32, Meters, MidiLearnShared, NebulaParams, PersistentStore, StoredPreset,
    StoredPresetSnapshot, MIDI_PARAM_COUNT, MIDI_PARAM_NAMES,
};

const BASE_W: f32 = 860.0;
const BASE_H: f32 = 640.0;
const DEFAULT_DPI: u32 = 96;
const TIMER_ID: usize = 7401;
const TIMER_MS: u32 = 33;
const VERSION_LABEL: &str = concat!(
    "v",
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR")
);
const SUBTITLE_LABEL: &str = concat!(
    "Sibilance Processor  |  Native Direct2D  |  ",
    "v",
    env!("CARGO_PKG_VERSION_MAJOR"),
    ".",
    env!("CARGO_PKG_VERSION_MINOR")
);

pub(super) fn create_editor(
    params: Arc<NebulaParams>,
    spectrum: Arc<Mutex<SpectrumData>>,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    storage: Arc<PersistentStore>,
) -> Option<Box<dyn Editor>> {
    Some(Box::new(NativeEditor {
        params,
        spectrum,
        meters,
        midi_learn,
        storage,
        scale_bits: AtomicU32::new(1.0_f32.to_bits()),
        size_scale_bits: Arc::new(AtomicU32::new(1.0_f32.to_bits())),
    }))
}

struct NativeEditor {
    params: Arc<NebulaParams>,
    spectrum: Arc<Mutex<SpectrumData>>,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    storage: Arc<PersistentStore>,
    scale_bits: AtomicU32,
    size_scale_bits: Arc<AtomicU32>,
}

impl Editor for NativeEditor {
    fn spawn(
        &self,
        parent: ParentWindowHandle,
        context: Arc<dyn GuiContext>,
    ) -> Box<dyn Any + Send> {
        let ParentWindowHandle::Win32Hwnd(parent_hwnd) = parent else {
            return Box::new(());
        };
        if parent_hwnd.is_null() || !register_window_class() {
            return Box::new(());
        }

        let parent_hwnd = HWND(parent_hwnd);
        let dpi_scope = DpiAwarenessScope::enter();
        let host_scale = f32::from_bits(self.scale_bits.load(Ordering::Acquire)).clamp(0.5, 3.0);
        let initial_dpi = dpi_for_window(parent_hwnd);
        let render_scale = host_scale.max(dpi_scale(initial_dpi)).clamp(0.5, 3.0);
        let size_scale = (render_scale / host_scale.max(0.5)).clamp(1.0, 3.0);
        self.size_scale_bits
            .store(size_scale.to_bits(), Ordering::Release);

        let scaled_width = (BASE_W * render_scale).round() as i32;
        let scaled_height = (BASE_H * render_scale).round() as i32;
        let (width, height) = client_size(parent_hwnd)
            .filter(|(width, height)| *width > 100 && *height > 100)
            .map(|(width, height)| {
                (
                    (width as i32).max(scaled_width),
                    (height as i32).max(scaled_height),
                )
            })
            .unwrap_or((scaled_width, scaled_height));

        let request_host_resize = size_scale > 1.01;
        let resize_context = context.clone();
        let state = Box::new(NativeWindowState::new(
            self.params.clone(),
            self.spectrum.clone(),
            self.meters.clone(),
            self.midi_learn.clone(),
            self.storage.clone(),
            self.size_scale_bits.clone(),
            context,
            parent_hwnd,
            host_scale,
            initial_dpi,
        ));
        let state_ptr = Box::into_raw(state);

        let hwnd = unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                class_name(),
                w!("Nebula De-Esser"),
                WS_CHILD | WS_VISIBLE | WS_CLIPCHILDREN | WS_CLIPSIBLINGS,
                0,
                0,
                width,
                height,
                Some(parent_hwnd),
                Option::<HMENU>::None,
                module_instance(),
                Some(state_ptr.cast::<c_void>()),
            )
        };
        drop(dpi_scope);

        match hwnd {
            Ok(hwnd) => unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = UpdateWindow(hwnd);
                if request_host_resize {
                    let _ = resize_context.request_resize();
                }
                Box::new(NativeWindowHandle {
                    hwnd: hwnd.0 as isize,
                })
            },
            Err(_) => unsafe {
                drop(Box::from_raw(state_ptr));
                Box::new(())
            },
        }
    }

    fn size(&self) -> (u32, u32) {
        let size_scale =
            f32::from_bits(self.size_scale_bits.load(Ordering::Acquire)).clamp(1.0, 3.0);
        (
            (BASE_W * size_scale).round() as u32,
            (BASE_H * size_scale).round() as u32,
        )
    }

    fn set_scale_factor(&self, factor: f32) -> bool {
        self.scale_bits
            .store(factor.max(0.5).to_bits(), Ordering::Release);
        self.size_scale_bits
            .store(1.0_f32.to_bits(), Ordering::Release);
        true
    }

    fn param_value_changed(&self, _id: &str, _normalized_value: f32) {}

    fn param_modulation_changed(&self, _id: &str, _modulation_offset: f32) {}

    fn param_values_changed(&self) {}
}

struct NativeWindowHandle {
    hwnd: isize,
}

unsafe impl Send for NativeWindowHandle {}

impl Drop for NativeWindowHandle {
    fn drop(&mut self) {
        if self.hwnd != 0 {
            let hwnd = HWND(self.hwnd as *mut c_void);
            let _ = unsafe { DestroyWindow(hwnd) };
            self.hwnd = 0;
        }
    }
}

struct NativeWindowState {
    hwnd: HWND,
    parent_hwnd: HWND,
    params: Arc<NebulaParams>,
    spectrum: Arc<Mutex<SpectrumData>>,
    meters: Arc<Meters>,
    midi_learn: Arc<MidiLearnShared>,
    storage: Arc<PersistentStore>,
    size_scale_bits: Arc<AtomicU32>,
    context: Arc<dyn GuiContext>,
    d2d_factory: Option<ID2D1Factory>,
    dwrite_factory: Option<IDWriteFactory>,
    render_target: Option<ID2D1HwndRenderTarget>,
    text_formats: Option<TextFormats>,
    smooth_mags: Vec<f32>,
    drag: Option<DragState>,
    drag_snapshot: Option<ParamSnapshot>,
    numeric_input: Option<NumericInput>,
    preset_save_open: bool,
    preset_menu_open: bool,
    midi_popup_open: bool,
    midi_context_menu_open: bool,
    midi_cleanup_menu_open: bool,
    preset_name_buf: String,
    presets: Vec<(String, ParamSnapshot)>,
    selected_preset: usize,
    state_a: Option<ParamSnapshot>,
    state_b: Option<ParamSnapshot>,
    active_state: char,
    undo_stack: Vec<ParamSnapshot>,
    redo_stack: Vec<ParamSnapshot>,
    host_scale: f32,
    dpi: u32,
}

impl NativeWindowState {
    fn new(
        params: Arc<NebulaParams>,
        spectrum: Arc<Mutex<SpectrumData>>,
        meters: Arc<Meters>,
        midi_learn: Arc<MidiLearnShared>,
        storage: Arc<PersistentStore>,
        size_scale_bits: Arc<AtomicU32>,
        context: Arc<dyn GuiContext>,
        parent_hwnd: HWND,
        host_scale: f32,
        dpi: u32,
    ) -> Self {
        let presets = storage
            .presets()
            .into_iter()
            .map(|preset| (preset.name, ParamSnapshot::from_stored(&preset.snapshot)))
            .collect();
        Self {
            hwnd: HWND::default(),
            parent_hwnd,
            params,
            spectrum,
            meters,
            midi_learn,
            storage,
            size_scale_bits,
            context,
            d2d_factory: None,
            dwrite_factory: None,
            render_target: None,
            text_formats: None,
            smooth_mags: vec![-120.0; 1025],
            drag: None,
            drag_snapshot: None,
            numeric_input: None,
            preset_save_open: false,
            preset_menu_open: false,
            midi_popup_open: false,
            midi_context_menu_open: false,
            midi_cleanup_menu_open: false,
            preset_name_buf: String::new(),
            presets,
            selected_preset: 0,
            state_a: None,
            state_b: None,
            active_state: 'A',
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            host_scale,
            dpi: dpi.max(DEFAULT_DPI),
        }
    }

    fn paint(&mut self) {
        self.refresh_dpi();
        let Some((w, h)) = self.logical_client_size() else {
            return;
        };
        let s = (w / BASE_W).min(h / BASE_H).max(0.45);
        let layout = Layout::new(w, h, s);
        let Some(rt) = self.ensure_render_target() else {
            return;
        };
        let Some(formats) = self.ensure_text_formats(layout.s) else {
            return;
        };
        let Some(brushes) = Brushes::new(&rt) else {
            return;
        };

        unsafe {
            rt.BeginDraw();
            rt.Clear(Some(&Colors::MICA_BASE));
        }

        self.draw_background(&rt, &brushes, &layout);
        self.draw_header(&rt, &brushes, &formats, &layout);
        self.draw_meter_panel(&rt, &brushes, &formats, layout.left_panel, true, s);
        self.draw_meter_panel(&rt, &brushes, &formats, layout.right_panel, false, s);
        self.draw_controls(&rt, &brushes, &formats, &layout);
        self.draw_command_bar(&rt, &brushes, &formats, &layout);
        self.draw_spectrum(&rt, &brushes, &formats, &layout);
        self.draw_preset_menu(&rt, &brushes, &formats, &layout);
        self.draw_midi_popup(&rt, &brushes, &formats, &layout);
        self.draw_midi_context_menu(&rt, &brushes, &formats, &layout);
        self.draw_midi_cleanup_menu(&rt, &brushes, &formats, &layout);
        self.draw_preset_save_popup(&rt, &brushes, &formats, &layout);
        self.draw_numeric_popup(&rt, &brushes, &formats, &layout);

        if unsafe { rt.EndDraw(None, None) }.is_err() {
            self.render_target = None;
        }
    }

    fn draw_background(&self, rt: &ID2D1HwndRenderTarget, brushes: &Brushes, layout: &Layout) {
        fill_rect(rt, layout.full, &brushes.mica_base);
        fill_rect(
            rt,
            UiRect::new(0.0, 0.0, layout.full.w, layout.full.h * 0.25),
            &brushes.mica_top,
        );
        fill_rect(
            rt,
            UiRect::new(0.0, layout.full.h * 0.8, layout.full.w, layout.full.h * 0.2),
            &brushes.mica_bot,
        );
    }

    fn draw_header(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        fill_rect(rt, layout.header, &brushes.panel);
        draw_line(
            rt,
            layout.header.x,
            layout.header.bottom(),
            layout.header.right(),
            layout.header.bottom(),
            &brushes.divider,
            1.0,
        );

        let s = layout.s;
        let icon = UiRect::new(16.0 * s, 17.0 * s, 20.0 * s, 20.0 * s);
        fill_round(rt, icon, 4.0 * s, &brushes.accent);
        draw_text(
            rt,
            "N",
            icon,
            &formats.small,
            &brushes.text_light,
            Align::Center,
        );

        draw_text(
            rt,
            "Nebula De-Esser",
            UiRect::new(44.0 * s, 9.0 * s, 220.0 * s, 22.0 * s),
            &formats.title,
            &brushes.text_primary,
            Align::Leading,
        );
        draw_text(
            rt,
            SUBTITLE_LABEL,
            UiRect::new(
                44.0 * s,
                30.0 * s,
                (layout.full.right() - 300.0 * s).max(260.0 * s),
                18.0 * s,
            ),
            &formats.small,
            &brushes.text_tertiary,
            Align::Leading,
        );

        let status_rect = UiRect::new(
            layout.full.right() - 220.0 * s,
            14.0 * s,
            142.0 * s,
            24.0 * s,
        );
        fill_round(
            rt,
            status_rect,
            4.0 * s,
            if self.params.bypass.value() > 0.5 {
                &brushes.red_soft
            } else {
                &brushes.control
            },
        );
        stroke_round(
            rt,
            status_rect,
            4.0 * s,
            if self.params.bypass.value() > 0.5 {
                &brushes.red
            } else {
                &brushes.border
            },
            1.0,
        );
        draw_text(
            rt,
            if self.params.bypass.value() > 0.5 {
                "Processor bypassed"
            } else {
                "Processor active"
            },
            status_rect,
            &formats.small,
            if self.params.bypass.value() > 0.5 {
                &brushes.red
            } else {
                &brushes.text_secondary
            },
            Align::Center,
        );

        draw_text(
            rt,
            VERSION_LABEL,
            UiRect::new(layout.full.right() - 70.0 * s, 16.0 * s, 54.0 * s, 20.0 * s),
            &formats.body,
            &brushes.text_tertiary,
            Align::Trailing,
        );
    }

    fn draw_command_bar(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        let s = layout.s;
        let bar = CommandBarRects::new(layout);
        fill_rect(rt, layout.command_bar, &brushes.panel);
        draw_line(
            rt,
            layout.command_bar.x,
            layout.command_bar.bottom(),
            layout.command_bar.right(),
            layout.command_bar.bottom(),
            &brushes.divider,
            1.0,
        );

        let bypass = self.params.bypass.value() > 0.5;
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            bar.bypass,
            if bypass { "Bypassed" } else { "Bypass" },
            bypass,
            true,
            s,
        );

        let preset_label = if self.presets.is_empty() {
            "Preset"
        } else {
            truncate_label(
                &self.presets[self.selected_preset.min(self.presets.len() - 1)].0,
                16,
            )
        };
        fill_round(
            rt,
            bar.preset,
            4.0 * s,
            if self.preset_menu_open {
                &brushes.accent_soft
            } else {
                &brushes.control
            },
        );
        stroke_round(
            rt,
            bar.preset,
            4.0 * s,
            if self.preset_menu_open {
                &brushes.accent
            } else {
                &brushes.border
            },
            1.0,
        );
        draw_text(
            rt,
            &format!("{preset_label}  v"),
            bar.preset,
            &formats.body,
            if self.preset_menu_open {
                &brushes.text_primary
            } else {
                &brushes.text_secondary
            },
            Align::Center,
        );

        self.draw_toolbar_button(rt, brushes, formats, bar.save, "Save", false, false, s);
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            bar.delete,
            "Delete",
            !self.presets.is_empty(),
            true,
            s,
        );
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            bar.undo,
            "Undo",
            !self.undo_stack.is_empty(),
            false,
            s,
        );
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            bar.redo,
            "Redo",
            !self.redo_stack.is_empty(),
            false,
            s,
        );
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            bar.ab,
            if self.active_state == 'A' {
                "A/B [A]"
            } else {
                "A/B [B]"
            },
            self.state_a.is_some() || self.state_b.is_some(),
            false,
            s,
        );

        let learning = self.midi_learn.learning_target.load(Ordering::Relaxed) >= 0;
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            bar.midi,
            if learning { "Learning" } else { "MIDI Learn" },
            learning || self.midi_popup_open,
            false,
            s,
        );

        let status = match self.params.sidechain_mode.value().round().clamp(0.0, 2.0) as u32 {
            1 => "External sidechain",
            2 => "MIDI sidechain",
            _ => "Internal detector",
        };
        draw_text(
            rt,
            status,
            bar.status,
            &formats.small,
            &brushes.text_tertiary,
            Align::Leading,
        );

        fill_round(rt, bar.os_rect, 4.0 * s, &brushes.control);
        stroke_round(rt, bar.os_rect, 4.0 * s, &brushes.border, 1.0);
        let os_labels = ["Off", "2x", "4x", "6x", "8x"];
        let os = self.params.oversampling.value().round().clamp(0.0, 4.0) as usize;
        for (idx, label) in os_labels.iter().enumerate() {
            if idx == os {
                fill_round(
                    rt,
                    bar.os_segments[idx].shrink(2.0 * s),
                    3.0 * s,
                    &brushes.accent,
                );
            }
            draw_text(
                rt,
                label,
                bar.os_segments[idx],
                &formats.small,
                if idx == os {
                    &brushes.text_light
                } else {
                    &brushes.text_secondary
                },
                Align::Center,
            );
        }
    }

    fn draw_toolbar_button(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        rect: UiRect,
        label: &str,
        active: bool,
        danger: bool,
        s: f32,
    ) {
        let fill = if active && danger {
            &brushes.red_soft
        } else if active {
            &brushes.accent
        } else {
            &brushes.control
        };
        let border = if active && danger {
            &brushes.red
        } else if active {
            &brushes.accent_dark
        } else {
            &brushes.border
        };
        fill_round(rt, rect, 4.0 * s, fill);
        stroke_round(rt, rect, 4.0 * s, border, 1.0);
        draw_text(
            rt,
            label,
            rect,
            &formats.body,
            if active {
                &brushes.text_light
            } else {
                &brushes.text_secondary
            },
            Align::Center,
        );
    }

    fn draw_meter_panel(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        rect: UiRect,
        detect: bool,
        s: f32,
    ) {
        card(rt, rect, 8.0 * s, brushes);
        let panel = MeterPanelLayout::new(rect, detect, s);
        let title = if detect { "Detect" } else { "Annihilation" };
        draw_text(
            rt,
            title,
            UiRect::new(rect.x, rect.y + 8.0 * s, rect.w, 18.0 * s),
            &formats.body,
            &brushes.text_secondary,
            Align::Center,
        );

        let value = if detect {
            u32_to_f32(self.meters.det_bits.load(Ordering::Relaxed))
        } else {
            u32_to_f32(self.meters.red_bits.load(Ordering::Relaxed))
        };
        let max_value = if detect {
            u32_to_f32(self.meters.det_max_bits.load(Ordering::Relaxed))
        } else {
            u32_to_f32(self.meters.red_max_bits.load(Ordering::Relaxed))
        };

        fill_round(rt, panel.max_rect, 4.0 * s, &brushes.control);
        stroke_round(rt, panel.max_rect, 4.0 * s, &brushes.border, 1.0);
        draw_text(
            rt,
            &format!("{max_value:.1}"),
            panel.max_rect,
            &formats.small,
            &brushes.text_secondary,
            Align::Center,
        );

        fill_round(rt, panel.meter_rect, 4.0 * s, &brushes.control);
        stroke_round(rt, panel.meter_rect, 4.0 * s, &brushes.border, 1.0);

        let norm = if detect {
            ((value + 60.0) / 60.0).clamp(0.0, 1.0)
        } else {
            (-value / 100.0).clamp(0.0, 1.0)
        };
        let fill_h = panel.meter_rect.h * norm;
        if fill_h > 1.0 {
            let fill = UiRect::new(
                panel.meter_rect.x,
                panel.meter_rect.bottom() - fill_h,
                panel.meter_rect.w,
                fill_h,
            );
            let brush = if norm > 0.75 {
                &brushes.red
            } else if norm > 0.55 {
                &brushes.yellow
            } else {
                &brushes.green
            };
            fill_round(rt, fill, 3.0 * s, brush);
        }

        fill_round(rt, panel.slider_rect, 3.0 * s, &brushes.control);
        stroke_round(rt, panel.slider_rect, 3.0 * s, &brushes.border, 1.0);

        let control_norm = if detect {
            (self.params.threshold.value() / 100.0).clamp(0.0, 1.0)
        } else {
            ((self.params.max_reduction.value() + 100.0) / 100.0).clamp(0.0, 1.0)
        };
        let thumb_y = panel.slider_rect.y + panel.slider_rect.h * (1.0 - control_norm);
        let thumb = UiRect::new(
            panel.slider_rect.x,
            thumb_y - 4.0 * s,
            panel.slider_rect.w,
            8.0 * s,
        );
        fill_round(
            rt,
            thumb,
            4.0 * s,
            if detect {
                &brushes.accent
            } else {
                &brushes.orange
            },
        );
        stroke_round(
            rt,
            thumb,
            4.0 * s,
            if detect {
                &brushes.accent_dark
            } else {
                &brushes.red_soft
            },
            1.0,
        );

        let bottom = if detect {
            format!("{:.0}%", self.params.threshold.value())
        } else {
            format!("{:.1} dB", self.params.max_reduction.value())
        };
        draw_text(
            rt,
            &bottom,
            panel.value_rect,
            &formats.small,
            &brushes.text_secondary,
            Align::Center,
        );
    }

    fn draw_controls(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        card(rt, layout.controls, 8.0 * layout.s, brushes);
        self.draw_segments(rt, brushes, formats, layout);
        self.draw_knobs(rt, brushes, formats, layout);
        self.draw_toggles(rt, brushes, formats, layout);
    }

    fn draw_segments(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        let s = layout.s;
        let groups = segment_groups(&self.params, layout);
        for group in groups {
            fill_round(rt, group.rect, 6.0 * s, &brushes.card);
            stroke_round(rt, group.rect, 6.0 * s, &brushes.border, 1.0);
            draw_text(
                rt,
                group.label,
                UiRect::new(group.rect.x, group.rect.y + 4.0 * s, group.rect.w, 16.0 * s),
                &formats.small,
                &brushes.text_tertiary,
                Align::Center,
            );
            for segment in group.segments {
                if segment.rect.w <= 0.0 {
                    continue;
                }
                let fill = if segment.active {
                    &brushes.accent
                } else {
                    &brushes.control
                };
                let text = if segment.active {
                    &brushes.text_light
                } else {
                    &brushes.text_secondary
                };
                fill_round(rt, segment.rect, 4.0 * s, fill);
                stroke_round(
                    rt,
                    segment.rect,
                    4.0 * s,
                    if segment.active {
                        &brushes.accent_dark
                    } else {
                        &brushes.border
                    },
                    1.0,
                );
                draw_text(
                    rt,
                    segment.label,
                    segment.rect,
                    &formats.small,
                    text,
                    Align::Center,
                );
            }
        }
    }

    fn draw_knobs(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        let s = layout.s;
        for group in knob_groups(layout) {
            draw_text(
                rt,
                group.label,
                UiRect::new(
                    group.rect.x + 8.0 * s,
                    group.rect.y - 10.0 * s,
                    group.rect.w - 16.0 * s,
                    10.0 * s,
                ),
                &formats.small,
                &brushes.text_tertiary,
                Align::Leading,
            );
            for spec in group.knobs {
                let value = self.target_value(spec.target);
                let norm = target_norm(spec.target, value);
                let disabled = spec.target == ControlTarget::LookaheadMs
                    && self.params.lookahead_enabled.value() <= 0.5;
                let accent = if disabled {
                    &brushes.text_tertiary
                } else {
                    spec.accent.brush(brushes)
                };
                draw_text(
                    rt,
                    spec.label,
                    UiRect::new(spec.rect.x, spec.rect.y + 2.0 * s, spec.rect.w, 10.0 * s),
                    &formats.small,
                    if disabled {
                        &brushes.text_tertiary
                    } else {
                        &brushes.text_secondary
                    },
                    Align::Center,
                );
                draw_knob(
                    rt,
                    spec.knob_rect.center_x(),
                    spec.knob_rect.center_y(),
                    spec.knob_rect.w.min(spec.knob_rect.h) * 0.5,
                    norm,
                    accent,
                    brushes,
                    s,
                );
                let value_rect = knob_value_rect(spec.rect, s);
                fill_round(rt, value_rect, 3.0 * s, &brushes.control);
                stroke_round(rt, value_rect, 3.0 * s, &brushes.border, 1.0);
                draw_text(
                    rt,
                    &format_value(spec.target, value),
                    value_rect,
                    &formats.small,
                    accent,
                    Align::Center,
                );
            }
        }
    }

    fn draw_toggles(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        let s = layout.s;
        for toggle in toggle_specs(layout) {
            let active = self.target_value(toggle.target) > 0.5;
            fill_round(
                rt,
                toggle.rect,
                6.0 * s,
                if active {
                    &brushes.accent_soft
                } else {
                    &brushes.card
                },
            );
            stroke_round(
                rt,
                toggle.rect,
                6.0 * s,
                if active {
                    &brushes.accent
                } else {
                    &brushes.border
                },
                1.0,
            );
            let pill = UiRect::new(
                toggle.rect.x + 8.0 * s,
                toggle.rect.y + 9.0 * s,
                34.0 * s,
                16.0 * s,
            );
            fill_round(
                rt,
                pill,
                8.0 * s,
                if active {
                    &brushes.accent
                } else {
                    &brushes.control
                },
            );
            let knob_x = if active {
                pill.right() - 8.0 * s
            } else {
                pill.x + 8.0 * s
            };
            fill_round(
                rt,
                UiRect::new(knob_x - 5.0 * s, pill.y + 3.0 * s, 10.0 * s, 10.0 * s),
                5.0 * s,
                &brushes.text_light,
            );
            draw_text(
                rt,
                toggle.label,
                UiRect::new(
                    toggle.rect.x + 48.0 * s,
                    toggle.rect.y,
                    toggle.rect.w - 54.0 * s,
                    toggle.rect.h,
                ),
                &formats.small,
                if active {
                    &brushes.text_primary
                } else {
                    &brushes.text_secondary
                },
                Align::Leading,
            );
        }
    }

    fn draw_spectrum(
        &mut self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        let s = layout.s;
        let rect = layout.spectrum;
        card(rt, rect, 8.0 * s, brushes);
        let inner = rect.shrink(8.0 * s);
        let graph = UiRect::new(inner.x, inner.y + 18.0 * s, inner.w, inner.h - 28.0 * s);
        draw_text(
            rt,
            "Spectrum Analyzer",
            UiRect::new(inner.x, inner.y, inner.w, 16.0 * s),
            &formats.body,
            &brushes.text_tertiary,
            Align::Leading,
        );

        for db in [-80.0_f32, -60.0, -40.0, -20.0, -10.0] {
            let y = graph.y + graph.h * (1.0 - ((db + 90.0) / 90.0));
            draw_line(rt, graph.x, y, graph.right(), y, &brushes.divider, 0.7);
            draw_text(
                rt,
                &format!("{}", db as i32),
                UiRect::new(graph.x + 2.0 * s, y - 13.0 * s, 34.0 * s, 12.0 * s),
                &formats.tiny,
                &brushes.text_tertiary,
                Align::Leading,
            );
        }
        for freq in [
            100.0_f32, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
        ] {
            let x = graph.x + freq_to_x(freq, graph.w);
            draw_line(rt, x, graph.y, x, graph.bottom(), &brushes.divider, 0.7);
            let label = if freq >= 1000.0 {
                format!("{}k", (freq / 1000.0) as i32)
            } else {
                format!("{}", freq as i32)
            };
            draw_text(
                rt,
                &label,
                UiRect::new(x - 16.0 * s, graph.bottom() + 1.0 * s, 32.0 * s, 12.0 * s),
                &formats.tiny,
                &brushes.text_tertiary,
                Align::Center,
            );
        }

        let min_x = graph.x + freq_to_x(self.params.min_freq.value(), graph.w);
        let max_x = graph.x + freq_to_x(self.params.max_freq.value(), graph.w);
        let node_y = graph.y + graph.h * 0.5;
        if max_x > min_x {
            fill_rect(
                rt,
                UiRect::new(min_x, graph.y, max_x - min_x, graph.h),
                &brushes.orange_wash,
            );
            draw_line(
                rt,
                min_x,
                graph.y,
                min_x,
                graph.bottom(),
                &brushes.teal,
                1.5 * s,
            );
            draw_line(
                rt,
                max_x,
                graph.y,
                max_x,
                graph.bottom(),
                &brushes.orange,
                1.5 * s,
            );
            draw_freq_node(
                rt,
                min_x,
                node_y,
                "Min",
                &brushes.teal,
                &brushes.card,
                formats,
                s,
            );
            draw_freq_node(
                rt,
                max_x,
                node_y,
                "Max",
                &brushes.orange,
                &brushes.card,
                formats,
                s,
            );
        }

        let (mags, sample_rate) = {
            let spec = self.spectrum.lock();
            (spec.magnitudes.clone(), spec.sample_rate as f32)
        };
        if !mags.is_empty() && sample_rate > 1000.0 {
            if self.smooth_mags.len() != mags.len() {
                self.smooth_mags = vec![-120.0; mags.len()];
            }
            for (idx, mag) in mags.iter().enumerate() {
                let m = mag.clamp(-90.0, 0.0);
                let current = self.smooth_mags[idx];
                self.smooth_mags[idx] = if m > current {
                    current * 0.3 + m * 0.7
                } else {
                    current * 0.85 + m * 0.15
                };
            }

            let nb = self.smooth_mags.len();
            let fft_size = ((nb.saturating_sub(1)) * 2).max(2);
            let columns = graph.w.max(2.0) as usize;
            let mut prev: Option<(f32, f32)> = None;
            for col in 0..=columns {
                let freq = x_to_freq(col as f32, graph.w);
                let bin_f = freq * fft_size as f32 / sample_rate;
                let bin_lo = (bin_f.floor() as usize).min(nb.saturating_sub(1));
                let bin_hi = (bin_lo + 1).min(nb.saturating_sub(1));
                let frac = (bin_f - bin_lo as f32).clamp(0.0, 1.0);
                let db_lo = self.smooth_mags.get(bin_lo).copied().unwrap_or(-90.0);
                let db_hi = self.smooth_mags.get(bin_hi).copied().unwrap_or(db_lo);
                let db = (db_lo + (db_hi - db_lo) * frac).clamp(-90.0, 0.0);
                let ny = 1.0 - ((db + 90.0) / 90.0);
                let point = (graph.x + col as f32, graph.y + ny * graph.h);
                if let Some(prev_point) = prev {
                    draw_line(
                        rt,
                        prev_point.0,
                        prev_point.1,
                        point.0,
                        point.1,
                        &brushes.accent_soft_line,
                        3.0 * s,
                    );
                    draw_line(
                        rt,
                        prev_point.0,
                        prev_point.1,
                        point.0,
                        point.1,
                        &brushes.accent_light,
                        1.2 * s,
                    );
                }
                prev = Some(point);
            }
        }
    }

    fn mouse_down(&mut self, x: f32, y: f32) {
        let _ = unsafe { SetFocus(Some(self.hwnd)) };
        let Some((w, h)) = self.logical_client_size() else {
            return;
        };
        let layout = Layout::new(w, h, self.render_scale());
        if self.handle_overlay_click(x, y, &layout) {
            invalidate(self.hwnd);
            return;
        }

        let zones = hit_zones(&self.params, &layout);
        for zone in zones {
            if !zone.rect.contains(x, y) {
                continue;
            }
            match zone.action {
                HitAction::Drag(target, track, mode) => {
                    self.begin_target(target);
                    self.drag_snapshot = Some(self.capture_snapshot());
                    self.drag = Some(DragState {
                        target,
                        track,
                        mode,
                        pointer_offset: if matches!(mode, DragMode::Horizontal) {
                            x - self.target_x(target, track)
                        } else {
                            0.0
                        },
                        start_y: y,
                        start_value: self.target_value(target),
                    });
                    unsafe {
                        let _ = SetCapture(self.hwnd);
                    }
                }
                HitAction::Set(target, value) => {
                    let before = self.capture_snapshot();
                    self.set_target_gesture(target, value);
                    self.record_undo(before);
                }
                HitAction::Toggle(target) => {
                    let before = self.capture_snapshot();
                    let next = if self.target_value(target) > 0.5 {
                        0.0
                    } else {
                        1.0
                    };
                    self.set_target_gesture(target, next);
                    self.record_undo(before);
                }
                HitAction::ResetDetect => {
                    self.meters.reset_det.store(1, Ordering::Release);
                }
                HitAction::ResetReduction => {
                    self.meters.reset_red.store(1, Ordering::Release);
                }
                HitAction::Command(action) => self.handle_command(action),
            }
            invalidate(self.hwnd);
            break;
        }
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        if let Some(drag) = self.drag {
            match drag.mode {
                DragMode::Horizontal => {
                    self.set_target_from_x(drag.target, drag.track, x - drag.pointer_offset)
                }
                DragMode::Vertical => {
                    self.set_target_from_y(drag.target, drag.start_value, drag.start_y, y)
                }
            }
            invalidate(self.hwnd);
        }
    }

    fn mouse_up(&mut self, x: f32, y: f32) {
        if let Some(drag) = self.drag.take() {
            match drag.mode {
                DragMode::Horizontal => {
                    self.set_target_from_x(drag.target, drag.track, x - drag.pointer_offset)
                }
                DragMode::Vertical => {
                    self.set_target_from_y(drag.target, drag.start_value, drag.start_y, y)
                }
            }
            self.end_target(drag.target);
            let _ = unsafe { ReleaseCapture() };
            if let Some(before) = self.drag_snapshot.take() {
                self.record_undo(before);
            }
            invalidate(self.hwnd);
        }
    }

    fn mouse_right_down(&mut self, x: f32, y: f32) {
        let _ = unsafe { SetFocus(Some(self.hwnd)) };
        if self.numeric_input.is_some() || self.preset_save_open || self.midi_popup_open {
            return;
        }

        let Some((w, h)) = self.logical_client_size() else {
            return;
        };
        let layout = Layout::new(w, h, self.render_scale());
        let bar = CommandBarRects::new(&layout);
        if bar.midi.contains(x, y) {
            self.midi_context_menu_open = !self.midi_context_menu_open;
            self.midi_cleanup_menu_open = false;
            self.preset_menu_open = false;
            invalidate(self.hwnd);
            return;
        }
        for zone in numeric_hit_zones(&self.params, &layout) {
            if zone.rect.contains(x, y) {
                self.open_numeric_input(zone.target);
                self.preset_menu_open = false;
                self.midi_context_menu_open = false;
                self.midi_cleanup_menu_open = false;
                invalidate(self.hwnd);
                break;
            }
        }
    }

    fn key_down(&mut self, vkey: u32) {
        match vkey {
            key if key == VK_ESCAPE.0 as u32 => {
                self.numeric_input = None;
                self.preset_save_open = false;
                self.preset_menu_open = false;
                self.midi_context_menu_open = false;
                self.midi_cleanup_menu_open = false;
                if self.midi_popup_open {
                    self.midi_learn.learning_target.store(-1, Ordering::Release);
                }
                self.midi_popup_open = false;
                invalidate(self.hwnd);
            }
            key if key == VK_RETURN.0 as u32 => {
                if self.numeric_input.is_some() {
                    self.confirm_numeric_input();
                    invalidate(self.hwnd);
                } else if self.preset_save_open {
                    self.confirm_preset_save();
                    invalidate(self.hwnd);
                }
            }
            _ => {}
        }
    }

    fn char_input(&mut self, ch: char) {
        if self.numeric_input.is_some() {
            let mut confirm = false;
            if let Some(input) = self.numeric_input.as_mut() {
                match ch {
                    '\u{8}' => {
                        input.value.pop();
                    }
                    '\r' | '\n' => confirm = true,
                    '-' | '.' | '0'..='9' => input.value.push(ch),
                    _ => {}
                }
            }
            if confirm {
                self.confirm_numeric_input();
            }
            invalidate(self.hwnd);
            return;
        }

        if self.preset_save_open {
            match ch {
                '\u{8}' => {
                    self.preset_name_buf.pop();
                }
                '\r' | '\n' => self.confirm_preset_save(),
                c if !c.is_control() => self.preset_name_buf.push(c),
                _ => {}
            }
            invalidate(self.hwnd);
        }
    }

    fn ensure_render_target(&mut self) -> Option<ID2D1HwndRenderTarget> {
        if self.render_target.is_none() {
            if self.d2d_factory.is_none() {
                self.d2d_factory = unsafe {
                    D2D1CreateFactory::<ID2D1Factory>(D2D1_FACTORY_TYPE_SINGLE_THREADED, None).ok()
                };
            }
            let factory = self.d2d_factory.as_ref()?;
            let (width, height) = client_size(self.hwnd)?;
            let rt_props = D2D1_RENDER_TARGET_PROPERTIES {
                r#type: D2D1_RENDER_TARGET_TYPE_DEFAULT,
                pixelFormat: D2D1_PIXEL_FORMAT {
                    format: DXGI_FORMAT_UNKNOWN,
                    alphaMode: D2D1_ALPHA_MODE_UNKNOWN,
                },
                dpiX: self.render_dpi(),
                dpiY: self.render_dpi(),
                usage: D2D1_RENDER_TARGET_USAGE_NONE,
                minLevel: D2D1_FEATURE_LEVEL_DEFAULT,
            };
            let hwnd_props = D2D1_HWND_RENDER_TARGET_PROPERTIES {
                hwnd: self.hwnd,
                pixelSize: D2D_SIZE_U {
                    width: width.max(1),
                    height: height.max(1),
                },
                presentOptions: D2D1_PRESENT_OPTIONS_NONE,
            };
            self.render_target =
                unsafe { factory.CreateHwndRenderTarget(&rt_props, &hwnd_props).ok() };
        }
        self.render_target.clone()
    }

    fn ensure_text_formats(&mut self, scale: f32) -> Option<TextFormats> {
        if self.text_formats.is_none() {
            if self.dwrite_factory.is_none() {
                self.dwrite_factory = unsafe {
                    DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED).ok()
                };
            }
            let factory = self.dwrite_factory.as_ref()?;
            let s = scale.max(0.45);
            self.text_formats = Some(TextFormats::new(factory, s)?);
        }
        self.text_formats.clone()
    }

    fn resize_to_parent(&mut self) {
        let _dpi_scope = DpiAwarenessScope::enter();
        self.refresh_dpi();
        let Some((parent_width, parent_height)) = client_size(self.parent_hwnd) else {
            return;
        };
        let Some((current_width, current_height)) = client_size(self.hwnd) else {
            return;
        };
        let (desired_width, desired_height) = self.desired_pixel_size();
        let target_width = parent_width.max(desired_width).max(1);
        let target_height = parent_height.max(desired_height).max(1);
        if target_width == current_width && target_height == current_height {
            return;
        }

        let _ = unsafe {
            SetWindowPos(
                self.hwnd,
                None,
                0,
                0,
                target_width as i32,
                target_height as i32,
                SWP_NOZORDER | SWP_NOACTIVATE,
            )
        };
        self.render_target = None;
        self.text_formats = None;
    }

    fn refresh_dpi(&mut self) {
        let dpi = dpi_for_window(self.hwnd);
        if dpi != self.dpi {
            self.dpi = dpi;
            self.update_size_scale();
            self.render_target = None;
            self.text_formats = None;
        }
    }

    fn handle_dpi_changed(&mut self, dpi: u32) {
        self.dpi = dpi.max(DEFAULT_DPI);
        self.update_size_scale();
        self.render_target = None;
        self.text_formats = None;
        let _ = self.context.request_resize();
        self.resize_to_parent();
        invalidate(self.hwnd);
    }

    fn update_size_scale(&self) {
        let size_scale = (self.render_scale() / self.host_scale.max(0.5)).clamp(1.0, 3.0);
        self.size_scale_bits
            .store(size_scale.to_bits(), Ordering::Release);
    }

    fn render_scale(&self) -> f32 {
        self.host_scale.max(dpi_scale(self.dpi)).clamp(0.5, 3.0)
    }

    fn render_dpi(&self) -> f32 {
        DEFAULT_DPI as f32 * self.render_scale()
    }

    fn desired_pixel_size(&self) -> (u32, u32) {
        let scale = self.render_scale();
        (
            (BASE_W * scale).round().max(1.0) as u32,
            (BASE_H * scale).round().max(1.0) as u32,
        )
    }

    fn logical_client_size(&self) -> Option<(f32, f32)> {
        let scale = self.render_scale();
        client_size(self.hwnd).map(|(width, height)| {
            (
                (width as f32 / scale).max(1.0),
                (height as f32 / scale).max(1.0),
            )
        })
    }

    fn logical_point(&self, x: f32, y: f32) -> (f32, f32) {
        let scale = self.render_scale();
        (x / scale, y / scale)
    }

    fn target_value(&self, target: ControlTarget) -> f32 {
        match target {
            ControlTarget::Threshold => self.params.threshold.value(),
            ControlTarget::MaxReduction => self.params.max_reduction.value(),
            ControlTarget::MinFreq => self.params.min_freq.value(),
            ControlTarget::MaxFreq => self.params.max_freq.value(),
            ControlTarget::ModeRelative => self.params.mode_relative.value(),
            ControlTarget::BasisMode => self.params.basis_mode.value(),
            ControlTarget::UseWideRange => self.params.use_wide_range.value(),
            ControlTarget::FilterSolo => self.params.filter_solo.value(),
            ControlTarget::LookaheadEnabled => self.params.lookahead_enabled.value(),
            ControlTarget::LookaheadMs => self.params.lookahead_ms.value(),
            ControlTarget::TriggerHear => self.params.trigger_hear.value(),
            ControlTarget::StereoLink => self.params.stereo_link.value(),
            ControlTarget::StereoMidSide => self.params.stereo_mid_side.value(),
            ControlTarget::SidechainMode => self.params.sidechain_mode.value(),
            ControlTarget::VocalMode => self.params.vocal_mode.value(),
            ControlTarget::InputLevel => self.params.input_level.value(),
            ControlTarget::InputPan => self.params.input_pan.value(),
            ControlTarget::OutputLevel => self.params.output_level.value(),
            ControlTarget::OutputPan => self.params.output_pan.value(),
            ControlTarget::Bypass => self.params.bypass.value(),
            ControlTarget::Oversampling => self.params.oversampling.value(),
            ControlTarget::CutWidth => self.params.cut_width.value(),
            ControlTarget::CutDepth => self.params.cut_depth.value(),
            ControlTarget::Mix => self.params.mix.value(),
            ControlTarget::CutSlope => self.params.cut_slope.value(),
        }
    }

    fn begin_target(&self, target: ControlTarget) {
        let setter = ParamSetter::new(self.context.as_ref());
        with_param(target, &self.params, |param| {
            setter.begin_set_parameter(param)
        });
    }

    fn end_target(&self, target: ControlTarget) {
        let setter = ParamSetter::new(self.context.as_ref());
        with_param(target, &self.params, |param| {
            setter.end_set_parameter(param)
        });
    }

    fn set_target_gesture(&self, target: ControlTarget, value: f32) {
        self.begin_target(target);
        self.set_target_plain(target, value);
        self.end_target(target);
    }

    fn set_target_plain(&self, target: ControlTarget, value: f32) {
        let setter = ParamSetter::new(self.context.as_ref());
        let value = target_clamp(target, value);
        with_param(target, &self.params, |param| {
            setter.set_parameter(param, value)
        });
    }

    fn set_target_from_x(&self, target: ControlTarget, track: UiRect, x: f32) {
        let norm = ((x - track.x) / track.w).clamp(0.0, 1.0);
        self.set_target_plain(target, value_from_norm(target, norm));
    }

    fn set_target_from_y(&self, target: ControlTarget, start_value: f32, start_y: f32, y: f32) {
        let start_norm = target_norm(target, start_value);
        let norm = (start_norm + (start_y - y) * 0.006).clamp(0.0, 1.0);
        self.set_target_plain(target, value_from_norm(target, norm));
    }

    fn target_x(&self, target: ControlTarget, track: UiRect) -> f32 {
        track.x + track.w * target_norm(target, self.target_value(target))
    }

    fn capture_snapshot(&self) -> ParamSnapshot {
        ParamSnapshot {
            threshold: self.params.threshold.value(),
            max_reduction: self.params.max_reduction.value(),
            min_freq: self.params.min_freq.value(),
            max_freq: self.params.max_freq.value(),
            mode_relative: self.params.mode_relative.value() > 0.5,
            basis_mode: self.params.basis_mode.value().round() as i32,
            use_wide_range: self.params.use_wide_range.value() > 0.5,
            filter_solo: self.params.filter_solo.value() > 0.5,
            lookahead_enabled: self.params.lookahead_enabled.value() > 0.5,
            lookahead_ms: self.params.lookahead_ms.value(),
            trigger_hear: self.params.trigger_hear.value() > 0.5,
            stereo_link: self.params.stereo_link.value(),
            stereo_mid_side: self.params.stereo_mid_side.value() > 0.5,
            sidechain_mode: self.params.sidechain_mode.value().round().clamp(0.0, 2.0) as i32,
            vocal_mode: self.params.vocal_mode.value() > 0.5,
            input_level: self.params.input_level.value(),
            input_pan: self.params.input_pan.value(),
            output_level: self.params.output_level.value(),
            output_pan: self.params.output_pan.value(),
            bypass: self.params.bypass.value() > 0.5,
            oversampling: self.params.oversampling.value().round() as i32,
            cut_width: self.params.cut_width.value(),
            cut_depth: self.params.cut_depth.value(),
            mix: self.params.mix.value(),
            cut_slope: self.params.cut_slope.value(),
        }
    }

    fn apply_snapshot(&self, snapshot: &ParamSnapshot) {
        self.set_target_plain(ControlTarget::Threshold, snapshot.threshold);
        self.set_target_plain(ControlTarget::MaxReduction, snapshot.max_reduction);
        self.set_target_plain(ControlTarget::MinFreq, snapshot.min_freq);
        self.set_target_plain(ControlTarget::MaxFreq, snapshot.max_freq);
        self.set_target_plain(
            ControlTarget::ModeRelative,
            if snapshot.mode_relative { 1.0 } else { 0.0 },
        );
        self.set_target_plain(ControlTarget::BasisMode, snapshot.basis_mode as f32);
        self.set_target_plain(
            ControlTarget::UseWideRange,
            if snapshot.use_wide_range { 1.0 } else { 0.0 },
        );
        self.set_target_plain(
            ControlTarget::FilterSolo,
            if snapshot.filter_solo { 1.0 } else { 0.0 },
        );
        self.set_target_plain(
            ControlTarget::LookaheadEnabled,
            if snapshot.lookahead_enabled { 1.0 } else { 0.0 },
        );
        self.set_target_plain(ControlTarget::LookaheadMs, snapshot.lookahead_ms);
        self.set_target_plain(
            ControlTarget::TriggerHear,
            if snapshot.trigger_hear { 1.0 } else { 0.0 },
        );
        self.set_target_plain(ControlTarget::StereoLink, snapshot.stereo_link);
        self.set_target_plain(
            ControlTarget::StereoMidSide,
            if snapshot.stereo_mid_side { 1.0 } else { 0.0 },
        );
        self.set_target_plain(ControlTarget::SidechainMode, snapshot.sidechain_mode as f32);
        self.set_target_plain(
            ControlTarget::VocalMode,
            if snapshot.vocal_mode { 1.0 } else { 0.0 },
        );
        self.set_target_plain(ControlTarget::InputLevel, snapshot.input_level);
        self.set_target_plain(ControlTarget::InputPan, snapshot.input_pan);
        self.set_target_plain(ControlTarget::OutputLevel, snapshot.output_level);
        self.set_target_plain(ControlTarget::OutputPan, snapshot.output_pan);
        self.set_target_plain(
            ControlTarget::Bypass,
            if snapshot.bypass { 1.0 } else { 0.0 },
        );
        self.set_target_plain(ControlTarget::Oversampling, snapshot.oversampling as f32);
        self.set_target_plain(ControlTarget::CutWidth, snapshot.cut_width);
        self.set_target_plain(ControlTarget::CutDepth, snapshot.cut_depth);
        self.set_target_plain(ControlTarget::Mix, snapshot.mix);
        self.set_target_plain(ControlTarget::CutSlope, snapshot.cut_slope);
    }

    fn record_undo(&mut self, before: ParamSnapshot) {
        if self.capture_snapshot() != before {
            self.undo_stack.push(before);
            self.undo_stack.truncate(50);
            self.redo_stack.clear();
        }
    }

    fn handle_command(&mut self, action: CommandAction) {
        match action {
            CommandAction::TogglePresetMenu => {
                self.preset_menu_open = !self.preset_menu_open && !self.presets.is_empty();
                self.numeric_input = None;
                self.preset_save_open = false;
                self.midi_popup_open = false;
                self.midi_context_menu_open = false;
                self.midi_cleanup_menu_open = false;
            }
            CommandAction::OpenPresetSave => {
                self.preset_save_open = true;
                self.preset_menu_open = false;
                self.midi_popup_open = false;
                self.numeric_input = None;
                self.midi_context_menu_open = false;
                self.midi_cleanup_menu_open = false;
                self.preset_name_buf.clear();
                let _ = unsafe { SetFocus(Some(self.hwnd)) };
            }
            CommandAction::DeletePreset => {
                if !self.presets.is_empty() {
                    self.presets
                        .remove(self.selected_preset.min(self.presets.len() - 1));
                    if self.selected_preset >= self.presets.len() && self.selected_preset > 0 {
                        self.selected_preset -= 1;
                    }
                    self.persist_presets();
                }
                self.preset_menu_open = false;
                self.midi_context_menu_open = false;
                self.midi_cleanup_menu_open = false;
            }
            CommandAction::Undo => self.undo(),
            CommandAction::Redo => self.redo(),
            CommandAction::ToggleAB => self.toggle_ab(),
            CommandAction::MidiLearn => {
                let learning = self.midi_learn.learning_target.load(Ordering::Relaxed) >= 0;
                if learning {
                    self.midi_learn.learning_target.store(-1, Ordering::Release);
                } else {
                    self.midi_popup_open = !self.midi_popup_open;
                    self.preset_menu_open = false;
                    self.preset_save_open = false;
                    self.numeric_input = None;
                    self.midi_context_menu_open = false;
                    self.midi_cleanup_menu_open = false;
                }
            }
        }
    }

    fn undo(&mut self) {
        if let Some(snapshot) = self.undo_stack.pop() {
            let current = self.capture_snapshot();
            self.redo_stack.push(current);
            self.redo_stack.truncate(50);
            self.apply_snapshot(&snapshot);
        }
    }

    fn redo(&mut self) {
        if let Some(snapshot) = self.redo_stack.pop() {
            let current = self.capture_snapshot();
            self.undo_stack.push(current);
            self.undo_stack.truncate(50);
            self.apply_snapshot(&snapshot);
        }
    }

    fn toggle_ab(&mut self) {
        let before = self.capture_snapshot();
        match self.active_state {
            'A' => self.state_a = Some(before.clone()),
            'B' => self.state_b = Some(before.clone()),
            _ => {}
        }

        self.active_state = if self.active_state == 'A' { 'B' } else { 'A' };
        let target = match self.active_state {
            'A' => self.state_a.clone(),
            'B' => self.state_b.clone(),
            _ => None,
        };
        if let Some(snapshot) = target {
            self.apply_snapshot(&snapshot);
            self.record_undo(before);
        }
    }

    fn open_numeric_input(&mut self, target: ControlTarget) {
        let (label, min, max) = numeric_spec(target);
        self.numeric_input = Some(NumericInput {
            target,
            label: label.to_string(),
            value: format!("{:.2}", self.target_value(target)),
            min,
            max,
        });
    }

    fn confirm_numeric_input(&mut self) {
        let Some(input) = self.numeric_input.take() else {
            return;
        };
        if let Ok(value) = input.value.trim().parse::<f32>() {
            let before = self.capture_snapshot();
            self.set_target_gesture(input.target, value.clamp(input.min, input.max));
            self.record_undo(before);
        }
    }

    fn confirm_preset_save(&mut self) {
        let name = default_preset_name(self.preset_name_buf.trim(), &self.presets);

        let snapshot = self.capture_snapshot();
        if let Some(index) = self
            .presets
            .iter()
            .position(|(preset_name, _)| preset_name == &name)
        {
            self.presets[index].1 = snapshot;
            self.selected_preset = index;
        } else {
            self.presets.push((name, snapshot));
            self.selected_preset = self.presets.len() - 1;
        }
        self.persist_presets();
        self.preset_save_open = false;
        self.preset_menu_open = false;
    }

    fn persist_midi_mapping(&self) {
        self.midi_learn.persist_as_saved(&self.storage);
    }

    fn persist_presets(&self) {
        let presets = self
            .presets
            .iter()
            .map(|(name, snapshot)| StoredPreset {
                name: name.clone(),
                snapshot: snapshot.to_stored(),
            })
            .collect();
        self.storage.save_presets(presets);
    }

    fn handle_overlay_click(&mut self, x: f32, y: f32, layout: &Layout) -> bool {
        if let Some(input) = self.numeric_input.as_ref() {
            let popup = NumericPopupLayout::new(layout, input);
            if popup.ok.contains(x, y) {
                self.confirm_numeric_input();
                return true;
            }
            if popup.cancel.contains(x, y) {
                self.numeric_input = None;
                return true;
            }
            return popup.dialog.contains(x, y) || layout.full.contains(x, y);
        }

        if self.preset_save_open {
            let popup = PresetSavePopupLayout::new(layout);
            if popup.ok.contains(x, y) {
                self.confirm_preset_save();
                return true;
            }
            if popup.cancel.contains(x, y) {
                self.preset_save_open = false;
                return true;
            }
            return popup.dialog.contains(x, y) || layout.full.contains(x, y);
        }

        if self.midi_popup_open {
            let popup = MidiPopupLayout::new(layout, layout.s);
            if popup.clear.contains(x, y) {
                self.midi_learn.mappings.lock().clear();
                self.midi_learn.sync_atomic_from_mutex();
                self.midi_learn.learning_target.store(-1, Ordering::Release);
                return true;
            }
            if popup.close.contains(x, y) {
                self.midi_learn.learning_target.store(-1, Ordering::Release);
                self.midi_popup_open = false;
                return true;
            }
            if let Some(index) = popup.hit_row(x, y) {
                let current = self.midi_learn.learning_target.load(Ordering::Relaxed);
                let next = if current == index as i32 {
                    -1
                } else {
                    index as i32
                };
                self.midi_learn
                    .learning_target
                    .store(next, Ordering::Release);
                return true;
            }
            return popup.dialog.contains(x, y) || layout.full.contains(x, y);
        }

        if self.midi_context_menu_open {
            self.midi_learn.sync_mutex_from_atomic_if_needed();
            let menu = MidiContextMenuLayout::new(layout);
            if self.midi_cleanup_menu_open {
                let cleanup =
                    MidiCleanupMenuLayout::new(layout, self.midi_learn.mappings.lock().len());
                if cleanup.clear.contains(x, y) {
                    self.midi_learn.mappings.lock().clear();
                    self.midi_learn.sync_atomic_from_mutex();
                    self.midi_learn.learning_target.store(-1, Ordering::Release);
                    self.midi_cleanup_menu_open = false;
                    self.midi_context_menu_open = false;
                    return true;
                }
                if let Some(index) = cleanup.hit_mapping(x, y) {
                    let mut sorted: Vec<u8> =
                        self.midi_learn.mappings.lock().keys().copied().collect();
                    sorted.sort_unstable();
                    if let Some(cc) = sorted.get(index).copied() {
                        self.midi_learn.mappings.lock().remove(&cc);
                        self.midi_learn.sync_atomic_from_mutex();
                    }
                    return true;
                }
                if cleanup.rect.contains(x, y) {
                    return true;
                }
            }

            if let Some(index) = menu.hit_item(x, y) {
                match index {
                    0 => {
                        let enabled = self.midi_learn.midi_enabled.load(Ordering::Relaxed);
                        self.midi_learn
                            .midi_enabled
                            .store(!enabled, Ordering::Release);
                        self.persist_midi_mapping();
                        self.midi_context_menu_open = false;
                    }
                    1 => {
                        self.midi_cleanup_menu_open = true;
                    }
                    2 => {
                        let saved = self.midi_learn.saved_mappings.lock().clone();
                        *self.midi_learn.mappings.lock() = saved;
                        self.midi_learn.sync_atomic_from_mutex();
                        self.midi_learn.learning_target.store(-1, Ordering::Release);
                        self.midi_context_menu_open = false;
                        self.midi_cleanup_menu_open = false;
                    }
                    3 => {
                        self.persist_midi_mapping();
                        self.midi_context_menu_open = false;
                        self.midi_cleanup_menu_open = false;
                    }
                    4 => {
                        self.midi_context_menu_open = false;
                        self.midi_cleanup_menu_open = false;
                    }
                    _ => {}
                }
                return true;
            }

            self.midi_context_menu_open = false;
            self.midi_cleanup_menu_open = false;
            return true;
        }

        if self.preset_menu_open {
            let popup = PresetMenuLayout::new(layout, self.presets.len());
            if let Some(index) = popup.hit_item(x, y) {
                if let Some((_, snapshot)) = self.presets.get(index).cloned() {
                    let before = self.capture_snapshot();
                    self.selected_preset = index;
                    self.apply_snapshot(&snapshot);
                    self.record_undo(before);
                }
                self.preset_menu_open = false;
                return true;
            }
            if !popup.rect.contains(x, y) {
                self.preset_menu_open = false;
            }
        }

        false
    }

    fn draw_numeric_popup(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        let Some(input) = self.numeric_input.as_ref() else {
            return;
        };
        let popup = NumericPopupLayout::new(layout, input);
        fill_rect(rt, layout.full, &brushes.mica_bot);
        card(rt, popup.dialog, 10.0 * layout.s, brushes);
        draw_text(
            rt,
            &input.label,
            popup.title,
            &formats.body,
            &brushes.text_primary,
            Align::Center,
        );
        fill_round(rt, popup.field, 4.0 * layout.s, &brushes.control);
        stroke_round(rt, popup.field, 4.0 * layout.s, &brushes.accent, 1.0);
        draw_text(
            rt,
            &input.value,
            popup.field,
            &formats.body,
            &brushes.text_primary,
            Align::Center,
        );
        self.draw_toolbar_button(rt, brushes, formats, popup.ok, "OK", true, false, layout.s);
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            popup.cancel,
            "Cancel",
            false,
            false,
            layout.s,
        );
    }

    fn draw_preset_save_popup(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        if !self.preset_save_open {
            return;
        }
        let popup = PresetSavePopupLayout::new(layout);
        fill_rect(rt, layout.full, &brushes.mica_bot);
        card(rt, popup.dialog, 10.0 * layout.s, brushes);
        draw_text(
            rt,
            "Save Preset",
            popup.title,
            &formats.body,
            &brushes.text_primary,
            Align::Center,
        );
        fill_round(rt, popup.field, 4.0 * layout.s, &brushes.control);
        stroke_round(rt, popup.field, 4.0 * layout.s, &brushes.accent, 1.0);
        let text = if self.preset_name_buf.is_empty() {
            "Preset name"
        } else {
            &self.preset_name_buf
        };
        draw_text(
            rt,
            text,
            popup.field,
            &formats.body,
            if self.preset_name_buf.is_empty() {
                &brushes.text_tertiary
            } else {
                &brushes.text_primary
            },
            Align::Center,
        );
        self.draw_toolbar_button(
            rt, brushes, formats, popup.ok, "Save", true, false, layout.s,
        );
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            popup.cancel,
            "Cancel",
            false,
            false,
            layout.s,
        );
    }

    fn draw_preset_menu(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        if !self.preset_menu_open || self.presets.is_empty() {
            return;
        }
        let popup = PresetMenuLayout::new(layout, self.presets.len());
        card(rt, popup.rect, 8.0 * layout.s, brushes);
        for (index, (name, _)) in self.presets.iter().enumerate() {
            let row = popup.item_rect(index);
            if index == self.selected_preset {
                fill_round(
                    rt,
                    row.shrink(2.0 * layout.s),
                    4.0 * layout.s,
                    &brushes.accent_soft,
                );
                stroke_round(
                    rt,
                    row.shrink(2.0 * layout.s),
                    4.0 * layout.s,
                    &brushes.accent,
                    1.0,
                );
            }
            draw_text(
                rt,
                truncate_label(name, 18),
                row,
                &formats.body,
                if index == self.selected_preset {
                    &brushes.text_primary
                } else {
                    &brushes.text_secondary
                },
                Align::Center,
            );
        }
    }

    fn draw_midi_popup(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        if !self.midi_popup_open {
            return;
        }
        self.midi_learn.sync_mutex_from_atomic_if_needed();
        let popup = MidiPopupLayout::new(layout, layout.s);
        fill_rect(rt, layout.full, &brushes.mica_bot);
        card(rt, popup.dialog, 10.0 * layout.s, brushes);
        draw_text(
            rt,
            "MIDI Learn",
            popup.title,
            &formats.body,
            &brushes.text_primary,
            Align::Center,
        );
        draw_text(
            rt,
            "Select a parameter, then move a MIDI CC",
            popup.subtitle,
            &formats.small,
            &brushes.text_secondary,
            Align::Center,
        );

        let learning = self.midi_learn.learning_target.load(Ordering::Relaxed);
        let mappings = self.midi_learn.mappings.lock().clone();
        for (index, label) in MIDI_PARAM_NAMES.iter().enumerate().take(MIDI_PARAM_COUNT) {
            let row = popup.row_rect(index);
            let selected = learning == index as i32;
            fill_round(
                rt,
                row,
                4.0 * layout.s,
                if selected {
                    &brushes.accent
                } else {
                    &brushes.control
                },
            );
            stroke_round(
                rt,
                row,
                4.0 * layout.s,
                if selected {
                    &brushes.accent_dark
                } else {
                    &brushes.border
                },
                1.0,
            );
            draw_text(
                rt,
                label,
                UiRect::new(row.x + 8.0 * layout.s, row.y, row.w * 0.66, row.h),
                &formats.tiny,
                if selected {
                    &brushes.text_light
                } else {
                    &brushes.text_primary
                },
                Align::Leading,
            );
            let cc_label = mappings
                .iter()
                .find(|(_, param)| **param == index as u8)
                .map(|(cc, _)| format!("CC{cc}"))
                .unwrap_or_else(|| "-".to_string());
            draw_text(
                rt,
                &cc_label,
                UiRect::new(row.right() - 52.0 * layout.s, row.y, 44.0 * layout.s, row.h),
                &formats.tiny,
                if selected {
                    &brushes.text_light
                } else {
                    &brushes.text_tertiary
                },
                Align::Trailing,
            );
        }

        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            popup.clear,
            "Clear All",
            true,
            true,
            layout.s,
        );
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            popup.close,
            "Close",
            true,
            false,
            layout.s,
        );
    }

    fn draw_midi_context_menu(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        if !self.midi_context_menu_open {
            return;
        }

        let s = layout.s;
        let menu = MidiContextMenuLayout::new(layout);
        let enabled = self.midi_learn.midi_enabled.load(Ordering::Relaxed);
        let items = ["MIDI On/Off", "Clean Up...", "Roll Back", "Save", "Close"];

        card(rt, menu.rect, 8.0 * s, brushes);
        for (index, label) in items.iter().enumerate() {
            let row = menu.item_rect(index);
            let emphasized = (index == 0 && enabled) || (index == 1 && self.midi_cleanup_menu_open);
            fill_round(
                rt,
                row,
                4.0 * s,
                if emphasized {
                    &brushes.accent_soft
                } else {
                    &brushes.control
                },
            );
            stroke_round(
                rt,
                row,
                4.0 * s,
                if emphasized {
                    &brushes.accent
                } else {
                    &brushes.border
                },
                1.0,
            );
            draw_text(
                rt,
                label,
                UiRect::new(row.x + 10.0 * s, row.y, row.w - 56.0 * s, row.h),
                &formats.small,
                if emphasized {
                    &brushes.text_primary
                } else {
                    &brushes.text_secondary
                },
                Align::Leading,
            );

            match index {
                0 => draw_text(
                    rt,
                    if enabled { "On" } else { "Off" },
                    UiRect::new(row.right() - 40.0 * s, row.y, 28.0 * s, row.h),
                    &formats.tiny,
                    if enabled {
                        &brushes.green
                    } else {
                        &brushes.red
                    },
                    Align::Trailing,
                ),
                1 => draw_text(
                    rt,
                    ">",
                    UiRect::new(row.right() - 28.0 * s, row.y, 16.0 * s, row.h),
                    &formats.body,
                    &brushes.text_tertiary,
                    Align::Trailing,
                ),
                _ => {}
            }
        }
    }

    fn draw_midi_cleanup_menu(
        &self,
        rt: &ID2D1HwndRenderTarget,
        brushes: &Brushes,
        formats: &TextFormats,
        layout: &Layout,
    ) {
        if !self.midi_context_menu_open || !self.midi_cleanup_menu_open {
            return;
        }

        self.midi_learn.sync_mutex_from_atomic_if_needed();
        let mappings = self.midi_learn.mappings.lock().clone();
        let mut sorted: Vec<(u8, u8)> = mappings.iter().map(|(&cc, &param)| (cc, param)).collect();
        sorted.sort_by_key(|&(cc, _)| cc);

        let s = layout.s;
        let menu = MidiCleanupMenuLayout::new(layout, sorted.len());
        card(rt, menu.rect, 8.0 * s, brushes);

        if sorted.is_empty() {
            let row = menu.empty_rect();
            fill_round(rt, row, 4.0 * s, &brushes.control);
            stroke_round(rt, row, 4.0 * s, &brushes.border, 1.0);
            draw_text(
                rt,
                "No mappings",
                row,
                &formats.small,
                &brushes.text_secondary,
                Align::Center,
            );
        } else {
            for (index, (cc, param_index)) in sorted.iter().enumerate() {
                let row = menu.mapping_rect(index);
                let label = format!(
                    "CC{cc} -> {}",
                    MIDI_PARAM_NAMES
                        .get(*param_index as usize)
                        .copied()
                        .unwrap_or("?")
                );
                fill_round(rt, row, 4.0 * s, &brushes.control);
                stroke_round(rt, row, 4.0 * s, &brushes.border, 1.0);
                draw_text(
                    rt,
                    &label,
                    UiRect::new(row.x + 10.0 * s, row.y, row.w - 66.0 * s, row.h),
                    &formats.tiny,
                    &brushes.text_secondary,
                    Align::Leading,
                );
                draw_text(
                    rt,
                    "Delete",
                    UiRect::new(row.right() - 52.0 * s, row.y, 40.0 * s, row.h),
                    &formats.tiny,
                    &brushes.red,
                    Align::Trailing,
                );
            }
        }

        fill_round(
            rt,
            menu.clear,
            4.0 * s,
            if sorted.is_empty() {
                &brushes.control
            } else {
                &brushes.red_soft
            },
        );
        stroke_round(
            rt,
            menu.clear,
            4.0 * s,
            if sorted.is_empty() {
                &brushes.border
            } else {
                &brushes.red
            },
            1.0,
        );
        draw_text(
            rt,
            "Clear All",
            menu.clear,
            &formats.small,
            if sorted.is_empty() {
                &brushes.text_tertiary
            } else {
                &brushes.red
            },
            Align::Center,
        );
    }
}

#[derive(Clone)]
struct TextFormats {
    tiny: IDWriteTextFormat,
    small: IDWriteTextFormat,
    body: IDWriteTextFormat,
    title: IDWriteTextFormat,
}

impl TextFormats {
    fn new(factory: &IDWriteFactory, scale: f32) -> Option<Self> {
        Some(Self {
            tiny: create_text_format(factory, 9.0 * scale, false)?,
            small: create_text_format(factory, 11.0 * scale, false)?,
            body: create_text_format(factory, 13.0 * scale, false)?,
            title: create_text_format(factory, 17.0 * scale, true)?,
        })
    }
}

fn create_text_format(
    factory: &IDWriteFactory,
    size: f32,
    bold: bool,
) -> Option<IDWriteTextFormat> {
    let format = unsafe {
        factory
            .CreateTextFormat(
                w!("Segoe UI"),
                Option::<&IDWriteFontCollection>::None,
                if bold {
                    DWRITE_FONT_WEIGHT_DEMI_BOLD
                } else {
                    DWRITE_FONT_WEIGHT_NORMAL
                },
                DWRITE_FONT_STYLE_NORMAL,
                DWRITE_FONT_STRETCH_NORMAL,
                size,
                w!("en-us"),
            )
            .ok()?
    };
    let _ = unsafe { format.SetTextAlignment(DWRITE_TEXT_ALIGNMENT_LEADING) };
    let _ = unsafe { format.SetParagraphAlignment(DWRITE_PARAGRAPH_ALIGNMENT_CENTER) };
    Some(format)
}

struct Brushes {
    mica_base: ID2D1SolidColorBrush,
    mica_top: ID2D1SolidColorBrush,
    mica_bot: ID2D1SolidColorBrush,
    panel: ID2D1SolidColorBrush,
    card: ID2D1SolidColorBrush,
    control: ID2D1SolidColorBrush,
    border: ID2D1SolidColorBrush,
    divider: ID2D1SolidColorBrush,
    accent: ID2D1SolidColorBrush,
    accent_dark: ID2D1SolidColorBrush,
    accent_light: ID2D1SolidColorBrush,
    accent_soft: ID2D1SolidColorBrush,
    accent_soft_line: ID2D1SolidColorBrush,
    orange: ID2D1SolidColorBrush,
    orange_wash: ID2D1SolidColorBrush,
    red: ID2D1SolidColorBrush,
    red_soft: ID2D1SolidColorBrush,
    teal: ID2D1SolidColorBrush,
    magenta: ID2D1SolidColorBrush,
    purple: ID2D1SolidColorBrush,
    green: ID2D1SolidColorBrush,
    yellow: ID2D1SolidColorBrush,
    text_light: ID2D1SolidColorBrush,
    text_primary: ID2D1SolidColorBrush,
    text_secondary: ID2D1SolidColorBrush,
    text_tertiary: ID2D1SolidColorBrush,
}

impl Brushes {
    fn new(rt: &ID2D1HwndRenderTarget) -> Option<Self> {
        Some(Self {
            mica_base: solid(rt, Colors::MICA_BASE)?,
            mica_top: solid(rt, Colors::MICA_TOP)?,
            mica_bot: solid(rt, Colors::MICA_BOT)?,
            panel: solid(rt, Colors::PANEL)?,
            card: solid(rt, Colors::CARD)?,
            control: solid(rt, Colors::CONTROL)?,
            border: solid(rt, Colors::BORDER)?,
            divider: solid(rt, Colors::DIVIDER)?,
            accent: solid(rt, Colors::ACCENT)?,
            accent_dark: solid(rt, Colors::ACCENT_DARK)?,
            accent_light: solid(rt, Colors::ACCENT_LIGHT)?,
            accent_soft: solid(rt, Colors::ACCENT_SOFT)?,
            accent_soft_line: solid(rt, Colors::ACCENT_SOFT_LINE)?,
            orange: solid(rt, Colors::ORANGE)?,
            orange_wash: solid(rt, Colors::ORANGE_WASH)?,
            red: solid(rt, Colors::RED)?,
            red_soft: solid(rt, Colors::RED_SOFT)?,
            teal: solid(rt, Colors::TEAL)?,
            magenta: solid(rt, Colors::MAGENTA)?,
            purple: solid(rt, Colors::PURPLE)?,
            green: solid(rt, Colors::GREEN)?,
            yellow: solid(rt, Colors::YELLOW)?,
            text_light: solid(rt, Colors::TEXT_LIGHT)?,
            text_primary: solid(rt, Colors::TEXT_PRIMARY)?,
            text_secondary: solid(rt, Colors::TEXT_SECONDARY)?,
            text_tertiary: solid(rt, Colors::TEXT_TERTIARY)?,
        })
    }
}

struct Colors;

impl Colors {
    const MICA_BASE: D2D1_COLOR_F = color(4, 2, 14, 255);
    const MICA_TOP: D2D1_COLOR_F = color(8, 4, 22, 255);
    const MICA_BOT: D2D1_COLOR_F = color(2, 1, 8, 255);
    const PANEL: D2D1_COLOR_F = color(10, 6, 26, 255);
    const CARD: D2D1_COLOR_F = color(14, 8, 34, 255);
    const CONTROL: D2D1_COLOR_F = color(18, 10, 38, 255);
    const BORDER: D2D1_COLOR_F = color(50, 30, 90, 255);
    const DIVIDER: D2D1_COLOR_F = color(30, 18, 60, 255);
    const ACCENT: D2D1_COLOR_F = color(0, 210, 255, 255);
    const ACCENT_DARK: D2D1_COLOR_F = color(0, 90, 130, 255);
    const ACCENT_LIGHT: D2D1_COLOR_F = color(80, 235, 255, 230);
    const ACCENT_SOFT: D2D1_COLOR_F = color(0, 210, 255, 40);
    const ACCENT_SOFT_LINE: D2D1_COLOR_F = color(0, 210, 255, 45);
    const ORANGE: D2D1_COLOR_F = color(255, 160, 0, 255);
    const ORANGE_WASH: D2D1_COLOR_F = color(255, 160, 0, 28);
    const RED: D2D1_COLOR_F = color(255, 55, 55, 255);
    const RED_SOFT: D2D1_COLOR_F = color(255, 55, 55, 42);
    const TEAL: D2D1_COLOR_F = color(0, 200, 180, 255);
    const MAGENTA: D2D1_COLOR_F = color(255, 0, 200, 255);
    const PURPLE: D2D1_COLOR_F = color(160, 40, 255, 255);
    const GREEN: D2D1_COLOR_F = color(0, 220, 90, 255);
    const YELLOW: D2D1_COLOR_F = color(255, 200, 0, 255);
    const TEXT_LIGHT: D2D1_COLOR_F = color(255, 255, 255, 255);
    const TEXT_PRIMARY: D2D1_COLOR_F = color(210, 235, 255, 255);
    const TEXT_SECONDARY: D2D1_COLOR_F = color(120, 165, 210, 255);
    const TEXT_TERTIARY: D2D1_COLOR_F = color(55, 85, 140, 255);
}

const fn color(r: u8, g: u8, b: u8, a: u8) -> D2D1_COLOR_F {
    D2D1_COLOR_F {
        r: r as f32 / 255.0,
        g: g as f32 / 255.0,
        b: b as f32 / 255.0,
        a: a as f32 / 255.0,
    }
}

#[derive(Clone, Copy)]
struct Layout {
    full: UiRect,
    header: UiRect,
    command_bar: UiRect,
    left_panel: UiRect,
    right_panel: UiRect,
    controls: UiRect,
    spectrum: UiRect,
    s: f32,
}

impl Layout {
    fn new(w: f32, h: f32, _scale_hint: f32) -> Self {
        let s = (w / BASE_W).min(h / BASE_H).max(0.45);
        let header_h = 52.0 * s;
        let command_h = 36.0 * s;
        let margin = 8.0 * s;
        let full = UiRect::new(0.0, 0.0, w, h);
        let header = UiRect::new(0.0, 0.0, w, header_h);
        let command_bar = UiRect::new(0.0, header_h, w, command_h);
        let content = UiRect::new(
            margin,
            header_h + command_h + margin,
            (w - margin * 2.0).max(1.0),
            (h - header_h - command_h - margin * 2.0).max(1.0),
        );
        let meter_w = 92.0 * s;
        let gap = 8.0 * s;
        let center_w = (content.w - meter_w * 2.0 - gap * 2.0).max(200.0 * s);
        let left_panel = UiRect::new(content.x, content.y, meter_w, content.h);
        let right_panel = UiRect::new(content.right() - meter_w, content.y, meter_w, content.h);
        let center = UiRect::new(content.x + meter_w + gap, content.y, center_w, content.h);
        let spectrum_h = (center.h * 0.30).max(130.0 * s);
        let controls = UiRect::new(center.x, center.y, center.w, center.h - spectrum_h - gap);
        let spectrum = UiRect::new(center.x, controls.bottom() + gap, center.w, spectrum_h);
        Self {
            full,
            header,
            command_bar,
            left_panel,
            right_panel,
            controls,
            spectrum,
            s,
        }
    }
}

#[derive(Clone, Copy)]
struct UiRect {
    x: f32,
    y: f32,
    w: f32,
    h: f32,
}

impl UiRect {
    const fn new(x: f32, y: f32, w: f32, h: f32) -> Self {
        Self { x, y, w, h }
    }

    fn right(self) -> f32 {
        self.x + self.w
    }

    fn bottom(self) -> f32 {
        self.y + self.h
    }

    fn center_x(self) -> f32 {
        self.x + self.w * 0.5
    }

    fn center_y(self) -> f32 {
        self.y + self.h * 0.5
    }

    fn contains(self, x: f32, y: f32) -> bool {
        x >= self.x && x <= self.right() && y >= self.y && y <= self.bottom()
    }

    fn shrink(self, amount: f32) -> Self {
        Self::new(
            self.x + amount,
            self.y + amount,
            (self.w - amount * 2.0).max(1.0),
            (self.h - amount * 2.0).max(1.0),
        )
    }

    fn d2d(self) -> D2D_RECT_F {
        D2D_RECT_F {
            left: self.x,
            top: self.y,
            right: self.right(),
            bottom: self.bottom(),
        }
    }
}

#[derive(Clone, PartialEq)]
struct ParamSnapshot {
    threshold: f32,
    max_reduction: f32,
    min_freq: f32,
    max_freq: f32,
    mode_relative: bool,
    basis_mode: i32,
    use_wide_range: bool,
    filter_solo: bool,
    lookahead_enabled: bool,
    lookahead_ms: f32,
    trigger_hear: bool,
    stereo_link: f32,
    stereo_mid_side: bool,
    sidechain_mode: i32,
    vocal_mode: bool,
    input_level: f32,
    input_pan: f32,
    output_level: f32,
    output_pan: f32,
    bypass: bool,
    oversampling: i32,
    cut_width: f32,
    cut_depth: f32,
    mix: f32,
    cut_slope: f32,
}

impl ParamSnapshot {
    fn from_stored(snapshot: &StoredPresetSnapshot) -> Self {
        Self {
            threshold: snapshot.threshold,
            max_reduction: snapshot.max_reduction,
            min_freq: snapshot.min_freq,
            max_freq: snapshot.max_freq,
            mode_relative: snapshot.mode_relative,
            basis_mode: snapshot.basis_mode,
            use_wide_range: snapshot.use_wide_range,
            filter_solo: snapshot.filter_solo,
            lookahead_enabled: snapshot.lookahead_enabled,
            lookahead_ms: snapshot.lookahead_ms,
            trigger_hear: snapshot.trigger_hear,
            stereo_link: snapshot.stereo_link,
            stereo_mid_side: snapshot.stereo_mid_side,
            sidechain_mode: snapshot.effective_sidechain_mode(),
            vocal_mode: snapshot.vocal_mode,
            input_level: snapshot.input_level,
            input_pan: snapshot.input_pan,
            output_level: snapshot.output_level,
            output_pan: snapshot.output_pan,
            bypass: snapshot.bypass,
            oversampling: snapshot.oversampling,
            cut_width: snapshot.cut_width,
            cut_depth: snapshot.cut_depth,
            mix: snapshot.mix,
            cut_slope: snapshot.cut_slope,
        }
    }

    fn to_stored(&self) -> StoredPresetSnapshot {
        StoredPresetSnapshot {
            threshold: self.threshold,
            max_reduction: self.max_reduction,
            min_freq: self.min_freq,
            max_freq: self.max_freq,
            mode_relative: self.mode_relative,
            basis_mode: self.basis_mode,
            use_wide_range: self.use_wide_range,
            filter_solo: self.filter_solo,
            lookahead_enabled: self.lookahead_enabled,
            lookahead_ms: self.lookahead_ms,
            trigger_hear: self.trigger_hear,
            stereo_link: self.stereo_link,
            stereo_mid_side: self.stereo_mid_side,
            sidechain_mode: self.sidechain_mode.clamp(0, 2),
            sidechain_external: self.sidechain_mode == 1,
            vocal_mode: self.vocal_mode,
            input_level: self.input_level,
            input_pan: self.input_pan,
            output_level: self.output_level,
            output_pan: self.output_pan,
            bypass: self.bypass,
            oversampling: self.oversampling,
            cut_width: self.cut_width,
            cut_depth: self.cut_depth,
            mix: self.mix,
            cut_slope: self.cut_slope,
        }
    }
}

#[derive(Clone)]
struct NumericInput {
    target: ControlTarget,
    label: String,
    value: String,
    min: f32,
    max: f32,
}

#[derive(Clone, Copy)]
struct NumericHitZone {
    rect: UiRect,
    target: ControlTarget,
}

#[derive(Clone, Copy)]
enum CommandAction {
    TogglePresetMenu,
    OpenPresetSave,
    DeletePreset,
    Undo,
    Redo,
    ToggleAB,
    MidiLearn,
}

#[derive(Clone, Copy)]
struct MeterPanelLayout {
    max_rect: UiRect,
    meter_rect: UiRect,
    slider_rect: UiRect,
    value_rect: UiRect,
}

impl MeterPanelLayout {
    fn new(rect: UiRect, detect: bool, s: f32) -> Self {
        let max_rect = UiRect::new(
            rect.x + 9.0 * s,
            rect.y + 30.0 * s,
            rect.w - 18.0 * s,
            20.0 * s,
        );
        let top = rect.y + 62.0 * s;
        let height = (rect.h - 112.0 * s).max(40.0 * s);
        let meter_w = 16.0 * s;
        let slider_w = 12.0 * s;
        let gap = 6.0 * s;
        let left = rect.center_x() - (meter_w + slider_w + gap) * 0.5;
        let slider_rect = if detect {
            UiRect::new(left + meter_w + gap, top, slider_w, height)
        } else {
            UiRect::new(left, top, slider_w, height)
        };
        let meter_rect = if detect {
            UiRect::new(left, top, meter_w, height)
        } else {
            UiRect::new(left + slider_w + gap, top, meter_w, height)
        };
        let value_rect = UiRect::new(
            rect.x + 4.0 * s,
            rect.bottom() - 34.0 * s,
            rect.w - 8.0 * s,
            20.0 * s,
        );
        Self {
            max_rect,
            meter_rect,
            slider_rect,
            value_rect,
        }
    }
}

#[derive(Clone, Copy)]
struct CommandBarRects {
    bypass: UiRect,
    preset: UiRect,
    save: UiRect,
    delete: UiRect,
    undo: UiRect,
    redo: UiRect,
    ab: UiRect,
    midi: UiRect,
    status: UiRect,
    os_rect: UiRect,
    os_segments: [UiRect; 5],
}

impl CommandBarRects {
    fn new(layout: &Layout) -> Self {
        let s = layout.s;
        let cy = layout.command_bar.center_y();
        let button_h = 24.0 * s;
        let y = cy - button_h * 0.5;
        let mut x = layout.command_bar.x + 8.0 * s;

        let bypass = UiRect::new(x, y, 84.0 * s, button_h);
        x = bypass.right() + 8.0 * s;
        let preset = UiRect::new(x, y, 144.0 * s, button_h);
        x = preset.right() + 4.0 * s;
        let save = UiRect::new(x, y, 40.0 * s, button_h);
        x = save.right() + 4.0 * s;
        let delete = UiRect::new(x, y, 48.0 * s, button_h);
        x = delete.right() + 8.0 * s;
        let undo = UiRect::new(x, y, 48.0 * s, button_h);
        x = undo.right() + 4.0 * s;
        let redo = UiRect::new(x, y, 48.0 * s, button_h);
        x = redo.right() + 8.0 * s;
        let ab = UiRect::new(x, y, 66.0 * s, button_h);
        x = ab.right() + 4.0 * s;
        let midi = UiRect::new(x, y, 90.0 * s, button_h);
        x = midi.right() + 10.0 * s;

        let os_rect = UiRect::new(
            layout.command_bar.right() - 182.0 * s,
            y,
            174.0 * s,
            button_h,
        );
        let status = UiRect::new(x, y, (os_rect.x - x - 8.0 * s).max(60.0 * s), button_h);
        let seg_w = os_rect.w / 5.0;
        let os_segments = std::array::from_fn(|idx| {
            UiRect::new(os_rect.x + idx as f32 * seg_w, os_rect.y, seg_w, os_rect.h)
        });

        Self {
            bypass,
            preset,
            save,
            delete,
            undo,
            redo,
            ab,
            midi,
            status,
            os_rect,
            os_segments,
        }
    }
}

#[derive(Clone, Copy)]
struct NumericPopupLayout {
    dialog: UiRect,
    title: UiRect,
    field: UiRect,
    ok: UiRect,
    cancel: UiRect,
}

impl NumericPopupLayout {
    fn new(layout: &Layout, _input: &NumericInput) -> Self {
        let s = layout.s;
        let dialog = UiRect::new(
            layout.full.center_x() - 130.0 * s,
            layout.full.center_y() - 58.0 * s,
            260.0 * s,
            118.0 * s,
        );
        Self {
            title: UiRect::new(
                dialog.x + 16.0 * s,
                dialog.y + 14.0 * s,
                dialog.w - 32.0 * s,
                18.0 * s,
            ),
            field: UiRect::new(
                dialog.x + 20.0 * s,
                dialog.y + 42.0 * s,
                dialog.w - 40.0 * s,
                26.0 * s,
            ),
            ok: UiRect::new(
                dialog.x + 30.0 * s,
                dialog.bottom() - 34.0 * s,
                82.0 * s,
                22.0 * s,
            ),
            cancel: UiRect::new(
                dialog.right() - 112.0 * s,
                dialog.bottom() - 34.0 * s,
                82.0 * s,
                22.0 * s,
            ),
            dialog,
        }
    }
}

#[derive(Clone, Copy)]
struct PresetSavePopupLayout {
    dialog: UiRect,
    title: UiRect,
    field: UiRect,
    ok: UiRect,
    cancel: UiRect,
}

impl PresetSavePopupLayout {
    fn new(layout: &Layout) -> Self {
        let s = layout.s;
        let dialog = UiRect::new(
            layout.full.center_x() - 130.0 * s,
            layout.full.center_y() - 60.0 * s,
            260.0 * s,
            122.0 * s,
        );
        Self {
            title: UiRect::new(
                dialog.x + 16.0 * s,
                dialog.y + 14.0 * s,
                dialog.w - 32.0 * s,
                18.0 * s,
            ),
            field: UiRect::new(
                dialog.x + 20.0 * s,
                dialog.y + 42.0 * s,
                dialog.w - 40.0 * s,
                28.0 * s,
            ),
            ok: UiRect::new(
                dialog.x + 30.0 * s,
                dialog.bottom() - 34.0 * s,
                82.0 * s,
                22.0 * s,
            ),
            cancel: UiRect::new(
                dialog.right() - 112.0 * s,
                dialog.bottom() - 34.0 * s,
                82.0 * s,
                22.0 * s,
            ),
            dialog,
        }
    }
}

#[derive(Clone, Copy)]
struct PresetMenuLayout {
    rect: UiRect,
    item_h: f32,
}

impl PresetMenuLayout {
    fn new(layout: &Layout, items: usize) -> Self {
        let s = layout.s;
        let bar = CommandBarRects::new(layout);
        let item_h = 24.0 * s;
        let count = items.max(1) as f32;
        Self {
            rect: UiRect::new(
                bar.preset.x,
                bar.preset.bottom() + 4.0 * s,
                bar.preset.w,
                count * item_h + 8.0 * s,
            ),
            item_h,
        }
    }

    fn item_rect(self, index: usize) -> UiRect {
        UiRect::new(
            self.rect.x + 4.0,
            self.rect.y + 4.0 + index as f32 * self.item_h,
            self.rect.w - 8.0,
            self.item_h - 2.0,
        )
    }

    fn hit_item(self, x: f32, y: f32) -> Option<usize> {
        if !self.rect.contains(x, y) {
            return None;
        }
        let local = (y - self.rect.y - 4.0).max(0.0);
        Some((local / self.item_h).floor() as usize)
    }
}

#[derive(Clone, Copy)]
struct MidiPopupLayout {
    dialog: UiRect,
    title: UiRect,
    subtitle: UiRect,
    rows: UiRect,
    clear: UiRect,
    close: UiRect,
    row_h: f32,
}

impl MidiPopupLayout {
    fn new(layout: &Layout, s: f32) -> Self {
        let dialog = UiRect::new(
            layout.full.center_x() - 150.0 * s,
            layout.full.center_y() - 164.0 * s,
            300.0 * s,
            328.0 * s,
        );
        let row_h = 22.0 * s;
        Self {
            title: UiRect::new(
                dialog.x + 16.0 * s,
                dialog.y + 12.0 * s,
                dialog.w - 32.0 * s,
                18.0 * s,
            ),
            subtitle: UiRect::new(
                dialog.x + 16.0 * s,
                dialog.y + 28.0 * s,
                dialog.w - 32.0 * s,
                18.0 * s,
            ),
            rows: UiRect::new(
                dialog.x + 10.0 * s,
                dialog.y + 50.0 * s,
                dialog.w - 20.0 * s,
                MIDI_PARAM_COUNT as f32 * row_h,
            ),
            clear: UiRect::new(
                dialog.x + 26.0 * s,
                dialog.bottom() - 34.0 * s,
                94.0 * s,
                22.0 * s,
            ),
            close: UiRect::new(
                dialog.right() - 120.0 * s,
                dialog.bottom() - 34.0 * s,
                94.0 * s,
                22.0 * s,
            ),
            dialog,
            row_h,
        }
    }

    fn row_rect(self, index: usize) -> UiRect {
        UiRect::new(
            self.rows.x,
            self.rows.y + index as f32 * self.row_h,
            self.rows.w,
            self.row_h - 2.0,
        )
    }

    fn hit_row(self, x: f32, y: f32) -> Option<usize> {
        if !self.rows.contains(x, y) {
            return None;
        }
        let idx = ((y - self.rows.y) / self.row_h).floor() as usize;
        (idx < MIDI_PARAM_COUNT).then_some(idx)
    }
}

#[derive(Clone, Copy)]
struct MidiContextMenuLayout {
    rect: UiRect,
    item_h: f32,
}

impl MidiContextMenuLayout {
    fn new(layout: &Layout) -> Self {
        let s = layout.s;
        let bar = CommandBarRects::new(layout);
        let item_h = 24.0 * s;
        Self {
            rect: UiRect::new(
                bar.midi.x,
                bar.midi.bottom() + 4.0 * s,
                172.0 * s,
                item_h * 5.0 + 8.0 * s,
            ),
            item_h,
        }
    }

    fn item_rect(self, index: usize) -> UiRect {
        UiRect::new(
            self.rect.x + 4.0,
            self.rect.y + 4.0 + index as f32 * self.item_h,
            self.rect.w - 8.0,
            self.item_h - 2.0,
        )
    }

    fn hit_item(self, x: f32, y: f32) -> Option<usize> {
        if !self.rect.contains(x, y) {
            return None;
        }
        let local = (y - self.rect.y - 4.0).max(0.0);
        let idx = (local / self.item_h).floor() as usize;
        (idx < 5).then_some(idx)
    }
}

#[derive(Clone, Copy)]
struct MidiCleanupMenuLayout {
    rect: UiRect,
    item_h: f32,
    mapping_count: usize,
    clear: UiRect,
}

impl MidiCleanupMenuLayout {
    fn new(layout: &Layout, mapping_count: usize) -> Self {
        let s = layout.s;
        let context = MidiContextMenuLayout::new(layout);
        let item_h = 24.0 * s;
        let body_rows = mapping_count.max(1) + 1;
        let menu_w = 210.0 * s;
        let min_x = 8.0 * s;
        let max_x = (layout.full.right() - menu_w - 8.0 * s).max(min_x);
        let preferred_right = context.rect.right() + 2.0 * s;
        let preferred_left = context.rect.x - menu_w - 2.0 * s;
        let x = if preferred_right + menu_w <= layout.full.right() - 8.0 * s {
            preferred_right
        } else if preferred_left >= min_x {
            preferred_left
        } else {
            preferred_right.clamp(min_x, max_x)
        };
        let rect = UiRect::new(
            x,
            context.rect.y + item_h,
            menu_w,
            body_rows as f32 * item_h + 8.0 * s,
        );
        let clear_y = rect.y + 4.0 + mapping_count.max(1) as f32 * item_h;
        let clear = UiRect::new(rect.x + 4.0, clear_y, rect.w - 8.0, item_h - 2.0);
        Self {
            rect,
            item_h,
            mapping_count,
            clear,
        }
    }

    fn mapping_rect(self, index: usize) -> UiRect {
        UiRect::new(
            self.rect.x + 4.0,
            self.rect.y + 4.0 + index as f32 * self.item_h,
            self.rect.w - 8.0,
            self.item_h - 2.0,
        )
    }

    fn empty_rect(self) -> UiRect {
        self.mapping_rect(0)
    }

    fn hit_mapping(self, x: f32, y: f32) -> Option<usize> {
        if self.mapping_count == 0 || !self.rect.contains(x, y) {
            return None;
        }
        let local = (y - self.rect.y - 4.0).max(0.0);
        let idx = (local / self.item_h).floor() as usize;
        (idx < self.mapping_count).then_some(idx)
    }
}

#[derive(Clone, Copy)]
struct SegmentGroup {
    label: &'static str,
    rect: UiRect,
    segments: [SegmentSpec; 3],
}

#[derive(Clone, Copy)]
struct SegmentSpec {
    label: &'static str,
    rect: UiRect,
    action: HitAction,
    active: bool,
}

#[derive(Clone)]
struct KnobGroup {
    label: &'static str,
    rect: UiRect,
    knobs: Vec<KnobSpec>,
}

#[derive(Clone, Copy)]
struct KnobSpec {
    label: &'static str,
    rect: UiRect,
    knob_rect: UiRect,
    target: ControlTarget,
    accent: AccentBrush,
}

#[derive(Clone, Copy)]
struct ToggleSpec {
    label: &'static str,
    rect: UiRect,
    target: ControlTarget,
}

#[derive(Clone, Copy)]
struct HitZone {
    rect: UiRect,
    action: HitAction,
}

#[derive(Clone, Copy)]
enum HitAction {
    Drag(ControlTarget, UiRect, DragMode),
    Set(ControlTarget, f32),
    Toggle(ControlTarget),
    ResetDetect,
    ResetReduction,
    Command(CommandAction),
}

#[derive(Clone, Copy)]
struct DragState {
    target: ControlTarget,
    track: UiRect,
    mode: DragMode,
    pointer_offset: f32,
    start_y: f32,
    start_value: f32,
}

#[derive(Clone, Copy)]
enum DragMode {
    Horizontal,
    Vertical,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ControlTarget {
    Threshold,
    MaxReduction,
    MinFreq,
    MaxFreq,
    ModeRelative,
    BasisMode,
    UseWideRange,
    FilterSolo,
    LookaheadEnabled,
    LookaheadMs,
    TriggerHear,
    StereoLink,
    StereoMidSide,
    SidechainMode,
    VocalMode,
    InputLevel,
    InputPan,
    OutputLevel,
    OutputPan,
    Bypass,
    Oversampling,
    CutWidth,
    CutDepth,
    Mix,
    CutSlope,
}

#[derive(Clone, Copy)]
enum AccentBrush {
    Accent,
    Orange,
    Magenta,
    Purple,
}

impl AccentBrush {
    fn brush<'a>(self, brushes: &'a Brushes) -> &'a ID2D1SolidColorBrush {
        match self {
            AccentBrush::Accent => &brushes.accent,
            AccentBrush::Orange => &brushes.orange,
            AccentBrush::Magenta => &brushes.magenta,
            AccentBrush::Purple => &brushes.purple,
        }
    }
}

fn segment_groups(params: &NebulaParams, layout: &Layout) -> Vec<SegmentGroup> {
    let s = layout.s;
    let inner = layout.controls.shrink(8.0 * s);
    let group_gap = 5.0 * s;
    let group_w = (inner.w - group_gap * 4.0) / 5.0;
    let y = inner.y;
    let h = 58.0 * s;

    let mut groups = Vec::with_capacity(5);
    for i in 0..5 {
        let x = inner.x + i as f32 * (group_w + group_gap);
        let rect = UiRect::new(x, y, group_w, h);
        let seg_y = y + 24.0 * s;
        let seg_h = 24.0 * s;
        let make_two = |a: (&'static str, HitAction, bool), b: (&'static str, HitAction, bool)| {
            let sw = (group_w - 10.0 * s) * 0.5;
            [
                SegmentSpec {
                    label: a.0,
                    rect: UiRect::new(x + 4.0 * s, seg_y, sw, seg_h),
                    action: a.1,
                    active: a.2,
                },
                SegmentSpec {
                    label: b.0,
                    rect: UiRect::new(x + 6.0 * s + sw, seg_y, sw, seg_h),
                    action: b.1,
                    active: b.2,
                },
                SegmentSpec {
                    label: "",
                    rect: UiRect::new(0.0, 0.0, 0.0, 0.0),
                    action: HitAction::Set(ControlTarget::Threshold, 0.0),
                    active: false,
                },
            ]
        };
        let group = match i {
            0 => SegmentGroup {
                label: "Mode",
                rect,
                segments: make_two(
                    (
                        "Rel",
                        HitAction::Set(ControlTarget::ModeRelative, 1.0),
                        params.mode_relative.value() > 0.5,
                    ),
                    (
                        "Abs",
                        HitAction::Set(ControlTarget::ModeRelative, 0.0),
                        params.mode_relative.value() <= 0.5,
                    ),
                ),
            },
            1 => SegmentGroup {
                label: "Range",
                rect,
                segments: make_two(
                    (
                        "Split",
                        HitAction::Set(ControlTarget::UseWideRange, 0.0),
                        params.use_wide_range.value() <= 0.5,
                    ),
                    (
                        "Wide",
                        HitAction::Set(ControlTarget::UseWideRange, 1.0),
                        params.use_wide_range.value() > 0.5,
                    ),
                ),
            },
            2 => {
                let sw = (group_w - 12.0 * s) / 3.0;
                let active = params.basis_mode.value().round() as i32;
                SegmentGroup {
                    label: "Basis",
                    rect,
                    segments: [
                        SegmentSpec {
                            label: "Odd",
                            rect: UiRect::new(x + 4.0 * s, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::BasisMode, 0.0),
                            active: active == 0,
                        },
                        SegmentSpec {
                            label: "Even",
                            rect: UiRect::new(x + 6.0 * s + sw, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::BasisMode, 1.0),
                            active: active == 1,
                        },
                        SegmentSpec {
                            label: "Both",
                            rect: UiRect::new(x + 8.0 * s + sw * 2.0, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::BasisMode, 2.0),
                            active: active == 2,
                        },
                    ],
                }
            }
            3 => {
                let sw = (group_w - 12.0 * s) / 3.0;
                let active = params.sidechain_mode.value().round().clamp(0.0, 2.0) as i32;
                SegmentGroup {
                    label: "Sidechain",
                    rect,
                    segments: [
                        SegmentSpec {
                            label: "Int",
                            rect: UiRect::new(x + 4.0 * s, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::SidechainMode, 0.0),
                            active: active == 0,
                        },
                        SegmentSpec {
                            label: "Ext",
                            rect: UiRect::new(x + 6.0 * s + sw, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::SidechainMode, 1.0),
                            active: active == 1,
                        },
                        SegmentSpec {
                            label: "MIDI",
                            rect: UiRect::new(x + 8.0 * s + sw * 2.0, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::SidechainMode, 2.0),
                            active: active == 2,
                        },
                    ],
                }
            }
            4 => SegmentGroup {
                label: "Vocal",
                rect,
                segments: make_two(
                    (
                        "Off",
                        HitAction::Set(ControlTarget::VocalMode, 0.0),
                        params.vocal_mode.value() <= 0.5,
                    ),
                    (
                        "On",
                        HitAction::Set(ControlTarget::VocalMode, 1.0),
                        params.vocal_mode.value() > 0.5,
                    ),
                ),
            },
            _ => unreachable!(),
        };
        groups.push(group);
    }
    groups
}

fn knob_groups(layout: &Layout) -> Vec<KnobGroup> {
    let s = layout.s;
    let inner = layout.controls.shrink(8.0 * s);
    let toggle_h = 34.0 * s;
    let toggle_gap = 10.0 * s;
    let row_gap = 10.0 * s;
    let mut y = inner.y + 76.0 * s;
    let rows_bottom = inner.bottom() - toggle_h - toggle_gap;
    let row_h = ((rows_bottom - y - row_gap * 2.0) / 3.0).max(52.0 * s);

    let main = knob_group(
        "Core",
        UiRect::new(inner.x, y, inner.w, row_h),
        &[
            ("TKEO", ControlTarget::Threshold, AccentBrush::Accent),
            ("Max Red", ControlTarget::MaxReduction, AccentBrush::Orange),
            ("Min Freq", ControlTarget::MinFreq, AccentBrush::Accent),
            ("Max Freq", ControlTarget::MaxFreq, AccentBrush::Orange),
            ("Lookahead", ControlTarget::LookaheadMs, AccentBrush::Accent),
            ("Stereo", ControlTarget::StereoLink, AccentBrush::Accent),
        ],
        s,
    );
    y += row_h + row_gap;

    let cut_w = inner.w * 0.84;
    let cut = knob_group(
        "Cut Shape",
        UiRect::new(inner.center_x() - cut_w * 0.5, y, cut_w, row_h),
        &[
            ("Width", ControlTarget::CutWidth, AccentBrush::Magenta),
            ("Depth", ControlTarget::CutDepth, AccentBrush::Magenta),
            ("Slope", ControlTarget::CutSlope, AccentBrush::Magenta),
            ("Mix", ControlTarget::Mix, AccentBrush::Accent),
        ],
        s,
    );
    y += row_h + row_gap;

    let io_w = inner.w * 0.80;
    let io = knob_group(
        "I/O",
        UiRect::new(inner.center_x() - io_w * 0.5, y, io_w, row_h),
        &[
            ("In Level", ControlTarget::InputLevel, AccentBrush::Purple),
            ("In Pan", ControlTarget::InputPan, AccentBrush::Purple),
            ("Out Level", ControlTarget::OutputLevel, AccentBrush::Purple),
            ("Out Pan", ControlTarget::OutputPan, AccentBrush::Purple),
        ],
        s,
    );

    vec![main, cut, io]
}

fn knob_group(
    label: &'static str,
    rect: UiRect,
    defs: &[(&'static str, ControlTarget, AccentBrush)],
    s: f32,
) -> KnobGroup {
    let slot_w = rect.w / defs.len().max(1) as f32;
    let knob_size = (slot_w * 0.46)
        .min((rect.h - 34.0 * s).max(12.0 * s))
        .min(40.0 * s)
        .max(12.0 * s);
    let knobs = defs
        .iter()
        .enumerate()
        .map(|(idx, (label, target, accent))| {
            let slot = UiRect::new(rect.x + idx as f32 * slot_w, rect.y, slot_w, rect.h);
            let knob_rect = UiRect::new(
                slot.center_x() - knob_size * 0.5,
                slot.y + 17.0 * s,
                knob_size,
                knob_size,
            );
            KnobSpec {
                label: *label,
                rect: slot,
                knob_rect,
                target: *target,
                accent: *accent,
            }
        })
        .collect();

    KnobGroup { label, rect, knobs }
}

fn knob_value_rect(rect: UiRect, s: f32) -> UiRect {
    UiRect::new(
        rect.x + 5.0 * s,
        rect.bottom() - 13.0 * s,
        rect.w - 10.0 * s,
        10.0 * s,
    )
}

fn numeric_hit_zones(params: &NebulaParams, layout: &Layout) -> Vec<NumericHitZone> {
    let mut zones = Vec::new();
    for group in knob_groups(layout) {
        for knob in group.knobs {
            if knob.target == ControlTarget::LookaheadMs && params.lookahead_enabled.value() <= 0.5
            {
                continue;
            }
            let value_rect = knob_value_rect(knob.rect, layout.s);
            zones.push(NumericHitZone {
                rect: knob.knob_rect,
                target: knob.target,
            });
            zones.push(NumericHitZone {
                rect: value_rect,
                target: knob.target,
            });
        }
    }
    zones
}

fn toggle_specs(layout: &Layout) -> Vec<ToggleSpec> {
    let s = layout.s;
    let inner = layout.controls.shrink(8.0 * s);
    let labels = [
        ("Filter Solo", ControlTarget::FilterSolo),
        ("Trigger Hear", ControlTarget::TriggerHear),
        ("Lookahead", ControlTarget::LookaheadEnabled),
        ("Mid/Side", ControlTarget::StereoMidSide),
    ];
    let gap = 6.0 * s;
    let h = 34.0 * s;
    let w = (inner.w - gap * 3.0) / 4.0;
    let y = inner.bottom() - h;
    labels
        .iter()
        .enumerate()
        .map(|(idx, (label, target))| ToggleSpec {
            label,
            target: *target,
            rect: UiRect::new(inner.x + idx as f32 * (w + gap), y, w, h),
        })
        .collect()
}

fn hit_zones(params: &NebulaParams, layout: &Layout) -> Vec<HitZone> {
    let mut zones = Vec::new();
    let s = layout.s;
    let bar = CommandBarRects::new(layout);
    zones.push(HitZone {
        rect: bar.bypass,
        action: HitAction::Toggle(ControlTarget::Bypass),
    });
    zones.push(HitZone {
        rect: bar.preset,
        action: HitAction::Command(CommandAction::TogglePresetMenu),
    });
    zones.push(HitZone {
        rect: bar.save,
        action: HitAction::Command(CommandAction::OpenPresetSave),
    });
    zones.push(HitZone {
        rect: bar.delete,
        action: HitAction::Command(CommandAction::DeletePreset),
    });
    zones.push(HitZone {
        rect: bar.undo,
        action: HitAction::Command(CommandAction::Undo),
    });
    zones.push(HitZone {
        rect: bar.redo,
        action: HitAction::Command(CommandAction::Redo),
    });
    zones.push(HitZone {
        rect: bar.ab,
        action: HitAction::Command(CommandAction::ToggleAB),
    });
    zones.push(HitZone {
        rect: bar.midi,
        action: HitAction::Command(CommandAction::MidiLearn),
    });
    for (idx, rect) in bar.os_segments.into_iter().enumerate() {
        zones.push(HitZone {
            rect,
            action: HitAction::Set(ControlTarget::Oversampling, idx as f32),
        });
    }

    let left_panel = MeterPanelLayout::new(layout.left_panel, true, s);
    let right_panel = MeterPanelLayout::new(layout.right_panel, false, s);
    zones.push(HitZone {
        rect: left_panel.max_rect,
        action: HitAction::ResetDetect,
    });
    zones.push(HitZone {
        rect: right_panel.max_rect,
        action: HitAction::ResetReduction,
    });
    zones.push(HitZone {
        rect: left_panel.slider_rect,
        action: HitAction::Drag(
            ControlTarget::Threshold,
            left_panel.slider_rect,
            DragMode::Vertical,
        ),
    });
    zones.push(HitZone {
        rect: right_panel.slider_rect,
        action: HitAction::Drag(
            ControlTarget::MaxReduction,
            right_panel.slider_rect,
            DragMode::Vertical,
        ),
    });
    for group in segment_groups(params, layout) {
        for segment in group.segments {
            if segment.rect.w > 0.0 {
                zones.push(HitZone {
                    rect: segment.rect,
                    action: segment.action,
                });
            }
        }
    }
    for group in knob_groups(layout) {
        for knob in group.knobs {
            if knob.target == ControlTarget::LookaheadMs && params.lookahead_enabled.value() <= 0.5
            {
                continue;
            }
            zones.push(HitZone {
                rect: knob.rect,
                action: HitAction::Drag(knob.target, knob.knob_rect, DragMode::Vertical),
            });
        }
    }
    let graph = {
        let rect = layout.spectrum;
        let inner = rect.shrink(8.0 * s);
        UiRect::new(inner.x, inner.y + 18.0 * s, inner.w, inner.h - 28.0 * s)
    };
    let min_x = graph.x + freq_to_x(params.min_freq.value(), graph.w);
    let max_x = graph.x + freq_to_x(params.max_freq.value(), graph.w);
    let node_y = graph.y + graph.h * 0.5;
    let node_hit = 22.0 * s;
    zones.push(HitZone {
        rect: UiRect::new(
            min_x - node_hit * 0.5,
            node_y - node_hit * 0.5,
            node_hit,
            node_hit,
        ),
        action: HitAction::Drag(
            ControlTarget::MinFreq,
            UiRect::new(graph.x, graph.y, graph.w, graph.h),
            DragMode::Horizontal,
        ),
    });
    zones.push(HitZone {
        rect: UiRect::new(
            max_x - node_hit * 0.5,
            node_y - node_hit * 0.5,
            node_hit,
            node_hit,
        ),
        action: HitAction::Drag(
            ControlTarget::MaxFreq,
            UiRect::new(graph.x, graph.y, graph.w, graph.h),
            DragMode::Horizontal,
        ),
    });
    for toggle in toggle_specs(layout) {
        zones.push(HitZone {
            rect: toggle.rect,
            action: HitAction::Toggle(toggle.target),
        });
    }
    zones
}

fn target_range(target: ControlTarget) -> (f32, f32) {
    match target {
        ControlTarget::Threshold => (0.0, 100.0),
        ControlTarget::MaxReduction => (-100.0, 0.0),
        ControlTarget::MinFreq | ControlTarget::MaxFreq => (1.0, 24_000.0),
        ControlTarget::LookaheadMs => (0.0, 20.0),
        ControlTarget::StereoLink => (0.0, 2.0),
        ControlTarget::CutWidth
        | ControlTarget::CutDepth
        | ControlTarget::Mix
        | ControlTarget::ModeRelative
        | ControlTarget::UseWideRange
        | ControlTarget::FilterSolo
        | ControlTarget::LookaheadEnabled
        | ControlTarget::TriggerHear
        | ControlTarget::StereoMidSide
        | ControlTarget::VocalMode
        | ControlTarget::Bypass => (0.0, 1.0),
        ControlTarget::InputLevel | ControlTarget::OutputLevel => (-100.0, 100.0),
        ControlTarget::InputPan | ControlTarget::OutputPan => (-1.0, 1.0),
        ControlTarget::BasisMode => (0.0, 2.0),
        ControlTarget::SidechainMode => (0.0, 2.0),
        ControlTarget::Oversampling => (0.0, 4.0),
        ControlTarget::CutSlope => (0.0, 100.0),
    }
}

fn target_norm(target: ControlTarget, value: f32) -> f32 {
    let (min, max) = target_range(target);
    if matches!(target, ControlTarget::MinFreq | ControlTarget::MaxFreq) {
        let min_l = min.log10();
        let max_l = max.log10();
        return ((value.max(min).log10() - min_l) / (max_l - min_l)).clamp(0.0, 1.0);
    }
    ((value - min) / (max - min)).clamp(0.0, 1.0)
}

fn value_from_norm(target: ControlTarget, norm: f32) -> f32 {
    let (min, max) = target_range(target);
    if matches!(target, ControlTarget::MinFreq | ControlTarget::MaxFreq) {
        let min_l = min.log10();
        let max_l = max.log10();
        return 10.0_f32.powf(min_l + norm.clamp(0.0, 1.0) * (max_l - min_l));
    }
    min + norm.clamp(0.0, 1.0) * (max - min)
}

fn target_clamp(target: ControlTarget, value: f32) -> f32 {
    let (min, max) = target_range(target);
    let value = value.clamp(min, max);
    if matches!(
        target,
        ControlTarget::BasisMode | ControlTarget::SidechainMode | ControlTarget::Oversampling
    ) {
        value.round()
    } else if is_bool_target(target) {
        if value > 0.5 {
            1.0
        } else {
            0.0
        }
    } else {
        value
    }
}

fn is_bool_target(target: ControlTarget) -> bool {
    matches!(
        target,
        ControlTarget::ModeRelative
            | ControlTarget::UseWideRange
            | ControlTarget::FilterSolo
            | ControlTarget::LookaheadEnabled
            | ControlTarget::TriggerHear
            | ControlTarget::StereoMidSide
            | ControlTarget::VocalMode
            | ControlTarget::Bypass
    )
}

fn format_value(target: ControlTarget, value: f32) -> String {
    match target {
        ControlTarget::MinFreq | ControlTarget::MaxFreq => {
            if value >= 1000.0 {
                format!("{:.1}k", value / 1000.0)
            } else {
                format!("{value:.0} Hz")
            }
        }
        ControlTarget::Threshold => format!("{value:.0}%"),
        ControlTarget::CutSlope => format!("{value:.1}"),
        ControlTarget::StereoLink => {
            if value <= 1.0 {
                format!("{:.0}%", value * 100.0)
            } else {
                format!("MS {:.0}%", (value - 1.0) * 100.0)
            }
        }
        ControlTarget::CutWidth | ControlTarget::CutDepth | ControlTarget::Mix => {
            format!("{:.0}%", value * 100.0)
        }
        ControlTarget::MaxReduction | ControlTarget::InputLevel | ControlTarget::OutputLevel => {
            format!("{value:.1} dB")
        }
        ControlTarget::LookaheadMs => format!("{value:.1} ms"),
        ControlTarget::InputPan | ControlTarget::OutputPan => {
            if value.abs() < 0.01 {
                "C".to_string()
            } else if value > 0.0 {
                format!("R{:.0}", value * 100.0)
            } else {
                format!("L{:.0}", -value * 100.0)
            }
        }
        _ => format!("{value:.0}"),
    }
}

fn numeric_spec(target: ControlTarget) -> (&'static str, f32, f32) {
    match target {
        ControlTarget::Threshold => ("TKEO", 0.0, 100.0),
        ControlTarget::MaxReduction => ("Max Reduction", -100.0, 0.0),
        ControlTarget::MinFreq => ("Min Frequency", 1.0, 24_000.0),
        ControlTarget::MaxFreq => ("Max Frequency", 1.0, 24_000.0),
        ControlTarget::LookaheadMs => ("Lookahead", 0.0, 20.0),
        ControlTarget::StereoLink => ("Stereo Link", 0.0, 2.0),
        ControlTarget::InputLevel => ("Input Level", -100.0, 100.0),
        ControlTarget::InputPan => ("Input Pan", -1.0, 1.0),
        ControlTarget::OutputLevel => ("Output Level", -100.0, 100.0),
        ControlTarget::OutputPan => ("Output Pan", -1.0, 1.0),
        ControlTarget::CutWidth => ("Cut Width", 0.0, 1.0),
        ControlTarget::CutDepth => ("Cut Depth", 0.0, 1.0),
        ControlTarget::CutSlope => ("Cut Slope", 0.0, 100.0),
        ControlTarget::Mix => ("Mix", 0.0, 1.0),
        _ => ("Value", 0.0, 1.0),
    }
}

fn default_preset_name(name: &str, presets: &[(String, ParamSnapshot)]) -> String {
    if !name.is_empty() {
        return name.to_string();
    }

    let mut index = 1;
    loop {
        let candidate = format!("Preset {index}");
        if !presets
            .iter()
            .any(|(preset_name, _)| preset_name == &candidate)
        {
            return candidate;
        }
        index += 1;
    }
}

fn truncate_label(text: &str, max_chars: usize) -> &str {
    text.char_indices()
        .nth(max_chars)
        .map(|(idx, _)| &text[..idx])
        .unwrap_or(text)
}

fn with_param<R>(
    target: ControlTarget,
    params: &NebulaParams,
    f: impl FnOnce(&FloatParam) -> R,
) -> R {
    match target {
        ControlTarget::Threshold => f(&params.threshold),
        ControlTarget::MaxReduction => f(&params.max_reduction),
        ControlTarget::MinFreq => f(&params.min_freq),
        ControlTarget::MaxFreq => f(&params.max_freq),
        ControlTarget::ModeRelative => f(&params.mode_relative),
        ControlTarget::BasisMode => f(&params.basis_mode),
        ControlTarget::UseWideRange => f(&params.use_wide_range),
        ControlTarget::FilterSolo => f(&params.filter_solo),
        ControlTarget::LookaheadEnabled => f(&params.lookahead_enabled),
        ControlTarget::LookaheadMs => f(&params.lookahead_ms),
        ControlTarget::TriggerHear => f(&params.trigger_hear),
        ControlTarget::StereoLink => f(&params.stereo_link),
        ControlTarget::StereoMidSide => f(&params.stereo_mid_side),
        ControlTarget::SidechainMode => f(&params.sidechain_mode),
        ControlTarget::VocalMode => f(&params.vocal_mode),
        ControlTarget::InputLevel => f(&params.input_level),
        ControlTarget::InputPan => f(&params.input_pan),
        ControlTarget::OutputLevel => f(&params.output_level),
        ControlTarget::OutputPan => f(&params.output_pan),
        ControlTarget::Bypass => f(&params.bypass),
        ControlTarget::Oversampling => f(&params.oversampling),
        ControlTarget::CutWidth => f(&params.cut_width),
        ControlTarget::CutDepth => f(&params.cut_depth),
        ControlTarget::Mix => f(&params.mix),
        ControlTarget::CutSlope => f(&params.cut_slope),
    }
}

fn freq_to_x(freq: f32, width: f32) -> f32 {
    let lmin = 20.0_f32.log10();
    let lmax = 22_000.0_f32.log10();
    (freq.clamp(20.0, 22_000.0).log10() - lmin) / (lmax - lmin) * width
}

fn x_to_freq(x: f32, width: f32) -> f32 {
    let lmin = 20.0_f32.log10();
    let lmax = 22_000.0_f32.log10();
    10.0_f32.powf(lmin + (x / width.max(1.0)).clamp(0.0, 1.0) * (lmax - lmin))
}

fn card(rt: &ID2D1HwndRenderTarget, rect: UiRect, radius: f32, brushes: &Brushes) {
    fill_round(rt, rect, radius, &brushes.panel);
    draw_line(
        rt,
        rect.x + radius * 0.5,
        rect.y,
        rect.right() - radius * 0.5,
        rect.y,
        &brushes.accent_dark,
        1.0,
    );
    stroke_round(rt, rect, radius, &brushes.border, 1.0);
}

fn solid(rt: &ID2D1HwndRenderTarget, color: D2D1_COLOR_F) -> Option<ID2D1SolidColorBrush> {
    unsafe { rt.CreateSolidColorBrush(&color, None).ok() }
}

fn fill_rect(rt: &ID2D1HwndRenderTarget, rect: UiRect, brush: &ID2D1SolidColorBrush) {
    unsafe {
        rt.FillRectangle(&rect.d2d(), brush);
    }
}

fn fill_round(rt: &ID2D1HwndRenderTarget, rect: UiRect, radius: f32, brush: &ID2D1SolidColorBrush) {
    let rr = D2D1_ROUNDED_RECT {
        rect: rect.d2d(),
        radiusX: radius,
        radiusY: radius,
    };
    unsafe {
        rt.FillRoundedRectangle(&rr, brush);
    }
}

fn stroke_round(
    rt: &ID2D1HwndRenderTarget,
    rect: UiRect,
    radius: f32,
    brush: &ID2D1SolidColorBrush,
    width: f32,
) {
    let rr = D2D1_ROUNDED_RECT {
        rect: rect.d2d(),
        radiusX: radius,
        radiusY: radius,
    };
    unsafe {
        rt.DrawRoundedRectangle(
            &rr,
            brush,
            width,
            Option::<&windows::Win32::Graphics::Direct2D::ID2D1StrokeStyle>::None,
        );
    }
}

fn draw_line(
    rt: &ID2D1HwndRenderTarget,
    x0: f32,
    y0: f32,
    x1: f32,
    y1: f32,
    brush: &ID2D1SolidColorBrush,
    width: f32,
) {
    unsafe {
        rt.DrawLine(
            Vector2 { X: x0, Y: y0 },
            Vector2 { X: x1, Y: y1 },
            brush,
            width,
            Option::<&windows::Win32::Graphics::Direct2D::ID2D1StrokeStyle>::None,
        );
    }
}

fn fill_circle(
    rt: &ID2D1HwndRenderTarget,
    x: f32,
    y: f32,
    radius: f32,
    brush: &ID2D1SolidColorBrush,
) {
    let ellipse = D2D1_ELLIPSE {
        point: Vector2 { X: x, Y: y },
        radiusX: radius,
        radiusY: radius,
    };
    unsafe {
        rt.FillEllipse(&ellipse, brush);
    }
}

fn stroke_circle(
    rt: &ID2D1HwndRenderTarget,
    x: f32,
    y: f32,
    radius: f32,
    brush: &ID2D1SolidColorBrush,
    width: f32,
) {
    let ellipse = D2D1_ELLIPSE {
        point: Vector2 { X: x, Y: y },
        radiusX: radius,
        radiusY: radius,
    };
    unsafe {
        rt.DrawEllipse(
            &ellipse,
            brush,
            width,
            Option::<&windows::Win32::Graphics::Direct2D::ID2D1StrokeStyle>::None,
        );
    }
}

fn draw_arc(
    rt: &ID2D1HwndRenderTarget,
    cx: f32,
    cy: f32,
    radius: f32,
    start: f32,
    end: f32,
    brush: &ID2D1SolidColorBrush,
    width: f32,
) {
    let steps = 34;
    let span = end - start;
    let mut prev = None;
    for idx in 0..=steps {
        let angle = start + idx as f32 / steps as f32 * span;
        let point = (cx + radius * angle.cos(), cy + radius * angle.sin());
        if let Some((px, py)) = prev {
            draw_line(rt, px, py, point.0, point.1, brush, width);
        }
        prev = Some(point);
    }
}

fn draw_knob(
    rt: &ID2D1HwndRenderTarget,
    cx: f32,
    cy: f32,
    radius: f32,
    norm: f32,
    accent: &ID2D1SolidColorBrush,
    brushes: &Brushes,
    s: f32,
) {
    let norm = norm.clamp(0.0, 1.0);
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let angle = start + sweep * norm;

    fill_circle(rt, cx, cy + 1.0 * s, radius + 1.0 * s, &brushes.mica_bot);
    fill_circle(rt, cx, cy, radius, &brushes.control);
    stroke_circle(rt, cx, cy, radius, &brushes.border, 1.0);
    draw_arc(
        rt,
        cx,
        cy,
        radius - 1.5 * s,
        std::f32::consts::PI * 1.08,
        std::f32::consts::PI * 1.92,
        &brushes.card,
        1.4 * s,
    );
    draw_arc(
        rt,
        cx,
        cy,
        radius * 0.73,
        start,
        start + sweep,
        &brushes.border,
        3.4 * s,
    );
    if norm > 0.004 {
        draw_arc(rt, cx, cy, radius * 0.73, start, angle, accent, 3.0 * s);
    }

    let dot_x = cx + radius * 0.50 * angle.cos();
    let dot_y = cy + radius * 0.50 * angle.sin();
    fill_circle(rt, dot_x, dot_y, 2.6 * s, accent);
    fill_circle(rt, dot_x, dot_y, 1.25 * s, &brushes.text_light);
    fill_circle(rt, cx, cy, 1.7 * s, accent);
}

fn draw_freq_node(
    rt: &ID2D1HwndRenderTarget,
    x: f32,
    y: f32,
    label: &str,
    accent: &ID2D1SolidColorBrush,
    fill: &ID2D1SolidColorBrush,
    formats: &TextFormats,
    s: f32,
) {
    fill_circle(rt, x, y, 7.5 * s, fill);
    stroke_circle(rt, x, y, 5.5 * s, accent, 1.2 * s);
    fill_circle(rt, x, y, 2.2 * s, accent);
    draw_text(
        rt,
        label,
        UiRect::new(x - 18.0 * s, y - 25.0 * s, 36.0 * s, 14.0 * s),
        &formats.tiny,
        accent,
        Align::Center,
    );
}

enum Align {
    Leading,
    Center,
    Trailing,
}

fn draw_text(
    rt: &ID2D1HwndRenderTarget,
    text: &str,
    rect: UiRect,
    format: &IDWriteTextFormat,
    brush: &ID2D1SolidColorBrush,
    align: Align,
) {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return;
    }
    let d2d_rect = rect.d2d();
    let alignment = match align {
        Align::Leading => DWRITE_TEXT_ALIGNMENT_LEADING,
        Align::Center => DWRITE_TEXT_ALIGNMENT_CENTER,
        Align::Trailing => DWRITE_TEXT_ALIGNMENT_TRAILING,
    };
    let _ = unsafe { format.SetTextAlignment(alignment) };
    let _ = unsafe {
        format.SetParagraphAlignment(if matches!(align, Align::Leading | Align::Trailing) {
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER
        } else {
            DWRITE_PARAGRAPH_ALIGNMENT_CENTER
        })
    };
    unsafe {
        rt.DrawText(
            &wide,
            format,
            &d2d_rect,
            brush,
            D2D1_DRAW_TEXT_OPTIONS_NONE,
            DWRITE_MEASURING_MODE_NATURAL,
        );
    }
}

fn client_size(hwnd: HWND) -> Option<(u32, u32)> {
    let mut rect = RECT::default();
    unsafe { GetClientRect(hwnd, &mut rect).ok()? };
    let width = (rect.right - rect.left).max(1) as u32;
    let height = (rect.bottom - rect.top).max(1) as u32;
    Some((width, height))
}

struct DpiAwarenessScope {
    previous: Option<DPI_AWARENESS_CONTEXT>,
}

impl DpiAwarenessScope {
    fn enter() -> Self {
        let previous =
            unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) };
        Self {
            previous: (!previous.0.is_null()).then_some(previous),
        }
    }
}

impl Drop for DpiAwarenessScope {
    fn drop(&mut self) {
        if let Some(previous) = self.previous {
            let _ = unsafe { SetThreadDpiAwarenessContext(previous) };
        }
    }
}

fn dpi_for_window(hwnd: HWND) -> u32 {
    let dpi = if hwnd.0.is_null() {
        0
    } else {
        unsafe { GetDpiForWindow(hwnd) }
    };
    if dpi == 0 {
        unsafe { GetDpiForSystem() }.max(DEFAULT_DPI)
    } else {
        dpi.max(DEFAULT_DPI)
    }
}

fn dpi_from_wparam(wparam: WPARAM) -> u32 {
    let dpi_x = (wparam.0 as u32) & 0xffff;
    dpi_x.max(DEFAULT_DPI)
}

fn dpi_scale(dpi: u32) -> f32 {
    (dpi.max(DEFAULT_DPI) as f32 / DEFAULT_DPI as f32).clamp(0.5, 3.0)
}

fn invalidate(hwnd: HWND) {
    let _ = unsafe { InvalidateRect(Some(hwnd), None, false) };
}

fn class_name() -> PCWSTR {
    w!("NebulaDesserNativeEditor")
}

fn module_instance() -> Option<HINSTANCE> {
    unsafe {
        GetModuleHandleW(None)
            .ok()
            .map(|module| HINSTANCE(module.0))
    }
}

fn register_window_class() -> bool {
    static REGISTER_ONCE: Once = Once::new();
    static REGISTERED: AtomicBool = AtomicBool::new(false);

    REGISTER_ONCE.call_once(|| {
        let Some(instance) = module_instance() else {
            return;
        };
        let cursor = unsafe { LoadCursorW(None, IDC_ARROW).unwrap_or_default() };
        let wc = WNDCLASSW {
            style: CS_HREDRAW | CS_VREDRAW,
            lpfnWndProc: Some(window_proc),
            cbClsExtra: 0,
            cbWndExtra: 0,
            hInstance: instance,
            hIcon: Default::default(),
            hCursor: cursor,
            hbrBackground: HBRUSH::default(),
            lpszMenuName: PCWSTR::null(),
            lpszClassName: class_name(),
        };
        let atom = unsafe { RegisterClassW(&wc) };
        if atom != 0 || unsafe { GetLastError() } == ERROR_CLASS_ALREADY_EXISTS {
            REGISTERED.store(true, Ordering::Release);
        }
    });

    REGISTERED.load(Ordering::Acquire)
}

extern "system" fn window_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        if msg == WM_NCCREATE {
            let create = lparam.0 as *const CREATESTRUCTW;
            if !create.is_null() {
                let state = (*create).lpCreateParams.cast::<NativeWindowState>();
                if !state.is_null() {
                    (*state).hwnd = hwnd;
                    (*state).dpi = dpi_for_window(hwnd);
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, state as isize);
                    let _ = SetTimer(Some(hwnd), TIMER_ID, TIMER_MS, None);
                    return LRESULT(1);
                }
            }
            return LRESULT(0);
        }

        let state_ptr = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *mut NativeWindowState;
        if msg == WM_NCDESTROY {
            let _ = KillTimer(Some(hwnd), TIMER_ID);
            if !state_ptr.is_null() {
                (*state_ptr).persist_midi_mapping();
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0);
                drop(Box::from_raw(state_ptr));
            }
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }

        if state_ptr.is_null() {
            return DefWindowProcW(hwnd, msg, wparam, lparam);
        }
        let state = &mut *state_ptr;

        match msg {
            WM_GETDLGCODE => {
                if state.numeric_input.is_some() || state.preset_save_open {
                    LRESULT((DLGC_WANTALLKEYS | DLGC_WANTCHARS) as isize)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                state.render_target = None;
                state.text_formats = None;
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_DPICHANGED => {
                state.handle_dpi_changed(dpi_from_wparam(wparam));
                LRESULT(0)
            }
            WM_DPICHANGED_AFTERPARENT => {
                state.handle_dpi_changed(dpi_for_window(hwnd));
                LRESULT(0)
            }
            WM_DPICHANGED_BEFOREPARENT => LRESULT(0),
            WM_TIMER => {
                state.resize_to_parent();
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                BeginPaint(hwnd, &mut ps);
                state.paint();
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_LBUTTONDOWN => {
                let (x, y) = point_from_lparam(lparam);
                let (x, y) = state.logical_point(x, y);
                state.mouse_down(x, y);
                LRESULT(0)
            }
            WM_RBUTTONDOWN => {
                let (x, y) = point_from_lparam(lparam);
                let (x, y) = state.logical_point(x, y);
                state.mouse_right_down(x, y);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if state.drag.is_some() {
                    let (x, y) = point_from_lparam(lparam);
                    let (x, y) = state.logical_point(x, y);
                    state.mouse_move(x, y);
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_LBUTTONUP => {
                let (x, y) = point_from_lparam(lparam);
                let (x, y) = state.logical_point(x, y);
                state.mouse_up(x, y);
                LRESULT(0)
            }
            WM_CHAR => {
                if let Some(ch) = char::from_u32(wparam.0 as u32) {
                    state.char_input(ch);
                }
                LRESULT(0)
            }
            WM_KEYDOWN => {
                state.key_down(wparam.0 as u32);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

fn point_from_lparam(lparam: LPARAM) -> (f32, f32) {
    let raw = lparam.0 as u32;
    let x = (raw & 0xffff) as u16 as i16 as f32;
    let y = ((raw >> 16) & 0xffff) as u16 as i16 as f32;
    (x, y)
}
