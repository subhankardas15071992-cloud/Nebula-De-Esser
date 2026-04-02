// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser v2.3.0 — Dark iOS/macOS-inspired GUI
// Scaling: all hardcoded pixel constants multiplied by `s` (scale factor).
// Scale = min(win_w/BASE_W, win_h/BASE_H), read from EguiState — no zoom_factor.
// ─────────────────────────────────────────────────────────────────────────────
use std::sync::Arc;
use parking_lot::Mutex;
use nih_plug_egui::egui::{
    self, Color32, FontId, Pos2, Rect, Sense, Stroke, Vec2, Ui, Context, FontFamily,
};
use nih_plug_egui::resizable_window::ResizableWindow;
use nih_plug_egui::EguiState;
use crate::analyzer::SpectrumData;
use crate::{MidiLearnShared, MIDI_PARAM_NAMES, MIDI_PARAM_COUNT};

// ─── Palette — dark macOS / iOS inspired ─────────────────────────────────────
// Backgrounds: deep charcoal layering, no pure black
const BG_BASE:    Color32 = Color32::from_rgb(14,  14,  16);   // window fill
const BG_PANEL:   Color32 = Color32::from_rgb(22,  22,  26);   // card surface
const BG_RAISED:  Color32 = Color32::from_rgb(30,  30,  36);   // raised element
const BG_WIDGET:  Color32 = Color32::from_rgb(38,  38,  46);   // input / field bg
const BG_INSET:   Color32 = Color32::from_rgb(10,  10,  12);   // inset / well

// Accent — single blue accent like macOS system blue, plus semantic colours
const ACCENT:     Color32 = Color32::from_rgb( 10, 132, 255);  // macOS blue
const ACCENT_DIM: Color32 = Color32::from_rgb(  8,  90, 180);
const RED:        Color32 = Color32::from_rgb(255,  69,  58);  // macOS red
const ORANGE:     Color32 = Color32::from_rgb(255, 159,  10);  // macOS orange
const GREEN:      Color32 = Color32::from_rgb( 48, 209,  88);  // macOS green
const TEAL:       Color32 = Color32::from_rgb( 90, 200, 250);  // iOS teal / info

// Meter colours
const M_GREEN:    Color32 = Color32::from_rgb( 48, 209,  88);
const M_YELLOW:   Color32 = Color32::from_rgb(255, 214,  10);
const M_RED:      Color32 = Color32::from_rgb(255,  69,  58);

// Text hierarchy
const TEXT_PRI:   Color32 = Color32::from_rgb(242, 242, 247);  // primary label
const TEXT_SEC:   Color32 = Color32::from_rgb(152, 152, 160);  // secondary label
const TEXT_TER:   Color32 = Color32::from_rgb( 72,  72,  80);  // tertiary / disabled

// Separator / border
const SEP:        Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 14);
const BORDER:     Color32 = Color32::from_rgba_premultiplied(255, 255, 255, 22);

const BASE_W: f32 = 860.0;
const BASE_H: f32 = 640.0;

#[inline] fn ga(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_premultiplied(
        ((c.r() as u16 * a as u16) / 255) as u8,
        ((c.g() as u16 * a as u16) / 255) as u8,
        ((c.b() as u16 * a as u16) / 255) as u8, a)
}
#[inline] fn lerp_c(a: Color32, b: Color32, t: f32) -> Color32 {
    let t = t.clamp(0.0, 1.0); let u = 1.0 - t;
    Color32::from_rgb(
        (a.r() as f32 * u + b.r() as f32 * t) as u8,
        (a.g() as f32 * u + b.g() as f32 * t) as u8,
        (a.b() as f32 * u + b.b() as f32 * t) as u8,
    )
}

// ─── Param Snapshot ───────────────────────────────────────────────────────────
#[derive(Clone, PartialEq)]
pub struct ParamSnapshot {
    pub threshold:f64, pub max_reduction:f64, pub min_freq:f64, pub max_freq:f64,
    pub mode_relative:bool, pub use_peak_filter:bool, pub use_wide_range:bool,
    pub filter_solo:bool, pub lookahead_enabled:bool, pub lookahead_ms:f64,
    pub trigger_hear:bool, pub stereo_link:f64, pub stereo_mid_side:bool,
    pub sidechain_external:bool, pub vocal_mode:bool,
    pub input_level:f64, pub input_pan:f64, pub output_level:f64, pub output_pan:f64,
}
impl ParamSnapshot {
    pub fn from_params(p: &GuiParams) -> Self {
        Self { threshold:p.threshold, max_reduction:p.max_reduction,
               min_freq:p.min_freq, max_freq:p.max_freq,
               mode_relative:p.mode_relative, use_peak_filter:p.use_peak_filter,
               use_wide_range:p.use_wide_range, filter_solo:p.filter_solo,
               lookahead_enabled:p.lookahead_enabled, lookahead_ms:p.lookahead_ms,
               trigger_hear:p.trigger_hear, stereo_link:p.stereo_link,
               stereo_mid_side:p.stereo_mid_side, sidechain_external:p.sidechain_external,
               vocal_mode:p.vocal_mode, input_level:p.input_level,
               input_pan:p.input_pan, output_level:p.output_level, output_pan:p.output_pan }
    }
    pub fn apply_to(&self, ch: &mut GuiChanges) {
        ch.threshold=Some(self.threshold); ch.max_reduction=Some(self.max_reduction);
        ch.min_freq=Some(self.min_freq); ch.max_freq=Some(self.max_freq);
        ch.mode_relative=Some(self.mode_relative); ch.use_peak_filter=Some(self.use_peak_filter);
        ch.use_wide_range=Some(self.use_wide_range); ch.filter_solo=Some(self.filter_solo);
        ch.lookahead_enabled=Some(self.lookahead_enabled); ch.lookahead_ms=Some(self.lookahead_ms);
        ch.trigger_hear=Some(self.trigger_hear); ch.stereo_link=Some(self.stereo_link);
        ch.stereo_mid_side=Some(self.stereo_mid_side); ch.sidechain_external=Some(self.sidechain_external);
        ch.vocal_mode=Some(self.vocal_mode); ch.input_level=Some(self.input_level);
        ch.input_pan=Some(self.input_pan); ch.output_level=Some(self.output_level);
        ch.output_pan=Some(self.output_pan);
    }
}

#[derive(Default, Clone, PartialEq)]
pub enum NumTarget {
    #[default] None,
    Threshold, MaxReduction, MinFreq, MaxFreq, Lookahead, StereoLink,
    InputLevel, InputPan, OutputLevel, OutputPan,
    CutWidth, CutDepth, Mix,
}
#[derive(Default, Clone)]
pub struct NumInput { pub open:bool, pub label:String, pub value_str:String, pub target:NumTarget, pub min:f64, pub max:f64 }

// ─── GUI State ────────────────────────────────────────────────────────────────
pub struct NebulaGui {
    pub spectrum:    Arc<Mutex<SpectrumData>>,
    pub midi_learn:  Arc<MidiLearnShared>,
    pub num_input:   NumInput,
    pub time:        f64,
    pub smooth_mags: Vec<f32>,
    pub presets:             Vec<(String, ParamSnapshot)>,
    pub preset_name_buf:     String,
    pub preset_save_popup:   bool,
    pub preset_dropdown_open:bool,
    pub selected_preset:     usize,
    pub state_a:             Option<ParamSnapshot>,
    pub state_b:             Option<ParamSnapshot>,
    pub active_state:        char,
    pub undo_stack:     Vec<ParamSnapshot>,
    pub redo_stack:     Vec<ParamSnapshot>,
    pub drag_snap:      Option<ParamSnapshot>,
    pub midi_popup:         bool,
    pub midi_context_menu:  bool,
    pub midi_context_anchor:Pos2,
    pub midi_cleanup_menu:  bool,
    pub midi_cleanup_anchor:Pos2,
    pub os_dropdown:        bool,
    pub os_anchor:          Pos2,
    pub preset_anchor:      Pos2,
}
impl NebulaGui {
    pub fn new(spectrum: Arc<Mutex<SpectrumData>>, midi_learn: Arc<MidiLearnShared>) -> Self {
        Self { spectrum, midi_learn, num_input:NumInput::default(), time:0.0,
               smooth_mags: vec![-120.0_f32; 1025],
               presets:Vec::new(), preset_name_buf:String::new(),
               preset_save_popup:false, preset_dropdown_open:false, selected_preset:0,
               state_a:None, state_b:None, active_state:'A',
               undo_stack:Vec::new(), redo_stack:Vec::new(), drag_snap:None,
               midi_popup:false, midi_context_menu:false, midi_context_anchor:Pos2::ZERO,
               midi_cleanup_menu:false, midi_cleanup_anchor:Pos2::ZERO,
               os_dropdown:false, os_anchor:Pos2::ZERO, preset_anchor:Pos2::ZERO }
    }
}

