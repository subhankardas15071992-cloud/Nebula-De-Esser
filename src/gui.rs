// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser v2.6.0 — Windows 11 WinUI 3 Dark Design Language
// Mica base, Acrylic panels, CommandBar toolbar, WinUI controls throughout.
// Scaling: all hardcoded pixel constants multiplied by `s` (scale factor).
// 
// NOTE: As of v2.5.0, the algorithm migrated from compression-based de-essing
// to Orthogonal Subspace Projection with Teager-Kaiser Energy Operator (TKEO).
// The "threshold" parameter now controls TKEO sensitivity, not compression.
// ─────────────────────────────────────────────────────────────────────────────
use crate::analyzer::SpectrumData;
use crate::{MidiLearnShared, MIDI_PARAM_COUNT, MIDI_PARAM_NAMES};
use nih_plug_egui::egui::{
    self, Color32, Context, FontFamily, FontId, Pos2, Rect, Sense, Stroke, Ui, Vec2,
};
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::EguiState;
use parking_lot::Mutex;
use std::sync::Arc;

// ─── Nebula Sci-Fi Dark Palette — neon accents on deep black ─────────────────
// WinUI layout structure preserved, sci-fi colour energy injected.
// Base — deep blacks with violet undertone, not grey.
const MICA_BASE: Color32 = Color32::from_rgb(4, 2, 14); // near-black, violet tint
const MICA_TOP: Color32 = Color32::from_rgb(8, 4, 22); // slightly lighter at top
const MICA_BOT: Color32 = Color32::from_rgb(2, 1, 8); // deeper at bottom

// Panel surfaces — dark with subtle violet tint
const ACRYLIC: Color32 = Color32::from_rgb(10, 6, 26); // deep violet-black panel
const ACRYLIC_CARD: Color32 = Color32::from_rgb(14, 8, 34); // slightly raised card
const ACRYLIC_HIGH: Color32 = Color32::from_rgb(20, 12, 46); // hover / active surface

// Control fills — dark tinted, not grey
const CTRL_DEFAULT: Color32 = Color32::from_rgb(18, 10, 38); // resting control
const CTRL_HOVER: Color32 = Color32::from_rgb(26, 16, 54); // hover lift

// Borders — neon-tinted strokes
const STROKE_DEF: Color32 = Color32::from_rgb(50, 30, 90); // purple-tinted border
const DIVIDER: Color32 = Color32::from_rgb(30, 18, 60); // separator

// Accent — electric cyan as primary (replaces Windows blue)
const ACCENT: Color32 = Color32::from_rgb(0, 210, 255); // electric cyan
const ACCENT_LIGHT: Color32 = Color32::from_rgb(80, 235, 255); // lighter cyan for lines/text
const ACCENT_DARK: Color32 = Color32::from_rgb(0, 140, 180); // darker cyan for borders
const ACCENT_BORDER: Color32 = Color32::from_rgb(0, 90, 130);

// Semantic neon colours
const RED: Color32 = Color32::from_rgb(255, 55, 55); // hot red
const ORANGE: Color32 = Color32::from_rgb(255, 160, 0); // neon amber
const TEAL: Color32 = Color32::from_rgb(0, 200, 180); // neon teal
const MAGENTA: Color32 = Color32::from_rgb(255, 0, 200); // hot magenta (knob accent)
const PURPLE: Color32 = Color32::from_rgb(160, 40, 255); // electric purple (I/O accent)

// Meter colours — neon
const M_GREEN: Color32 = Color32::from_rgb(0, 220, 90);
const M_YELLOW: Color32 = Color32::from_rgb(255, 200, 0);
const M_RED: Color32 = Color32::from_rgb(255, 55, 55);

// Text — bright cool-white on deep black for high contrast
const TEXT_PRI: Color32 = Color32::from_rgb(210, 235, 255); // cool white — primary
const TEXT_SEC: Color32 = Color32::from_rgb(120, 165, 210); // muted blue-white — secondary
const TEXT_TER: Color32 = Color32::from_rgb(55, 85, 140); // dim blue — tertiary
const TEXT_DIS: Color32 = Color32::from_rgb(25, 40, 70); // disabled

// Card top-edge highlight — neon cyan edge
const CARD_TOP: Color32 = Color32::from_rgb(0, 160, 210);

const BASE_W: f32 = 860.0;
const BASE_H: f32 = 640.0;

#[inline]
fn ga(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        ((c.r() as u16 * a as u16) / 255) as u8,
        ((c.g() as u16 * a as u16) / 255) as u8,
        ((c.b() as u16 * a as u16) / 255) as u8,
        a,
    )
}
#[inline]
fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0);
    let u = 1.0 - t;
    Color32::from_rgb(
        (a.r() as f32 * u + b.r() as f32 * t) as u8,
        (a.g() as f32 * u + b.g() as f32 * t) as u8,
        (a.b() as f32 * u + b.b() as f32 * t) as u8,
    )
}

// Draw Acrylic card: tinted fill + top highlight stroke + outer border
fn acrylic_card(pa: &egui::Painter, rect: Rect, radius: f32) {
    pa.rect_filled(rect, radius, ACRYLIC);
    // Top highlight — slightly lighter top border to simulate the Acrylic top edge
    let tl = Pos2::new(rect.min.x + radius * 0.5, rect.min.y);
    let tr = Pos2::new(rect.max.x - radius * 0.5, rect.min.y);
    pa.line_segment([tl, tr], Stroke::new(1.0, CARD_TOP));
    // Outer card border
    pa.rect_stroke(
        rect,
        radius,
        Stroke::new(1.0, STROKE_DEF),
        egui::StrokeKind::Outside,
    );
}

// Draw Mica background with top-to-bottom luminosity gradient
fn mica_bg(pa: &egui::Painter, rect: Rect) {
    pa.rect_filled(rect, 0.0, MICA_BASE);
    // Subtle top lift — slightly lighter strip, matching Win11 dark Mica
    let top_strip = Rect::from_min_size(rect.min, Vec2::new(rect.width(), rect.height() * 0.25));
    pa.rect_filled(top_strip, 0.0, MICA_TOP);
    // Bottom darkening
    let bot_strip = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.max.y - rect.height() * 0.2),
        Vec2::new(rect.width(), rect.height() * 0.2),
    );
    pa.rect_filled(bot_strip, 0.0, MICA_BOT);
}

// ─── Param Snapshot ───────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
pub struct ParamSnapshot {
    pub threshold: f64,
    pub max_reduction: f64,
    pub min_freq: f64,
    pub max_freq: f64,
    pub mode_relative: bool,
    pub use_peak_filter: bool,
    pub use_wide_range: bool,
    pub filter_solo: bool,
    pub lookahead_enabled: bool,
    pub lookahead_ms: f64,
    pub trigger_hear: bool,
    pub stereo_link: f64,
    pub stereo_mid_side: bool,
    pub sidechain_external: bool,
    pub vocal_mode: bool,
    pub input_level: f64,
    pub input_pan: f64,
    pub output_level: f64,
    pub output_pan: f64,
    pub cut_width: f64,
    pub cut_depth: f64,
    pub cut_slope: f64,
    pub mix: f64,
}
impl ParamSnapshot {
    pub fn from_params(p: &GuiParams) -> Self {
        Self {
            threshold: p.threshold,
            max_reduction: p.max_reduction,
            min_freq: p.min_freq,
            max_freq: p.max_freq,
            mode_relative: p.mode_relative,
            use_peak_filter: p.use_peak_filter,
            use_wide_range: p.use_wide_range,
            filter_solo: p.filter_solo,
            lookahead_enabled: p.lookahead_enabled,
            lookahead_ms: p.lookahead_ms,
            trigger_hear: p.trigger_hear,
            stereo_link: p.stereo_link,
            stereo_mid_side: p.stereo_mid_side,
            sidechain_external: p.sidechain_external,
            vocal_mode: p.vocal_mode,
            input_level: p.input_level,
            input_pan: p.input_pan,
            output_level: p.output_level,
            output_pan: p.output_pan,
            cut_width: p.cut_width,
            cut_depth: p.cut_depth,
            cut_slope: p.cut_slope,
            mix: p.mix,
        }
    }
    pub fn apply_to(&self, ch: &mut GuiChanges) {
        ch.threshold = Some(self.threshold);
        ch.max_reduction = Some(self.max_reduction);
        ch.min_freq = Some(self.min_freq);
        ch.max_freq = Some(self.max_freq);
        ch.mode_relative = Some(self.mode_relative);
        ch.use_peak_filter = Some(self.use_peak_filter);
        ch.use_wide_range = Some(self.use_wide_range);
        ch.filter_solo = Some(self.filter_solo);
        ch.lookahead_enabled = Some(self.lookahead_enabled);
        ch.lookahead_ms = Some(self.lookahead_ms);
        ch.trigger_hear = Some(self.trigger_hear);
        ch.stereo_link = Some(self.stereo_link);
        ch.stereo_mid_side = Some(self.stereo_mid_side);
        ch.sidechain_external = Some(self.sidechain_external);
        ch.vocal_mode = Some(self.vocal_mode);
        ch.input_level = Some(self.input_level);
        ch.input_pan = Some(self.input_pan);
        ch.output_level = Some(self.output_level);
        ch.output_pan = Some(self.output_pan);
        ch.cut_width = Some(self.cut_width);
        ch.cut_depth = Some(self.cut_depth);
        ch.cut_slope = Some(self.cut_slope);
        ch.mix = Some(self.mix);
    }
}

#[derive(Default, Clone, PartialEq)]
pub enum NumTarget {
    #[default]
    None,
    Threshold,
    MaxReduction,
    MinFreq,
    MaxFreq,
    Lookahead,
    StereoLink,
    InputLevel,
    InputPan,
    OutputLevel,
    OutputPan,
    CutWidth,
    CutDepth,
    CutSlope,
    Mix,
}
#[derive(Default, Clone)]
pub struct NumInput {
    pub open: bool,
    pub label: String,
    pub value_str: String,
    pub target: NumTarget,
    pub min: f64,
    pub max: f64,
}

// ─── GUI State ────────────────────────────────────────────────────────────────
pub struct NebulaGui {
    pub spectrum: Arc<Mutex<SpectrumData>>,
    pub midi_learn: Arc<MidiLearnShared>,
    pub num_input: NumInput,
    pub time: f64,
    pub smooth_mags: Vec<f32>,
    pub presets: Vec<(String, ParamSnapshot)>,
    pub preset_name_buf: String,
    pub preset_save_popup: bool,
    pub preset_dropdown_open: bool,
    pub selected_preset: usize,
    pub state_a: Option<ParamSnapshot>,
    pub state_b: Option<ParamSnapshot>,
    pub active_state: char,
    pub undo_stack: Vec<ParamSnapshot>,
    pub redo_stack: Vec<ParamSnapshot>,
    pub drag_snap: Option<(f64, f64)>,
    pub midi_popup: bool,
    pub midi_context_menu: bool,
    pub midi_context_anchor: Pos2,
    pub midi_cleanup_menu: bool,
    pub midi_cleanup_anchor: Pos2,
    pub os_dropdown: bool,
    pub os_anchor: Pos2,
    pub preset_anchor: Pos2,
}
impl NebulaGui {
    pub fn new(spectrum: Arc<Mutex<SpectrumData>>, midi_learn: Arc<MidiLearnShared>) -> Self {
        Self {
            spectrum,
            midi_learn,
            num_input: NumInput::default(),
            time: 0.0,
            smooth_mags: vec![-120.0_f32; 1025],
            presets: Vec::new(),
            preset_name_buf: String::new(),
            preset_save_popup: false,
            preset_dropdown_open: false,
            selected_preset: 0,
            state_a: None,
            state_b: None,
            active_state: 'A',
            undo_stack: Vec::new(),
            redo_stack: Vec::new(),
            drag_snap: None,
            midi_popup: false,
            midi_context_menu: false,
            midi_context_anchor: Pos2::ZERO,
            midi_cleanup_menu: false,
            midi_cleanup_anchor: Pos2::ZERO,
            os_dropdown: false,
            os_anchor: Pos2::ZERO,
            preset_anchor: Pos2::ZERO,
        }
    }
}

