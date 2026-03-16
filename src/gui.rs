// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser — Synthwave / Alien Realm GUI
// Built with nih_plug_egui's egui (single version)
// Pattern: all interactions FIRST, then all painting
// ─────────────────────────────────────────────────────────────────────────────

use std::sync::Arc;
use parking_lot::Mutex;

use nih_plug_egui::egui::{
    self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, Ui, Context, FontFamily,
};

use crate::analyzer::SpectrumData;

// ─── Color Palette ───────────────────────────────────────────────────────────

const BG_VOID:       Color32 = Color32::from_rgb(4,  2,  14);
const BG_PANEL:      Color32 = Color32::from_rgb(8,  5,  22);
const BG_DEEP:       Color32 = Color32::from_rgb(12, 6,  30);
const ACCENT_CYAN:   Color32 = Color32::from_rgb(0, 230, 255);
const ACCENT_MAGENTA:Color32 = Color32::from_rgb(255, 0, 200);
const ACCENT_PURPLE: Color32 = Color32::from_rgb(140, 0, 255);
const ACCENT_GOLD:   Color32 = Color32::from_rgb(255, 200, 0);
const METER_BLUE:    Color32 = Color32::from_rgb(0, 100, 220);
const METER_YELLOW:  Color32 = Color32::from_rgb(255, 200, 0);
const METER_RED:     Color32 = Color32::from_rgb(255, 40,  40);
const GRID_LINE:     Color32 = Color32::from_rgba_premultiplied(0, 230, 255, 18);
const TEXT_PRIMARY:  Color32 = Color32::from_rgb(200, 220, 255);
const TEXT_DIM:      Color32 = Color32::from_rgb(70,  90, 130);

fn ga(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        ((c.r() as u32 * a as u32) / 255) as u8,
        ((c.g() as u32 * a as u32) / 255) as u8,
        ((c.b() as u32 * a as u32) / 255) as u8,
        a,
    )
}

// ─── Numeric Input Popup ──────────────────────────────────────────────────────

#[derive(Default, Clone, PartialEq)]
pub enum NumericTarget {
    #[default] None,
    Threshold, MaxReduction, MinFreq, MaxFreq, Lookahead, StereoLink,
}

#[derive(Default, Clone)]
pub struct NumericInputState {
    pub open: bool,
    pub label: String,
    pub value_str: String,
    pub target: NumericTarget,
    pub min: f64,
    pub max: f64,
}

// ─── GUI State ────────────────────────────────────────────────────────────────

pub struct NebulaGui {
    pub spectrum: Arc<Mutex<SpectrumData>>,
    pub num_input: NumericInputState,
    pub time: f64,
}

impl NebulaGui {
    pub fn new(spectrum: Arc<Mutex<SpectrumData>>) -> Self {
        Self { spectrum, num_input: NumericInputState::default(), time: 0.0 }
    }
}

// ─── GUI Params / Changes ─────────────────────────────────────────────────────

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
}

// ─── Main Draw ────────────────────────────────────────────────────────────────

pub fn draw(ctx: &Context, gui: &mut NebulaGui, params: &GuiParams) -> GuiChanges {
    gui.time += ctx.input(|i| i.unstable_dt) as f64;
    let mut changes = GuiChanges::default();

    // Custom style
    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = BG_VOID;
    style.visuals.override_text_color = Some(TEXT_PRIMARY);
    style.visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    style.visuals.widgets.inactive.bg_fill = BG_DEEP;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0, 40, 60);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, ACCENT_CYAN);
    style.spacing.item_spacing = Vec2::new(4.0, 3.0);
    ctx.set_style(style);

    egui::CentralPanel::default()
        .frame(egui::Frame::NONE.fill(BG_VOID))
        .show(ctx, |ui| {
            let full = ui.max_rect();

            // Background grid (pure paint, no interaction needed)
            draw_grid(ui.painter_at(full), full, gui.time);

            let margin = 8.0;
            let title_h = 44.0;

            // Title bar (paint only)
            paint_title(ui.painter_at(full), full);

            // Content area
            let content = Rect::from_min_size(
                Pos2::new(full.min.x + margin, full.min.y + title_h + margin),
                Vec2::new(full.width() - margin * 2.0, full.height() - title_h - margin * 2.0),
            );

            let meters_w  = 90.0;
            let gap       = 6.0;
            let center_w  = content.width() - meters_w * 2.0 - gap * 2.0;
            let analyzer_h_frac = 0.42_f32;

            let left_rect = Rect::from_min_size(
                content.min,
                Vec2::new(meters_w, content.height()),
            );
            let center_rect = Rect::from_min_size(
                Pos2::new(content.min.x + meters_w + gap, content.min.y),
                Vec2::new(center_w, content.height()),
            );
            let right_rect = Rect::from_min_size(
                Pos2::new(center_rect.max.x + gap, content.min.y),
                Vec2::new(meters_w, content.height()),
            );

            let controls_h = center_rect.height() * (1.0 - analyzer_h_frac);
            let analyzer_h = center_rect.height() * analyzer_h_frac;
            let controls_rect = Rect::from_min_size(center_rect.min, Vec2::new(center_w, controls_h));
            let analyzer_rect = Rect::from_min_size(
                Pos2::new(center_rect.min.x, center_rect.min.y + controls_h + 4.0),
                Vec2::new(center_w, analyzer_h - 4.0),
            );

            // Detection meter (left)
            draw_detection_panel(ui, left_rect, params, &mut changes);

            // Reduction meter (right)
            draw_reduction_panel(ui, right_rect, params, &mut changes);

            // Controls (center top)
            draw_controls_panel(ui, controls_rect, params, &mut changes, gui);

            // Analyzer (center bottom)
            draw_analyzer_panel(ui, analyzer_rect, gui, params, &mut changes);
        });

    // Numeric popup overlay
    if gui.num_input.open {
        draw_numeric_popup(ctx, gui, &mut changes);
    }

    changes
}