// ─── Params / Changes ────────────────────────────────────────────────────────
pub struct GuiParams {
    pub threshold:f64, pub max_reduction:f64, pub min_freq:f64, pub max_freq:f64,
    pub mode_relative:bool, pub use_peak_filter:bool, pub use_wide_range:bool,
    pub filter_solo:bool, pub lookahead_enabled:bool, pub lookahead_ms:f64,
    pub trigger_hear:bool, pub stereo_link:f64, pub stereo_mid_side:bool,
    pub sidechain_external:bool, pub vocal_mode:bool,
    pub detection_db:f32, pub detection_max_db:f32, pub reduction_db:f32, pub reduction_max_db:f32,
    pub input_level:f64, pub input_pan:f64, pub output_level:f64, pub output_pan:f64,
    pub bypass:bool, pub oversampling:u32,
    pub cut_width:f64, pub cut_depth:f64, pub mix:f64,
}
#[derive(Default)]
pub struct GuiChanges {
    pub threshold:Option<f64>, pub max_reduction:Option<f64>,
    pub min_freq:Option<f64>, pub max_freq:Option<f64>,
    pub mode_relative:Option<bool>, pub use_peak_filter:Option<bool>,
    pub use_wide_range:Option<bool>, pub filter_solo:Option<bool>,
    pub lookahead_enabled:Option<bool>, pub lookahead_ms:Option<f64>,
    pub trigger_hear:Option<bool>, pub stereo_link:Option<f64>,
    pub stereo_mid_side:Option<bool>, pub sidechain_external:Option<bool>,
    pub vocal_mode:Option<bool>,
    pub detection_max_reset:bool, pub reduction_max_reset:bool,
    pub input_level:Option<f64>, pub input_pan:Option<f64>,
    pub output_level:Option<f64>, pub output_pan:Option<f64>,
    pub bypass:Option<bool>, pub oversampling:Option<u32>,
    pub cut_width:Option<f64>, pub cut_depth:Option<f64>, pub mix:Option<f64>,
}

// ─── Main Draw ────────────────────────────────────────────────────────────────
pub fn draw(ctx: &Context, egui_state: &EguiState, gui: &mut NebulaGui, params: &GuiParams) -> GuiChanges {
    gui.time += ctx.input(|i| i.unstable_dt) as f64;
    let mut ch = GuiChanges::default();

    let (win_w, win_h) = egui_state.size();
    let s = (win_w as f32 / BASE_W).min(win_h as f32 / BASE_H).max(0.25);

    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = BG_BASE;
    style.visuals.override_text_color = Some(TEXT_PRI);
    style.visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    style.visuals.widgets.inactive.bg_fill = BG_RAISED;
    style.visuals.widgets.hovered.bg_fill = BG_WIDGET;
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.0 * s, ACCENT);
    style.spacing.item_spacing = Vec2::new(4.0 * s, 3.0 * s);
    ctx.set_style(style);

    ResizableWindow::new("nebula_deesser_resize")
        .min_size(Vec2::new(400.0, 300.0))
        .show(ctx, egui_state, |ui| {
            let full = ui.max_rect();
            draw_bg(ui.painter_at(full), full, params.bypass);

            let title_h   = 48.0 * s;
            let toolbar_h = 32.0 * s;
            let margin    = 10.0 * s;

            draw_title(ui.painter_at(full), full, params.bypass, s);
            draw_toolbar(ui, Rect::from_min_size(
                Pos2::new(full.min.x, full.min.y + title_h),
                Vec2::new(full.width(), toolbar_h)), params, gui, &mut ch, s);

            let content = Rect::from_min_size(
                Pos2::new(full.min.x + margin, full.min.y + title_h + toolbar_h + margin),
                Vec2::new(full.width() - margin*2.0, full.height() - title_h - toolbar_h - margin*2.0));

            let mw = 90.0 * s; let gap = 8.0 * s;
            let cw = content.width() - mw * 2.0 - gap * 2.0;

            let left_r  = Rect::from_min_size(content.min, Vec2::new(mw, content.height()));
            let right_r = Rect::from_min_size(Pos2::new(content.max.x - mw, content.min.y), Vec2::new(mw, content.height()));
            let ctr_r   = Rect::from_min_size(Pos2::new(content.min.x + mw + gap, content.min.y), Vec2::new(cw, content.height()));

            let spec_frac = 0.38_f32;
            let ctrl_h = ctr_r.height() * (1.0 - spec_frac);
            let ctrl_r = Rect::from_min_size(ctr_r.min, Vec2::new(cw, ctrl_h));
            let spec_r = Rect::from_min_size(
                Pos2::new(ctr_r.min.x, ctr_r.min.y + ctrl_h + 6.0 * s),
                Vec2::new(cw, ctr_r.height() * spec_frac - 6.0 * s));

            draw_det_panel(ui, left_r,  params, &mut ch, s);
            draw_red_panel(ui, right_r, params, &mut ch, s);
            draw_controls(ui, ctrl_r, params, &mut ch, gui, s);
            draw_spectrum(ui, spec_r, gui, params, &mut ch, s);
        });

    if gui.num_input.open        { draw_num_popup(ctx, gui, &mut ch, s); }
    if gui.preset_save_popup     { draw_preset_save(ctx, gui, params, &mut ch, s); }
    if gui.midi_popup            { draw_midi_popup(ctx, gui, s); }
    if gui.os_dropdown           { draw_os_dropdown(ctx, gui, params, &mut ch, s); }
    if gui.preset_dropdown_open  { draw_preset_dropdown(ctx, gui, &mut ch, s); }
    if gui.midi_context_menu     { draw_midi_context_menu(ctx, gui, s); }
    ch
}

// ─── Background ──────────────────────────────────────────────────────────────
fn draw_bg(painter: egui::Painter, rect: Rect, bypass: bool) {
    painter.rect_filled(rect, 0.0, BG_BASE);
    // Subtle bypass tint — no animated grid, clean and still
    if bypass {
        painter.rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(80, 0, 0, 18));
    }
}

// ─── Title Bar ───────────────────────────────────────────────────────────────
fn draw_title(painter: egui::Painter, rect: Rect, bypass: bool, s: f32) {
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 48.0 * s));
    painter.rect_filled(bar, 0.0, BG_PANEL);
    // Single hairline separator at bottom
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(0.5 * s, SEP));

    let ty = bar.center().y;
    let tx = bar.min.x + 18.0 * s;

    // Plugin name — SF-style weight contrast: name bold, subtitle light
    painter.text(Pos2::new(tx, ty - 5.0 * s), egui::Align2::LEFT_CENTER,
        "Nebula De-Esser", FontId::new(15.0 * s, FontFamily::Proportional), TEXT_PRI);
    painter.text(Pos2::new(tx, ty + 8.0 * s), egui::Align2::LEFT_CENTER,
        "Spectrum Processor  ·  64-bit  ·  CLAP",
        FontId::new(7.5 * s, FontFamily::Proportional), TEXT_TER);

    // Version pill — subtle, right-aligned
    let ver_x = bar.max.x - 14.0 * s;
    painter.text(Pos2::new(ver_x, ty), egui::Align2::RIGHT_CENTER,
        "v2.3", FontId::new(8.0 * s, FontFamily::Proportional), TEXT_SEC);

    // Bypass badge — only visible when active
    if bypass {
        let bx = bar.max.x - 90.0 * s;
        let br = Rect::from_center_size(Pos2::new(bx, ty), Vec2::new(60.0 * s, 18.0 * s));
        painter.rect_filled(br, 9.0 * s, ga(RED, 28));
        painter.rect_stroke(br, 9.0 * s, Stroke::new(0.8 * s, ga(RED, 160)), egui::StrokeKind::Outside);
        painter.text(br.center(), egui::Align2::CENTER_CENTER,
            "Bypassed", FontId::new(7.5 * s, FontFamily::Proportional), RED);
    }
}

