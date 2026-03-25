// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser v2.2.0 — Alien Synthwave GUI
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

// ─── Palette ─────────────────────────────────────────────────────────────────
const BG_VOID:    Color32 = Color32::from_rgb(2,   1,  10);
const BG_PANEL:   Color32 = Color32::from_rgb(7,   4,  20);
const BG_DEEP:    Color32 = Color32::from_rgb(10,  5,  26);
const BG_WIDGET:  Color32 = Color32::from_rgb(14,  8,  34);
const CYAN:       Color32 = Color32::from_rgb(0,  230, 255);
const MAGENTA:    Color32 = Color32::from_rgb(255,  0, 200);
const PURPLE:     Color32 = Color32::from_rgb(140,  0, 255);
const GOLD:       Color32 = Color32::from_rgb(255, 200,   0);
const RED_HOT:    Color32 = Color32::from_rgb(255,  55,  55);
const GREEN_NEON: Color32 = Color32::from_rgb(0,   220,  90);
const M_BLUE:     Color32 = Color32::from_rgb(0,   100, 220);
const M_YELLOW:   Color32 = Color32::from_rgb(255, 200,   0);
const M_RED:      Color32 = Color32::from_rgb(255,  40,  40);
const GRID:       Color32 = Color32::from_rgba_premultiplied(0, 200, 255, 14);
const TEXT_HI:    Color32 = Color32::from_rgb(210, 230, 255);
const TEXT_MID:   Color32 = Color32::from_rgb(110, 140, 180);
const TEXT_LO:    Color32 = Color32::from_rgb(50,   70, 110);

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

    // Compute scale from EguiState logical size — stable, no feedback loop.
    // All hardcoded pixel constants in every draw_* function are multiplied by `s`.
    let (win_w, win_h) = egui_state.size();
    let s = (win_w as f32 / BASE_W).min(win_h as f32 / BASE_H).max(0.25);

    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = BG_VOID;
    style.visuals.override_text_color = Some(TEXT_HI);
    style.visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    style.visuals.widgets.inactive.bg_fill = BG_DEEP;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0, 35, 55);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.5 * s, CYAN);
    style.spacing.item_spacing = Vec2::new(4.0 * s, 3.0 * s);
    ctx.set_style(style);

    ResizableWindow::new("nebula_deesser_resize")
        .min_size(Vec2::new(400.0, 300.0))
        .show(ctx, egui_state, |ui| {
            let full = ui.max_rect();
            draw_bg(ui.painter_at(full), full, gui.time, params.bypass, s);

            let title_h   = 46.0 * s;
            let toolbar_h = 30.0 * s;
            let margin    = 8.0  * s;

            draw_title(ui.painter_at(full), full, params.bypass, s);
            draw_toolbar(ui, Rect::from_min_size(
                Pos2::new(full.min.x, full.min.y + title_h),
                Vec2::new(full.width(), toolbar_h)), params, gui, &mut ch, s);

            let content = Rect::from_min_size(
                Pos2::new(full.min.x + margin, full.min.y + title_h + toolbar_h + margin),
                Vec2::new(full.width() - margin*2.0, full.height() - title_h - toolbar_h - margin*2.0));

            let mw = 88.0 * s; let gap = 6.0 * s;
            let cw = content.width() - mw * 2.0 - gap * 2.0;

            let left_r  = Rect::from_min_size(content.min, Vec2::new(mw, content.height()));
            let right_r = Rect::from_min_size(Pos2::new(content.max.x - mw, content.min.y), Vec2::new(mw, content.height()));
            let ctr_r   = Rect::from_min_size(Pos2::new(content.min.x + mw + gap, content.min.y), Vec2::new(cw, content.height()));

            let spec_frac = 0.38_f32;
            let ctrl_h = ctr_r.height() * (1.0 - spec_frac);
            let ctrl_r = Rect::from_min_size(ctr_r.min, Vec2::new(cw, ctrl_h));
            let spec_r = Rect::from_min_size(
                Pos2::new(ctr_r.min.x, ctr_r.min.y + ctrl_h + 4.0 * s),
                Vec2::new(cw, ctr_r.height() * spec_frac - 4.0 * s));

            draw_det_panel(ui, left_r,  params, &mut ch, s);
            draw_red_panel(ui, right_r, params, &mut ch, s);
            draw_controls(ui, ctrl_r, params, &mut ch, gui, s);
            draw_spectrum(ui, spec_r, gui, params, &mut ch, s);
        });

    if gui.num_input.open    { draw_num_popup(ctx, gui, &mut ch, s); }
    if gui.preset_save_popup { draw_preset_save(ctx, gui, params, &mut ch, s); }
    if gui.midi_popup        { draw_midi_popup(ctx, gui, s); }
    if gui.os_dropdown       { draw_os_dropdown(ctx, gui, params, &mut ch, s); }
    if gui.preset_dropdown_open { draw_preset_dropdown(ctx, gui, &mut ch, s); }
    if gui.midi_context_menu { draw_midi_context_menu(ctx, gui, s); }
    ch
}