// ─── Grid Background ─────────────────────────────────────────────────────────

fn draw_grid(painter: egui::Painter, rect: Rect, time: f64) {
    let sp = 40.0_f32;
    let ox = (time as f32 * 8.0).rem_euclid(sp);
    let oy = (time as f32 * 4.0).rem_euclid(sp);
    let mut x = rect.min.x - ox;
    while x < rect.max.x + sp {
        painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], Stroke::new(0.4, GRID_LINE));
        x += sp;
    }
    let mut y = rect.min.y - oy;
    while y < rect.max.y + sp {
        painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], Stroke::new(0.4, GRID_LINE));
        y += sp;
    }
}

fn paint_title(painter: egui::Painter, rect: Rect) {
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 42.0));
    painter.rect_filled(bar, 0.0, BG_PANEL);
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(2.0, ACCENT_CYAN));
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(8.0, ga(ACCENT_CYAN, 35)));
    painter.text(
        Pos2::new(bar.min.x + 14.0, bar.center().y),
        egui::Align2::LEFT_CENTER,
        "NEBULA  DE-ESSER",
        FontId::new(17.0, FontFamily::Monospace),
        ACCENT_CYAN,
    );
    painter.text(
        Pos2::new(bar.min.x + 230.0, bar.center().y + 1.0),
        egui::Align2::LEFT_CENTER,
        "ALIEN SPECTRUM PROCESSOR  //  64-BIT CLAP",
        FontId::new(8.5, FontFamily::Monospace),
        TEXT_DIM,
    );
    painter.text(
        Pos2::new(bar.max.x - 14.0, bar.center().y),
        egui::Align2::RIGHT_CENTER,
        "v1.0",
        FontId::new(8.5, FontFamily::Monospace),
        ga(ACCENT_PURPLE, 200),
    );
}

// ─── Detection Meter Panel ────────────────────────────────────────────────────