// ─── Toolbar ─────────────────────────────────────────────────────────────────
fn draw_toolbar(ui: &mut Ui, rect: Rect, params: &GuiParams, gui: &mut NebulaGui, ch: &mut GuiChanges, s: f32) {
    { let p = ui.painter_at(rect);
      p.rect_filled(rect, 0.0, BG_PANEL);
      p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(0.5 * s, SEP)); }

    let cy = rect.center().y;
    let bh = 20.0 * s;
    let mut cx = rect.min.x + 10.0 * s;

    // Pill-style toolbar button — macOS-inspired filled pill when active
    macro_rules! tbtn {
        ($label:expr, $active:expr, $accent:expr, $w:expr) => {{
            let w = $w * s;
            let r = Rect::from_min_max(Pos2::new(cx, cy - bh * 0.5), Pos2::new(cx + w, cy + bh * 0.5));
            cx += w + 5.0 * s;
            let resp = ui.allocate_rect(r, Sense::click());
            let hov  = resp.hovered();
            let (bg, fg) = if $active {
                (ga($accent, 220), BG_BASE)
            } else if hov {
                (ga($accent, 30), $accent)
            } else {
                (BG_RAISED, TEXT_SEC)
            };
            { let p = ui.painter_at(rect);
              p.rect_filled(r, bh * 0.5, bg);
              if !$active && !hov {
                  p.rect_stroke(r, bh * 0.5, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
              }
              p.text(r.center(), egui::Align2::CENTER_CENTER, $label,
                  FontId::new(7.5 * s, FontFamily::Proportional), fg); }
            resp
        }};
    }

    if tbtn!(if params.bypass { "Bypassed" } else { "Bypass" }, params.bypass, RED, 62.0).clicked() {
        ch.bypass = Some(!params.bypass);
    }
    cx += 4.0 * s;

    // Preset selector — rounded rect, chevron
    let pw = 140.0 * s;
    { let pr = Rect::from_min_max(Pos2::new(cx, cy - bh * 0.5), Pos2::new(cx + pw, cy + bh * 0.5));
      cx += pw + 5.0 * s;
      let resp = ui.allocate_rect(pr, Sense::click());
      let hov  = resp.hovered();
      let lbl  = if gui.presets.is_empty() { "Preset".to_string() }
          else { let n = &gui.presets[gui.selected_preset.min(gui.presets.len()-1)].0;
                 format!("{}  ⌄", if n.len() > 16 { &n[..16] } else { n }) };
      { let p = ui.painter_at(rect);
        p.rect_filled(pr, bh * 0.5, if hov { ga(ACCENT, 20) } else { BG_RAISED });
        p.rect_stroke(pr, bh * 0.5, Stroke::new(0.5 * s, if hov { ga(ACCENT, 120) } else { BORDER }), egui::StrokeKind::Outside);
        p.text(pr.center(), egui::Align2::CENTER_CENTER, &lbl,
            FontId::new(7.5 * s, FontFamily::Proportional), if hov { ACCENT } else { TEXT_SEC }); }
      gui.preset_anchor = Pos2::new(pr.min.x, pr.max.y + 3.0 * s);
      if resp.clicked() { gui.preset_dropdown_open = !gui.preset_dropdown_open; }
    }

    if tbtn!("Save", false, ACCENT, 38.0).clicked() {
        gui.preset_name_buf.clear(); gui.preset_save_popup = true; gui.preset_dropdown_open = false;
    }
    if tbtn!("Delete", false, RED, 44.0).clicked() && !gui.presets.is_empty() {
        gui.presets.remove(gui.selected_preset.min(gui.presets.len()-1));
        if gui.selected_preset > 0 { gui.selected_preset -= 1; }
    }
    cx += 4.0 * s;

    let can_undo = !gui.undo_stack.is_empty();
    let can_redo = !gui.redo_stack.is_empty();
    { let w = 44.0 * s;
      let r = Rect::from_min_max(Pos2::new(cx, cy - bh * 0.5), Pos2::new(cx + w, cy + bh * 0.5));
      cx += w + 5.0 * s;
      let resp = ui.allocate_rect(r, Sense::click());
      let c = if can_undo { if resp.hovered() { ACCENT } else { TEXT_SEC } } else { TEXT_TER };
      { let p = ui.painter_at(rect);
        p.rect_filled(r, bh * 0.5, BG_RAISED);
        p.rect_stroke(r, bh * 0.5, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
        p.text(r.center(), egui::Align2::CENTER_CENTER, "↩ Undo",
            FontId::new(7.5 * s, FontFamily::Proportional), c); }
      if resp.clicked() && can_undo {
          if let Some(snap) = gui.undo_stack.pop() {
              gui.redo_stack.push(ParamSnapshot::from_params(params));
              gui.redo_stack.truncate(50);
              snap.apply_to(ch);
          }
      }
    }
    { let w = 44.0 * s;
      let r = Rect::from_min_max(Pos2::new(cx, cy - bh * 0.5), Pos2::new(cx + w, cy + bh * 0.5));
      cx += w + 5.0 * s;
      let resp = ui.allocate_rect(r, Sense::click());
      let c = if can_redo { if resp.hovered() { ACCENT } else { TEXT_SEC } } else { TEXT_TER };
      { let p = ui.painter_at(rect);
        p.rect_filled(r, bh * 0.5, BG_RAISED);
        p.rect_stroke(r, bh * 0.5, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
        p.text(r.center(), egui::Align2::CENTER_CENTER, "Redo ↪",
            FontId::new(7.5 * s, FontFamily::Proportional), c); }
      if resp.clicked() && can_redo {
          if let Some(snap) = gui.redo_stack.pop() {
              gui.undo_stack.push(ParamSnapshot::from_params(params));
              gui.undo_stack.truncate(50);
              snap.apply_to(ch);
          }
      }
    }
    cx += 4.0 * s;

    let ab_label = if gui.active_state == 'A' { "A/B  A" } else { "A/B  B" };
    let ab_active = gui.state_a.is_some() || gui.state_b.is_some();
    let ab_resp = tbtn!(ab_label, ab_active, GREEN, 56.0);
    if ab_resp.clicked() {
        let snap = ParamSnapshot::from_params(params);
        match gui.active_state { 'A' => gui.state_a = Some(snap), 'B' => gui.state_b = Some(snap), _ => {} }
        gui.active_state = if gui.active_state == 'A' { 'B' } else { 'A' };
        match (gui.active_state, &gui.state_a, &gui.state_b) {
            ('A', Some(a), _) => a.clone().apply_to(ch),
            ('B', _, Some(b)) => b.clone().apply_to(ch),
            _ => {}
        }
    }
    if ab_resp.secondary_clicked() {
        let snap = ParamSnapshot::from_params(params);
        match gui.active_state { 'A' => gui.state_a = Some(snap), 'B' => gui.state_b = Some(snap), _ => {} }
    }
    cx += 4.0 * s;

    let learning = gui.midi_learn.learning_target.load(std::sync::atomic::Ordering::Relaxed) >= 0;
    let midi_btn = tbtn!(if learning { "● Learning" } else { "MIDI Learn" }, learning, ORANGE, 82.0);
    if midi_btn.clicked() {
        if learning { gui.midi_learn.learning_target.store(-1, std::sync::atomic::Ordering::Release); }
        else { gui.midi_popup = true; }
    }
    if midi_btn.secondary_clicked() {
        gui.midi_context_menu = true;
        gui.midi_context_anchor = Pos2::new(midi_btn.rect.min.x, midi_btn.rect.max.y + 3.0 * s);
    }
    cx += 4.0 * s;

    let os_labels = ["Off", "2×", "4×", "6×", "8×"];
    let cur = os_labels.get(params.oversampling as usize).copied().unwrap_or("Off");
    let os_w = 88.0 * s;
    { let or_ = Rect::from_min_max(Pos2::new(cx, cy - bh * 0.5), Pos2::new(cx + os_w, cy + bh * 0.5));
      gui.os_anchor = Pos2::new(or_.min.x, or_.max.y + 3.0 * s);
      let resp = ui.allocate_rect(or_, Sense::click());
      let active = params.oversampling > 0;
      let hov = resp.hovered();
      let (bg, fg) = if active { (ga(TEAL, 28), TEAL) } else if hov { (ga(TEAL, 14), TEAL) } else { (BG_RAISED, TEXT_SEC) };
      { let p = ui.painter_at(rect);
        p.rect_filled(or_, bh * 0.5, bg);
        p.rect_stroke(or_, bh * 0.5, Stroke::new(0.5 * s, if active { ga(TEAL, 80) } else { BORDER }), egui::StrokeKind::Outside);
        p.text(or_.center(), egui::Align2::CENTER_CENTER,
            format!("OS  {}  ⌄", cur), FontId::new(7.5 * s, FontFamily::Proportional), fg); }
      if resp.clicked() { gui.os_dropdown = !gui.os_dropdown; }
    }
}

// ─── Detection Meter Panel ────────────────────────────────────────────────────
fn draw_det_panel(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges, s: f32) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 8.0 * s, BG_PANEL);
      pa.rect_stroke(rect, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
      pa.text(Pos2::new(rect.center().x, rect.min.y + 11.0 * s), egui::Align2::CENTER_CENTER,
          "Detect", FontId::new(8.0 * s, FontFamily::Proportional), TEXT_SEC); }

    let cx = rect.center().x;
    let mt = rect.min.y + 44.0 * s;
    let mh = rect.height() - 82.0 * s;
    let mw = 12.0 * s; let sw = 12.0 * s;
    let sx = cx - (mw + sw + 4.0 * s) * 0.5;

    let max_r = Rect::from_center_size(Pos2::new(cx, rect.min.y + 28.0 * s), Vec2::new(rect.width() - 14.0 * s, 13.0 * s));
    if ui.allocate_rect(max_r, Sense::click()).clicked() { ch.detection_max_reset = true; }

    let sl_r = Rect::from_min_size(Pos2::new(sx + mw + 4.0 * s, mt), Vec2::new(sw, mh));
    let sr   = ui.allocate_rect(sl_r, Sense::drag());
    if sr.dragged() {
        let n = (((p.threshold + 60.0) / 60.0) as f32 - sr.drag_delta().y / mh).clamp(0.0, 1.0);
        ch.threshold = Some(-60.0 + n as f64 * 60.0);
    }

    { let pa = ui.painter_at(rect);
      // Peak hold pill
      pa.rect_filled(max_r, 4.0 * s, BG_INSET);
      pa.rect_stroke(max_r, 4.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
      pa.text(max_r.center(), egui::Align2::CENTER_CENTER, format!("{:.1}", p.detection_max_db),
          FontId::new(7.5 * s, FontFamily::Proportional), TEXT_SEC);

      let mr = Rect::from_min_size(Pos2::new(sx, mt), Vec2::new(mw, mh));
      clean_meter(&pa, mr, p.detection_db, -60.0, 0.0, s);

      for db in [-60_i32, -48, -36, -24, -12, 0] {
          let y = mt + mh * (1.0 - ((db as f32 + 60.0) / 60.0));
          pa.line_segment([Pos2::new(mr.min.x - 3.0 * s, y), Pos2::new(mr.min.x, y)],
              Stroke::new(0.5 * s, TEXT_TER));
      }

      // Threshold slider track
      pa.rect_filled(sl_r, 3.0 * s, BG_INSET);
      pa.rect_stroke(sl_r, 3.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
      let tn = ((p.threshold + 60.0) / 60.0).clamp(0.0, 1.0) as f32;
      let ty = mt + mh * (1.0 - tn);
      // Thumb — clean rounded pill
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, ty), Vec2::new(sw - 2.0 * s, 8.0 * s)), 3.0 * s, ACCENT);

      pa.text(Pos2::new(cx, mt + mh + 11.0 * s), egui::Align2::CENTER_CENTER,
          format!("{:.1} dB", p.threshold), FontId::new(7.0 * s, FontFamily::Proportional), TEXT_SEC);
    }
}