// ─── Background ──────────────────────────────────────────────────────────────
fn draw_bg(painter: egui::Painter, rect: Rect, time: f64, bypass: bool, s: f32) {
    let sp = 38.0 * s;
    let ox = (time as f32 * 9.0).rem_euclid(sp);
    let oy = (time as f32 * 4.5).rem_euclid(sp);
    let mut x = rect.min.x - ox;
    while x < rect.max.x + sp { painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], Stroke::new(0.35 * s, GRID)); x += sp; }
    let mut y = rect.min.y - oy;
    while y < rect.max.y + sp { painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], Stroke::new(0.35 * s, GRID)); y += sp; }
    let vr = rect.width().min(rect.height()) * 0.4;
    for corner in [rect.min, Pos2::new(rect.max.x, rect.min.y), rect.max, Pos2::new(rect.min.x, rect.max.y)] {
        painter.circle_filled(corner, vr, Color32::from_rgba_premultiplied(0,0,0,18));
    }
    if bypass { painter.rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(60,0,0,22)); }
}

// ─── Title Bar ───────────────────────────────────────────────────────────────
fn draw_title(painter: egui::Painter, rect: Rect, bypass: bool, s: f32) {
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 46.0 * s));
    painter.rect_filled(bar, 0.0, BG_PANEL);
    let col = if bypass { RED_HOT } else { CYAN };
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(3.0 * s, col));
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(10.0 * s, ga(col, 28)));
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(20.0 * s, ga(col, 10)));
    let tx = bar.min.x + 15.0 * s; let ty = bar.center().y;
    painter.text(Pos2::new(tx+s, ty+s), egui::Align2::LEFT_CENTER,
        "NEBULA  DE-ESSER", FontId::new(18.0 * s, FontFamily::Monospace), ga(CYAN, 25));
    painter.text(Pos2::new(tx, ty), egui::Align2::LEFT_CENTER,
        "NEBULA  DE-ESSER", FontId::new(18.0 * s, FontFamily::Monospace), CYAN);
    painter.text(Pos2::new(bar.min.x + 238.0 * s, ty + 1.5 * s), egui::Align2::LEFT_CENTER,
        "ALIEN SPECTRUM PROCESSOR  //  64-BIT  CLAP",
        FontId::new(8.0 * s, FontFamily::Monospace), TEXT_LO);
    painter.text(Pos2::new(bar.max.x - 12.0 * s, ty), egui::Align2::RIGHT_CENTER,
        "v2.2", FontId::new(8.5 * s, FontFamily::Monospace), ga(PURPLE, 210));
    if bypass {
        let bx = bar.max.x - 74.0 * s;
        let br = Rect::from_center_size(Pos2::new(bx, ty), Vec2::new(56.0 * s, 16.0 * s));
        painter.rect_filled(br, 4.0 * s, ga(RED_HOT, 40));
        painter.rect_stroke(br, 4.0 * s, Stroke::new(s, RED_HOT), egui::StrokeKind::Outside);
        painter.text(br.center(), egui::Align2::CENTER_CENTER,
            "BYPASSED", FontId::new(7.5 * s, FontFamily::Monospace), RED_HOT);
    }
}