// ─── Params / Changes ────────────────────────────────────────────────────────
pub struct GuiParams {
    pub threshold: f64,
    pub max_reduction: f64,
    pub min_freq: f64,
    pub max_freq: f64,
    pub mode_relative: bool,
    pub use_peak_filter: bool,
    pub use_wide_range: bool,
    pub filter_solo: bool,
    pub lookahead_enabled: bool,
    pub lookahead_ms: f64,
    pub trigger_hear: bool,
    pub stereo_link: f64,
    pub stereo_mid_side: bool,
    pub sidechain_external: bool,
    pub vocal_mode: bool,
    pub detection_db: f32,
    pub detection_max_db: f32,
    pub reduction_db: f32,
    pub reduction_max_db: f32,
    pub input_level: f64,
    pub input_pan: f64,
    pub output_level: f64,
    pub output_pan: f64,
    pub bypass: bool,
    pub oversampling: u32,
    pub cut_width: f64,
    pub cut_depth: f64,
    pub mix: f64,
    pub cut_slope: f64,
}
#[derive(Default)]
pub struct GuiChanges {
    pub threshold: Option<f64>,
    pub max_reduction: Option<f64>,
    pub min_freq: Option<f64>,
    pub max_freq: Option<f64>,
    pub mode_relative: Option<bool>,
    pub use_peak_filter: Option<bool>,
    pub use_wide_range: Option<bool>,
    pub filter_solo: Option<bool>,
    pub lookahead_enabled: Option<bool>,
    pub lookahead_ms: Option<f64>,
    pub trigger_hear: Option<bool>,
    pub stereo_link: Option<f64>,
    pub stereo_mid_side: Option<bool>,
    pub sidechain_external: Option<bool>,
    pub vocal_mode: Option<bool>,
    pub detection_max_reset: bool,
    pub reduction_max_reset: bool,
    pub input_level: Option<f64>,
    pub input_pan: Option<f64>,
    pub output_level: Option<f64>,
    pub output_pan: Option<f64>,
    pub bypass: Option<bool>,
    pub oversampling: Option<u32>,
    pub cut_width: Option<f64>,
    pub cut_depth: Option<f64>,
    pub cut_slope: Option<f64>,
    pub mix: Option<f64>,
}

// ─── Main Draw ────────────────────────────────────────────────────────────────
pub fn draw(
    ctx: &Context,
    egui_state: &EguiState,
    gui: &mut NebulaGui,
    params: &GuiParams,
) -> GuiChanges {
    gui.time += ctx.input(|i| i.unstable_dt) as f64;
    let mut ch = GuiChanges::default();

    let (win_w, win_h) = egui_state.size();
    let s = (win_w as f32 / BASE_W).min(win_h as f32 / BASE_H).max(0.25);

    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = MICA_BASE;
    style.visuals.override_text_color = Some(TEXT_PRI);
    style.visuals.widgets.noninteractive.bg_fill = ACRYLIC;
    style.visuals.widgets.inactive.bg_fill = ACRYLIC_CARD;
    style.visuals.widgets.hovered.bg_fill = ACRYLIC_HIGH;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0 * s, ACCENT_LIGHT);
    style.spacing.item_spacing = Vec2::new(4.0 * s, 3.0 * s);
    ctx.set_style(style);

    ResizableWindow::new("nebula_deesser_resize")
        .min_size(Vec2::new(400.0, 300.0))
        .show(ctx, egui_state, |ui| {
            let full = ui.max_rect();
            // Mica background
            mica_bg(&ui.painter_at(full), full);

            let title_h = 52.0 * s;
            let toolbar_h = 36.0 * s;
            let margin = 8.0 * s;

            draw_nav_header(ui.painter_at(full), full, params.bypass, s);
            draw_command_bar(
                ui,
                Rect::from_min_size(
                    Pos2::new(full.min.x, full.min.y + title_h),
                    Vec2::new(full.width(), toolbar_h),
                ),
                params,
                gui,
                &mut ch,
                s,
            );

            let content = Rect::from_min_size(
                Pos2::new(
                    full.min.x + margin,
                    full.min.y + title_h + toolbar_h + margin,
                ),
                Vec2::new(
                    full.width() - margin * 2.0,
                    full.height() - title_h - toolbar_h - margin * 2.0,
                ),
            );

            let mw = 92.0 * s;
            let gap = 8.0 * s;
            let cw = content.width() - mw * 2.0 - gap * 2.0;

            let left_r = Rect::from_min_size(content.min, Vec2::new(mw, content.height()));
            let right_r = Rect::from_min_size(
                Pos2::new(content.max.x - mw, content.min.y),
                Vec2::new(mw, content.height()),
            );
            let ctr_r = Rect::from_min_size(
                Pos2::new(content.min.x + mw + gap, content.min.y),
                Vec2::new(cw, content.height()),
            );

            let spec_frac = 0.28_f32;
            let ctrl_h = ctr_r.height() * (1.0 - spec_frac);
            let ctrl_r = Rect::from_min_size(ctr_r.min, Vec2::new(cw, ctrl_h));
            let spec_r = Rect::from_min_size(
                Pos2::new(ctr_r.min.x, ctr_r.min.y + ctrl_h + 6.0 * s),
                Vec2::new(cw, ctr_r.height() * spec_frac - 6.0 * s),
            );

            draw_det_panel(ui, left_r, params, &mut ch, s);
            draw_red_panel(ui, right_r, params, &mut ch, s);
            draw_controls(ui, ctrl_r, params, &mut ch, gui, s);
            draw_spectrum(ui, spec_r, gui, params, &mut ch, s);
        });

    if gui.num_input.open {
        draw_content_dialog_num(ctx, gui, &mut ch, s);
    }
    if gui.preset_save_popup {
        draw_content_dialog_preset(ctx, gui, params, &mut ch, s);
    }
    if gui.midi_popup {
        draw_content_dialog_midi(ctx, gui, s);
    }
    if gui.os_dropdown {
        draw_flyout_os(ctx, gui, params, &mut ch, s);
    }
    if gui.preset_dropdown_open {
        draw_flyout_preset(ctx, gui, &mut ch, s);
    }
    if gui.midi_context_menu {
        draw_context_menu_midi(ctx, gui, s);
    }
    ch
}

// ─── NavigationView Header (replaces title bar) ───────────────────────────────
fn draw_nav_header(painter: egui::Painter, rect: Rect, bypass: bool, s: f32) {
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 52.0 * s));
    // Acrylic header background
    painter.rect_filled(bar, 0.0, ACRYLIC);
    // Bottom divider — WinUI NavigationView divider
    painter.line_segment(
        [bar.left_bottom(), bar.right_bottom()],
        Stroke::new(1.0, DIVIDER),
    );

    let ty = bar.center().y;
    let tx = bar.min.x + 16.0 * s;

    // Segoe UI Variable Display — simulated with Proportional
    // App icon placeholder — small accent square
    let icon_r = Rect::from_center_size(Pos2::new(tx + 8.0 * s, ty), Vec2::splat(18.0 * s));
    painter.rect_filled(icon_r, 4.0 * s, ACCENT);
    painter.text(
        icon_r.center(),
        egui::Align2::CENTER_CENTER,
        "N",
        FontId::new(11.5 * s, FontFamily::Proportional),
        Color32::WHITE,
    );

    // Title — Segoe UI Variable Subtitle weight
    painter.text(
        Pos2::new(tx + 24.0 * s, ty - 5.0 * s),
        egui::Align2::LEFT_CENTER,
        "Nebula De-Esser",
        FontId::new(15.5 * s, FontFamily::Proportional),
        TEXT_PRI,
    );
    painter.text(
        Pos2::new(tx + 24.0 * s, ty + 8.0 * s),
        egui::Align2::LEFT_CENTER,
        "TKEO Subspace Processor · v2.6.0",
        FontId::new(11.5 * s, FontFamily::Proportional),
        TEXT_TER,
    );

    // Version — right-aligned, tertiary text
    painter.text(
        Pos2::new(bar.max.x - 12.0 * s, ty),
        egui::Align2::RIGHT_CENTER,
        "v2.6",
        FontId::new(12.5 * s, FontFamily::Proportional),
        TEXT_TER,
    );

    // Bypass badge — WinUI InfoBadge style
    if bypass {
        let bx = bar.max.x - 100.0 * s;
        let br = Rect::from_center_size(Pos2::new(bx, ty), Vec2::new(68.0 * s, 22.0 * s));
        painter.rect_filled(br, 4.0 * s, ga(RED, 40));
        painter.rect_stroke(
            br,
            4.0 * s,
            Stroke::new(1.0, ga(RED, 180)),
            egui::StrokeKind::Outside,
        );
        painter.text(
            br.center(),
            egui::Align2::CENTER_CENTER,
            "Bypassed",
            FontId::new(12.5 * s, FontFamily::Proportional),
            RED,
        );
    }
}