fn draw_red_panel(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges, s: f32) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 8.0 * s, BG_PANEL);
      pa.rect_stroke(rect, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
      pa.text(Pos2::new(rect.center().x, rect.min.y + 11.0 * s), egui::Align2::CENTER_CENTER,
          "Reduce", FontId::new(8.0 * s, FontFamily::Proportional), TEXT_SEC); }

    let cx = rect.center().x;
    let mt = rect.min.y + 44.0 * s;
    let mh = rect.height() - 82.0 * s;
    let mw = 12.0 * s; let sw = 12.0 * s;
    let sx = cx - (mw + sw + 4.0 * s) * 0.5;

    let max_r = Rect::from_center_size(Pos2::new(cx, rect.min.y + 28.0 * s), Vec2::new(rect.width() - 14.0 * s, 13.0 * s));
    if ui.allocate_rect(max_r, Sense::click()).clicked() { ch.reduction_max_reset = true; }

    let sl_r = Rect::from_min_size(Pos2::new(sx, mt), Vec2::new(sw, mh));
    let sr   = ui.allocate_rect(sl_r, Sense::drag());
    if sr.dragged() {
        let n = ((p.max_reduction / 40.0) as f32 - sr.drag_delta().y / mh).clamp(0.0, 1.0);
        ch.max_reduction = Some(n as f64 * 40.0);
    }

    { let pa = ui.painter_at(rect);
      pa.rect_filled(max_r, 4.0 * s, BG_INSET);
      pa.rect_stroke(max_r, 4.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
      pa.text(max_r.center(), egui::Align2::CENTER_CENTER, format!("{:.1}", p.reduction_max_db),
          FontId::new(7.5 * s, FontFamily::Proportional), TEXT_SEC);

      pa.rect_filled(sl_r, 3.0 * s, BG_INSET);
      pa.rect_stroke(sl_r, 3.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
      let mrn = (p.max_reduction / 40.0).clamp(0.0, 1.0) as f32;
      let mry = mt + mh * (1.0 - mrn);
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, mry), Vec2::new(sw - 2.0 * s, 8.0 * s)), 3.0 * s, ORANGE);

      let mr2 = Rect::from_min_size(Pos2::new(sx + sw + 4.0 * s, mt), Vec2::new(mw, mh));
      pa.rect_filled(mr2, 3.0 * s, BG_INSET);
      pa.rect_stroke(mr2, 3.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
      let rn = (-p.reduction_db / 40.0).clamp(0.0, 1.0);
      let fh = mr2.height() * rn;
      if fh > 0.5 {
          let fr = Rect::from_min_size(mr2.min, Vec2::new(mr2.width(), fh));
          let col = lerp_c(ORANGE, RED, rn);
          pa.rect_filled(fr, 2.0 * s, ga(col, 200));
      }
      for db in [0_i32, -10, -20, -30, -40] {
          let y = mt + mh * (-db as f32 / 40.0);
          pa.text(Pos2::new(mr2.max.x + 4.0 * s, y), egui::Align2::LEFT_CENTER,
              format!("{db}"), FontId::new(6.0 * s, FontFamily::Proportional), TEXT_TER);
      }
      pa.text(Pos2::new(cx, mt + mh + 11.0 * s), egui::Align2::CENTER_CENTER,
          format!("{:.1} dB", p.max_reduction), FontId::new(7.0 * s, FontFamily::Proportional), TEXT_SEC);
    }
}

fn clean_meter(pa: &egui::Painter, rect: Rect, db: f32, min_db: f32, max_db: f32, s: f32) {
    pa.rect_filled(rect, 3.0 * s, BG_INSET);
    pa.rect_stroke(rect, 3.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
    let n = ((db - min_db) / (max_db - min_db)).clamp(0.0, 1.0);
    let fh = rect.height() * n;
    if fh > 0.5 {
        let fr = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y - fh), Vec2::new(rect.width(), fh));
        let col = if db > -12.0 { M_RED } else if db > -24.0 { M_YELLOW } else { M_GREEN };
        pa.rect_filled(fr, 2.0 * s, ga(col, 210));
        // Peak line
        if fh > 3.0 {
            let pk = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y - fh), Vec2::new(rect.width(), 1.5 * s));
            pa.rect_filled(pk, 0.0, col);
        }
    }
}

// ─── Controls Panel ──────────────────────────────────────────────────────────
fn draw_controls(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges, gui: &mut NebulaGui, s: f32) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 8.0 * s, BG_PANEL);
      pa.rect_stroke(rect, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside); }

    let inner = rect.shrink(8.0 * s);
    let top_h = 60.0 * s;
    let kh    = 74.0 * s;
    let btn_h = 22.0 * s;
    let gap   = 6.0  * s;

    let cw = inner.width() / 5.0;
    let cols: Vec<Rect> = (0..5).map(|i|
        Rect::from_min_size(Pos2::new(inner.min.x + i as f32 * cw, inner.min.y), Vec2::new(cw - 4.0 * s, top_h))).collect();

    if let Some(i) = seg2(ui, cols[0], "Mode",      ["Relative", "Absolute"], if p.mode_relative{0}else{1}, s) { push_undo(gui,p); ch.mode_relative=Some(i==0); }
    if let Some(i) = seg2(ui, cols[1], "Range",     ["Split", "Wide"],        if p.use_wide_range{1}else{0}, s) { push_undo(gui,p); ch.use_wide_range=Some(i==1); }
    if let Some(i) = seg2(ui, cols[2], "Filter",    ["Lowpass", "Peak"],      if p.use_peak_filter{1}else{0}, s) { push_undo(gui,p); ch.use_peak_filter=Some(i==1); }
    if let Some(i) = seg2(ui, cols[3], "Sidechain", ["Internal", "External"], if p.sidechain_external{1}else{0}, s) { push_undo(gui,p); ch.sidechain_external=Some(i==1); }
    if let Some(i) = seg2(ui, cols[4], "Vocal",     ["Off", "On"],            if p.vocal_mode{1}else{0}, s) { push_undo(gui,p); ch.vocal_mode=Some(i==1); }

    let y2 = inner.min.y + top_h + gap;
    let main_k: &[(&str, f64, f64, f64, &str, NumTarget)] = &[
        ("Threshold",  p.threshold,    -60.0,  0.0,   "dB", NumTarget::Threshold),
        ("Max Red",    p.max_reduction,  0.0, 40.0,   "dB", NumTarget::MaxReduction),
        ("Min Freq",   p.min_freq,    1000.0,16000.0, "Hz", NumTarget::MinFreq),
        ("Max Freq",   p.max_freq,    1000.0,20000.0, "Hz", NumTarget::MaxFreq),
        ("Lookahead",  p.lookahead_ms,  0.0, 20.0,   "ms", NumTarget::Lookahead),
        ("Stereo Lnk", p.stereo_link,   0.0,  1.0,   "%",  NumTarget::StereoLink),
    ];
    knob_row(ui, rect, inner, y2, kh, main_k, ch, gui, p, ACCENT, s);

    let y2b = y2 + kh + gap;
    { let pa = ui.painter_at(rect);
      pa.line_segment([Pos2::new(inner.min.x + 16.0 * s, y2b - s), Pos2::new(inner.max.x - 16.0 * s, y2b - s)],
          Stroke::new(0.5 * s, SEP)); }

    let cut_k: &[(&str, f64, f64, f64, &str, NumTarget)] = &[
        ("Cut Width", p.cut_width, 0.0, 1.0, "%", NumTarget::CutWidth),
        ("Cut Depth", p.cut_depth, 0.0, 1.0, "%", NumTarget::CutDepth),
        ("Mix",       p.mix,       0.0, 1.0, "%", NumTarget::Mix),
    ];
    let cut_inner = Rect::from_min_size(
        Pos2::new(inner.min.x + inner.width() * 0.2, y2b),
        Vec2::new(inner.width() * 0.6, kh));
    knob_row(ui, rect, cut_inner, y2b, kh, cut_k, ch, gui, p, TEAL, s);

    let y3 = y2b + kh + gap;
    { let pa = ui.painter_at(rect);
      pa.line_segment([Pos2::new(inner.min.x + 16.0 * s, y3 - s), Pos2::new(inner.max.x - 16.0 * s, y3 - s)],
          Stroke::new(0.5 * s, SEP)); }

    let io_k: &[(&str, f64, f64, f64, &str, NumTarget)] = &[
        ("In Level",  p.input_level,  -60.0, 12.0, "dB",  NumTarget::InputLevel),
        ("In Pan",    p.input_pan,     -1.0,  1.0, "pan", NumTarget::InputPan),
        ("Out Level", p.output_level, -60.0, 12.0, "dB",  NumTarget::OutputLevel),
        ("Out Pan",   p.output_pan,   -1.0,  1.0, "pan", NumTarget::OutputPan),
    ];
    let io_inner = Rect::from_min_size(
        Pos2::new(inner.min.x + inner.width() * 0.1, y3),
        Vec2::new(inner.width() * 0.8, kh));
    knob_row(ui, rect, io_inner, y3, kh, io_k, ch, gui, p, GREEN, s);

    let y4 = y3 + kh + gap;
    let btns: &[(&str, bool)] = &[
        ("Filter Solo",  p.filter_solo),
        ("Trigger Hear", p.trigger_hear),
        ("Lookahead",    p.lookahead_enabled),
        ("Mid / Side",   p.stereo_mid_side),
    ];
    let bw = inner.width() / btns.len() as f32 - 4.0 * s;
    for (i, (lbl, active)) in btns.iter().enumerate() {
        let bx = inner.min.x + (bw + 4.0 * s) * i as f32;
        let br = Rect::from_min_size(Pos2::new(bx, y4), Vec2::new(bw, btn_h));
        let r  = ui.allocate_rect(br, Sense::click());
        let hov = r.hovered();
        { let pa = ui.painter_at(rect);
          let (bg, fg) = if *active {
              (ga(ACCENT, 220), BG_BASE)
          } else if hov {
              (ga(ACCENT, 22), ACCENT)
          } else {
              (BG_RAISED, TEXT_SEC)
          };
          pa.rect_filled(br, btn_h * 0.5, bg);
          if !*active && !hov {
              pa.rect_stroke(br, btn_h * 0.5, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
          }
          pa.text(br.center(), egui::Align2::CENTER_CENTER, *lbl,
              FontId::new(7.0 * s, FontFamily::Proportional), fg); }
        if r.clicked() {
            push_undo(gui, p);
            match *lbl {
                "Filter Solo"  => ch.filter_solo       = Some(!active),
                "Trigger Hear" => ch.trigger_hear      = Some(!active),
                "Lookahead"    => ch.lookahead_enabled = Some(!active),
                "Mid / Side"   => ch.stereo_mid_side   = Some(!active),
                _ => {}
            }
        }
    }
}