fn draw_detection_panel(ui: &mut Ui, rect: Rect, params: &GuiParams, changes: &mut GuiChanges) {
    // Paint background
    {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, BG_PANEL);
        p.rect_stroke(rect, 4.0, Stroke::new(1.0, ga(ACCENT_CYAN, 55)), egui::StrokeKind::Outside);
        p.text(Pos2::new(rect.center().x, rect.min.y + 9.0), egui::Align2::CENTER_CENTER,
            "DETECT", FontId::new(8.5, FontFamily::Monospace), ACCENT_CYAN);
    }

    let cx = rect.center().x;
    let meter_top = rect.min.y + 42.0;
    let meter_h = rect.height() - 80.0;
    let meter_w = 16.0;
    let slider_w = 16.0;
    let total_w = meter_w + slider_w + 4.0;
    let start_x = cx - total_w * 0.5;

    // Max field - interaction first
    let max_rect = Rect::from_center_size(
        Pos2::new(cx, rect.min.y + 27.0),
        Vec2::new(rect.width() - 12.0, 14.0),
    );
    let max_resp = ui.allocate_rect(max_rect, Sense::click());
    if max_resp.clicked() { changes.detection_max_reset = true; }

    // Slider - interaction
    let slider_rect = Rect::from_min_size(
        Pos2::new(start_x + meter_w + 4.0, meter_top),
        Vec2::new(slider_w, meter_h),
    );
    let slider_resp = ui.allocate_rect(slider_rect, Sense::drag());
    if slider_resp.dragged() {
        let dy = slider_resp.drag_delta().y;
        let norm = (((params.threshold + 60.0) / 60.0) as f32 - dy / meter_h).clamp(0.0, 1.0);
        changes.threshold = Some(-60.0 + norm as f64 * 60.0);
    }

    // Now paint everything
    {
        let p = ui.painter_at(rect);

        // Max field
        p.rect_filled(max_rect, 2.0, BG_DEEP);
        p.rect_stroke(max_rect, 2.0, Stroke::new(0.8, ga(METER_YELLOW, 120)), egui::StrokeKind::Outside);
        p.text(max_rect.center(), egui::Align2::CENTER_CENTER,
            format!("{:.1}", params.detection_max_db),
            FontId::new(8.0, FontFamily::Monospace), METER_YELLOW);

        // Meter bar
        let meter_rect = Rect::from_min_size(
            Pos2::new(start_x, meter_top),
            Vec2::new(meter_w, meter_h),
        );
        paint_vertical_meter(&p, meter_rect, params.detection_db, -60.0, 0.0);

        // dB scale
        for db in &[-60_i32, -48, -36, -24, -12, 0] {
            let y = meter_top + meter_h * (1.0 - (*db as f32 + 60.0) / 60.0);
            p.line_segment(
                [Pos2::new(meter_rect.min.x, y), Pos2::new(meter_rect.max.x, y)],
                Stroke::new(0.4, ga(ACCENT_CYAN, 25)),
            );
            p.text(Pos2::new(meter_rect.min.x - 3.0, y), egui::Align2::RIGHT_CENTER,
                if *db == 0 { "0".to_string() } else { format!("{db}") },
                FontId::new(7.0, FontFamily::Monospace), TEXT_DIM);
        }

        // Threshold indicator on slider
        let t_norm = ((params.threshold + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
        let t_y = meter_top + meter_h * (1.0 - t_norm);
        p.rect_filled(slider_rect, 2.0, BG_DEEP);
        p.rect_stroke(slider_rect, 2.0, Stroke::new(0.8, ga(ACCENT_CYAN, 50)), egui::StrokeKind::Outside);
        p.rect_filled(
            Rect::from_center_size(Pos2::new(slider_rect.center().x, t_y), Vec2::new(slider_w - 2.0, 5.0)),
            2.0, ACCENT_CYAN,
        );
        p.rect_filled(
            Rect::from_center_size(Pos2::new(slider_rect.center().x, t_y), Vec2::new(slider_w - 2.0, 5.0)),
            2.0, ga(ACCENT_CYAN, 60),
        );

        // Threshold label
        p.text(Pos2::new(cx, meter_top + meter_h + 10.0), egui::Align2::CENTER_CENTER,
            format!("T:{:.1}", params.threshold),
            FontId::new(8.0, FontFamily::Monospace), ACCENT_CYAN);
    }
}

// ─── Reduction Meter Panel ────────────────────────────────────────────────────

fn draw_reduction_panel(ui: &mut Ui, rect: Rect, params: &GuiParams, changes: &mut GuiChanges) {
    {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, BG_PANEL);
        p.rect_stroke(rect, 4.0, Stroke::new(1.0, ga(ACCENT_MAGENTA, 55)), egui::StrokeKind::Outside);
        p.text(Pos2::new(rect.center().x, rect.min.y + 9.0), egui::Align2::CENTER_CENTER,
            "REDUCE", FontId::new(8.5, FontFamily::Monospace), ACCENT_MAGENTA);
    }

    let cx = rect.center().x;
    let meter_top = rect.min.y + 42.0;
    let meter_h = rect.height() - 80.0;
    let meter_w = 16.0;
    let slider_w = 16.0;
    let total_w = meter_w + slider_w + 4.0;
    let start_x = cx - total_w * 0.5;

    let max_rect = Rect::from_center_size(
        Pos2::new(cx, rect.min.y + 27.0),
        Vec2::new(rect.width() - 12.0, 14.0),
    );
    let max_resp = ui.allocate_rect(max_rect, Sense::click());
    if max_resp.clicked() { changes.reduction_max_reset = true; }

    // Max reduction slider
    let slider_rect = Rect::from_min_size(
        Pos2::new(start_x, meter_top),
        Vec2::new(slider_w, meter_h),
    );
    let slider_resp = ui.allocate_rect(slider_rect, Sense::drag());
    if slider_resp.dragged() {
        let dy = slider_resp.drag_delta().y;
        let norm = ((params.max_reduction / 40.0) as f32 - dy / meter_h).clamp(0.0, 1.0);
        changes.max_reduction = Some(norm as f64 * 40.0);
    }

    {
        let p = ui.painter_at(rect);

        p.rect_filled(max_rect, 2.0, BG_DEEP);
        p.rect_stroke(max_rect, 2.0, Stroke::new(0.8, ga(ACCENT_MAGENTA, 120)), egui::StrokeKind::Outside);
        p.text(max_rect.center(), egui::Align2::CENTER_CENTER,
            format!("{:.1}", params.reduction_max_db),
            FontId::new(8.0, FontFamily::Monospace), ACCENT_MAGENTA);

        // Slider
        p.rect_filled(slider_rect, 2.0, BG_DEEP);
        p.rect_stroke(slider_rect, 2.0, Stroke::new(0.8, ga(ACCENT_MAGENTA, 50)), egui::StrokeKind::Outside);
        let mr_norm = (params.max_reduction / 40.0).clamp(0.0, 1.0) as f32;
        let mr_y = meter_top + meter_h * (1.0 - mr_norm);
        p.rect_filled(
            Rect::from_center_size(Pos2::new(slider_rect.center().x, mr_y), Vec2::new(slider_w - 2.0, 5.0)),
            2.0, ACCENT_MAGENTA,
        );

        // Reduction meter
        let meter_rect = Rect::from_min_size(
            Pos2::new(start_x + slider_w + 4.0, meter_top),
            Vec2::new(meter_w, meter_h),
        );
        p.rect_filled(meter_rect, 2.0, BG_DEEP);
        p.rect_stroke(meter_rect, 2.0, Stroke::new(0.8, ga(ACCENT_MAGENTA, 40)), egui::StrokeKind::Outside);
        let red_norm = (-params.reduction_db / 40.0).clamp(0.0, 1.0);
        let fill_h = meter_rect.height() * red_norm;
        if fill_h > 0.0 {
            let fr = Rect::from_min_size(meter_rect.min, Vec2::new(meter_rect.width(), fill_h));
            p.rect_filled(fr, 1.0, ACCENT_MAGENTA);
            p.rect_filled(fr, 1.0, ga(ACCENT_MAGENTA, 40));
        }

        // Scale
        for db in &[0_i32, -10, -20, -30, -40] {
            let y = meter_top + meter_h * (-*db as f32 / 40.0);
            p.text(Pos2::new(meter_rect.max.x + 3.0, y), egui::Align2::LEFT_CENTER,
                format!("{db}"),
                FontId::new(7.0, FontFamily::Monospace), TEXT_DIM);
        }

        p.text(Pos2::new(cx, meter_top + meter_h + 10.0), egui::Align2::CENTER_CENTER,
            format!("M:{:.1}", params.max_reduction),
            FontId::new(8.0, FontFamily::Monospace), ACCENT_MAGENTA);
    }
}