// ─── CommandBar (replaces toolbar) ───────────────────────────────────────────
fn draw_command_bar(
    ui: &mut Ui,
    rect: Rect,
    params: &GuiParams,
    gui: &mut NebulaGui,
    ch: &mut GuiChanges,
    s: f32,
) {
    {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 0.0, ACRYLIC);
        p.line_segment(
            [rect.left_bottom(), rect.right_bottom()],
            Stroke::new(1.0, DIVIDER),
        );
    }

    let cy = rect.center().y;
    let bh = 24.0 * s;
    let mut cx = rect.min.x + 8.0 * s;

    // AppBarButton — WinUI CommandBar button style
    // Active: AccentFillColorDefault fill, white text
    // Hover: ControlFillColorSecondary, primary text
    // Rest: ControlFillColorDefault, secondary text
    macro_rules! appbar_btn {
        ($label:expr, $active:expr, $danger:expr, $w:expr) => {{
            let w = $w * s;
            let r = Rect::from_min_max(
                Pos2::new(cx, cy - bh * 0.5),
                Pos2::new(cx + w, cy + bh * 0.5),
            );
            cx += w + 4.0 * s;
            let resp = ui.allocate_rect(r, Sense::click());
            let hov = resp.hovered();
            let (bg, fg, border) = if $active && $danger {
                (
                    Color32::from_rgb(160, 40, 30),
                    Color32::from_rgb(242, 242, 242),
                    Color32::from_rgb(180, 60, 50),
                )
            } else if $active {
                (ACCENT, Color32::from_rgb(242, 242, 242), ACCENT_DARK)
            } else if hov && $danger {
                (
                    Color32::from_rgb(80, 30, 28),
                    RED,
                    Color32::from_rgb(100, 40, 38),
                )
            } else if hov {
                (CTRL_HOVER, TEXT_PRI, STROKE_DEF)
            } else {
                (CTRL_DEFAULT, TEXT_SEC, STROKE_DEF)
            };
            {
                let p = ui.painter_at(rect);
                p.rect_filled(r, 4.0 * s, bg);
                p.rect_stroke(
                    r,
                    4.0 * s,
                    Stroke::new(1.0, border),
                    egui::StrokeKind::Outside,
                );
                p.text(
                    r.center(),
                    egui::Align2::CENTER_CENTER,
                    $label,
                    FontId::new(12.5 * s, FontFamily::Proportional),
                    fg,
                );
            }
            resp
        }};
    }

    if appbar_btn!(
        if params.bypass {
            "⏸ Bypassed"
        } else {
            "⏸ Bypass"
        },
        params.bypass,
        true,
        70.0
    )
    .clicked()
    {
        ch.bypass = Some(!params.bypass);
    }
    // Separator
    {
        let p = ui.painter_at(rect);
        p.line_segment(
            [
                Pos2::new(cx + 2.0 * s, cy - bh * 0.4),
                Pos2::new(cx + 2.0 * s, cy + bh * 0.4),
            ],
            Stroke::new(1.0, DIVIDER),
        );
    }
    cx += 8.0 * s;

    // Preset selector
    let pw = 144.0 * s;
    {
        let pr = Rect::from_min_max(
            Pos2::new(cx, cy - bh * 0.5),
            Pos2::new(cx + pw, cy + bh * 0.5),
        );
        cx += pw + 4.0 * s;
        let resp = ui.allocate_rect(pr, Sense::click());
        let hov = resp.hovered();
        let lbl = if gui.presets.is_empty() {
            "Preset v".to_string()
        } else {
            let n = &gui.presets[gui.selected_preset.min(gui.presets.len() - 1)].0;
            format!("{} v", if n.len() > 16 { &n[..16] } else { n })
        };
        {
            let p = ui.painter_at(rect);
            p.rect_filled(pr, 4.0 * s, if hov { CTRL_HOVER } else { CTRL_DEFAULT });
            p.rect_stroke(
                pr,
                4.0 * s,
                Stroke::new(1.0, STROKE_DEF),
                egui::StrokeKind::Outside,
            );
            p.text(
                pr.center(),
                egui::Align2::CENTER_CENTER,
                &lbl,
                FontId::new(12.5 * s, FontFamily::Proportional),
                if hov { TEXT_PRI } else { TEXT_SEC },
            );
        }
        gui.preset_anchor = Pos2::new(pr.min.x, pr.max.y + 2.0 * s);
        if resp.clicked() {
            gui.preset_dropdown_open = !gui.preset_dropdown_open;
        }
    }
    if appbar_btn!("Save", false, false, 40.0).clicked() {
        gui.preset_name_buf.clear();
        gui.preset_save_popup = true;
        gui.preset_dropdown_open = false;
    }
    if appbar_btn!("Delete", false, true, 46.0).clicked() && !gui.presets.is_empty() {
        gui.presets
            .remove(gui.selected_preset.min(gui.presets.len() - 1));
        if gui.selected_preset > 0 {
            gui.selected_preset -= 1;
        }
    }
    {
        let p = ui.painter_at(rect);
        p.line_segment(
            [
                Pos2::new(cx + 2.0 * s, cy - bh * 0.4),
                Pos2::new(cx + 2.0 * s, cy + bh * 0.4),
            ],
            Stroke::new(1.0, DIVIDER),
        );
    }
    cx += 8.0 * s;

    let can_undo = !gui.undo_stack.is_empty();
    let can_redo = !gui.redo_stack.is_empty();
    {
        let w = 48.0 * s;
        let r = Rect::from_min_max(
            Pos2::new(cx, cy - bh * 0.5),
            Pos2::new(cx + w, cy + bh * 0.5),
        );
        cx += w + 4.0 * s;
        let resp = ui.allocate_rect(r, Sense::click());
        let hov = resp.hovered();
        let (bg, fg) = if !can_undo {
            (CTRL_DEFAULT, TEXT_DIS)
        } else if hov {
            (CTRL_HOVER, TEXT_PRI)
        } else {
            (CTRL_DEFAULT, TEXT_SEC)
        };
        {
            let p = ui.painter_at(rect);
            p.rect_filled(r, 4.0 * s, bg);
            p.rect_stroke(
                r,
                4.0 * s,
                Stroke::new(1.0, STROKE_DEF),
                egui::StrokeKind::Outside,
            );
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "< Undo",
                FontId::new(12.5 * s, FontFamily::Proportional),
                fg,
            );
        }
        if resp.clicked() && can_undo {
            if let Some(snap) = gui.undo_stack.pop() {
                gui.redo_stack.push(ParamSnapshot::from_params(params));
                gui.redo_stack.truncate(50);
                snap.apply_to(ch);
            }
        }
    }
    {
        let w = 48.0 * s;
        let r = Rect::from_min_max(
            Pos2::new(cx, cy - bh * 0.5),
            Pos2::new(cx + w, cy + bh * 0.5),
        );
        cx += w + 4.0 * s;
        let resp = ui.allocate_rect(r, Sense::click());
        let hov = resp.hovered();
        let (bg, fg) = if !can_redo {
            (CTRL_DEFAULT, TEXT_DIS)
        } else if hov {
            (CTRL_HOVER, TEXT_PRI)
        } else {
            (CTRL_DEFAULT, TEXT_SEC)
        };
        {
            let p = ui.painter_at(rect);
            p.rect_filled(r, 4.0 * s, bg);
            p.rect_stroke(
                r,
                4.0 * s,
                Stroke::new(1.0, STROKE_DEF),
                egui::StrokeKind::Outside,
            );
            p.text(
                r.center(),
                egui::Align2::CENTER_CENTER,
                "Redo >",
                FontId::new(12.5 * s, FontFamily::Proportional),
                fg,
            );
        }
        if resp.clicked() && can_redo {
            if let Some(snap) = gui.redo_stack.pop() {
                gui.undo_stack.push(ParamSnapshot::from_params(params));
                gui.undo_stack.truncate(50);
                snap.apply_to(ch);
            }
        }
    }
    {
        let p = ui.painter_at(rect);
        p.line_segment(
            [
                Pos2::new(cx + 2.0 * s, cy - bh * 0.4),
                Pos2::new(cx + 2.0 * s, cy + bh * 0.4),
            ],
            Stroke::new(1.0, DIVIDER),
        );
    }
    cx += 8.0 * s;

    let ab_label = if gui.active_state == 'A' {
        "A/B [A]"
    } else {
        "A/B [B]"
    };
    let ab_active = gui.state_a.is_some() || gui.state_b.is_some();
    let ab_resp = appbar_btn!(ab_label, ab_active, false, 64.0);
    if ab_resp.clicked() {
        let snap = ParamSnapshot::from_params(params);
        match gui.active_state {
            'A' => gui.state_a = Some(snap),
            'B' => gui.state_b = Some(snap),
            _ => {}
        }
        gui.active_state = if gui.active_state == 'A' { 'B' } else { 'A' };
        match (gui.active_state, &gui.state_a, &gui.state_b) {
            ('A', Some(a), _) => a.clone().apply_to(ch),
            ('B', _, Some(b)) => b.clone().apply_to(ch),
            _ => {}
        }
    }
    if ab_resp.secondary_clicked() {
        let snap = ParamSnapshot::from_params(params);
        match gui.active_state {
            'A' => gui.state_a = Some(snap),
            'B' => gui.state_b = Some(snap),
            _ => {}
        }
    }

    let learning = gui
        .midi_learn
        .learning_target
        .load(std::sync::atomic::Ordering::Relaxed)
        >= 0;
    let midi_btn = appbar_btn!(
        if learning { "* Learning" } else { "MIDI Learn" },
        learning,
        false,
        84.0
    );
    if midi_btn.clicked() {
        if learning {
            gui.midi_learn
                .learning_target
                .store(-1, std::sync::atomic::Ordering::Release);
        } else {
            gui.midi_popup = true;
        }
    }
    if midi_btn.secondary_clicked() {
        gui.midi_context_menu = true;
        gui.midi_context_anchor = Pos2::new(midi_btn.rect.min.x, midi_btn.rect.max.y + 2.0 * s);
    }

    let os_labels = ["Off", "2×", "4×", "6×", "8×"];
    let cur = os_labels
        .get(params.oversampling as usize)
        .copied()
        .unwrap_or("Off");
    let os_w = 90.0 * s;
    {
        let or_ = Rect::from_min_max(
            Pos2::new(cx, cy - bh * 0.5),
            Pos2::new(cx + os_w, cy + bh * 0.5),
        );
        gui.os_anchor = Pos2::new(or_.min.x, or_.max.y + 2.0 * s);
        let resp = ui.allocate_rect(or_, Sense::click());
        let active = params.oversampling > 0;
        let hov = resp.hovered();
        let (bg, fg) = if active {
            (ACCENT, Color32::from_rgb(242, 242, 242))
        } else if hov {
            (CTRL_HOVER, TEXT_PRI)
        } else {
            (CTRL_DEFAULT, TEXT_SEC)
        };
        {
            let p = ui.painter_at(rect);
            p.rect_filled(or_, 4.0 * s, bg);
            p.rect_stroke(
                or_,
                4.0 * s,
                Stroke::new(1.0, if active { ga(ACCENT, 80) } else { STROKE_DEF }),
                egui::StrokeKind::Outside,
            );
            p.text(
                or_.center(),
                egui::Align2::CENTER_CENTER,
                cur,
                FontId::new(11.5 * s, FontFamily::Proportional),
                fg,
            );
        }
        if resp.clicked() {
            gui.os_dropdown = !gui.os_dropdown;
        }
    }
}