// ─── Segmented Control (replaces sec2) ───────────────────────────────────────
fn seg2(ui: &mut Ui, rect: Rect, hdr: &str, labs: [&str; 2], ai: usize, s: f32) -> Option<usize> {
    { let pa = ui.painter_at(rect);
      pa.text(Pos2::new(rect.center().x, rect.min.y + 7.0 * s), egui::Align2::CENTER_CENTER,
          hdr, FontId::new(6.5 * s, FontFamily::Proportional), TEXT_TER); }

    // Draw the segmented track background
    let track_h = 15.0 * s;
    let track = Rect::from_min_size(
        Pos2::new(rect.min.x, rect.min.y + 14.0 * s),
        Vec2::new(rect.width(), track_h * 2.0 + 2.0 * s));
    { let pa = ui.painter_at(rect);
      pa.rect_filled(track, 4.0 * s, BG_INSET);
      pa.rect_stroke(track, 4.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside); }

    let mut res = None;
    for (i, lbl) in labs.iter().enumerate() {
        let br = Rect::from_min_size(
            Pos2::new(rect.min.x, rect.min.y + 14.0 * s + i as f32 * (track_h + 1.0 * s)),
            Vec2::new(rect.width(), track_h));
        let r  = ui.allocate_rect(br, Sense::click());
        let ia = i == ai; let hov = r.hovered();
        { let pa = ui.painter_at(rect);
          if ia {
              pa.rect_filled(br, 3.0 * s, ga(ACCENT, 220));
              pa.text(br.center(), egui::Align2::CENTER_CENTER, *lbl,
                  FontId::new(6.5 * s, FontFamily::Proportional), BG_BASE);
          } else {
              if hov { pa.rect_filled(br, 3.0 * s, ga(ACCENT, 14)); }
              pa.text(br.center(), egui::Align2::CENTER_CENTER, *lbl,
                  FontId::new(6.5 * s, FontFamily::Proportional), if hov { ACCENT } else { TEXT_SEC });
          }
        }
        if r.clicked() { res = Some(i); }
    }
    res
}

fn knob_row(
    ui: &mut Ui, rect: Rect, inner: Rect, y: f32, _h: f32,
    defs: &[(&str, f64, f64, f64, &str, NumTarget)],
    ch: &mut GuiChanges, gui: &mut NebulaGui, p: &GuiParams, col: Color32, s: f32,
) {
    let n = defs.len(); let kw = inner.width() / n as f32;
    let ks = (kw * 0.62).min(34.0 * s);
    for (i, (lbl, val, min, max, unit, tgt)) in defs.iter().enumerate() {
        let kx  = inner.min.x + kw * i as f32 + kw * 0.5;
        let kc  = Pos2::new(kx, y + 12.0 * s + ks * 0.5);
        let kr  = Rect::from_center_size(kc, Vec2::splat(ks));
        let fr  = Rect::from_center_size(Pos2::new(kx, kr.max.y + 9.0 * s), Vec2::new(kw - 10.0 * s, 13.0 * s));
        { let pa = ui.painter_at(rect);
          pa.text(Pos2::new(kx, y + 5.0 * s), egui::Align2::CENTER_CENTER,
              *lbl, FontId::new(6.5 * s, FontFamily::Proportional), TEXT_TER); }
        let resp = ui.allocate_rect(kr, Sense::drag().union(Sense::click()));
        if resp.drag_started() { gui.drag_snap = Some(ParamSnapshot::from_params(p)); }
        if resp.dragged() {
            let n = ((*val - *min) / (*max - *min)) as f32;
            let nv = (*min + (n - resp.drag_delta().y * 0.006).clamp(0.0, 1.0) as f64 * (*max - *min)).clamp(*min, *max);
            apply_ch(tgt, nv, ch);
        }
        if resp.drag_stopped() {
            if let Some(snap) = gui.drag_snap.take() {
                gui.undo_stack.push(snap); gui.undo_stack.truncate(50); gui.redo_stack.clear();
            }
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                let n = ((*val - *min) / (*max - *min)) as f32;
                let nv = (*min + (n + sc * 0.008).clamp(0.0, 1.0) as f64 * (*max - *min)).clamp(*min, *max);
                apply_ch(tgt, nv, ch);
            }
        }
        if resp.secondary_clicked() {
            gui.num_input = NumInput { open: true, label: lbl.to_string(),
                value_str: format!("{:.2}", val), target: tgt.clone(), min: *min, max: *max };
        }
        if ui.allocate_rect(fr, Sense::click()).secondary_clicked() {
            gui.num_input = NumInput { open: true, label: lbl.to_string(),
                value_str: format!("{:.2}", val), target: tgt.clone(), min: *min, max: *max };
        }
        { let pa = ui.painter_at(rect);
          draw_knob(&pa, kc, ks * 0.5, *val, *min, *max, col, s);
          let disp = fmt_knob(*val, *unit);
          draw_value_field(&pa, fr, &disp, col, s); }
    }
}

fn fmt_knob(v: f64, unit: &str) -> String {
    match unit {
        "Hz"  => if v >= 1000.0 { format!("{:.1}k", v / 1000.0) } else { format!("{:.0}", v) },
        "%"   => format!("{:.0}%", v * 100.0),
        "pan" => if v.abs() < 0.01 { "C".into() } else if v > 0.0 { format!("R{:.0}", v * 100.0) } else { format!("L{:.0}", -v * 100.0) },
        _     => format!("{:.1}", v),
    }
}

fn apply_ch(t: &NumTarget, v: f64, ch: &mut GuiChanges) {
    match t {
        NumTarget::Threshold    => ch.threshold     = Some(v),
        NumTarget::MaxReduction => ch.max_reduction = Some(v),
        NumTarget::MinFreq      => ch.min_freq      = Some(v),
        NumTarget::MaxFreq      => ch.max_freq      = Some(v),
        NumTarget::Lookahead    => ch.lookahead_ms  = Some(v),
        NumTarget::StereoLink   => ch.stereo_link   = Some(v),
        NumTarget::InputLevel   => ch.input_level   = Some(v),
        NumTarget::InputPan     => ch.input_pan     = Some(v),
        NumTarget::OutputLevel  => ch.output_level  = Some(v),
        NumTarget::OutputPan    => ch.output_pan    = Some(v),
        NumTarget::CutWidth     => ch.cut_width     = Some(v),
        NumTarget::CutDepth     => ch.cut_depth     = Some(v),
        NumTarget::Mix          => ch.mix           = Some(v),
        NumTarget::None         => {}
    }
}

fn push_undo(g: &mut NebulaGui, p: &GuiParams) {
    g.undo_stack.push(ParamSnapshot::from_params(p)); g.undo_stack.truncate(50); g.redo_stack.clear();
}

// ─── Knob — clean macOS-style ─────────────────────────────────────────────────
fn draw_knob(pa: &egui::Painter, c: Pos2, r: f32, val: f64, min: f64, max: f64, col: Color32, s: f32) {
    let norm  = ((val - min) / (max - min)).clamp(0.0, 1.0) as f32;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let angle = start + norm * sweep;

    // Outer subtle shadow ring
    pa.circle_filled(c, r + 1.5 * s, ga(Color32::BLACK, 60));
    // Body — dark raised surface
    pa.circle_filled(c, r, BG_RAISED);
    // Thin border
    pa.circle_stroke(c, r, Stroke::new(0.8 * s, BORDER));
    // Track groove
    arc(pa, c, r * 0.74, start, start + sweep, ga(col, 22), 3.0 * s);
    // Filled arc — accent colour
    if norm > 0.005 {
        arc(pa, c, r * 0.74, start, angle, ga(col, 60), 4.0 * s);
        arc(pa, c, r * 0.74, start, angle, col,         1.8 * s);
    }
    // Indicator dot — clean white dot on accent
    let ix = c.x + r * 0.50 * angle.cos();
    let iy = c.y + r * 0.50 * angle.sin();
    pa.circle_filled(Pos2::new(ix, iy), 2.2 * s, col);
    pa.circle_filled(Pos2::new(ix, iy), 1.2 * s, Color32::WHITE);
    // Centre pip
    pa.circle_filled(c, 1.8 * s, ga(col, 120));
}