// ─── Toolbar ─────────────────────────────────────────────────────────────────
fn draw_toolbar(ui: &mut Ui, rect: Rect, params: &GuiParams, gui: &mut NebulaGui, ch: &mut GuiChanges, s: f32) {
    { let p = ui.painter_at(rect);
      p.rect_filled(rect, 0.0, Color32::from_rgb(5, 3, 16));
      p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(s, ga(PURPLE, 55))); }

    let cy = rect.center().y; let bh = 19.0 * s;
    let mut cx = rect.min.x + 8.0 * s;

    macro_rules! tbtn {
        ($label:expr, $active:expr, $col:expr, $w:expr) => {{
            let w = $w * s;
            let r = Rect::from_min_max(Pos2::new(cx, cy-bh*0.5), Pos2::new(cx+w, cy+bh*0.5));
            cx += w + 5.0 * s;
            let resp = ui.allocate_rect(r, Sense::click());
            let hov  = resp.hovered();
            let c    = if $active { $col } else if hov { ga($col, 180) } else { ga($col, 95) };
            let bg   = if $active { ga($col, 32) } else if hov { ga($col, 12) } else { BG_WIDGET };
            { let p = ui.painter_at(rect);
              p.rect_filled(r, 4.0 * s, bg);
              p.rect_stroke(r, 4.0 * s, Stroke::new(if $active { 1.3 * s } else { 0.8 * s }, c), egui::StrokeKind::Outside);
              if $active { p.rect_stroke(r, 4.0 * s, Stroke::new(5.0 * s, ga(c, 25)), egui::StrokeKind::Outside); }
              p.text(r.center(), egui::Align2::CENTER_CENTER, $label, FontId::new(7.5 * s, FontFamily::Monospace), c); }
            resp
        }};
    }

    if tbtn!(if params.bypass {"⊗ BYPASSED"} else {"⊗ BYPASS"}, params.bypass, RED_HOT, 72.0).clicked() {
        ch.bypass = Some(!params.bypass);
    }
    cx += 3.0 * s;

    let pw = 138.0 * s;
    { let pr = Rect::from_min_max(Pos2::new(cx, cy-bh*0.5), Pos2::new(cx+pw, cy+bh*0.5));
      cx += pw + 5.0 * s;
      let resp = ui.allocate_rect(pr, Sense::click());
      let hov  = resp.hovered();
      let lbl  = if gui.presets.is_empty() { "PRESET  ─".to_string() }
          else { let n = &gui.presets[gui.selected_preset.min(gui.presets.len()-1)].0;
                 format!("▾ {}", if n.len()>15 { &n[..15] } else { n }) };
      { let p = ui.painter_at(rect);
        p.rect_filled(pr, 4.0 * s, BG_WIDGET);
        p.rect_stroke(pr, 4.0 * s, Stroke::new(if hov {1.2*s} else {0.8*s}, if hov {CYAN} else {ga(CYAN,60)}), egui::StrokeKind::Outside);
        p.text(pr.center(), egui::Align2::CENTER_CENTER, &lbl, FontId::new(7.5 * s, FontFamily::Monospace), if hov {CYAN} else {ga(CYAN,150)}); }
      gui.preset_anchor = Pos2::new(pr.min.x, pr.max.y + 2.0 * s);
      if resp.clicked() { gui.preset_dropdown_open = !gui.preset_dropdown_open; }
    }

    if tbtn!("SAVE", false, GOLD, 42.0).clicked() {
        gui.preset_name_buf.clear(); gui.preset_save_popup=true; gui.preset_dropdown_open=false;
    }
    if tbtn!("DEL", false, MAGENTA, 34.0).clicked() && !gui.presets.is_empty() {
        gui.presets.remove(gui.selected_preset.min(gui.presets.len()-1));
        if gui.selected_preset > 0 { gui.selected_preset -= 1; }
    }
    cx += 3.0 * s;

    let can_undo = !gui.undo_stack.is_empty();
    let can_redo = !gui.redo_stack.is_empty();
    { let w = 46.0 * s;
      let r = Rect::from_min_max(Pos2::new(cx,cy-bh*0.5), Pos2::new(cx+w,cy+bh*0.5)); cx += w + 5.0*s;
      let resp = ui.allocate_rect(r, Sense::click());
      let c = if can_undo { if resp.hovered() {CYAN} else {ga(CYAN,150)} } else {ga(TEXT_LO,70)};
      { let p = ui.painter_at(rect);
        p.rect_filled(r, 4.0*s, BG_WIDGET);
        p.rect_stroke(r, 4.0*s, Stroke::new(0.8*s, ga(c,120)), egui::StrokeKind::Outside);
        p.text(r.center(), egui::Align2::CENTER_CENTER, "◄ UNDO", FontId::new(7.5*s, FontFamily::Monospace), c); }
      if resp.clicked() && can_undo {
          let snap = gui.undo_stack.pop().unwrap();
          gui.redo_stack.push(ParamSnapshot::from_params(params)); gui.redo_stack.truncate(50);
          snap.apply_to(ch);
      }
    }
    { let w = 46.0 * s;
      let r = Rect::from_min_max(Pos2::new(cx,cy-bh*0.5), Pos2::new(cx+w,cy+bh*0.5)); cx += w + 5.0*s;
      let resp = ui.allocate_rect(r, Sense::click());
      let c = if can_redo { if resp.hovered() {CYAN} else {ga(CYAN,150)} } else {ga(TEXT_LO,70)};
      { let p = ui.painter_at(rect);
        p.rect_filled(r, 4.0*s, BG_WIDGET);
        p.rect_stroke(r, 4.0*s, Stroke::new(0.8*s, ga(c,120)), egui::StrokeKind::Outside);
        p.text(r.center(), egui::Align2::CENTER_CENTER, "REDO ►", FontId::new(7.5*s, FontFamily::Monospace), c); }
      if resp.clicked() && can_redo {
          let snap = gui.redo_stack.pop().unwrap();
          gui.undo_stack.push(ParamSnapshot::from_params(params)); gui.undo_stack.truncate(50);
          snap.apply_to(ch);
      }
    }
    cx += 3.0 * s;

    let ab_label = if gui.active_state == 'A' { "A/B  A" } else { "A/B  B" };
    let ab_active = gui.state_a.is_some() || gui.state_b.is_some();
    let ab_resp = tbtn!(ab_label, ab_active, GREEN_NEON, 58.0);
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
    cx += 3.0 * s;

    let learning = gui.midi_learn.learning_target.load(std::sync::atomic::Ordering::Relaxed) >= 0;
    let midi_btn = tbtn!(if learning {"● LEARNING"} else {"MIDI LEARN"}, learning, MAGENTA, 86.0);
    if midi_btn.clicked() {
        if learning { gui.midi_learn.learning_target.store(-1, std::sync::atomic::Ordering::Release); }
        else { gui.midi_popup = true; }
    }
    if midi_btn.secondary_clicked() {
        gui.midi_context_menu = true;
        gui.midi_context_anchor = Pos2::new(midi_btn.rect.min.x, midi_btn.rect.max.y + 2.0 * s);
    }
    cx += 3.0 * s;

    let os_labels = ["OFF", "2×", "4×", "6×", "8×"];
    let cur = os_labels.get(params.oversampling as usize).copied().unwrap_or("OFF");
    let os_w = 94.0 * s;
    { let or_ = Rect::from_min_max(Pos2::new(cx,cy-bh*0.5), Pos2::new(cx+os_w,cy+bh*0.5));
      gui.os_anchor = Pos2::new(or_.min.x, or_.max.y + 2.0 * s);
      let resp = ui.allocate_rect(or_, Sense::click());
      let active = params.oversampling > 0;
      let c = if active {GOLD} else if resp.hovered() {ga(GOLD,180)} else {ga(GOLD,85)};
      { let p = ui.painter_at(rect);
        p.rect_filled(or_, 4.0*s, if active {ga(GOLD,18)} else {BG_WIDGET});
        p.rect_stroke(or_, 4.0*s, Stroke::new(if active {1.2*s} else {0.8*s}, c), egui::StrokeKind::Outside);
        if active { p.rect_stroke(or_, 4.0*s, Stroke::new(5.0*s, ga(c,20)), egui::StrokeKind::Outside); }
        p.text(or_.center(), egui::Align2::CENTER_CENTER,
            format!("OS  {}  ▾", cur), FontId::new(7.5*s, FontFamily::Monospace), c); }
      if resp.clicked() { gui.os_dropdown = !gui.os_dropdown; }
    }
}