// ─── Controls Panel ──────────────────────────────────────────────────────────

fn draw_controls_panel(
    ui: &mut Ui,
    rect: Rect,
    params: &GuiParams,
    changes: &mut GuiChanges,
    gui: &mut NebulaGui,
) {
    {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, BG_PANEL);
        p.rect_stroke(rect, 4.0, Stroke::new(1.0, ga(ACCENT_PURPLE, 55)), egui::StrokeKind::Outside);
    }

    let inner = rect.shrink(8.0);
    let top_h = 78.0;
    let knob_h = 90.0;
    let btn_h  = 30.0;

    // ── Row 1: mode / range / filter / sidechain / vocal ─────────────────
    let top_rect = Rect::from_min_size(inner.min, Vec2::new(inner.width(), top_h));
    let ncols = 5;
    let cw = inner.width() / ncols as f32;
    let col_rects: Vec<Rect> = (0..ncols).map(|i| {
        Rect::from_min_size(
            Pos2::new(top_rect.min.x + i as f32 * cw, top_rect.min.y),
            Vec2::new(cw - 4.0, top_h),
        )
    }).collect();

    draw_mode_section(ui, col_rects[0], params.mode_relative, changes);
    draw_range_section(ui, col_rects[1], params.use_wide_range, changes);
    draw_filter_section(ui, col_rects[2], params.use_peak_filter, changes);
    draw_sidechain_section(ui, col_rects[3], params.sidechain_external, changes);
    draw_vocal_section(ui, col_rects[4], params.vocal_mode, changes);

    // ── Row 2: Knobs ──────────────────────────────────────────────────────
    let knob_y = inner.min.y + top_h + 8.0;
    let knob_defs: &[(&str, f64, f64, f64, &str, NumericTarget)] = &[
        ("THRESH",   params.threshold,     -60.0,  0.0, "dB", NumericTarget::Threshold),
        ("MAX RED",  params.max_reduction,   0.0,  40.0, "dB", NumericTarget::MaxReduction),
        ("MIN FREQ", params.min_freq,     1000.0,16000.0,"Hz", NumericTarget::MinFreq),
        ("MAX FREQ", params.max_freq,     1000.0,20000.0,"Hz", NumericTarget::MaxFreq),
        ("LOOKAHD",  params.lookahead_ms,   0.0,  20.0, "ms", NumericTarget::Lookahead),
        ("ST LINK",  params.stereo_link,    0.0,   1.0,  "%", NumericTarget::StereoLink),
    ];

    let nknobs = knob_defs.len();
    let kw = inner.width() / nknobs as f32;
    let knob_size = 46.0_f32;

    for (i, (label, value, min, max, unit, target)) in knob_defs.iter().enumerate() {
        let kx = inner.min.x + kw * i as f32 + kw * 0.5;
        let ky = knob_y;

        // Label (painted before interaction)
        {
            let p = ui.painter_at(rect);
            p.text(Pos2::new(kx, ky + 6.0), egui::Align2::CENTER_CENTER,
                *label, FontId::new(7.5, FontFamily::Monospace), TEXT_DIM);
        }

        let knob_center = Pos2::new(kx, ky + 16.0 + knob_size * 0.5);
        let knob_rect = Rect::from_center_size(knob_center, Vec2::splat(knob_size));
        let field_rect = Rect::from_center_size(
            Pos2::new(kx, knob_rect.max.y + 12.0),
            Vec2::new(kw - 10.0, 14.0),
        );

        // Interaction: drag knob
        let kresp = ui.allocate_rect(knob_rect, Sense::drag().union(Sense::click()));
        if kresp.dragged() {
            let delta = -kresp.drag_delta().y * 0.006;
            let norm = ((*value - *min) / (*max - *min)) as f32;
            let new_norm = (norm + delta).clamp(0.0, 1.0);
            let new_val = *min + new_norm as f64 * (*max - *min);
            match target {
                NumericTarget::Threshold    => changes.threshold     = Some(new_val),
                NumericTarget::MaxReduction => changes.max_reduction = Some(new_val),
                NumericTarget::MinFreq      => changes.min_freq      = Some(new_val),
                NumericTarget::MaxFreq      => changes.max_freq      = Some(new_val),
                NumericTarget::Lookahead    => changes.lookahead_ms  = Some(new_val),
                NumericTarget::StereoLink   => changes.stereo_link   = Some(new_val),
                NumericTarget::None         => {}
            }
        }
        // Scroll
        if kresp.hovered() {
            let scroll = ui.input(|i| i.smooth_scroll_delta.y);
            if scroll != 0.0 {
                let norm = ((*value - *min) / (*max - *min)) as f32;
                let new_norm = (norm + scroll * 0.008).clamp(0.0, 1.0);
                let new_val = *min + new_norm as f64 * (*max - *min);
                match target {
                    NumericTarget::Threshold    => changes.threshold     = Some(new_val),
                    NumericTarget::MaxReduction => changes.max_reduction = Some(new_val),
                    NumericTarget::MinFreq      => changes.min_freq      = Some(new_val),
                    NumericTarget::MaxFreq      => changes.max_freq      = Some(new_val),
                    NumericTarget::Lookahead    => changes.lookahead_ms  = Some(new_val),
                    NumericTarget::StereoLink   => changes.stereo_link   = Some(new_val),
                    NumericTarget::None         => {}
                }
            }
        }
        // Right-click
        if kresp.secondary_clicked() {
            gui.num_input = NumericInputState {
                open: true,
                label: label.to_string(),
                value_str: format!("{:.2}", value),
                target: target.clone(),
                min: *min,
                max: *max,
            };
        }

        // Field right-click
        let fresp = ui.allocate_rect(field_rect, Sense::click());
        if fresp.secondary_clicked() {
            gui.num_input = NumericInputState {
                open: true,
                label: label.to_string(),
                value_str: format!("{:.2}", value),
                target: target.clone(),
                min: *min,
                max: *max,
            };
        }

        // Paint knob and field
        {
            let p = ui.painter_at(rect);
            paint_knob(&p, knob_center, knob_size * 0.5, *value, *min, *max, ACCENT_CYAN);
            paint_value_field(&p, field_rect, *value, *unit, ACCENT_CYAN);
        }
    }

    // ── Row 3: Toggle buttons ─────────────────────────────────────────────
    let btn_y = knob_y + knob_h + 8.0;
    let btn_defs: &[(&str, bool)] = &[
        ("FILTER SOLO",   params.filter_solo),
        ("TRIGGER HEAR",  params.trigger_hear),
        ("LOOKAHEAD ON",  params.lookahead_enabled),
        ("MID / SIDE",    params.stereo_mid_side),
    ];
    let nbtns = btn_defs.len();
    let bw = inner.width() / nbtns as f32 - 4.0;

    for (i, (label, active)) in btn_defs.iter().enumerate() {
        let bx = inner.min.x + (bw + 4.0) * i as f32;
        let brect = Rect::from_min_size(Pos2::new(bx, btn_y), Vec2::new(bw, btn_h));
        let bresp = ui.allocate_rect(brect, Sense::click());
        let is_hover = bresp.hovered();
        let clicked  = bresp.clicked();

        {
            let p = ui.painter_at(rect);
            let color = if *active { ACCENT_CYAN } else if is_hover { ga(ACCENT_CYAN, 150) } else { TEXT_DIM };
            let bg = if *active { Color32::from_rgb(0, 38, 55) } else { BG_DEEP };
            p.rect_filled(brect, 4.0, bg);
            p.rect_stroke(brect, 4.0, Stroke::new(1.0, color), egui::StrokeKind::Outside);
            if *active {
                p.rect_stroke(brect, 4.0, Stroke::new(4.0, ga(color, 35)), egui::StrokeKind::Outside);
            }
            p.text(brect.center(), egui::Align2::CENTER_CENTER,
                *label, FontId::new(7.5, FontFamily::Monospace), color);
        }

        if clicked {
            match *label {
                "FILTER SOLO"  => changes.filter_solo      = Some(!*active),
                "TRIGGER HEAR" => changes.trigger_hear     = Some(!*active),
                "LOOKAHEAD ON" => changes.lookahead_enabled = Some(!*active),
                "MID / SIDE"   => changes.stereo_mid_side  = Some(!*active),
                _ => {}
            }
        }
    }
}