// ─── Detection Panel (LEFT) — TKEO Sensitivity ───────────────────────────────
fn draw_det_panel(ui: &mut Ui, rect: Rect, params: &GuiParams, ch: &mut GuiChanges, s: f32) {
    let p = ui.painter_at(rect);
    acrylic_card(&p, rect, 8.0 * s);

    let cx = rect.center().x;
    let mut cy = rect.min.y + 12.0 * s;

    // Panel title
    p.text(
        Pos2::new(cx, cy),
        egui::Align2::CENTER_TOP,
        "Detection",
        FontId::new(12.5 * s, FontFamily::Proportional),
        TEXT_PRI,
    );
    cy += 22.0 * s;

    // TKEO Sensitivity slider (formerly "Threshold")
    // Controls how sharp/erratic an energy spike must be to classify as sibilance
    let slider_r = Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0 * s, cy),
        Vec2::new(rect.width() - 16.0 * s, 18.0 * s),
    );
    
    // Label with info tooltip
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("TKEO Sensitivity")
            .color(TEXT_PRI)
            .font(FontId::new(11.5 * s, FontFamily::Proportional)));
        ui.add(egui::Label::new(
            egui::RichText::new("ⓘ")
                .color(TEXT_TER)
                .font(FontId::new(9.0 * s, FontFamily::Monospace))
        ).on_hover_text(
            "Controls how sharp or erratic an energy spike must be\n\
             before the algorithm classifies it as sibilance.\n\
             • Lower = more sensitive (catches subtle spikes)\n\
             • Higher = less sensitive (only aggressive transients)\n\
             \n\
             Uses Teager-Kaiser Energy Operator for detection."
        ));
    });
    cy += 18.0 * s;

    // Draw slider track
    let track = Rect::from_min_size(
        Pos2::new(slider_r.min.x, slider_r.center().y - 2.0 * s),
        Vec2::new(slider_r.width(), 4.0 * s),
    );
    p.rect_filled(track, 2.0 * s, CTRL_DEFAULT);
    p.rect_stroke(track, 2.0 * s, Stroke::new(1.0, STROKE_DEF), egui::StrokeKind::Outside);

    // Calculate thumb position: -60..0 dB range
    let norm = ((params.threshold - -60.0) / 60.0).clamp(0.0, 1.0);
    let thumb_x = slider_r.min.x + norm * slider_r.width();
    let thumb_r = Rect::from_center_size(
        Pos2::new(thumb_x, slider_r.center().y),
        Vec2::new(14.0 * s, 14.0 * s),
    );

    // Thumb with accent gradient
    p.rect_filled(thumb_r, 3.0 * s, ACCENT);
    p.rect_stroke(
        thumb_r,
        3.0 * s,
        Stroke::new(1.0, ACCENT_BORDER),
        egui::StrokeKind::Outside,
    );

    // Value display
    p.text(
        Pos2::new(slider_r.max.x + 4.0 * s, slider_r.center().y),
        egui::Align2::LEFT_CENTER,
        &format!("{:.1} dB", params.threshold),
        FontId::new(10.0 * s, FontFamily::Proportional),
        TEXT_SEC,
    );

    // Interaction
    let slider_resp = ui.allocate_rect(slider_r, Sense::drag());
    if slider_resp.dragged() {
        let dx = slider_resp.drag_delta().x;
        let new_val = (params.threshold + (dx / slider_r.width()) * 60.0).clamp(-60.0, 0.0);
        ch.threshold = Some(new_val);
    }
    if slider_resp.clicked() {
        if let Some(pos) = slider_resp.interact_pointer_pos() {
            let norm = ((pos.x - slider_r.min.x) / slider_r.width()).clamp(0.0, 1.0);
            ch.threshold = Some(-60.0 + norm * 60.0);
        }
    }
    if slider_resp.secondary_clicked() {
        gui.num_input = NumInput {
            open: true,
            label: "TKEO Sensitivity".to_string(),
            value_str: format!("{:.1}", params.threshold),
            target: NumTarget::Threshold,
            min: -60.0,
            max: 0.0,
        };
    }

    // Meter display
    cy += 28.0 * s;
    let meter_h = 48.0 * s;
    let meter_r = Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0 * s, cy),
        Vec2::new(rect.width() - 16.0 * s, meter_h),
    );
    
    // Meter background
    p.rect_filled(meter_r, 4.0 * s, CTRL_DEFAULT);
    p.rect_stroke(
        meter_r,
        4.0 * s,
        Stroke::new(1.0, STROKE_DEF),
        egui::StrokeKind::Outside,
    );

    // Meter fill based on detection_db
    let fill_norm = ((params.detection_db - -60.0) / 60.0).clamp(0.0, 1.0);
    let fill_w = meter_r.width() * fill_norm;
    let fill_r = Rect::from_min_size(meter_r.min, Vec2::new(fill_w, meter_r.height()));
    
    // Gradient fill: green -> yellow -> red
    let fill_col = if params.detection_db > -12.0 {
        M_RED
    } else if params.detection_db > -24.0 {
        M_YELLOW
    } else {
        M_GREEN
    };
    p.rect_filled(fill_r, 4.0 * s, ga(fill_col, 180));

    // Peak marker
    let peak_norm = ((params.detection_max_db - -60.0) / 60.0).clamp(0.0, 1.0);
    let peak_x = meter_r.min.x + peak_norm * meter_r.width();
    p.line_segment(
        [
            Pos2::new(peak_x, meter_r.min.y + 2.0 * s),
            Pos2::new(peak_x, meter_r.max.y - 2.0 * s),
        ],
        Stroke::new(2.0 * s, ACCENT_LIGHT),
    );

    // Reset peak button (small)
    let reset_r = Rect::from_min_size(
        Pos2::new(meter_r.max.x - 24.0 * s, meter_r.min.y + 2.0 * s),
        Vec2::new(20.0 * s, 16.0 * s),
    );
    let reset_resp = ui.allocate_rect(reset_r, Sense::click());
    if reset_resp.hovered() {
        p.rect_filled(reset_r, 3.0 * s, CTRL_HOVER);
    }
    p.text(
        reset_r.center(),
        egui::Align2::CENTER_CENTER,
        "↺",
        FontId::new(11.0 * s, FontFamily::Monospace),
        TEXT_SEC,
    );
    if reset_resp.clicked() {
        ch.detection_max_reset = true;
    }

    // Meter labels
    p.text(
        Pos2::new(meter_r.min.x + 4.0 * s, meter_r.max.y - 4.0 * s),
        egui::Align2::LEFT_BOTTOM,
        "-60 dB",
        FontId::new(8.0 * s, FontFamily::Proportional),
        TEXT_TER,
    );
    p.text(
        Pos2::new(meter_r.max.x - 4.0 * s, meter_r.max.y - 4.0 * s),
        egui::Align2::RIGHT_BOTTOM,
        "0 dB",
        FontId::new(8.0 * s, FontFamily::Proportional),
        TEXT_TER,
    );

    // Info text
    p.text(
        Pos2::new(cx, meter_r.max.y + 10.0 * s),
        egui::Align2::CENTER_TOP,
        "TKEO Energy Detection",
        FontId::new(9.0 * s, FontFamily::Proportional),
        TEXT_TER,
    );
}

// ─── Reduction Panel (RIGHT) — TKEO Threshold Knob ───────────────────────────
fn draw_red_panel(ui: &mut Ui, rect: Rect, params: &GuiParams, ch: &mut GuiChanges, s: f32) {
    let p = ui.painter_at(rect);
    acrylic_card(&p, rect, 8.0 * s);

    let cx = rect.center().x;
    let mut cy = rect.min.y + 12.0 * s;

    // Panel title
    p.text(
        Pos2::new(cx, cy),
        egui::Align2::CENTER_TOP,
        "Processing",
        FontId::new(12.5 * s, FontFamily::Proportional),
        TEXT_PRI,
    );
    cy += 22.0 * s;

    // TKEO Threshold knob (circular)
    let knob_size = 84.0 * s;
    let knob_rect = Rect::from_center_size(Pos2::new(cx, cy + knob_size * 0.55), Vec2::splat(knob_size));
    
    // Knob background ring
    p.circle_stroke(
        knob_rect.center(),
        knob_size * 0.45,
        Stroke::new(3.0 * s, CTRL_DEFAULT),
    );
    
    // Active arc based on value (-60..0 dB -> 270° sweep)
    let norm = ((params.threshold - -60.0) / 60.0).clamp(0.0, 1.0);
    let start_angle = 3.0 * PI / 2.0; // 270° (top)
    let end_angle = start_angle + norm * 1.5 * PI; // 270° sweep
    
    let arc_path: Vec<Pos2> = (0..=32)
        .map(|i| {
            let t = i as f32 / 32.0;
            let angle = start_angle + t * 1.5 * PI;
            let r = knob_size * 0.42;
            Pos2::new(
                knob_rect.center().x + r * angle.cos(),
                knob_rect.center().y + r * angle.sin(),
            )
        })
        .collect();
    
    for i in 0..arc_path.len() - 1 {
        p.line_segment(
            [arc_path[i], arc_path[i + 1]],
            Stroke::new(4.0 * s, ACCENT),
        );
    }

    // Knob center button (clickable)
    let center_r = Rect::from_center_size(knob_rect.center(), Vec2::splat(28.0 * s));
    let knob_resp = ui.allocate_rect(knob_rect, Sense::click_and_drag());
    
    // Center fill
    p.circle_filled(knob_rect.center(), 12.0 * s, ACRYLIC_CARD);
    p.circle_stroke(
        knob_rect.center(),
        12.0 * s,
        Stroke::new(1.5 * s, ACCENT_BORDER),
    );
    
    // Value text in center
    p.text(
        Pos2::new(knob_rect.center().x, knob_rect.center().y - 2.0 * s),
        egui::Align2::CENTER_CENTER,
        &format!("{:.0}", params.threshold),
        FontId::new(11.0 * s, FontFamily::Proportional),
        ACCENT_LIGHT,
    );
    p.text(
        Pos2::new(knob_rect.center().x, knob_rect.center().y + 8.0 * s),
        egui::Align2::CENTER_CENTER,
        "dB",
        FontId::new(8.0 * s, FontFamily::Proportional),
        TEXT_TER,
    );

    // Knob label above
    p.text(
        Pos2::new(knob_rect.center().x, knob_rect.min.y + 6.0 * s),
        egui::Align2::CENTER_TOP,
        "TKEO Threshold",
        FontId::new(10.0 * s, FontFamily::Proportional),
        TEXT_SEC,
    );

    // Numeric field below knob
    let num_w = 56.0 * s;
    let num_h = 22.0 * s;
    let num_rect = Rect::from_center_size(
        Pos2::new(cx, knob_rect.max.y + 18.0 * s),
        Vec2::new(num_w, num_h),
    );
    
    p.rect_filled(num_rect, 4.0 * s, CTRL_DEFAULT);
    p.rect_stroke(
        num_rect,
        4.0 * s,
        Stroke::new(1.0, STROKE_DEF),
        egui::StrokeKind::Outside,
    );
    p.text(
        num_rect.center(),
        egui::Align2::CENTER_CENTER,
        &format!("{:.1}", params.threshold),
        FontId::new(10.0 * s, FontFamily::Proportional),
        TEXT_PRI,
    );

    let num_resp = ui.allocate_rect(num_rect, Sense::click());
    if knob_resp.secondary_clicked() || num_resp.clicked() {
        gui.num_input = NumInput {
            open: true,
            label: "TKEO Sensitivity".to_string(),
            value_str: format!("{:.1}", params.threshold),
            target: NumTarget::Threshold,
            min: -60.0,
            max: 0.0,
        };
    }
    
    // Hover tooltip for numeric field
    ui.allocate_rect(num_rect, Sense::hover())
        .on_hover_text(
            "Set the Teager-Kaiser Energy detection threshold.\n\
             Controls amplification level above which energy spikes\n\
             are analyzed for sibilance classification."
        );

    // Reduction meter
    cy = num_rect.max.y + 24.0 * s;
    let meter_h = 48.0 * s;
    let meter_r = Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0 * s, cy),
        Vec2::new(rect.width() - 16.0 * s, meter_h),
    );
    
    p.rect_filled(meter_r, 4.0 * s, CTRL_DEFAULT);
    p.rect_stroke(
        meter_r,
        4.0 * s,
        Stroke::new(1.0, STROKE_DEF),
        egui::StrokeKind::Outside,
    );

    // Fill based on reduction (0..-40 dB, inverted)
    let fill_norm = ((params.reduction_db - 0.0) / -40.0).clamp(0.0, 1.0);
    let fill_w = meter_r.width() * fill_norm;
    let fill_r = Rect::from_min_size(meter_r.min, Vec2::new(fill_w, meter_r.height()));
    
    let fill_col = if params.reduction_db < -30.0 {
        RED
    } else if params.reduction_db < -15.0 {
        ORANGE
    } else {
        TEAL
    };
    p.rect_filled(fill_r, 4.0 * s, ga(fill_col, 180));

    // Peak marker
    let peak_norm = ((params.reduction_max_db - 0.0) / -40.0).clamp(0.0, 1.0);
    let peak_x = meter_r.min.x + peak_norm * meter_r.width();
    p.line_segment(
        [
            Pos2::new(peak_x, meter_r.min.y + 2.0 * s),
            Pos2::new(peak_x, meter_r.max.y - 2.0 * s),
        ],
        Stroke::new(2.0 * s, ACCENT_LIGHT),
    );

    // Reset peak
    let reset_r = Rect::from_min_size(
        Pos2::new(meter_r.max.x - 24.0 * s, meter_r.min.y + 2.0 * s),
        Vec2::new(20.0 * s, 16.0 * s),
    );
    let reset_resp = ui.allocate_rect(reset_r, Sense::click());
    if reset_resp.hovered() {
        p.rect_filled(reset_r, 3.0 * s, CTRL_HOVER);
    }
    p.text(
        reset_r.center(),
        egui::Align2::CENTER_CENTER,
        "↺",
        FontId::new(11.0 * s, FontFamily::Monospace),
        TEXT_SEC,
    );
    if reset_resp.clicked() {
        ch.reduction_max_reset = true;
    }

    p.text(
        Pos2::new(meter_r.min.x + 4.0 * s, meter_r.max.y - 4.0 * s),
        egui::Align2::LEFT_BOTTOM,
        "0 dB",
        FontId::new(8.0 * s, FontFamily::Proportional),
        TEXT_TER,
    );
    p.text(
        Pos2::new(meter_r.max.x - 4.0 * s, meter_r.max.y - 4.0 * s),
        egui::Align2::RIGHT_BOTTOM,
        "-40 dB",
        FontId::new(8.0 * s, FontFamily::Proportional),
        TEXT_TER,
    );

    p.text(
        Pos2::new(cx, meter_r.max.y + 10.0 * s),
        egui::Align2::CENTER_TOP,
        "Gain Reduction",
        FontId::new(9.0 * s, FontFamily::Proportional),
        TEXT_TER,
    );
}