fn arc(pa: &egui::Painter, c: Pos2, r: f32, a0: f32, a1: f32, col: Color32, w: f32) {
    let steps = 32; let span = a1 - a0;
    let pts: Vec<Pos2> = (0..=steps).map(|i| {
        let a = a0 + i as f32 / steps as f32 * span;
        Pos2::new(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect();
    for i in 0..pts.len() - 1 { pa.line_segment([pts[i], pts[i + 1]], Stroke::new(w, col)); }
}

fn draw_value_field(pa: &egui::Painter, rect: Rect, text: &str, col: Color32, s: f32) {
    pa.rect_filled(rect, 3.0 * s, BG_INSET);
    pa.rect_stroke(rect, 3.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
    pa.text(rect.center(), egui::Align2::CENTER_CENTER, text,
        FontId::new(7.0 * s, FontFamily::Proportional), ga(col, 200));
}

// ─── Spectrum Analyzer ───────────────────────────────────────────────────────
fn freq_to_x(freq: f32, w: f32) -> f32 {
    let lmin = 20.0_f32.log10(); let lmax = 22000.0_f32.log10();
    (freq.clamp(20.0, 22000.0).log10() - lmin) / (lmax - lmin) * w
}
fn x_to_freq(x: f32, w: f32) -> f32 {
    let lmin = 20.0_f32.log10(); let lmax = 22000.0_f32.log10();
    10.0_f32.powf(lmin + (x / w) * (lmax - lmin))
}

fn draw_spectrum(ui: &mut Ui, rect: Rect, gui: &mut NebulaGui, p: &GuiParams, ch: &mut GuiChanges, s: f32) {
    if rect.height() < 24.0 { return; }
    let pa = ui.painter_at(rect);
    pa.rect_filled(rect, 8.0 * s, BG_INSET);
    pa.rect_stroke(rect, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
    let inner = rect.shrink(5.0 * s);
    let ph    = (inner.height() - 16.0 * s).max(10.0);
    let sr    = 44100.0_f32;

    // Grid lines — very subtle
    for &db in &[-80.0_f32, -60.0, -40.0, -20.0, -10.0] {
        let ny = 1.0 - (db - (-90.0)) / 90.0;
        let y  = inner.min.y + ny * ph;
        pa.line_segment([Pos2::new(inner.min.x, y), Pos2::new(inner.max.x, y)],
            Stroke::new(0.4 * s, ga(TEXT_TER, 60)));
        pa.text(Pos2::new(inner.min.x + 3.0 * s, y - 2.0 * s), egui::Align2::LEFT_BOTTOM,
            format!("{}", db as i32), FontId::new(5.5 * s, FontFamily::Proportional), ga(TEXT_TER, 120));
    }
    for &freq in &[100.0_f32, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0] {
        let x = inner.min.x + freq_to_x(freq, inner.width());
        pa.line_segment([Pos2::new(x, inner.min.y), Pos2::new(x, inner.min.y + ph)],
            Stroke::new(0.4 * s, ga(TEXT_TER, 40)));
        let lbl = if freq >= 1000.0 { format!("{}k", (freq / 1000.0) as i32) } else { format!("{}", freq as i32) };
        pa.text(Pos2::new(x, inner.max.y - 3.0 * s), egui::Align2::CENTER_CENTER,
            lbl, FontId::new(5.5 * s, FontFamily::Proportional), TEXT_TER);
    }

    // Smooth spectrum magnitudes
    {
        let spec = gui.spectrum.lock();
        let mags = &spec.magnitudes;
        let nb   = mags.len();
        if gui.smooth_mags.len() != nb { gui.smooth_mags = vec![-90.0_f32; nb]; }
        let atk = 0.30_f32; let rel = 0.85_f32;
        for (i, &mag) in mags.iter().enumerate().take(nb) {
            let m = mag.clamp(-90.0, 0.0);
            gui.smooth_mags[i] = if m > gui.smooth_mags[i] {
                gui.smooth_mags[i] * atk + m * (1.0 - atk)
            } else {
                gui.smooth_mags[i] * rel + m * (1.0 - rel)
            };
        }
    }

    let nb = gui.smooth_mags.len();
    let fft_size = (nb - 1) * 2;
    let db_min = -90.0_f32; let db_max = 0.0_f32; let db_rng = db_max - db_min;
    let cols = inner.width() as usize;
    let mut pts: Vec<Pos2> = Vec::with_capacity(cols + 2);
    for col in 0..=cols {
        let freq  = x_to_freq(col as f32, inner.width());
        let bin_f = freq * fft_size as f32 / sr;
        let bin   = (bin_f as usize).min(nb.saturating_sub(1));
        let db    = gui.smooth_mags[bin].clamp(db_min, db_max);
        let ny    = 1.0 - (db - db_min) / db_rng;
        pts.push(Pos2::new(inner.min.x + col as f32, inner.min.y + ny * ph));
    }

    if pts.len() >= 2 {
        let bottom_y = inner.min.y + ph;
        // Fill — very subtle, single colour
        let fill_col = ga(ACCENT, 18);
        for i in 0..pts.len().saturating_sub(1) {
            let tl = pts[i]; let tr = pts[i + 1];
            let bl = Pos2::new(tl.x, bottom_y); let br2 = Pos2::new(tr.x, bottom_y);
            pa.add(egui::Shape::convex_polygon(vec![tl, tr, br2, bl], fill_col, Stroke::NONE));
        }
        // Line — two passes: glow then crisp
        for i in 0..pts.len() - 1 { pa.line_segment([pts[i], pts[i + 1]], Stroke::new(3.0 * s, ga(ACCENT, 18))); }
        for i in 0..pts.len() - 1 { pa.line_segment([pts[i], pts[i + 1]], Stroke::new(1.2 * s, ga(ACCENT, 200))); }
    }

    // Frequency band overlay
    let min_x = inner.min.x + freq_to_x(p.min_freq as f32, inner.width());
    let max_x = inner.min.x + freq_to_x(p.max_freq as f32, inner.width());
    if max_x > min_x {
        let br = Rect::from_min_max(Pos2::new(min_x, inner.min.y), Pos2::new(max_x, inner.min.y + ph));
        pa.rect_filled(br, 0.0, ga(ORANGE, 14));
        pa.line_segment([Pos2::new(min_x, inner.min.y), Pos2::new(min_x, inner.min.y + ph)],
            Stroke::new(1.2 * s, ga(TEAL, 200)));
        pa.line_segment([Pos2::new(min_x, inner.min.y), Pos2::new(min_x, inner.min.y + ph)],
            Stroke::new(4.0 * s, ga(TEAL, 30)));
        pa.line_segment([Pos2::new(max_x, inner.min.y), Pos2::new(max_x, inner.min.y + ph)],
            Stroke::new(1.2 * s, ga(ORANGE, 200)));
        pa.line_segment([Pos2::new(max_x, inner.min.y), Pos2::new(max_x, inner.min.y + ph)],
            Stroke::new(4.0 * s, ga(ORANGE, 30)));
    }

    // Draggable frequency nodes
    let node_y = inner.min.y + ph * 0.5;
    let hit_sz = 22.0 * s;
    let min_hit = Rect::from_center_size(Pos2::new(min_x, node_y), Vec2::splat(hit_sz));
    let mr = ui.allocate_rect(min_hit, Sense::drag());
    if mr.dragged() {
        let nx = (min_x + mr.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        ch.min_freq = Some((x_to_freq(nx, inner.width()) as f64).clamp(1000.0, p.max_freq - 100.0));
    }
    let max_hit = Rect::from_center_size(Pos2::new(max_x, node_y), Vec2::splat(hit_sz));
    let xr = ui.allocate_rect(max_hit, Sense::drag());
    if xr.dragged() {
        let nx = (max_x + xr.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        ch.max_freq = Some((x_to_freq(nx, inner.width()) as f64).clamp(p.min_freq + 100.0, 20000.0));
    }
    freq_node(&pa, Pos2::new(min_x, node_y), TEAL,   "Min", s);
    freq_node(&pa, Pos2::new(max_x, node_y), ORANGE, "Max", s);

    pa.text(Pos2::new(inner.min.x + 6.0 * s, inner.min.y + 7.0 * s), egui::Align2::LEFT_CENTER,
        "Spectrum", FontId::new(6.5 * s, FontFamily::Proportional), TEXT_TER);
    ui.ctx().request_repaint();
}

fn freq_node(pa: &egui::Painter, c: Pos2, col: Color32, lbl: &str, s: f32) {
    pa.circle_filled(c, 8.0 * s, ga(col, 18));
    pa.circle_filled(c, 5.5 * s, BG_RAISED);
    pa.circle_stroke(c, 5.5 * s, Stroke::new(1.2 * s, col));
    pa.text(Pos2::new(c.x, c.y - 13.0 * s), egui::Align2::CENTER_CENTER,
        lbl, FontId::new(6.0 * s, FontFamily::Proportional), col);
}

// ─── Numeric Popup ───────────────────────────────────────────────────────────
fn draw_num_popup(ctx: &Context, gui: &mut NebulaGui, ch: &mut GuiChanges, s: f32) {
    let sc  = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(220.0 * s, 108.0 * s));
    let fr  = Rect::from_center_size(Pos2::new(pop.center().x, pop.center().y - 4.0 * s), Vec2::new(180.0 * s, 24.0 * s));
    let ok  = Rect::from_center_size(Pos2::new(pop.center().x - 44.0 * s, pop.max.y - 15.0 * s), Vec2::new(68.0 * s, 20.0 * s));
    let cx_ = Rect::from_center_size(Pos2::new(pop.center().x + 44.0 * s, pop.max.y - 15.0 * s), Vec2::new(68.0 * s, 20.0 * s));
    let lbl = gui.num_input.label.clone();
    egui::Area::new(egui::Id::new("neb_num")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        { let p = ui.painter();
          p.rect_filled(sc, 0.0, Color32::from_black_alpha(160));
          p.rect_filled(pop, 10.0 * s, BG_PANEL);
          p.rect_stroke(pop, 10.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
          p.text(Pos2::new(pop.center().x, pop.min.y + 16.0 * s), egui::Align2::CENTER_CENTER,
              format!("Set  {}", lbl), FontId::new(9.5 * s, FontFamily::Proportional), TEXT_PRI);
          p.rect_filled(fr, 5.0 * s, BG_INSET);
          p.rect_stroke(fr, 5.0 * s, Stroke::new(0.8 * s, ga(ACCENT, 160)), egui::StrokeKind::Outside);
          // OK button — filled accent
          p.rect_filled(ok, ok.height() * 0.5, ga(ACCENT, 220));
          p.text(ok.center(), egui::Align2::CENTER_CENTER, "OK",
              FontId::new(8.5 * s, FontFamily::Proportional), BG_BASE);
          // Cancel — ghost
          p.rect_filled(cx_, cx_.height() * 0.5, BG_RAISED);
          p.rect_stroke(cx_, cx_.height() * 0.5, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
          p.text(cx_.center(), egui::Align2::CENTER_CENTER, "Cancel",
              FontId::new(8.5 * s, FontFamily::Proportional), TEXT_SEC); }
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(fr), |ui| {
            let te = egui::TextEdit::singleline(&mut gui.num_input.value_str)
                .font(FontId::new(10.0 * s, FontFamily::Proportional))
                .text_color(TEXT_PRI).frame(false).desired_width(fr.width());
            let r = ui.add(te); r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { apply_num(gui, ch); }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { gui.num_input.open = false; }
        if ui.allocate_rect(ok,  Sense::click()).clicked() { apply_num(gui, ch); }
        if ui.allocate_rect(cx_, Sense::click()).clicked() { gui.num_input.open = false; }
    });
}

fn apply_num(gui: &mut NebulaGui, ch: &mut GuiChanges) {
    if let Ok(v) = gui.num_input.value_str.trim().parse::<f64>() {
        let v = v.clamp(gui.num_input.min, gui.num_input.max);
        apply_ch(&gui.num_input.target, v, ch);
    }
    gui.num_input.open = false;
}

// ─── Preset Save Popup ───────────────────────────────────────────────────────
fn draw_preset_save(ctx: &Context, gui: &mut NebulaGui, p: &GuiParams, _ch: &mut GuiChanges, s: f32) {
    let sc  = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(250.0 * s, 112.0 * s));
    let fr  = Rect::from_center_size(Pos2::new(pop.center().x, pop.center().y - 4.0 * s), Vec2::new(210.0 * s, 24.0 * s));
    let ok  = Rect::from_center_size(Pos2::new(pop.center().x - 48.0 * s, pop.max.y - 15.0 * s), Vec2::new(74.0 * s, 20.0 * s));
    let cx_ = Rect::from_center_size(Pos2::new(pop.center().x + 48.0 * s, pop.max.y - 15.0 * s), Vec2::new(74.0 * s, 20.0 * s));
    egui::Area::new(egui::Id::new("neb_prsave")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        { let pa = ui.painter();
          pa.rect_filled(sc, 0.0, Color32::from_black_alpha(160));
          pa.rect_filled(pop, 10.0 * s, BG_PANEL);
          pa.rect_stroke(pop, 10.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
          pa.text(Pos2::new(pop.center().x, pop.min.y + 16.0 * s), egui::Align2::CENTER_CENTER,
              "Save Preset", FontId::new(9.5 * s, FontFamily::Proportional), TEXT_PRI);
          pa.rect_filled(fr, 5.0 * s, BG_INSET);
          pa.rect_stroke(fr, 5.0 * s, Stroke::new(0.8 * s, ga(ACCENT, 160)), egui::StrokeKind::Outside);
          pa.rect_filled(ok, ok.height() * 0.5, ga(ACCENT, 220));
          pa.text(ok.center(), egui::Align2::CENTER_CENTER, "Save",
              FontId::new(8.5 * s, FontFamily::Proportional), BG_BASE);
          pa.rect_filled(cx_, cx_.height() * 0.5, BG_RAISED);
          pa.rect_stroke(cx_, cx_.height() * 0.5, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
          pa.text(cx_.center(), egui::Align2::CENTER_CENTER, "Cancel",
              FontId::new(8.5 * s, FontFamily::Proportional), TEXT_SEC); }
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(fr), |ui| {
            let te = egui::TextEdit::singleline(&mut gui.preset_name_buf)
                .font(FontId::new(10.0 * s, FontFamily::Proportional))
                .text_color(TEXT_PRI).frame(false).desired_width(fr.width()).hint_text("Preset name…");
            let r = ui.add(te); r.request_focus();
            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) { do_save(gui, p); }
        });
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { gui.preset_save_popup = false; }
        if ui.allocate_rect(ok,  Sense::click()).clicked() { do_save(gui, p); }
        if ui.allocate_rect(cx_, Sense::click()).clicked() { gui.preset_save_popup = false; }
    });
}

fn do_save(gui: &mut NebulaGui, p: &GuiParams) {
    let name = gui.preset_name_buf.trim().to_string();
    if name.is_empty() { return; }
    let snap = ParamSnapshot::from_params(p);
    if let Some(idx) = gui.presets.iter().position(|(n, _)| n == &name) {
        gui.presets[idx].1 = snap; gui.selected_preset = idx;
    } else {
        gui.presets.push((name, snap)); gui.selected_preset = gui.presets.len() - 1;
    }
    gui.preset_save_popup = false;
}

// ─── MIDI Learn Popup ────────────────────────────────────────────────────────
fn draw_midi_popup(ctx: &Context, gui: &mut NebulaGui, s: f32) {
    let sc  = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(280.0 * s, 310.0 * s));
    egui::Area::new(egui::Id::new("neb_midi")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        { let pa = ui.painter();
          pa.rect_filled(sc, 0.0, Color32::from_black_alpha(160));
          pa.rect_filled(pop, 12.0 * s, BG_PANEL);
          pa.rect_stroke(pop, 12.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
          pa.text(Pos2::new(pop.center().x, pop.min.y + 16.0 * s), egui::Align2::CENTER_CENTER,
              "MIDI Learn", FontId::new(11.0 * s, FontFamily::Proportional), TEXT_PRI);
          pa.text(Pos2::new(pop.center().x, pop.min.y + 30.0 * s), egui::Align2::CENTER_CENTER,
              "Select a parameter, then move a CC knob",
              FontId::new(7.0 * s, FontFamily::Proportional), TEXT_SEC); }

        let learning = gui.midi_learn.learning_target.load(std::sync::atomic::Ordering::Relaxed);
        let mappings = gui.midi_learn.mappings.lock().clone();
        for (idx, &name) in MIDI_PARAM_NAMES.iter().enumerate().take(MIDI_PARAM_COUNT) {
            let cc_s: String = mappings.iter()
                .find(|(_, &v)| v == idx as u8)
                .map(|(&cc, _)| format!("CC{}", cc))
                .unwrap_or_else(|| "—".to_string());
            let ih = 21.0 * s;
            let rr = Rect::from_min_size(
                Pos2::new(pop.min.x + 10.0 * s, pop.min.y + 46.0 * s + idx as f32 * ih),
                Vec2::new(pop.width() - 20.0 * s, ih - 2.0 * s));
            let resp = ui.allocate_rect(rr, Sense::click());
            let isl  = learning == idx as i32; let hov = resp.hovered();
            { let pa = ui.painter_at(Rect::EVERYTHING);
              let (bg, fg) = if isl { (ga(ACCENT, 220), BG_BASE) }
                  else if hov { (ga(ACCENT, 18), ACCENT) }
                  else { (BG_RAISED, TEXT_PRI) };
              pa.rect_filled(rr, 4.0 * s, bg);
              if !isl && !hov {
                  pa.rect_stroke(rr, 4.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside);
              }
              pa.text(Pos2::new(rr.min.x + 8.0 * s, rr.center().y), egui::Align2::LEFT_CENTER,
                  name, FontId::new(7.5 * s, FontFamily::Proportional), fg);
              pa.text(Pos2::new(rr.max.x - 8.0 * s, rr.center().y), egui::Align2::RIGHT_CENTER,
                  &cc_s, FontId::new(7.5 * s, FontFamily::Proportional),
                  if isl { BG_BASE } else { TEXT_SEC }); }
            if resp.clicked() {
                let t = if isl { -1 } else { idx as i32 };
                gui.midi_learn.learning_target.store(t, std::sync::atomic::Ordering::Release);
            }
        }

        let clr = Rect::from_center_size(Pos2::new(pop.center().x - 52.0 * s, pop.max.y - 16.0 * s), Vec2::new(82.0 * s, 22.0 * s));
        let cls = Rect::from_center_size(Pos2::new(pop.center().x + 52.0 * s, pop.max.y - 16.0 * s), Vec2::new(82.0 * s, 22.0 * s));
        { let pa = ui.painter_at(Rect::EVERYTHING);
          pa.rect_filled(clr, clr.height() * 0.5, ga(RED, 28));
          pa.rect_stroke(clr, clr.height() * 0.5, Stroke::new(0.5 * s, ga(RED, 120)), egui::StrokeKind::Outside);
          pa.text(clr.center(), egui::Align2::CENTER_CENTER, "Clear All",
              FontId::new(7.5 * s, FontFamily::Proportional), RED);
          pa.rect_filled(cls, cls.height() * 0.5, ga(ACCENT, 220));
          pa.text(cls.center(), egui::Align2::CENTER_CENTER, "Close",
              FontId::new(7.5 * s, FontFamily::Proportional), BG_BASE); }
        if ui.allocate_rect(clr, Sense::click()).clicked() {
            gui.midi_learn.mappings.lock().clear();
            gui.midi_learn.learning_target.store(-1, std::sync::atomic::Ordering::Release);
        }
        if ui.allocate_rect(cls, Sense::click()).clicked() || ui.input(|i| i.key_pressed(egui::Key::Escape)) {
            gui.midi_learn.learning_target.store(-1, std::sync::atomic::Ordering::Release);
            gui.midi_popup = false;
        }
    });
}

// ─── MIDI Context Menu ────────────────────────────────────────────────────────
fn draw_midi_context_menu(ctx: &Context, gui: &mut NebulaGui, s: f32) {
    let menu_w = 170.0 * s; let ih = 22.0 * s;
    let items = [("MIDI On/Off", 0usize), ("Clean Up...", 1), ("Roll Back", 2), ("Save", 3), ("Close", 4)];
    let menu_h = items.len() as f32 * ih + 10.0 * s;
    let anchor = gui.midi_context_anchor;
    let menu_rect = Rect::from_min_size(anchor, Vec2::new(menu_w, menu_h));
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("neb_midi_ctx_bg")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        if ui.allocate_rect(screen, Sense::click()).clicked() { gui.midi_context_menu = false; }
    });
    egui::Area::new(egui::Id::new("neb_midi_ctx")).fixed_pos(anchor).order(egui::Order::Tooltip).show(ctx, |ui| {
        { let p = ui.painter();
          p.rect_filled(menu_rect, 8.0 * s, BG_PANEL);
          p.rect_stroke(menu_rect, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside); }
        for (i, (label, idx)) in items.iter().enumerate() {
            let item_rect = Rect::from_min_size(
                Pos2::new(anchor.x + 5.0 * s, anchor.y + 5.0 * s + i as f32 * ih),
                Vec2::new(menu_w - 10.0 * s, ih - 2.0 * s));
            let resp = ui.allocate_rect(item_rect, Sense::click());
            let hov = resp.hovered();
            { let p = ui.painter();
              if hov { p.rect_filled(item_rect, 4.0 * s, ga(ACCENT, 18)); }
              p.text(Pos2::new(item_rect.min.x + 10.0 * s, item_rect.center().y),
                  egui::Align2::LEFT_CENTER, *label,
                  FontId::new(8.0 * s, FontFamily::Proportional),
                  if hov { ACCENT } else { TEXT_PRI }); }
            if resp.clicked() {
                match idx {
                    0 => { let cur = gui.midi_learn.midi_enabled.load(std::sync::atomic::Ordering::Relaxed);
                           gui.midi_learn.midi_enabled.store(!cur, std::sync::atomic::Ordering::Release); }
                    1 => { gui.midi_cleanup_menu = true;
                           gui.midi_cleanup_anchor = Pos2::new(item_rect.max.x + 2.0 * s, item_rect.min.y); }
                    2 => { let saved = gui.midi_learn.saved_mappings.lock().clone();
                           *gui.midi_learn.mappings.lock() = saved; }
                    3 => { let cur = gui.midi_learn.mappings.lock().clone();
                           *gui.midi_learn.saved_mappings.lock() = cur; }
                    4 => { gui.midi_context_menu = false; }
                    _ => {}
                }
                if *idx != 1 { gui.midi_context_menu = false; }
            }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { gui.midi_context_menu = false; }
    });
    if gui.midi_cleanup_menu { draw_midi_cleanup_menu(ctx, gui, s); }
}

fn draw_midi_cleanup_menu(ctx: &Context, gui: &mut NebulaGui, s: f32) {
    let mappings = gui.midi_learn.mappings.lock().clone();
    let mut sorted: Vec<(u8, u8)> = mappings.iter().map(|(&cc, &p)| (cc, p)).collect();
    sorted.sort_by_key(|&(cc, _)| cc);
    let sub_w = 200.0 * s; let ih = 22.0 * s;
    let sub_h = (sorted.len() + 2) as f32 * ih + 10.0 * s;
    let anchor = gui.midi_cleanup_anchor;
    let sub_rect = Rect::from_min_size(anchor, Vec2::new(sub_w, sub_h));
    egui::Area::new(egui::Id::new("neb_midi_cleanup")).fixed_pos(anchor).order(egui::Order::Tooltip).show(ctx, |ui| {
        { let p = ui.painter();
          p.rect_filled(sub_rect, 8.0 * s, BG_PANEL);
          p.rect_stroke(sub_rect, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside); }
        if sorted.is_empty() {
            let er = Rect::from_min_size(Pos2::new(anchor.x + 5.0 * s, anchor.y + 5.0 * s), Vec2::new(sub_w - 10.0 * s, ih));
            ui.painter().text(er.center(), egui::Align2::CENTER_CENTER, "No mappings",
                FontId::new(7.5 * s, FontFamily::Proportional), TEXT_SEC);
        }
        for (i, (cc, pidx)) in sorted.iter().enumerate() {
            let pname = MIDI_PARAM_NAMES.get(*pidx as usize).copied().unwrap_or("?");
            let ir = Rect::from_min_size(
                Pos2::new(anchor.x + 5.0 * s, anchor.y + 5.0 * s + i as f32 * ih),
                Vec2::new(sub_w - 10.0 * s, ih - 2.0 * s));
            let resp = ui.allocate_rect(ir, Sense::click()); let hov = resp.hovered();
            { let p = ui.painter();
              if hov { p.rect_filled(ir, 3.0 * s, ga(RED, 14)); }
              p.text(Pos2::new(ir.min.x + 10.0 * s, ir.center().y), egui::Align2::LEFT_CENTER,
                  format!("CC{}  →  {}", cc, pname),
                  FontId::new(7.5 * s, FontFamily::Proportional),
                  if hov { RED } else { TEXT_PRI }); }
            if resp.clicked() { gui.midi_learn.mappings.lock().remove(cc); }
        }
        let clear_y = anchor.y + 5.0 * s + (sorted.len() + 1) as f32 * ih;
        let cr = Rect::from_min_size(Pos2::new(anchor.x + 5.0 * s, clear_y), Vec2::new(sub_w - 10.0 * s, ih - 2.0 * s));
        let resp = ui.allocate_rect(cr, Sense::click()); let hov = resp.hovered();
        { let p = ui.painter();
          if hov { p.rect_filled(cr, 3.0 * s, ga(RED, 20)); }
          p.text(cr.center(), egui::Align2::CENTER_CENTER, "Clear All",
              FontId::new(7.5 * s, FontFamily::Proportional),
              if hov { RED } else { TEXT_SEC }); }
        if resp.clicked() { gui.midi_learn.mappings.lock().clear(); gui.midi_cleanup_menu = false; gui.midi_context_menu = false; }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { gui.midi_cleanup_menu = false; }
    });
}

// ─── OS Dropdown ─────────────────────────────────────────────────────────────
fn draw_os_dropdown(ctx: &Context, gui: &mut NebulaGui, params: &GuiParams, ch: &mut GuiChanges, s: f32) {
    let os_labels = ["Off", "2×", "4×", "6×", "8×"];
    let os_w = 88.0 * s; let ih = 22.0 * s;
    let drop_h = os_labels.len() as f32 * ih + 6.0 * s;
    let anchor = gui.os_anchor;
    let dr = Rect::from_min_size(anchor, Vec2::new(os_w, drop_h));
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("neb_os_bg")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        if ui.allocate_rect(screen, Sense::click()).clicked() { gui.os_dropdown = false; }
    });
    egui::Area::new(egui::Id::new("neb_os_drop")).fixed_pos(anchor).order(egui::Order::Tooltip).show(ctx, |ui| {
        { let p = ui.painter();
          p.rect_filled(dr, 8.0 * s, BG_PANEL);
          p.rect_stroke(dr, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside); }
        for (i, &lbl) in os_labels.iter().enumerate() {
            let ir = Rect::from_min_size(Pos2::new(anchor.x + 3.0 * s, anchor.y + 3.0 * s + i as f32 * ih), Vec2::new(os_w - 6.0 * s, ih - 2.0 * s));
            let resp = ui.allocate_rect(ir, Sense::click());
            let isel = i == params.oversampling as usize; let hov = resp.hovered();
            { let p = ui.painter();
              if isel { p.rect_filled(ir, 4.0 * s, ga(ACCENT, 220)); }
              else if hov { p.rect_filled(ir, 4.0 * s, ga(ACCENT, 14)); }
              p.text(Pos2::new(ir.min.x + 10.0 * s, ir.center().y), egui::Align2::LEFT_CENTER,
                  lbl, FontId::new(8.5 * s, FontFamily::Proportional),
                  if isel { BG_BASE } else if hov { ACCENT } else { TEXT_PRI }); }
            if resp.clicked() { ch.oversampling = Some(i as u32); gui.os_dropdown = false; }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { gui.os_dropdown = false; }
    });
}

// ─── Preset Dropdown ─────────────────────────────────────────────────────────
fn draw_preset_dropdown(ctx: &Context, gui: &mut NebulaGui, ch: &mut GuiChanges, s: f32) {
    if gui.presets.is_empty() { gui.preset_dropdown_open = false; return; }
    let pw = 148.0 * s; let ih = 22.0 * s;
    let drop_h = gui.presets.len() as f32 * ih + 6.0 * s;
    let anchor = gui.preset_anchor;
    let dr = Rect::from_min_size(anchor, Vec2::new(pw, drop_h));
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("neb_pr_bg")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        if ui.allocate_rect(screen, Sense::click()).clicked() { gui.preset_dropdown_open = false; }
    });
    let presets_clone = gui.presets.clone();
    egui::Area::new(egui::Id::new("neb_pr_drop")).fixed_pos(anchor).order(egui::Order::Tooltip).show(ctx, |ui| {
        { let p = ui.painter();
          p.rect_filled(dr, 8.0 * s, BG_PANEL);
          p.rect_stroke(dr, 8.0 * s, Stroke::new(0.5 * s, BORDER), egui::StrokeKind::Outside); }
        for (i, (name, snap)) in presets_clone.iter().enumerate() {
            let ir = Rect::from_min_size(Pos2::new(anchor.x + 3.0 * s, anchor.y + 3.0 * s + i as f32 * ih), Vec2::new(pw - 6.0 * s, ih - 2.0 * s));
            let resp = ui.allocate_rect(ir, Sense::click());
            let isel = i == gui.selected_preset; let hov = resp.hovered();
            { let p = ui.painter();
              if isel { p.rect_filled(ir, 4.0 * s, ga(ACCENT, 220)); }
              else if hov { p.rect_filled(ir, 4.0 * s, ga(ACCENT, 14)); }
              let display = if name.len() > 20 { &name[..20] } else { name };
              p.text(Pos2::new(ir.min.x + 10.0 * s, ir.center().y), egui::Align2::LEFT_CENTER,
                  display, FontId::new(8.0 * s, FontFamily::Proportional),
                  if isel { BG_BASE } else if hov { ACCENT } else { TEXT_PRI }); }
            if resp.clicked() { gui.selected_preset = i; gui.preset_dropdown_open = false; snap.apply_to(ch); }
        }
        if ui.input(|i| i.key_pressed(egui::Key::Escape)) { gui.preset_dropdown_open = false; }
    });
}