// ─── Section Selectors ────────────────────────────────────────────────────────

fn section_header(ui: &mut Ui, parent: Rect, rect: Rect, label: &str) {
    let p = ui.painter_at(parent);
    p.text(Pos2::new(rect.center().x, rect.min.y + 7.0), egui::Align2::CENTER_CENTER,
        label, FontId::new(7.0, FontFamily::Monospace), ga(ACCENT_PURPLE, 220));
}

fn two_button_section(
    ui: &mut Ui,
    parent: Rect,
    rect: Rect,
    header: &str,
    labels: [&str; 2],
    active_idx: usize,
) -> Option<usize> {
    section_header(ui, parent, rect, header);
    let bh = 18.0;
    let mut result = None;
    for (i, label) in labels.iter().enumerate() {
        let br = Rect::from_min_size(
            Pos2::new(rect.min.x, rect.min.y + 16.0 + i as f32 * (bh + 3.0)),
            Vec2::new(rect.width(), bh),
        );
        let resp = ui.allocate_rect(br, Sense::click());
        let is_active = i == active_idx;
        let hov = resp.hovered();
        {
            let p = ui.painter_at(parent);
            let col = if is_active { ACCENT_CYAN } else if hov { ga(ACCENT_CYAN, 150) } else { TEXT_DIM };
            p.rect_filled(br, 2.0, if is_active { Color32::from_rgb(0, 38, 55) } else { BG_DEEP });
            p.rect_stroke(br, 2.0, Stroke::new(0.8, col), egui::StrokeKind::Outside);
            p.text(br.center(), egui::Align2::CENTER_CENTER,
                *label, FontId::new(7.0, FontFamily::Monospace), col);
        }
        if resp.clicked() { result = Some(i); }
    }
    result
}