// ─── Detection Meter Panel ────────────────────────────────────────────────────
fn draw_det_panel(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges, s: f32) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 6.0*s, BG_PANEL);
      pa.rect_stroke(rect, 6.0*s, Stroke::new(s, ga(CYAN,45)), egui::StrokeKind::Outside);
      pa.rect_stroke(rect, 6.0*s, Stroke::new(4.0*s, ga(CYAN,12)), egui::StrokeKind::Outside);
      pa.text(Pos2::new(rect.center().x, rect.min.y+10.0*s), egui::Align2::CENTER_CENTER,
          "DETECT", FontId::new(8.0*s, FontFamily::Monospace), ga(CYAN,200)); }
    let cx = rect.center().x;
    let mt = rect.min.y + 44.0*s; let mh = rect.height() - 82.0*s;
    let mw = 14.0*s; let sw = 14.0*s;
    let sx = cx - (mw+sw+4.0*s)*0.5;
    let max_r = Rect::from_center_size(Pos2::new(cx, rect.min.y+28.0*s), Vec2::new(rect.width()-14.0*s, 13.0*s));
    if ui.allocate_rect(max_r, Sense::click()).clicked() { ch.detection_max_reset = true; }
    let sl_r = Rect::from_min_size(Pos2::new(sx+mw+4.0*s, mt), Vec2::new(sw, mh));
    let sr   = ui.allocate_rect(sl_r, Sense::drag());
    if sr.dragged() {
        let n = (((p.threshold+60.0)/60.0) as f32 - sr.drag_delta().y/mh).clamp(0.0,1.0);
        ch.threshold = Some(-60.0 + n as f64 * 60.0);
    }
    { let pa = ui.painter_at(rect);
      pa.rect_filled(max_r, 3.0*s, BG_DEEP);
      pa.rect_stroke(max_r, 3.0*s, Stroke::new(0.8*s, ga(M_YELLOW,110)), egui::StrokeKind::Outside);
      pa.text(max_r.center(), egui::Align2::CENTER_CENTER, format!("{:.1}", p.detection_max_db),
          FontId::new(7.5*s, FontFamily::Monospace), M_YELLOW);
      let mr = Rect::from_min_size(Pos2::new(sx, mt), Vec2::new(mw, mh));
      fancy_meter(&pa, mr, p.detection_db, -60.0, 0.0, s);
      for db in [-60_i32,-48,-36,-24,-12,0] {
          let y = mt + mh*(1.0-((db as f32+60.0)/60.0));
          pa.line_segment([Pos2::new(mr.min.x-3.0*s,y), Pos2::new(mr.min.x,y)], Stroke::new(0.6*s, ga(CYAN,40)));
      }
      pa.rect_filled(sl_r, 3.0*s, BG_DEEP);
      pa.rect_stroke(sl_r, 3.0*s, Stroke::new(0.8*s, ga(CYAN,45)), egui::StrokeKind::Outside);
      let tn = ((p.threshold+60.0)/60.0).clamp(0.0,1.0) as f32;
      let ty = mt + mh*(1.0-tn);
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, ty), Vec2::new(sw-2.0*s, 6.0*s)), 2.0*s, ga(CYAN,60));
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, ty), Vec2::new(sw-4.0*s, 4.0*s)), s, CYAN);
      pa.text(Pos2::new(cx, mt+mh+11.0*s), egui::Align2::CENTER_CENTER,
          format!("T {:.1}", p.threshold), FontId::new(7.5*s, FontFamily::Monospace), ga(CYAN,190));
    }
}