// ─── Controls Panel (CENTER TOP) — Mode Toggle & Parameters ──────────────────
fn draw_controls(
    ui: &mut Ui,
    rect: Rect,
    params: &GuiParams,
    ch: &mut GuiChanges,
    gui: &mut NebulaGui,
    s: f32,
) {
    let p = ui.painter_at(rect);
    acrylic_card(&p, rect, 8.0 * s);

    let cx = rect.center().x;
    let mut cy = rect.min.y + 12.0 * s;

    // Panel title
    p.text(
        Pos2::new(cx, cy),
        egui::Align2::CENTER_TOP,
        "Processing Mode",
        FontId::new(12.5 * s, FontFamily::Proportional),
        TEXT_PRI,
    );
    cy += 24.0 * s;

    // Relative/Absolute mode toggle — UPDATED FOR SUBSPACE PROJECTION
    let toggle_w = rect.width() - 16.0 * s;
    let toggle_h = 28.0 * s;
    let toggle_r = Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0 * s, cy),
        Vec2::new(toggle_w, toggle_h),
    );

    // Toggle background
    p.rect_filled(toggle_r, 4.0 * s, CTRL_DEFAULT);
    p.rect_stroke(
        toggle_r,
        4.0 * s,
        Stroke::new(1.0, STROKE_DEF),
        egui::StrokeKind::Outside,
    );

    // Split into two halves
    let half_w = toggle_w / 2.0 - 2.0 * s;
    let abs_r = Rect::from_min_size(toggle_r.min, Vec2::new(half_w, toggle_h));
    let rel_r = Rect::from_min_size(
        Pos2::new(toggle_r.min.x + half_w + 4.0 * s, toggle_r.min.y),
        Vec2::new(half_w, toggle_h),
    );

    // Absolute mode button
    let abs_resp = ui.allocate_rect(abs_r, Sense::click());
    let abs_active = !params.mode_relative;
    let abs_bg = if abs_active { ACCENT } else { CTRL_DEFAULT };
    let abs_fg = if abs_active { Color32::WHITE } else { TEXT_SEC };
    
    p.rect_filled(abs_r, 4.0 * s, abs_bg);
    if abs_active {
        p.rect_stroke(abs_r, 4.0 * s, Stroke::new(1.0, ACCENT_BORDER), egui::StrokeKind::Outside);
    }
    p.text(
        abs_r.center(),
        egui::Align2::CENTER_CENTER,
        "Absolute",
        FontId::new(11.0 * s, FontFamily::Proportional),
        abs_fg,
    );

    // Relative mode button
    let rel_resp = ui.allocate_rect(rel_r, Sense::click());
    let rel_active = params.mode_relative;
    let rel_bg = if rel_active { ACCENT } else { CTRL_DEFAULT };
    let rel_fg = if rel_active { Color32::WHITE } else { TEXT_SEC };
    
    p.rect_filled(rel_r, 4.0 * s, rel_bg);
    if rel_active {
        p.rect_stroke(rel_r, 4.0 * s, Stroke::new(1.0, ACCENT_BORDER), egui::StrokeKind::Outside);
    }
    p.text(
        rel_r.center(),
        egui::Align2::CENTER_CENTER,
        "Relative",
        FontId::new(11.0 * s, FontFamily::Proportional),
        rel_fg,
    );

    // Mode tooltips — UPDATED WITH SUBSPACE PROJECTION EXPLANATIONS
    ui.allocate_rect(abs_r, Sense::hover()).on_hover_text(
        "3-Vector Subspace Mode (Fixed Dimensions)\n\
         \n\
         • Voiced Axis: Periodic, harmonic 'vowel-like' energy\n\
         • Unvoiced Axis: Aperiodic, TKEO-detected sibilance\n\
         • Residual Axis: Mathematical remainder\n\
         \n\
         Uses fixed 3-dimensional orthogonal separation.\n\
         Best for consistent vocal timbres."
    );
    
    ui.allocate_rect(rel_r, Sense::hover()).on_hover_text(
        "Multi-Vector Adaptive Mode (N-dimensional)\n\
         \n\
         • Higher-Order Correlation: Analyzes cross-frequency\n\
           relationships (e.g., 12kHz air vs. 300Hz chest resonance)\n\
         • Contextual Intelligence: Dynamically expands vector\n\
           space based on signal complexity\n\
         • Adaptive Subspace: Allows dimensions to expand/contract\n\
           in real-time for transparent processing\n\
         \n\
         Ideal for breathy vocals, dynamic performers, or singers\n\
         who shift between whispers and belts."
    );

    if abs_resp.clicked() {
        ch.mode_relative = Some(false);
    }
    if rel_resp.clicked() {
        ch.mode_relative = Some(true);
    }

    // Mode indicator badge
    cy += toggle_h + 8.0 * s;
    let badge_text = if params.mode_relative {
        "Relative: Adaptive N-D Subspace"
    } else {
        "Absolute: Fixed 3-Vector Subspace"
    };
    p.text(
        Pos2::new(cx, cy),
        egui::Align2::CENTER_TOP,
        badge_text,
        FontId::new(9.0 * s, FontFamily::Proportional),
        if params.mode_relative { ACCENT_LIGHT } else { TEXT_TER },
    );

    // Additional controls row
    cy += 24.0 * s;
    
    // Filter type toggle (Peak/Notch)
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Filter Type:")
            .color(TEXT_SEC)
            .font(FontId::new(10.0 * s, FontFamily::Proportional)));
        
        let peak_resp = ui.selectable_label(
            params.use_peak_filter,
            egui::RichText::new("Peak").color(if params.use_peak_filter { ACCENT } else { TEXT_SEC })
        );
        let notch_resp = ui.selectable_label(
            !params.use_peak_filter,
            egui::RichText::new("Notch").color(if !params.use_peak_filter { ACCENT } else { TEXT_SEC })
        );
        
        if peak_resp.clicked() { ch.use_peak_filter = Some(true); }
        if notch_resp.clicked() { ch.use_peak_filter = Some(false); }
    });

    // Lookahead toggle
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new("Lookahead:")
            .color(TEXT_SEC)
            .font(FontId::new(10.0 * s, FontFamily::Proportional)));
        
        let enabled_resp = ui.selectable_label(
            params.lookahead_enabled,
            egui::RichText::new("On").color(if params.lookahead_enabled { ACCENT } else { TEXT_SEC })
        );
        let disabled_resp = ui.selectable_label(
            !params.lookahead_enabled,
            egui::RichText::new("Off").color(if !params.lookahead_enabled { ACCENT } else { TEXT_SEC })
        );
        
        if enabled_resp.clicked() { ch.lookahead_enabled = Some(true); }
        if disabled_resp.clicked() { ch.lookahead_enabled = Some(false); }
    });

    // Lookahead ms field (only if enabled)
    if params.lookahead_enabled {
        let la_r = Rect::from_min_size(
            Pos2::new(rect.min.x + 8.0 * s, cy + 4.0 * s),
            Vec2::new(80.0 * s, 20.0 * s),
        );
        p.rect_filled(la_r, 4.0 * s, CTRL_DEFAULT);
        p.rect_stroke(la_r, 4.0 * s, Stroke::new(1.0, STROKE_DEF), egui::StrokeKind::Outside);
        p.text(
            la_r.center(),
            egui::Align2::CENTER_CENTER,
            &format!("{:.1} ms", params.lookahead_ms),
            FontId::new(10.0 * s, FontFamily::Proportional),
            TEXT_PRI,
        );
        let la_resp = ui.allocate_rect(la_r, Sense::click());
        if la_resp.clicked() {
            gui.num_input = NumInput {
                open: true,
                label: "Lookahead".to_string(),
                value_str: format!("{:.1}", params.lookahead_ms),
                target: NumTarget::Lookahead,
                min: 0.0,
                max: 20.0,
            };
        }
    }
}