fn draw_mode_section(ui: &mut Ui, rect: Rect, relative: bool, changes: &mut GuiChanges) {
    if let Some(i) = two_button_section(ui, rect, rect, "MODE", ["RELATIVE", "ABSOLUTE"], if relative { 0 } else { 1 }) {
        changes.mode_relative = Some(i == 0);
    }
}
fn draw_range_section(ui: &mut Ui, rect: Rect, wide: bool, changes: &mut GuiChanges) {
    if let Some(i) = two_button_section(ui, rect, rect, "RANGE", ["SPLIT", "WIDE"], if wide { 1 } else { 0 }) {
        changes.use_wide_range = Some(i == 1);
    }
}
fn draw_filter_section(ui: &mut Ui, rect: Rect, peak: bool, changes: &mut GuiChanges) {
    if let Some(i) = two_button_section(ui, rect, rect, "FILTER", ["LOWPASS", "PEAK"], if peak { 1 } else { 0 }) {
        changes.use_peak_filter = Some(i == 1);
    }
}
fn draw_sidechain_section(ui: &mut Ui, rect: Rect, ext: bool, changes: &mut GuiChanges) {
    if let Some(i) = two_button_section(ui, rect, rect, "SIDECHAIN", ["INTERNAL", "EXTERNAL"], if ext { 1 } else { 0 }) {
        changes.sidechain_external = Some(i == 1);
    }
}
fn draw_vocal_section(ui: &mut Ui, rect: Rect, vocal: bool, changes: &mut GuiChanges) {
    if let Some(i) = two_button_section(ui, rect, rect, "PROC MODE", ["VOCAL", "ALLROUND"], if vocal { 0 } else { 1 }) {
        changes.vocal_mode = Some(i == 0);
    }
}

// ─── Analyzer Panel ───────────────────────────────────────────────────────────