fn draw_red_panel(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges, s: f32) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 6.0*s, BG_PANEL);
      pa.rect_stroke(rect, 6.0*s, Stroke::new(s, ga(MAGENTA,45)), egui::StrokeKind::Outside);
      pa.rect_stroke(rect, 6.0*s, Stroke::new(4.0*s, ga(MAGENTA,10)), egui::StrokeKind::Outside);
      pa.text(Pos2::new(rect.center().x, rect.min.y+10.0*s), egui::Align2::CENTER_CENTER,
          "REDUCE", FontId::new(8.0*s, FontFamily::Monospace), ga(MAGENTA,200)); }
    let cx = rect.center().x;
    let mt = rect.min.y + 44.0*s; let mh = rect.height() - 82.0*s;
    let mw = 14.0*s; let sw = 14.0*s;
    let sx = cx - (mw+sw+4.0*s)*0.5;
    let max_r = Rect::from_center_size(Pos2::new(cx, rect.min.y+28.0*s), Vec2::new(rect.width()-14.0*s, 13.0*s));
    if ui.allocate_rect(max_r, Sense::click()).clicked() { ch.reduction_max_reset = true; }
    let sl_r = Rect::from_min_size(Pos2::new(sx, mt), Vec2::new(sw, mh));
    let sr   = ui.allocate_rect(sl_r, Sense::drag());
    if sr.dragged() {
        let n = ((p.max_reduction/40.0) as f32 - sr.drag_delta().y/mh).clamp(0.0,1.0);
        ch.max_reduction = Some(n as f64 * 40.0);
    }
    { let pa = ui.painter_at(rect);
      pa.rect_filled(max_r, 3.0*s, BG_DEEP);
      pa.rect_stroke(max_r, 3.0*s, Stroke::new(0.8*s, ga(MAGENTA,110)), egui::StrokeKind::Outside);
      pa.text(max_r.center(), egui::Align2::CENTER_CENTER, format!("{:.1}", p.reduction_max_db),
          FontId::new(7.5*s, FontFamily::Monospace), MAGENTA);
      pa.rect_filled(sl_r, 3.0*s, BG_DEEP);
      pa.rect_stroke(sl_r, 3.0*s, Stroke::new(0.8*s, ga(MAGENTA,45)), egui::StrokeKind::Outside);
      let mrn = (p.max_reduction/40.0).clamp(0.0,1.0) as f32;
      let mry = mt + mh*(1.0-mrn);
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, mry), Vec2::new(sw-2.0*s, 6.0*s)), 2.0*s, ga(MAGENTA,55));
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, mry), Vec2::new(sw-4.0*s, 4.0*s)), s, MAGENTA);
      let mr2 = Rect::from_min_size(Pos2::new(sx+sw+4.0*s, mt), Vec2::new(mw, mh));
      pa.rect_filled(mr2, 3.0*s, BG_DEEP);
      pa.rect_stroke(mr2, 3.0*s, Stroke::new(0.8*s, ga(MAGENTA,35)), egui::StrokeKind::Outside);
      let rn = (-p.reduction_db/40.0).clamp(0.0,1.0);
      let fh = mr2.height()*rn;
      if fh > 0.5 {
          let fr = Rect::from_min_size(mr2.min, Vec2::new(mr2.width(), fh));
          let col = lerp_c(MAGENTA, Color32::from_rgb(255,80,80), rn);
          pa.rect_filled(fr, s, ga(col, 180));
      }
      for db in [0_i32,-10,-20,-30,-40] {
          let y = mt + mh*(-db as f32/40.0);
          pa.text(Pos2::new(mr2.max.x+4.0*s, y), egui::Align2::LEFT_CENTER,
              format!("{db}"), FontId::new(6.5*s, FontFamily::Monospace), TEXT_LO);
      }
      pa.text(Pos2::new(cx, mt+mh+11.0*s), egui::Align2::CENTER_CENTER,
          format!("MAX {:.1}", p.max_reduction), FontId::new(7.5*s, FontFamily::Monospace), ga(MAGENTA,190));
    }
}

fn fancy_meter(pa: &egui::Painter, rect: Rect, db: f32, min_db: f32, max_db: f32, s: f32) {
    pa.rect_filled(rect, 3.0*s, BG_DEEP);
    pa.rect_stroke(rect, 3.0*s, Stroke::new(0.8*s, ga(CYAN,30)), egui::StrokeKind::Outside);
    let n = ((db-min_db)/(max_db-min_db)).clamp(0.0,1.0);
    let fh = rect.height()*n;
    if fh > 0.5 {
        let fr = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y-fh), Vec2::new(rect.width(), fh));
        let col = if db > -12.0 { M_RED } else if db > -24.0 { M_YELLOW } else { M_BLUE };
        pa.rect_filled(fr, s, ga(col, 200));
        if fh > 3.0 {
            let pk = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y-fh), Vec2::new(rect.width(), 2.0*s));
            pa.rect_filled(pk, 0.0, col);
        }
    }
}