// ─── Spectrum Analyzer (CENTER BOTTOM) ───────────────────────────────────────
fn draw_spectrum(
    ui: &mut Ui,
    rect: Rect,
    gui: &mut NebulaGui,
    params: &GuiParams,
    ch: &mut GuiChanges,
    s: f32,
) {
    let p = ui.painter_at(rect);
    acrylic_card(&p, rect, 8.0 * s);

    let inner = Rect::from_min_size(
        Pos2::new(rect.min.x + 8.0 * s, rect.min.y + 24.0 * s),
        Vec2::new(rect.width() - 16.0 * s, rect.height() - 32.0 * s),
    );

    // Grid background
    p.rect_filled(inner, 4.0 * s, CTRL_DEFAULT);
    p.rect_stroke(inner, 4.0 * s, Stroke::new(1.0, STROKE_DEF), egui::StrokeKind::Outside);

    // Frequency grid lines (log scale)
    let freqs = [100.0, 500.0, 1_000.0, 2_000.0, 5_000.0, 10_000.0, 20_000.0];
    for &f in &freqs {
        let x = inner.min.x + freq_to_x(f, inner.width());
        p.line_segment(
            [Pos2::new(x, inner.min.y), Pos2::new(x, inner.max.y)],
            Stroke::new(0.5 * s, ga(DIVIDER, 80)),
        );
        if f >= params.min_freq as f32 && f <= params.max_freq as f32 {
            p.text(
                Pos2::new(x + 2.0 * s, inner.max.y - 4.0 * s),
                egui::Align2::LEFT_BOTTOM,
                &format!("{:.0}", f),
                FontId::new(7.0 * s, FontFamily::Proportional),
                TEXT_TER,
            );
        }
    }

    // dB grid lines
    for db in [-90.0, -60.0, -30.0, 0.0] {
        let y = db_to_y(db, inner.height(), inner.min.y);
        p.line_segment(
            [Pos2::new(inner.min.x, y), Pos2::new(inner.max.x, y)],
            Stroke::new(0.5 * s, ga(DIVIDER, 80)),
        );
        p.text(
            Pos2::new(inner.min.x + 4.0 * s, y - 2.0 * s),
            egui::Align2::LEFT_CENTER,
            &format!("{:.0}", db),
            FontId::new(7.0 * s, FontFamily::Proportional),
            TEXT_TER,
        );
    }

    // Spectrum data
    let data = gui.spectrum.lock();
    let mags = &data.mags;
    let ph = inner.height();

    if !mags.is_empty() {
        // Smooth the display
        for (i, &mag) in mags.iter().enumerate().take(1025) {
            let smooth = 0.85;
            gui.smooth_mags[i] = gui.smooth_mags[i] * smooth + mag * (1.0 - smooth);
        }

        // Draw spectrum line
        let mut pts = Vec::with_capacity(1025);
        for i in 0..1025 {
            let x = inner.min.x + (i as f32 / 1024.0) * inner.width();
            let y = db_to_y(gui.smooth_mags[i].clamp(-120.0, 0.0), ph, inner.min.y);
            pts.push(Pos2::new(x, y));
        }

        // Fill under curve with gradient
        let mut fill_col = ga(ACCENT, 20);
        for i in 0..pts.len() - 1 {
            let mag = gui.smooth_mags[i].clamp(-120.0, 0.0);
            let alpha = ((mag + 120.0) / 120.0 * 40.0) as u8;
            fill_col = ga(ACCENT, alpha);
            p.line_segment([pts[i], pts[i + 1]], Stroke::new(3.0 * s, ga(ACCENT, 14)));
        }
        for i in 0..pts.len() - 1 {
            p.line_segment(
                [pts[i], pts[i + 1]],
                Stroke::new(1.2 * s, ga(ACCENT_LIGHT, 200)),
            );
        }
    }

    // Band overlay (min/max frequency selection)
    let min_x = inner.min.x + freq_to_x(params.min_freq as f32, inner.width());
    let max_x = inner.min.x + freq_to_x(params.max_freq as f32, inner.width());
    
    if max_x > min_x {
        let br = Rect::from_min_max(
            Pos2::new(min_x, inner.min.y),
            Pos2::new(max_x, inner.min.y + ph),
        );
        p.rect_filled(br, 0.0, ga(ORANGE, 12));
        
        // Min freq marker
        p.line_segment(
            [Pos2::new(min_x, inner.min.y), Pos2::new(min_x, inner.max.y)],
            Stroke::new(1.2 * s, ga(TEAL, 200)),
        );
        p.line_segment(
            [Pos2::new(min_x, inner.min.y), Pos2::new(min_x, inner.max.y)],
            Stroke::new(4.0 * s, ga(TEAL, 28)),
        );
        
        // Max freq marker
        p.line_segment(
            [Pos2::new(max_x, inner.min.y), Pos2::new(max_x, inner.max.y)],
            Stroke::new(1.2 * s, ga(ORANGE, 200)),
        );
        p.line_segment(
            [Pos2::new(max_x, inner.min.y), Pos2::new(max_x, inner.max.y)],
            Stroke::new(4.0 * s, ga(ORANGE, 28)),
        );
    }

    // Draggable nodes
    let node_y = inner.min.y + ph * 0.5;
    let hit_sz = 22.0 * s;
    
    let min_hit = Rect::from_center_size(Pos2::new(min_x, node_y), Vec2::splat(hit_sz));
    let mr = ui.allocate_rect(min_hit, Sense::drag());
    if mr.dragged() {
        let nx = (min_x + mr.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        ch.min_freq = Some((x_to_freq(nx, inner.width()) as f64).clamp(1.0, params.max_freq - 1.0));
    }
    
    let max_hit = Rect::from_center_size(Pos2::new(max_x, node_y), Vec2::splat(hit_sz));
    let xr = ui.allocate_rect(max_hit, Sense::drag());
    if xr.dragged() {
        let nx = (max_x + xr.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        ch.max_freq = Some((x_to_freq(nx, inner.width()) as f64).clamp(params.min_freq + 1.0, 24000.0));
    }
    
    freq_node(&p, Pos2::new(min_x, node_y), TEAL, "Min", s);
    freq_node(&p, Pos2::new(max_x, node_y), ORANGE, "Max", s);

    // Labels
    p.text(
        Pos2::new(inner.min.x + 6.0 * s, inner.min.y + 7.0 * s),
        egui::Align2::LEFT_CENTER,
        "TKEO Detection Band",
        FontId::new(12.5 * s, FontFamily::Proportional),
        TEXT_TER,
    );
    
    ui.ctx().request_repaint();
}

fn freq_to_x(freq: f32, width: f32) -> f32 {
    let min_log = 100.0f32.log10();
    let max_log = 24000.0f32.log10();
    let f_log = freq.log10();
    ((f_log - min_log) / (max_log - min_log)) * width
}

fn x_to_freq(x: f32, width: f32) -> f32 {
    let min_log = 100.0f32.log10();
    let max_log = 24000.0f32.log10();
    let t = (x / width).clamp(0.0, 1.0);
    10.0f32.powf(min_log + t * (max_log - min_log))
}

fn db_to_y(db: f32, height: f32, min_y: f32) -> f32 {
    // -120 dB at bottom, 0 dB at top
    min_y + height - ((db + 120.0) / 120.0 * height)
}

fn freq_node(pa: &egui::Painter, c: Pos2, col: Color32, lbl: &str, s: f32) {
    pa.circle_filled(c, 8.0 * s, ga(col, 20));
    pa.circle_filled(c, 5.5 * s, ACRYLIC_CARD);
    pa.circle_stroke(c, 5.5 * s, Stroke::new(1.2 * s, col));
    pa.text(
        Pos2::new(c.x, c.y - 13.0 * s),
        egui::Align2::CENTER_CENTER,
        lbl,
        FontId::new(9.0 * s, FontFamily::Proportional),
        col,
    );
}

// ─── ContentDialog — Numeric Input ───────────────────────────────────────────
fn draw_content_dialog_num(ctx: &Context, gui: &mut NebulaGui, ch: &mut GuiChanges, s: f32) {
    let sc = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(240.0 * s, 120.0 * s));
    let fr = Rect::from_center_size(
        Pos2::new(pop.center().x, pop.center().y - 6.0 * s),
        Vec2::new(200.0 * s, 26.0 * s),
    );
    let ok = Rect::from_center_size(
        Pos2::new(pop.center().x - 50.0 * s, pop.max.y - 16.0 * s),
        Vec2::new(80.0 * s, 22.0 * s),
    );
    let cx_ = Rect::from_center_size(
        Pos2::new(pop.center().x + 50.0 * s, pop.max.y - 16.0 * s),
        Vec2::new(80.0 * s, 22.0 * s),
    );
    let lbl = gui.num_input.label.clone();
    
    egui::Area::new(egui::Id::new("neb_num"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            {
                let p = ui.painter();
                // Scrim
                p.rect_filled(sc, 0.0, Color32::from_black_alpha(140));
                // ContentDialog card
                p.rect_filled(
                    Rect::from_center_size(pop.center() + Vec2::new(0.0, 3.0 * s), pop.size()),
                    12.0 * s,
                    Color32::from_black_alpha(60),
                );
                acrylic_card(p, pop, 12.0 * s);
                p.text(
                    Pos2::new(pop.center().x, pop.min.y + 18.0 * s),
                    egui::Align2::CENTER_CENTER,
                    format!("Set {}", lbl),
                    FontId::new(11.5 * s, FontFamily::Proportional),
                    TEXT_PRI,
                );
                // TextBox
                p.rect_filled(fr, 4.0 * s, CTRL_DEFAULT);
                p.rect_stroke(fr, 4.0 * s, Stroke::new(1.0, STROKE_DEF), egui::StrokeKind::Outside);
                p.line_segment(
                    [Pos2::new(fr.min.x + 4.0 * s, fr.max.y), Pos2::new(fr.max.x - 4.0 * s, fr.max.y)],
                    Stroke::new(2.0, ACCENT),
                );
                // Buttons
                p.rect_filled(ok, 4.0 * s, ACCENT);
                p.rect_stroke(ok, 4.0 * s, Stroke::new(1.0, ACCENT_BORDER), egui::StrokeKind::Outside);
                p.text(ok.center(), egui::Align2::CENTER_CENTER, "OK", 
                    FontId::new(11.5 * s, FontFamily::Proportional), Color32::WHITE);
                p.rect_filled(cx_, 4.0 * s, CTRL_DEFAULT);
                p.rect_stroke(cx_, 4.0 * s, Stroke::new(1.0, STROKE_DEF), egui::StrokeKind::Outside);
                p.text(cx_.center(), egui::Align2::CENTER_CENTER, "Cancel",
                    FontId::new(11.5 * s, FontFamily::Proportional), TEXT_SEC);
            }
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(fr), |ui| {
                let te = egui::TextEdit::singleline(&mut gui.num_input.value_str)
                    .font(FontId::new(11.5 * s, FontFamily::Proportional))
                    .text_color(TEXT_PRI)
                    .frame(false)
                    .desired_width(fr.width());
                let r = ui.add(te);
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    apply_num(gui, ch);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.num_input.open = false;
            }
            if ui.allocate_rect(ok, Sense::click()).clicked() {
                apply_num(gui, ch);
            }
            if ui.allocate_rect(cx_, Sense::click()).clicked() {
                gui.num_input.open = false;
            }
        });
}

fn apply_num(gui: &mut NebulaGui, ch: &mut GuiChanges) {
    if let Ok(v) = gui.num_input.value_str.trim().parse::<f64>() {
        let v = v.clamp(gui.num_input.min, gui.num_input.max);
        apply_ch(&gui.num_input.target, v, ch);
    }
    gui.num_input.open = false;
}

// ─── Parameter Application Helper ───────────────────────────────────────────
// NOTE: As of v2.6.0, the `threshold` parameter no longer controls
// compression gain reduction. It now sets the Teager-Kaiser Energy Operator
// sensitivity threshold for sibilance detection in the Orthogonal Subspace
// Projection pipeline.
//
// Interpretation:
// • Value represents dB level of TKEO output that triggers analysis
// • Lower values = lower energy spikes trigger detection = more aggressive
// • Higher values = only high-energy transients trigger = more transparent
// • The actual gain reduction amount is controlled by `max_reduction`
// ────────────────────────────────────────────────────────────────────────────
fn apply_ch(target: &NumTarget, value: f64, ch: &mut GuiChanges) {
    match target {
        NumTarget::Threshold => ch.threshold = Some(value),
        NumTarget::MaxReduction => ch.max_reduction = Some(value),
        NumTarget::MinFreq => ch.min_freq = Some(value),
        NumTarget::MaxFreq => ch.max_freq = Some(value),
        NumTarget::Lookahead => ch.lookahead_ms = Some(value),
        NumTarget::StereoLink => ch.stereo_link = Some(value),
        NumTarget::InputLevel => ch.input_level = Some(value),
        NumTarget::InputPan => ch.input_pan = Some(value),
        NumTarget::OutputLevel => ch.output_level = Some(value),
        NumTarget::OutputPan => ch.output_pan = Some(value),
        NumTarget::CutWidth => ch.cut_width = Some(value),
        NumTarget::CutDepth => ch.cut_depth = Some(value),
        NumTarget::CutSlope => ch.cut_slope = Some(value),
        NumTarget::Mix => ch.mix = Some(value),
        _ => {}
    }
}

// ─── ContentDialog — Preset Save ─────────────────────────────────────────────
fn draw_content_dialog_preset(
    ctx: &Context,
    gui: &mut NebulaGui,
    p: &GuiParams,
    _ch: &mut GuiChanges,
    s: f32,
) {
    let sc = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(260.0 * s, 120.0 * s));
    let fr = Rect::from_center_size(
        Pos2::new(pop.center().x, pop.center().y - 6.0 * s),
        Vec2::new(220.0 * s, 26.0 * s),
    );
    let ok = Rect::from_center_size(
        Pos2::new(pop.center().x - 54.0 * s, pop.max.y - 16.0 * s),
        Vec2::new(84.0 * s, 22.0 * s),
    );
    let cx_ = Rect::from_center_size(
        Pos2::new(pop.center().x + 54.0 * s, pop.max.y - 16.0 * s),
        Vec2::new(84.0 * s, 22.0 * s),
    );
    
    egui::Area::new(egui::Id::new("neb_prsave"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            {
                let pa = ui.painter();
                pa.rect_filled(sc, 0.0, Color32::from_black_alpha(140));
                pa.rect_filled(
                    Rect::from_center_size(pop.center() + Vec2::new(0.0, 3.0 * s), pop.size()),
                    12.0 * s,
                    Color32::from_black_alpha(60),
                );
                acrylic_card(pa, pop, 12.0 * s);
                pa.text(
                    Pos2::new(pop.center().x, pop.min.y + 18.0 * s),
                    egui::Align2::CENTER_CENTER,
                    "Save Preset",
                    FontId::new(11.5 * s, FontFamily::Proportional),
                    TEXT_PRI,
                );
                pa.rect_filled(fr, 4.0 * s, CTRL_DEFAULT);
                pa.rect_stroke(fr, 4.0 * s, Stroke::new(1.0, STROKE_DEF), egui::StrokeKind::Outside);
                pa.line_segment(
                    [Pos2::new(fr.min.x + 4.0 * s, fr.max.y), Pos2::new(fr.max.x - 4.0 * s, fr.max.y)],
                    Stroke::new(2.0, ACCENT),
                );
                pa.rect_filled(ok, 4.0 * s, ACCENT);
                pa.rect_stroke(ok, 4.0 * s, Stroke::new(1.0, ACCENT_BORDER), egui::StrokeKind::Outside);
                pa.text(ok.center(), egui::Align2::CENTER_CENTER, "Save",
                    FontId::new(11.5 * s, FontFamily::Proportional), Color32::WHITE);
                pa.rect_filled(cx_, 4.0 * s, CTRL_DEFAULT);
                pa.rect_stroke(cx_, 4.0 * s, Stroke::new(1.0, STROKE_DEF), egui::StrokeKind::Outside);
                pa.text(cx_.center(), egui::Align2::CENTER_CENTER, "Cancel",
                    FontId::new(11.5 * s, FontFamily::Proportional), TEXT_SEC);
            }
            ui.allocate_new_ui(egui::UiBuilder::new().max_rect(fr), |ui| {
                let te = egui::TextEdit::singleline(&mut gui.preset_name_buf)
                    .font(FontId::new(11.5 * s, FontFamily::Proportional))
                    .text_color(TEXT_PRI)
                    .frame(false)
                    .desired_width(fr.width())
                    .hint_text("Preset name…");
                let r = ui.add(te);
                r.request_focus();
                if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    do_save(gui, p);
                }
            });
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.preset_save_popup = false;
            }
            if ui.allocate_rect(ok, Sense::click()).clicked() {
                do_save(gui, p);
            }
            if ui.allocate_rect(cx_, Sense::click()).clicked() {
                gui.preset_save_popup = false;
            }
        });
}

