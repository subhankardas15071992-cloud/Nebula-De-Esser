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
use windows::Win32::UI::Input::KeyboardAndMouse::{ReleaseCapture, SetCapture};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW, KillTimer,
    LoadCursorW, RegisterClassW, SetTimer, SetWindowLongPtrW, ShowWindow, CREATESTRUCTW,
    CS_HREDRAW, CS_VREDRAW, GWLP_USERDATA, HMENU, IDC_ARROW, SW_SHOW, WINDOW_EX_STYLE,
    WM_ERASEBKGND, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_MOUSEMOVE, WM_NCCREATE, WM_NCDESTROY, WM_PAINT,
    WM_SIZE, WM_TIMER, WNDCLASSW, WS_CHILD, WS_CLIPCHILDREN, WS_CLIPSIBLINGS, WS_VISIBLE,
};
use windows_numerics::Vector2;

use super::analyzer::SpectrumData;
use super::{u32_to_f32, Meters, NebulaParams};

const BASE_W: f32 = 860.0;
const BASE_H: f32 = 640.0;
const TIMER_ID: usize = 7401;
const TIMER_MS: u32 = 33;

pub(super) fn create_editor(
    params: Arc<NebulaParams>,
    spectrum: Arc<Mutex<SpectrumData>>,
    meters: Arc<Meters>,
) -> Option<Box<dyn Editor>> {
    Some(Box::new(NativeEditor {
        params,
        spectrum,
        meters,
        scale_bits: AtomicU32::new(1.0_f32.to_bits()),
    }))
}

struct NativeEditor {
    params: Arc<NebulaParams>,
    spectrum: Arc<Mutex<SpectrumData>>,
    meters: Arc<Meters>,
    scale_bits: AtomicU32,
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

        let scale = f32::from_bits(self.scale_bits.load(Ordering::Acquire)).clamp(0.5, 3.0);
        let width = (BASE_W * scale).round() as i32;
        let height = (BASE_H * scale).round() as i32;

        let state = Box::new(NativeWindowState::new(
            self.params.clone(),
            self.spectrum.clone(),
            self.meters.clone(),
            context,
            scale,
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
                Some(HWND(parent_hwnd)),
                Option::<HMENU>::None,
                module_instance(),
                Some(state_ptr.cast::<c_void>()),
            )
        };