fn draw_analyzer_panel(
    ui: &mut Ui,
    rect: Rect,
    gui: &mut NebulaGui,
    params: &GuiParams,
    changes: &mut GuiChanges,
) {
    if rect.height() < 20.0 { return; }

    {
        let p = ui.painter_at(rect);
        p.rect_filled(rect, 4.0, BG_DEEP);
        p.rect_stroke(rect, 4.0, Stroke::new(1.0, ga(ACCENT_PURPLE, 80)), egui::StrokeKind::Outside);
    }

    let inner = rect.shrink(3.0);
    let plot_h = inner.height() - 16.0;
    let sample_rate = 44100.0_f32;

    // Frequency markers (paint only)
    {
        let p = ui.painter_at(rect);
        for &freq in &[100.0_f32, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0_f32] {
            let nx = freq_to_x(freq, inner.width(), sample_rate);
            let x = inner.min.x + nx;
            p.line_segment([Pos2::new(x, inner.min.y), Pos2::new(x, inner.min.y + plot_h)],
                Stroke::new(0.4, ga(ACCENT_CYAN, 22)));
            let lbl = if freq >= 1000.0 { format!("{}k", (freq / 1000.0) as i32) } else { format!("{}", freq as i32) };
            p.text(Pos2::new(x, inner.max.y - 5.0), egui::Align2::CENTER_CENTER,
                lbl, FontId::new(6.5, FontFamily::Monospace), TEXT_DIM);
        }
    }

    // Spectrum (paint only)
    {
        if let Some(spec) = gui.spectrum.try_lock() {
            let mags = &spec.magnitudes;
            let nb = mags.len();
            let p = ui.painter_at(rect);
            let db_min = -90.0_f32;
            let db_max = 0.0_f32;
            let db_range = db_max - db_min;
            let mut pts: Vec<Pos2> = Vec::with_capacity(nb);
            for i in 1..nb {
                let bin_freq = i as f32 * sample_rate / (nb as f32 * 2.0);
                if bin_freq < 20.0 || bin_freq > 22000.0 { continue; }
                let nx = freq_to_x(bin_freq, inner.width(), sample_rate);
                let db = mags[i].clamp(db_min, db_max);
                let ny = 1.0 - (db - db_min) / db_range;
                pts.push(Pos2::new(inner.min.x + nx, inner.min.y + ny * plot_h));
            }
            if pts.len() > 2 {
                let mut fill = pts.clone();
                fill.push(Pos2::new(pts.last().unwrap().x, inner.min.y + plot_h));
                fill.push(Pos2::new(pts.first().unwrap().x, inner.min.y + plot_h));
                p.add(egui::Shape::convex_polygon(fill, ga(ACCENT_CYAN, 14), Stroke::NONE));
                for i in 0..pts.len() - 1 {
                    p.line_segment([pts[i], pts[i+1]], Stroke::new(1.2, ga(ACCENT_CYAN, 180)));
                }
            }
        }
    }

    // Band overlay (paint only)
    {
        let p = ui.painter_at(rect);
        let min_x = inner.min.x + freq_to_x(params.min_freq as f32, inner.width(), sample_rate);
        let max_x = inner.min.x + freq_to_x(params.max_freq as f32, inner.width(), sample_rate);
        if max_x > min_x {
            let br = Rect::from_min_max(
                Pos2::new(min_x, inner.min.y),
                Pos2::new(max_x, inner.min.y + plot_h),
            );
            p.rect_filled(br, 0.0, ga(ACCENT_PURPLE, 18));
            p.line_segment([Pos2::new(min_x, inner.min.y), Pos2::new(min_x, inner.min.y + plot_h)],
                Stroke::new(1.0, ga(ACCENT_MAGENTA, 160)));
            p.line_segment([Pos2::new(max_x, inner.min.y), Pos2::new(max_x, inner.min.y + plot_h)],
                Stroke::new(1.0, ga(ACCENT_GOLD, 160)));
        }
    }

    // Interactive nodes — interaction first
    let node_y = inner.min.y + plot_h * 0.5;
    let min_nx = inner.min.x + freq_to_x(params.min_freq as f32, inner.width(), sample_rate);
    let max_nx = inner.min.x + freq_to_x(params.max_freq as f32, inner.width(), sample_rate);

    let min_hit = Rect::from_center_size(Pos2::new(min_nx, node_y), Vec2::splat(20.0));
    let max_hit = Rect::from_center_size(Pos2::new(max_nx, node_y), Vec2::splat(20.0));

    let min_resp = ui.allocate_rect(min_hit, Sense::drag());
    if min_resp.dragged() {
        let new_x = (min_nx + min_resp.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        let new_f = x_to_freq(new_x, inner.width(), sample_rate) as f64;
        changes.min_freq = Some(new_f.clamp(1000.0, params.max_freq - 100.0));
    }

    let max_resp = ui.allocate_rect(max_hit, Sense::drag());
    if max_resp.dragged() {
        let new_x = (max_nx + max_resp.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        let new_f = x_to_freq(new_x, inner.width(), sample_rate) as f64;
        changes.max_freq = Some(new_f.clamp(params.min_freq + 100.0, 20000.0));
    }

    // Paint nodes
    {
        let p = ui.painter_at(rect);
        paint_freq_node(&p, Pos2::new(min_nx, node_y), ACCENT_MAGENTA, "MIN");
        paint_freq_node(&p, Pos2::new(max_nx, node_y), ACCENT_GOLD, "MAX");

        p.text(Pos2::new(inner.min.x + 6.0, inner.min.y + 9.0), egui::Align2::LEFT_CENTER,
            "SPECTRUM ANALYZER", FontId::new(7.5, FontFamily::Monospace), ga(ACCENT_PURPLE, 200));
    }
}

// ─── Knob Paint ──────────────────────────────────────────────────────────────

fn paint_knob(
    painter: &egui::Painter,
    center: Pos2,
    radius: f32,
    value: f64, min: f64, max: f64,
    color: Color32,
) {
    let norm = ((value - min) / (max - min)).clamp(0.0, 1.0) as f32;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let angle = start + norm * sweep;

    painter.circle_filled(center, radius, BG_DEEP);
    painter.circle_stroke(center, radius, Stroke::new(1.2, ga(color, 70)));
    painter.circle_filled(center, radius, ga(color, 8));

    // Track
    draw_arc_seg(painter, center, radius * 0.78, start, start + sweep, ga(color, 40), 1.8);
    // Value arc
    if norm > 0.0 {
        draw_arc_seg(painter, center, radius * 0.78, start, angle, color, 2.2);
    }

    // Indicator
    let ix = center.x + radius * 0.58 * angle.cos();
    let iy = center.y + radius * 0.58 * angle.sin();
    painter.line_segment([center, Pos2::new(ix, iy)], Stroke::new(2.0, color));
    painter.circle_filled(center, 2.5, color);
}

fn draw_arc_seg(painter: &egui::Painter, c: Pos2, r: f32, a0: f32, a1: f32, col: Color32, w: f32) {
    let steps = 24;
    let span = a1 - a0;
    let pts: Vec<Pos2> = (0..=steps).map(|i| {
        let a = a0 + i as f32 / steps as f32 * span;
        Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect();
    for i in 0..pts.len() - 1 {
        painter.line_segment([pts[i], pts[i+1]], Stroke::new(w, col));
    }
}

fn paint_value_field(painter: &egui::Painter, rect: Rect, value: f64, unit: &str, color: Color32) {
    painter.rect_filled(rect, 2.0, BG_DEEP);
    painter.rect_stroke(rect, 2.0, Stroke::new(0.7, ga(color, 80)), egui::StrokeKind::Outside);
    let text = if unit == "Hz" {
        if value >= 1000.0 { format!("{:.1}k", value / 1000.0) } else { format!("{:.0}", value) }
    } else if unit == "%" {
        format!("{:.0}%", value * 100.0)
    } else {
        format!("{:.1}", value)
    };
    painter.text(rect.center(), egui::Align2::CENTER_CENTER,
        text, FontId::new(7.5, FontFamily::Monospace), color);
}

fn paint_vertical_meter(painter: &egui::Painter, rect: Rect, db: f32, min_db: f32, max_db: f32) {
    painter.rect_filled(rect, 2.0, BG_DEEP);
    painter.rect_stroke(rect, 2.0, Stroke::new(0.8, ga(ACCENT_CYAN, 35)), egui::StrokeKind::Outside);
    let norm = ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0);
    let fh = rect.height() * norm;
    if fh > 0.0 {
        let fr = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y - fh), Vec2::new(rect.width(), fh));
        let col = if db > -12.0 { METER_RED } else if db > -24.0 { METER_YELLOW } else { METER_BLUE };
        painter.rect_filled(fr, 1.0, col);
        painter.rect_filled(fr, 1.0, ga(col, 30));
    }
}

fn paint_freq_node(painter: &egui::Painter, center: Pos2, color: Color32, label: &str) {
    painter.circle_filled(center, 7.0, color);
    painter.circle_filled(center, 7.0, ga(color, 70));
    painter.circle_stroke(center, 7.0, Stroke::new(1.5, Color32::WHITE));
    painter.text(Pos2::new(center.x, center.y - 13.0), egui::Align2::CENTER_CENTER,
        label, FontId::new(6.5, FontFamily::Monospace), color);
}

fn freq_to_x(freq: f32, width: f32, _sr: f32) -> f32 {
    let lmin = (20.0_f32).log10();
    let lmax = (22000.0_f32).log10();
    (freq.max(20.0).min(22000.0).log10() - lmin) / (lmax - lmin) * width
}

fn x_to_freq(x: f32, width: f32, _sr: f32) -> f32 {
    let lmin = (20.0_f32).log10();
    let lmax = (22000.0_f32).log10();
    10.0_f32.powf(lmin + (x / width) * (lmax - lmin))
}

// ─── Numeric Input Popup ──────────────────────────────────────────────────────

fn draw_numeric_popup(ctx: &Context, gui: &mut NebulaGui, changes: &mut GuiChanges) {
    let screen = ctx.screen_rect();
    let popup = Rect::from_center_size(screen.center(), Vec2::new(210.0, 108.0));
    let field_rect = Rect::from_center_size(
        Pos2::new(popup.center().x, popup.center().y - 4.0),
        Vec2::new(170.0, 22.0),
    );
    let ok_rect = Rect::from_center_size(
        Pos2::new(popup.center().x - 42.0, popup.max.y - 15.0),
        Vec2::new(64.0, 18.0),
    );
    let cancel_rect = Rect::from_center_size(
        Pos2::new(popup.center().x + 42.0, popup.max.y - 15.0),
        Vec2::new(64.0, 18.0),
    );

    let label = gui.num_input.label.clone();

    egui::Area::new(egui::Id::new("nebula_num_popup"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            // ── Paint background ── (scoped to end before interaction)
            {
                let p = ui.painter();
                p.rect_filled(screen, 0.0, Color32::from_black_alpha(160));
                p.rect_filled(popup, 6.0, BG_PANEL);
                p.rect_stroke(popup, 6.0, Stroke::new(2.0, ACCENT_CYAN), egui::StrokeKind::Outside);
                p.rect_stroke(popup, 6.0, Stroke::new(7.0, ga(ACCENT_CYAN, 35)), egui::StrokeKind::Outside);
                p.text(
                    Pos2::new(popup.center().x, popup.min.y + 16.0),
                    egui::Align2::CENTER_CENTER,
                    format!("SET  {}", label),
                    FontId::new(9.5, FontFamily::Monospace),
                    ACCENT_CYAN,
                );
                p.rect_filled(field_rect, 3.0, BG_DEEP);
                p.rect_stroke(field_rect, 3.0, Stroke::new(1.2, ACCENT_PURPLE), egui::StrokeKind::Outside);
                p.rect_filled(ok_rect, 3.0, Color32::from_rgb(0, 55, 75));
                p.rect_stroke(ok_rect, 3.0, Stroke::new(1.0, ACCENT_CYAN), egui::StrokeKind::Outside);
                p.text(ok_rect.center(), egui::Align2::CENTER_CENTER,
                    "OK", FontId::new(8.5, FontFamily::Monospace), ACCENT_CYAN);
                p.rect_filled(cancel_rect, 3.0, Color32::from_rgb(55, 0, 0));
                p.rect_stroke(cancel_rect, 3.0, Stroke::new(1.0, ACCENT_MAGENTA), egui::StrokeKind::Outside);
                p.text(cancel_rect.center(), egui::Align2::CENTER_CENTER,
                    "CANCEL", FontId::new(8.5, FontFamily::Monospace), ACCENT_MAGENTA);
            }

            // ── Interactions (painter borrow is dropped above) ──
            ui.allocate_new_ui(
                egui::UiBuilder::new().max_rect(field_rect),
                |ui| {
                    let te = egui::TextEdit::singleline(&mut gui.num_input.value_str)
                        .font(FontId::new(10.0, FontFamily::Monospace))
                        .text_color(ACCENT_CYAN)
                        .frame(false)
                        .desired_width(168.0);
                    let r = ui.add(te);
                    r.request_focus();
                    if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        apply_num_input(gui, changes);
                    }
                },
            );

            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.num_input.open = false;
            }

            let ok_resp = ui.allocate_rect(ok_rect, Sense::click());
            let cancel_resp = ui.allocate_rect(cancel_rect, Sense::click());
            if ok_resp.clicked()     { apply_num_input(gui, changes); }
            if cancel_resp.clicked() { gui.num_input.open = false; }
        });
}

fn apply_num_input(gui: &mut NebulaGui, changes: &mut GuiChanges) {
    if let Ok(v) = gui.num_input.value_str.trim().parse::<f64>() {
        let v = v.clamp(gui.num_input.min, gui.num_input.max);
        match &gui.num_input.target {
            NumericTarget::Threshold    => changes.threshold     = Some(v),
            NumericTarget::MaxReduction => changes.max_reduction = Some(v),
            NumericTarget::MinFreq      => changes.min_freq      = Some(v),
            NumericTarget::MaxFreq      => changes.max_freq      = Some(v),
            NumericTarget::Lookahead    => changes.lookahead_ms  = Some(v),
            NumericTarget::StereoLink   => changes.stereo_link   = Some(v),
            NumericTarget::None         => {}
        }
    }
    gui.num_input.open = false;
}