fn do_save(gui: &mut NebulaGui, p: &GuiParams) {
    let name = gui.preset_name_buf.trim().to_string();
    if name.is_empty() {
        return;
    }
    let snap = ParamSnapshot::from_params(p);
    if let Some(idx) = gui.presets.iter().position(|(n, _)| n == &name) {
        gui.presets[idx].1 = snap;
        gui.selected_preset = idx;
    } else {
        gui.presets.push((name, snap));
        gui.selected_preset = gui.presets.len() - 1;
    }
    gui.preset_save_popup = false;
}

// ─── ContentDialog — MIDI Learn ──────────────────────────────────────────────
fn draw_content_dialog_midi(ctx: &Context, gui: &mut NebulaGui, s: f32) {
    let sc = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(290.0 * s, 320.0 * s));
    
    egui::Area::new(egui::Id::new("neb_midi"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            {
                let pa = ui.painter();
                pa.rect_filled(sc, 0.0, Color32::from_black_alpha(140));
                pa.rect_filled(
                    Rect::from_center_size(pop.center() + Vec2::new(0.0, 4.0 * s), pop.size()),
                    12.0 * s,
                    Color32::from_black_alpha(60),
                );
                acrylic_card(pa, pop, 12.0 * s);
                pa.text(
                    Pos2::new(pop.center().x, pop.min.y + 18.0 * s),
                    egui::Align2::CENTER_CENTER,
                    "MIDI Learn",
                    FontId::new(12.5 * s, FontFamily::Proportional),
                    TEXT_PRI,
                );
                pa.text(
                    Pos2::new(pop.center().x, pop.min.y + 32.0 * s),
                    egui::Align2::CENTER_CENTER,
                    "Select a parameter, then move a CC knob",
                    FontId::new(11.5 * s, FontFamily::Proportional),
                    TEXT_SEC,
                );
            }

            let learning = gui
                .midi_learn
                .learning_target
                .load(std::sync::atomic::Ordering::Relaxed);
            let mappings = gui.midi_learn.mappings.lock().clone();
            
            for (idx, &name) in MIDI_PARAM_NAMES.iter().enumerate().take(MIDI_PARAM_COUNT) {
                let cc_s: String = mappings
                    .iter()
                    .find(|(_, &v)| v == idx as u8)
                    .map(|(&cc, _)| format!("CC{}", cc))
                    .unwrap_or_else(|| "—".to_string());
                let ih = 21.0 * s;
                let rr = Rect::from_min_size(
                    Pos2::new(pop.min.x + 10.0 * s, pop.min.y + 46.0 * s + idx as f32 * ih),
                    Vec2::new(pop.width() - 20.0 * s, ih - 2.0 * s),
                );
                let resp = ui.allocate_rect(rr, Sense::click());
                let isl = learning == idx as i32;
                let hov = resp.hovered();
                {
                    let pa = ui.painter_at(Rect::EVERYTHING);
                    let (bg, border) = if isl {
                        (ACCENT, ACCENT_BORDER)
                    } else if hov {
                        (CTRL_HOVER, STROKE_DEF)
                    } else {
                        (CTRL_DEFAULT, STROKE_DEF)
                    };
                    pa.rect_filled(rr, 4.0 * s, bg);
                    pa.rect_stroke(rr, 4.0 * s, Stroke::new(1.0, border), egui::StrokeKind::Outside);
                    pa.text(
                        Pos2::new(rr.min.x + 8.0 * s, rr.center().y),
                        egui::Align2::LEFT_CENTER,
                        name,
                        FontId::new(9.0 * s, FontFamily::Proportional),
                        if isl { Color32::WHITE } else { TEXT_PRI },
                    );
                    pa.text(
                        Pos2::new(rr.max.x - 8.0 * s, rr.center().y),
                        egui::Align2::RIGHT_CENTER,
                        &cc_s,
                        FontId::new(9.0 * s, FontFamily::Proportional),
                        if isl { Color32::WHITE } else { TEXT_TER },
                    );
                }
                if resp.clicked() {
                    let t = if isl { -1 } else { idx as i32 };
                    gui.midi_learn
                        .learning_target
                        .store(t, std::sync::atomic::Ordering::Release);
                }
            }

            let clr = Rect::from_center_size(
                Pos2::new(pop.center().x - 56.0 * s, pop.max.y - 16.0 * s),
                Vec2::new(88.0 * s, 22.0 * s),
            );
            let cls = Rect::from_center_size(
                Pos2::new(pop.center().x + 56.0 * s, pop.max.y - 16.0 * s),
                Vec2::new(88.0 * s, 22.0 * s),
            );
            {
                let pa = ui.painter_at(Rect::EVERYTHING);
                pa.rect_filled(clr, 4.0 * s, ga(RED, 30));
                pa.rect_stroke(clr, 4.0 * s, Stroke::new(1.0, ga(RED, 100)), egui::StrokeKind::Outside);
                pa.text(clr.center(), egui::Align2::CENTER_CENTER, "Clear All",
                    FontId::new(9.0 * s, FontFamily::Proportional), RED);
                pa.rect_filled(cls, 4.0 * s, ACCENT);
                pa.rect_stroke(cls, 4.0 * s, Stroke::new(1.0, ACCENT_BORDER), egui::StrokeKind::Outside);
                pa.text(cls.center(), egui::Align2::CENTER_CENTER, "Close",
                    FontId::new(9.0 * s, FontFamily::Proportional), Color32::WHITE);
            }
            if ui.allocate_rect(clr, Sense::click()).clicked() {
                gui.midi_learn.mappings.lock().clear();
                gui.midi_learn
                    .learning_target
                    .store(-1, std::sync::atomic::Ordering::Release);
            }
            if ui.allocate_rect(cls, Sense::click()).clicked()
                || ui.input(|i| i.key_pressed(egui::Key::Escape))
            {
                gui.midi_learn
                    .learning_target
                    .store(-1, std::sync::atomic::Ordering::Release);
                gui.midi_popup = false;
            }
        });
}