        match hwnd {
            Ok(hwnd) => unsafe {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = UpdateWindow(hwnd);
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
        (BASE_W as u32, BASE_H as u32)
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
    params: Arc<NebulaParams>,
    spectrum: Arc<Mutex<SpectrumData>>,
    meters: Arc<Meters>,
    context: Arc<dyn GuiContext>,
    d2d_factory: Option<ID2D1Factory>,
    dwrite_factory: Option<IDWriteFactory>,
    render_target: Option<ID2D1HwndRenderTarget>,
    text_formats: Option<TextFormats>,
    smooth_mags: Vec<f32>,
    drag: Option<DragState>,
    scale: f32,
}

impl NativeWindowState {
    fn new(
        params: Arc<NebulaParams>,
        spectrum: Arc<Mutex<SpectrumData>>,
        meters: Arc<Meters>,
        context: Arc<dyn GuiContext>,
        scale: f32,
    ) -> Self {
        Self {
            hwnd: HWND::default(),
            params,
            spectrum,
            meters,
            context,
            d2d_factory: None,
            dwrite_factory: None,
            render_target: None,
            text_formats: None,
            smooth_mags: vec![-120.0; 1025],
            drag: None,
            scale,
        }
    }

    fn paint(&mut self) {
        let Some(rt) = self.ensure_render_target() else {
            return;
        };
        let Some(formats) = self.ensure_text_formats() else {
            return;
        };
        let Some(size) = client_size(self.hwnd) else {
            return;
        };
        let w = size.0.max(1) as f32;
        let h = size.1.max(1) as f32;
        let s = (w / BASE_W).min(h / BASE_H).max(0.45);
        let layout = Layout::new(w, h, s);
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
            "Sibilance Processor  |  Native Direct2D",
            UiRect::new(44.0 * s, 30.0 * s, 260.0 * s, 18.0 * s),
            &formats.small,
            &brushes.text_tertiary,
            Align::Leading,
        );

        let bypass = self.params.bypass.value() > 0.5;
        let bypass_rect = layout.bypass_rect;
        let bypass_fill = if bypass {
            &brushes.red_soft
        } else {
            &brushes.control
        };
        fill_round(rt, bypass_rect, 4.0 * s, bypass_fill);
        stroke_round(
            rt,
            bypass_rect,
            4.0 * s,
            if bypass {
                &brushes.red
            } else {
                &brushes.border
            },
            1.0,
        );
        draw_text(
            rt,
            if bypass { "Bypassed" } else { "Bypass" },
            bypass_rect,
            &formats.body,
            if bypass {
                &brushes.red
            } else {
                &brushes.text_secondary
            },
            Align::Center,
        );

        draw_text(
            rt,
            "v2.4",
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

        let mut x = layout.command_bar.x + 8.0 * s;
        let cy = layout.command_bar.center_y();
        let button_h = 24.0 * s;
        let bypass = self.params.bypass.value() > 0.5;
        let bypass_rect = UiRect::new(x, cy - button_h * 0.5, 84.0 * s, button_h);
        self.draw_toolbar_button(
            rt,
            brushes,
            formats,
            bypass_rect,
            if bypass { "Bypassed" } else { "Bypass" },
            bypass,
            true,
            s,
        );
        x = bypass_rect.right() + 10.0 * s;

        draw_line(
            rt,
            x,
            cy - button_h * 0.36,
            x,
            cy + button_h * 0.36,
            &brushes.divider,
            1.0,
        );
        x += 10.0 * s;

        let os_labels = ["Off", "2x", "4x", "6x", "8x"];
        let os = self.params.oversampling.value().round().clamp(0.0, 4.0) as usize;
        let os_rect = UiRect::new(x, cy - button_h * 0.5, 174.0 * s, button_h);
        fill_round(rt, os_rect, 4.0 * s, &brushes.control);
        stroke_round(rt, os_rect, 4.0 * s, &brushes.border, 1.0);
        let segment_w = os_rect.w / os_labels.len() as f32;
        for (idx, label) in os_labels.iter().enumerate() {
            let segment = UiRect::new(os_rect.x + idx as f32 * segment_w, os_rect.y, segment_w, os_rect.h);
            if idx == os {
                fill_round(rt, segment.shrink(2.0 * s), 3.0 * s, &brushes.accent);
            }
            draw_text(
                rt,
                label,
                segment,
                &formats.small,
                if idx == os {
                    &brushes.text_light
                } else {
                    &brushes.text_secondary
                },
                Align::Center,
            );
        }
        x = os_rect.right() + 10.0 * s;

        let status = if self.params.lookahead_enabled.value() > 0.5 {
            "Lookahead active"
        } else if self.params.sidechain_external.value() > 0.5 {
            "External sidechain"
        } else {
            "Internal detector"
        };
        draw_text(
            rt,
            status,
            UiRect::new(x, cy - button_h * 0.5, 160.0 * s, button_h),
            &formats.small,
            &brushes.text_tertiary,
            Align::Leading,
        );

        draw_text(
            rt,
            "A/B  |  MIDI Learn",
            UiRect::new(
                layout.command_bar.right() - 144.0 * s,
                cy - button_h * 0.5,
                132.0 * s,
                button_h,
            ),
            &formats.small,
            &brushes.text_tertiary,
            Align::Trailing,
        );
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
        let title = if detect { "Detect" } else { "Annihilation" };
        draw_text(
            rt,
            title,
            UiRect::new(rect.x, rect.y + 8.0 * s, rect.w, 20.0 * s),
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

        let max_rect = UiRect::new(
            rect.x + 9.0 * s,
            rect.y + 30.0 * s,
            rect.w - 18.0 * s,
            20.0 * s,
        );
        fill_round(rt, max_rect, 4.0 * s, &brushes.control);
        stroke_round(rt, max_rect, 4.0 * s, &brushes.border, 1.0);
        draw_text(
            rt,
            &format!("{max_value:.1}"),
            max_rect,
            &formats.small,
            &brushes.text_secondary,
            Align::Center,
        );

        let meter = UiRect::new(
            rect.center_x() - 8.0 * s,
            rect.y + 62.0 * s,
            16.0 * s,
            rect.h - 104.0 * s,
        );
        fill_round(rt, meter, 4.0 * s, &brushes.control);
        stroke_round(rt, meter, 4.0 * s, &brushes.border, 1.0);

        let norm = if detect {
            ((value + 60.0) / 60.0).clamp(0.0, 1.0)
        } else {
            (-value / 100.0).clamp(0.0, 1.0)
        };
        let fill_h = meter.h * norm;
        if fill_h > 1.0 {
            let fill = UiRect::new(meter.x, meter.bottom() - fill_h, meter.w, fill_h);
            let brush = if norm > 0.75 {
                &brushes.red
            } else if norm > 0.55 {
                &brushes.yellow
            } else {
                &brushes.green
            };
            fill_round(rt, fill, 3.0 * s, brush);
        }

        let bottom = if detect {
            format!("{value:.1} dB")
        } else {
            format!("{value:.1} dB")
        };
        draw_text(
            rt,
            &bottom,
            UiRect::new(
                rect.x + 4.0 * s,
                rect.bottom() - 32.0 * s,
                rect.w - 8.0 * s,
                20.0 * s,
            ),
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
                    group.rect.y - 4.0 * s,
                    group.rect.w - 16.0 * s,
                    14.0 * s,
                ),
                &formats.small,
                &brushes.text_tertiary,
                Align::Leading,
            );
            for spec in group.knobs {
                let value = self.target_value(spec.target);
                let norm = target_norm(spec.target, value);
                draw_text(
                    rt,
                    spec.label,
                    UiRect::new(spec.rect.x, spec.rect.y + 2.0 * s, spec.rect.w, 14.0 * s),
                    &formats.small,
                    &brushes.text_tertiary,
                    Align::Center,
                );
                draw_knob(
                    rt,
                    spec.knob_rect.center_x(),
                    spec.knob_rect.center_y(),
                    spec.knob_rect.w.min(spec.knob_rect.h) * 0.5,
                    norm,
                    spec.accent,
                    brushes,
                    s,
                );
                let value_rect = UiRect::new(
                    spec.rect.x + 5.0 * s,
                    spec.rect.bottom() - 18.0 * s,
                    spec.rect.w - 10.0 * s,
                    15.0 * s,
                );
                fill_round(rt, value_rect, 3.0 * s, &brushes.control);
                stroke_round(rt, value_rect, 3.0 * s, &brushes.border, 1.0);
                draw_text(
                    rt,
                    &format_value(spec.target, value),
                    value_rect,
                    &formats.small,
                    spec.accent.brush(brushes),
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
            draw_freq_node(rt, min_x, node_y, "Min", &brushes.teal, &brushes.card, formats, s);
            draw_freq_node(rt, max_x, node_y, "Max", &brushes.orange, &brushes.card, formats, s);
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
        let Some(size) = client_size(self.hwnd) else {
            return;
        };
        let layout = Layout::new(size.0 as f32, size.1 as f32, self.scale);
        let zones = hit_zones(&self.params, &layout);
        for zone in zones {
            if !zone.rect.contains(x, y) {
                continue;
            }
            match zone.action {
                HitAction::Drag(target, track, mode) => {
                    self.begin_target(target);
                    match mode {
                        DragMode::Horizontal => self.set_target_from_x(target, track, x),
                        DragMode::Vertical => {}
                    }
                    self.drag = Some(DragState {
                        target,
                        track,
                        mode,
                        start_y: y,
                        start_value: self.target_value(target),
                    });
                    unsafe {
                        let _ = SetCapture(self.hwnd);
                    }
                }
                HitAction::Set(target, value) => {
                    self.set_target_gesture(target, value);
                }
                HitAction::Toggle(target) => {
                    let next = if self.target_value(target) > 0.5 {
                        0.0
                    } else {
                        1.0
                    };
                    self.set_target_gesture(target, next);
                }
                HitAction::ResetDetect => {
                    self.meters.reset_det.store(1, Ordering::Release);
                }
                HitAction::ResetReduction => {
                    self.meters.reset_red.store(1, Ordering::Release);
                }
            }
            invalidate(self.hwnd);
            break;
        }
    }

    fn mouse_move(&mut self, x: f32, y: f32) {
        if let Some(drag) = self.drag {
            match drag.mode {
                DragMode::Horizontal => self.set_target_from_x(drag.target, drag.track, x),
                DragMode::Vertical => self.set_target_from_y(drag.target, drag.start_value, drag.start_y, y),
            }
            invalidate(self.hwnd);
        }
    }

    fn mouse_up(&mut self, x: f32, y: f32) {
        if let Some(drag) = self.drag.take() {
            match drag.mode {
                DragMode::Horizontal => self.set_target_from_x(drag.target, drag.track, x),
                DragMode::Vertical => self.set_target_from_y(drag.target, drag.start_value, drag.start_y, y),
            }
            self.end_target(drag.target);
            let _ = unsafe { ReleaseCapture() };
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
                dpiX: 0.0,
                dpiY: 0.0,
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

    fn ensure_text_formats(&mut self) -> Option<TextFormats> {
        if self.text_formats.is_none() {
            if self.dwrite_factory.is_none() {
                self.dwrite_factory = unsafe {
                    DWriteCreateFactory::<IDWriteFactory>(DWRITE_FACTORY_TYPE_SHARED).ok()
                };
            }
            let factory = self.dwrite_factory.as_ref()?;
            let s = self.scale.max(0.5);
            self.text_formats = Some(TextFormats::new(factory, s)?);
        }
        self.text_formats.clone()
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
            ControlTarget::SidechainExternal => self.params.sidechain_external.value(),
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
    bypass_rect: UiRect,
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
        let bypass_rect = UiRect::new(w - 118.0 * s, 14.0 * s, 78.0 * s, 24.0 * s);
        Self {
            full,
            header,
            command_bar,
            bypass_rect,
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
}

#[derive(Clone, Copy)]
struct DragState {
    target: ControlTarget,
    track: UiRect,
    mode: DragMode,
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
    SidechainExternal,
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
    let group_w = (inner.w - group_gap * 5.0) / 6.0;
    let y = inner.y;
    let h = 58.0 * s;

    let mut groups = Vec::with_capacity(6);
    for i in 0..6 {
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
            3 => SegmentGroup {
                label: "Sidechain",
                rect,
                segments: make_two(
                    (
                        "Int",
                        HitAction::Set(ControlTarget::SidechainExternal, 0.0),
                        params.sidechain_external.value() <= 0.5,
                    ),
                    (
                        "Ext",
                        HitAction::Set(ControlTarget::SidechainExternal, 1.0),
                        params.sidechain_external.value() > 0.5,
                    ),
                ),
            },
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
            _ => {
                let sw = (group_w - 12.0 * s) / 3.0;
                let active = params.oversampling.value().round() as i32;
                SegmentGroup {
                    label: "OS",
                    rect,
                    segments: [
                        SegmentSpec {
                            label: "Off",
                            rect: UiRect::new(x + 4.0 * s, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::Oversampling, 0.0),
                            active: active == 0,
                        },
                        SegmentSpec {
                            label: "2x",
                            rect: UiRect::new(x + 6.0 * s + sw, seg_y, sw, seg_h),
                            action: HitAction::Set(ControlTarget::Oversampling, 1.0),
                            active: active == 1,
                        },
                        SegmentSpec {
                            label: "4x+",
                            rect: UiRect::new(x + 8.0 * s + sw * 2.0, seg_y, sw, seg_h),
                            action: HitAction::Set(
                                ControlTarget::Oversampling,
                                if active >= 2 {
                                    (active + 1).min(4) as f32
                                } else {
                                    2.0
                                },
                            ),
                            active: active >= 2,
                        },
                    ],
                }
            }
        };
        groups.push(group);
    }
    groups
}

fn knob_groups(layout: &Layout) -> Vec<KnobGroup> {
    let s = layout.s;
    let inner = layout.controls.shrink(8.0 * s);
    let row_h = 70.0 * s;
    let row_gap = 7.0 * s;
    let mut y = inner.y + 72.0 * s;

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
    let knob_size = (slot_w * 0.46).min(33.0 * s).max(18.0 * s);
    let knobs = defs
        .iter()
        .enumerate()
        .map(|(idx, (label, target, accent))| {
            let slot = UiRect::new(rect.x + idx as f32 * slot_w, rect.y, slot_w, rect.h);
            let knob_rect = UiRect::new(
                slot.center_x() - knob_size * 0.5,
                slot.y + 19.0 * s,
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
    zones.push(HitZone {
        rect: layout.bypass_rect,
        action: HitAction::Toggle(ControlTarget::Bypass),
    });
    let s = layout.s;
    zones.push(HitZone {
        rect: UiRect::new(
            layout.left_panel.x + 9.0 * s,
            layout.left_panel.y + 30.0 * s,
            layout.left_panel.w - 18.0 * s,
            20.0 * s,
        ),
        action: HitAction::ResetDetect,
    });
    zones.push(HitZone {
        rect: UiRect::new(
            layout.right_panel.x + 9.0 * s,
            layout.right_panel.y + 30.0 * s,
            layout.right_panel.w - 18.0 * s,
            20.0 * s,
        ),
        action: HitAction::ResetReduction,
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
        rect: UiRect::new(min_x - node_hit * 0.5, node_y - node_hit * 0.5, node_hit, node_hit),
        action: HitAction::Drag(
            ControlTarget::MinFreq,
            UiRect::new(graph.x, graph.y, graph.w, graph.h),
            DragMode::Horizontal,
        ),
    });
    zones.push(HitZone {
        rect: UiRect::new(max_x - node_hit * 0.5, node_y - node_hit * 0.5, node_hit, node_hit),
        action: HitAction::Drag(
            ControlTarget::MaxFreq,
            UiRect::new(graph.x, graph.y, graph.w, graph.h),
            DragMode::Horizontal,
        ),
    });
    for idx in 0..5 {
        let seg_w = 174.0 * s / 5.0;
        let cy = layout.command_bar.center_y();
        let os_rect = UiRect::new(
            layout.command_bar.x + 8.0 * s + 84.0 * s + 20.0 * s + idx as f32 * seg_w,
            cy - 12.0 * s,
            seg_w,
            24.0 * s,
        );
        zones.push(HitZone {
            rect: os_rect,
            action: HitAction::Set(ControlTarget::Oversampling, idx as f32),
        });
    }
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
        ControlTarget::StereoLink
        | ControlTarget::CutWidth
        | ControlTarget::CutDepth
        | ControlTarget::Mix
        | ControlTarget::ModeRelative
        | ControlTarget::UseWideRange
        | ControlTarget::FilterSolo
        | ControlTarget::LookaheadEnabled
        | ControlTarget::TriggerHear
        | ControlTarget::StereoMidSide
        | ControlTarget::SidechainExternal
        | ControlTarget::VocalMode
        | ControlTarget::Bypass => (0.0, 1.0),
        ControlTarget::InputLevel | ControlTarget::OutputLevel => (-100.0, 100.0),
        ControlTarget::InputPan | ControlTarget::OutputPan => (-1.0, 1.0),
        ControlTarget::BasisMode => (0.0, 2.0),
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
        ControlTarget::BasisMode | ControlTarget::Oversampling
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
            | ControlTarget::SidechainExternal
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
        ControlTarget::StereoLink
        | ControlTarget::CutWidth
        | ControlTarget::CutDepth
        | ControlTarget::Mix => {
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
        ControlTarget::SidechainExternal => f(&params.sidechain_external),
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
    accent: AccentBrush,
    brushes: &Brushes,
    s: f32,
) {
    let norm = norm.clamp(0.0, 1.0);
    let accent = accent.brush(brushes);
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let angle = start + sweep * norm;

    fill_circle(rt, cx, cy + 1.5 * s, radius + 1.5 * s, &brushes.mica_bot);
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
    draw_arc(rt, cx, cy, radius * 0.73, start, start + sweep, &brushes.border, 3.4 * s);
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
            WM_ERASEBKGND => LRESULT(1),
            WM_SIZE => {
                state.render_target = None;
                invalidate(hwnd);
                LRESULT(0)
            }
            WM_TIMER => {
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
                state.mouse_down(x, y);
                LRESULT(0)
            }
            WM_MOUSEMOVE => {
                if state.drag.is_some() {
                    let (x, y) = point_from_lparam(lparam);
                    state.mouse_move(x, y);
                    LRESULT(0)
                } else {
                    DefWindowProcW(hwnd, msg, wparam, lparam)
                }
            }
            WM_LBUTTONUP => {
                let (x, y) = point_from_lparam(lparam);
                state.mouse_up(x, y);
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