// ─── Context Menu — MIDI ──────────────────────────────────────────────────────
fn draw_context_menu_midi(ctx: &Context, gui: &mut NebulaGui, s: f32) {
    let menu_w = 172.0 * s;
    let ih = 24.0 * s;
    let items = [
        ("MIDI On/Off", 0usize),
        ("Clean Up...", 1),
        ("Roll Back", 2),
        ("Save", 3),
        ("Close", 4),
    ];
    let menu_h = items.len() as f32 * ih + 8.0 * s;
    let anchor = gui.midi_context_anchor;
    let menu_rect = Rect::from_min_size(anchor, Vec2::new(menu_w, menu_h));
    let screen = ctx.screen_rect();
    
    egui::Area::new(egui::Id::new("neb_midi_ctx_bg"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            if ui.allocate_rect(screen, Sense::click()).clicked() {
                gui.midi_context_menu = false;
            }
        });
    
    egui::Area::new(egui::Id::new("neb_midi_ctx"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            {
                let p = ui.painter();
                p.rect_filled(
                    Rect::from_min_size(anchor + Vec2::new(2.0, 3.0), Vec2::new(menu_w, menu_h)),
                    8.0 * s,
                    Color32::from_black_alpha(80),
                );
                acrylic_card(p, menu_rect, 8.0 * s);
            }
            for (i, (label, idx)) in items.iter().enumerate() {
                let item_rect = Rect::from_min_size(
                    Pos2::new(anchor.x + 4.0 * s, anchor.y + 4.0 * s + i as f32 * ih),
                    Vec2::new(menu_w - 8.0 * s, ih - 2.0 * s),
                );
                let resp = ui.allocate_rect(item_rect, Sense::click());
                let hov = resp.hovered();
                {
                    let p = ui.painter();
                    if hov {
                        p.rect_filled(item_rect, 4.0 * s, CTRL_HOVER);
                    }
                    p.text(
                        Pos2::new(item_rect.min.x + 12.0 * s, item_rect.center().y),
                        egui::Align2::LEFT_CENTER,
                        *label,
                        FontId::new(11.5 * s, FontFamily::Proportional),
                        if hov { TEXT_PRI } else { TEXT_SEC },
                    );
                }
                if resp.clicked() {
                    match idx {
                        0 => {
                            let cur = gui
                                .midi_learn
                                .midi_enabled
                                .load(std::sync::atomic::Ordering::Relaxed);
                            gui.midi_learn
                                .midi_enabled
                                .store(!cur, std::sync::atomic::Ordering::Release);
                        }
                        1 => {
                            gui.midi_cleanup_menu = true;
                            gui.midi_cleanup_anchor =
                                Pos2::new(item_rect.max.x + 2.0 * s, item_rect.min.y);
                        }
                        2 => {
                            let saved = gui.midi_learn.saved_mappings.lock().clone();
                            *gui.midi_learn.mappings.lock() = saved;
                        }
                        3 => {
                            let cur = gui.midi_learn.mappings.lock().clone();
                            *gui.midi_learn.saved_mappings.lock() = cur;
                        }
                        4 => {
                            gui.midi_context_menu = false;
                        }
                        _ => {}
                    }
                    if *idx != 1 {
                        gui.midi_context_menu = false;
                    }
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.midi_context_menu = false;
            }
        });
    
    if gui.midi_cleanup_menu {
        draw_midi_cleanup_menu(ctx, gui, s);
    }
}

fn draw_midi_cleanup_menu(ctx: &Context, gui: &mut NebulaGui, s: f32) {
    let mappings = gui.midi_learn.mappings.lock().clone();
    let mut sorted: Vec<(u8, u8)> = mappings.iter().map(|(&cc, &p)| (cc, p)).collect();
    sorted.sort_by_key(|&(cc, _)| cc);
    
    let sub_w = 210.0 * s;
    let ih = 24.0 * s;
    let sub_h = (sorted.len() + 2) as f32 * ih + 8.0 * s;
    let anchor = gui.midi_cleanup_anchor;
    let sub_rect = Rect::from_min_size(anchor, Vec2::new(sub_w, sub_h));
    
    egui::Area::new(egui::Id::new("neb_midi_cleanup"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            {
                let p = ui.painter();
                p.rect_filled(
                    Rect::from_min_size(anchor + Vec2::new(2.0, 3.0), Vec2::new(sub_w, sub_h)),
                    8.0 * s,
                    Color32::from_black_alpha(80),
                );
                acrylic_card(p, sub_rect, 8.0 * s);
            }
            if sorted.is_empty() {
                let er = Rect::from_min_size(
                    Pos2::new(anchor.x + 4.0 * s, anchor.y + 4.0 * s),
                    Vec2::new(sub_w - 8.0 * s, ih),
                );
                ui.painter().text(
                    er.center(),
                    egui::Align2::CENTER_CENTER,
                    "No mappings",
                    FontId::new(12.5 * s, FontFamily::Proportional),
                    TEXT_SEC,
                );
            }
            for (i, (cc, pidx)) in sorted.iter().enumerate() {
                let pname = MIDI_PARAM_NAMES.get(*pidx as usize).copied().unwrap_or("?");
                let ir = Rect::from_min_size(
                    Pos2::new(anchor.x + 4.0 * s, anchor.y + 4.0 * s + i as f32 * ih),
                    Vec2::new(sub_w - 8.0 * s, ih - 2.0 * s),
                );
                let resp = ui.allocate_rect(ir, Sense::click());
                let hov = resp.hovered();
                {
                    let p = ui.painter();
                    if hov {
                        p.rect_filled(ir, 4.0 * s, ga(RED, 18));
                    }
                    p.text(
                        Pos2::new(ir.min.x + 12.0 * s, ir.center().y),
                        egui::Align2::LEFT_CENTER,
                        format!("CC{} → {}", cc, pname),
                        FontId::new(12.5 * s, FontFamily::Proportional),
                        if hov { RED } else { TEXT_SEC },
                    );
                }
                if resp.clicked() {
                    gui.midi_learn.mappings.lock().remove(cc);
                }
            }
            let clear_y = anchor.y + 4.0 * s + (sorted.len() + 1) as f32 * ih;
            let cr = Rect::from_min_size(
                Pos2::new(anchor.x + 4.0 * s, clear_y),
                Vec2::new(sub_w - 8.0 * s, ih - 2.0 * s),
            );
            let resp = ui.allocate_rect(cr, Sense::click());
            let hov = resp.hovered();
            {
                let p = ui.painter();
                if hov {
                    p.rect_filled(cr, 4.0 * s, ga(RED, 22));
                }
                p.text(
                    cr.center(),
                    egui::Align2::CENTER_CENTER,
                    "Clear All",
                    FontId::new(12.5 * s, FontFamily::Proportional),
                    if hov { RED } else { TEXT_SEC },
                );
            }
            if resp.clicked() {
                gui.midi_learn.mappings.lock().clear();
                gui.midi_cleanup_menu = false;
                gui.midi_context_menu = false;
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.midi_cleanup_menu = false;
            }
        });
}

// ─── Flyout — OS Dropdown ─────────────────────────────────────────────────────
fn draw_flyout_os(
    ctx: &Context,
    gui: &mut NebulaGui,
    params: &GuiParams,
    ch: &mut GuiChanges,
    s: f32,
) {
    let os_labels = ["Off", "2×", "4×", "6×", "8×"];
    let os_w = 90.0 * s;
    let ih = 24.0 * s;
    let drop_h = os_labels.len() as f32 * ih + 8.0 * s;
    let anchor = gui.os_anchor;
    let dr = Rect::from_min_size(anchor, Vec2::new(os_w, drop_h));
    let screen = ctx.screen_rect();
    
    egui::Area::new(egui::Id::new("neb_os_bg"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            if ui.allocate_rect(screen, Sense::click()).clicked() {
                gui.os_dropdown = false;
            }
        });
    
    egui::Area::new(egui::Id::new("neb_os_drop"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            {
                let p = ui.painter();
                p.rect_filled(
                    Rect::from_min_size(anchor + Vec2::new(2.0, 3.0), Vec2::new(os_w, drop_h)),
                    8.0 * s,
                    Color32::from_black_alpha(80),
                );
                acrylic_card(p, dr, 8.0 * s);
            }
            for (i, &lbl) in os_labels.iter().enumerate() {
                let ir = Rect::from_min_size(
                    Pos2::new(anchor.x + 4.0 * s, anchor.y + 4.0 * s + i as f32 * ih),
                    Vec2::new(os_w - 8.0 * s, ih - 2.0 * s),
                );
                let resp = ui.allocate_rect(ir, Sense::click());
                let isel = i == params.oversampling as usize;
                let hov = resp.hovered();
                {
                    let p = ui.painter();
                    if isel {
                        p.rect_filled(ir, 4.0 * s, ACCENT);
                    } else if hov {
                        p.rect_filled(ir, 4.0 * s, CTRL_HOVER);
                    }
                    p.text(
                        Pos2::new(ir.min.x + 12.0 * s, ir.center().y),
                        egui::Align2::LEFT_CENTER,
                        lbl,
                        FontId::new(11.5 * s, FontFamily::Proportional),
                        if isel {
                            Color32::WHITE
                        } else if hov {
                            TEXT_PRI
                        } else {
                            TEXT_SEC
                        },
                    );
                }
                if resp.clicked() {
                    ch.oversampling = Some(i as u32);
                    gui.os_dropdown = false;
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.os_dropdown = false;
            }
        });
}

// ─── Flyout — Preset Dropdown ─────────────────────────────────────────────────
fn draw_flyout_preset(ctx: &Context, gui: &mut NebulaGui, ch: &mut GuiChanges, s: f32) {
    if gui.presets.is_empty() {
        gui.preset_dropdown_open = false;
        return;
    }
    let pw = 152.0 * s;
    let ih = 24.0 * s;
    let drop_h = gui.presets.len() as f32 * ih + 8.0 * s;
    let anchor = gui.preset_anchor;
    let dr = Rect::from_min_size(anchor, Vec2::new(pw, drop_h));
    let screen = ctx.screen_rect();
    
    egui::Area::new(egui::Id::new("neb_pr_bg"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            if ui.allocate_rect(screen, Sense::click()).clicked() {
                gui.preset_dropdown_open = false;
            }
        });
    
    let presets_clone = gui.presets.clone();
    egui::Area::new(egui::Id::new("neb_pr_drop"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            {
                let p = ui.painter();
                p.rect_filled(
                    Rect::from_min_size(anchor + Vec2::new(2.0, 3.0), Vec2::new(pw, drop_h)),
                    8.0 * s,
                    Color32::from_black_alpha(80),
                );
                acrylic_card(p, dr, 8.0 * s);
            }
            for (i, (name, snap)) in presets_clone.iter().enumerate() {
                let ir = Rect::from_min_size(
                    Pos2::new(anchor.x + 4.0 * s, anchor.y + 4.0 * s + i as f32 * ih),
                    Vec2::new(pw - 8.0 * s, ih - 2.0 * s),
                );
                let resp = ui.allocate_rect(ir, Sense::click());
                let isel = i == gui.selected_preset;
                let hov = resp.hovered();
                {
                    let p = ui.painter();
                    if isel {
                        p.rect_filled(ir, 4.0 * s, ACCENT);
                    } else if hov {
                        p.rect_filled(ir, 4.0 * s, CTRL_HOVER);
                    }
                    let display = if name.len() > 20 { &name[..20] } else { name };
                    p.text(
                        Pos2::new(ir.min.x + 12.0 * s, ir.center().y),
                        egui::Align2::LEFT_CENTER,
                        display,
                        FontId::new(11.5 * s, FontFamily::Proportional),
                        if isel {
                            Color32::WHITE
                        } else if hov {
                            TEXT_PRI
                        } else {
                            TEXT_SEC
                        },
                    );
                }
                if resp.clicked() {
                    gui.selected_preset = i;
                    gui.preset_dropdown_open = false;
                    snap.apply_to(ch);
                }
            }
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.preset_dropdown_open = false;
            }
        });
}
