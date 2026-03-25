// ─────────────────────────────────────────────────────────────────────────────
// Nebula DeEsser v2.2.0 — Beautified Alien Synthwave GUI
// Refinements applied: log-warped spectrum, glow layers, premium controls
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
const CYAN_DIM:   Color32 = Color32::from_rgb(0,  120, 160);
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

// ─── Numeric Popup ───────────────────────────────────────────────────────────
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
    // A/B States
    pub state_a:             Option<ParamSnapshot>,
    pub state_b:             Option<ParamSnapshot>,
    pub active_state:        char, // 'A' or 'B'
    // Undo/Redo
    pub undo_stack:     Vec<ParamSnapshot>,
    pub redo_stack:     Vec<ParamSnapshot>,
    pub drag_snap:      Option<ParamSnapshot>,
    // Popups
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
    pub cut_width:f64, pub cut_depth:f64,
    pub mix:f64,
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
    pub cut_width:Option<f64>, pub cut_depth:Option<f64>,
    pub mix:Option<f64>,
}

// Base design resolution — all coordinates are authored at this size
const BASE_W: f32 = 860.0;
const BASE_H: f32 = 640.0;

// ─── Main Draw ────────────────────────────────────────────────────────────────
pub fn draw(ctx: &Context, egui_state: &EguiState, gui: &mut NebulaGui, params: &GuiParams) -> GuiChanges {
    gui.time += ctx.input(|i| i.unstable_dt) as f64;
    let mut ch = GuiChanges::default();

    // ── Scale all content proportionally to the current window size ───────
    // zoom_factor uniformly scales every widget, font, and painter coordinate.
    // This means the UI authored at BASE_W×BASE_H fills any window size cleanly.
    let screen = ctx.screen_rect();
    let scale = (screen.width() / BASE_W).min(screen.height() / BASE_H).max(0.25);
    ctx.set_zoom_factor(scale);    let mut style = (*ctx.style()).clone();
    style.visuals.panel_fill = BG_VOID;
    style.visuals.override_text_color = Some(TEXT_HI);
    style.visuals.widgets.noninteractive.bg_fill = BG_PANEL;
    style.visuals.widgets.inactive.bg_fill = BG_DEEP;
    style.visuals.widgets.hovered.bg_fill = Color32::from_rgb(0, 35, 55);
    style.visuals.widgets.hovered.fg_stroke = Stroke::new(1.5, CYAN);
    style.spacing.item_spacing = Vec2::new(4.0, 3.0);
    ctx.set_style(style);

    ResizableWindow::new("nebula_deesser_resize")
        .min_size(Vec2::new(400.0, 300.0))
        .show(ctx, egui_state, |ui| {
            let full = ui.max_rect();
            draw_bg(ui.painter_at(full), full, gui.time, params.bypass);

            let title_h   = 46.0;
            let toolbar_h = 30.0;
            let margin    = 8.0;

            draw_title(ui.painter_at(full), full, params.bypass);
            draw_toolbar(ui, Rect::from_min_size(
                Pos2::new(full.min.x, full.min.y + title_h),
                Vec2::new(full.width(), toolbar_h)), params, gui, &mut ch);

            let content = Rect::from_min_size(
                Pos2::new(full.min.x + margin, full.min.y + title_h + toolbar_h + margin),
                Vec2::new(full.width() - margin*2.0, full.height() - title_h - toolbar_h - margin*2.0));

            let mw = 88.0; let gap = 6.0;
            let cw = content.width() - mw * 2.0 - gap * 2.0;

            let left_r  = Rect::from_min_size(content.min, Vec2::new(mw, content.height()));
            let right_r = Rect::from_min_size(Pos2::new(content.max.x - mw, content.min.y), Vec2::new(mw, content.height()));
            let ctr_r   = Rect::from_min_size(Pos2::new(content.min.x + mw + gap, content.min.y), Vec2::new(cw, content.height()));

            let spec_frac = 0.38_f32;
            let ctrl_h = ctr_r.height() * (1.0 - spec_frac);
            let ctrl_r = Rect::from_min_size(ctr_r.min, Vec2::new(cw, ctrl_h));
            let spec_r = Rect::from_min_size(
                Pos2::new(ctr_r.min.x, ctr_r.min.y + ctrl_h + 4.0),
                Vec2::new(cw, ctr_r.height() * spec_frac - 4.0));

            draw_det_panel(ui, left_r,  params, &mut ch);
            draw_red_panel(ui, right_r, params, &mut ch);
            draw_controls(ui, ctrl_r, params, &mut ch, gui);
            draw_spectrum(ui, spec_r, gui, params, &mut ch);
        });

    if gui.num_input.open    { draw_num_popup(ctx, gui, &mut ch); }
    if gui.preset_save_popup { draw_preset_save(ctx, gui, params, &mut ch); }
    if gui.midi_popup        { draw_midi_popup(ctx, gui); }
    // Dropdowns rendered as floating Areas so they are never clipped by toolbar
    if gui.os_dropdown       { draw_os_dropdown(ctx, gui, params, &mut ch); }
    if gui.preset_dropdown_open { draw_preset_dropdown(ctx, gui, &mut ch); }
    if gui.midi_context_menu { draw_midi_context_menu(ctx, gui); }
    ch
}

// ─── Animated Background ─────────────────────────────────────────────────────
fn draw_bg(painter: egui::Painter, rect: Rect, time: f64, bypass: bool) {
    // Animated grid
    let sp = 38.0_f32;
    let ox = (time as f32 * 9.0).rem_euclid(sp);
    let oy = (time as f32 * 4.5).rem_euclid(sp);
    let mut x = rect.min.x - ox;
    while x < rect.max.x + sp { painter.line_segment([Pos2::new(x, rect.min.y), Pos2::new(x, rect.max.y)], Stroke::new(0.35, GRID)); x += sp; }
    let mut y = rect.min.y - oy;
    while y < rect.max.y + sp { painter.line_segment([Pos2::new(rect.min.x, y), Pos2::new(rect.max.x, y)], Stroke::new(0.35, GRID)); y += sp; }

    // Subtle vignette corners
    let vr = rect.width().min(rect.height()) * 0.4;
    for corner in [rect.min, Pos2::new(rect.max.x, rect.min.y),
                   rect.max, Pos2::new(rect.min.x, rect.max.y)] {
        painter.circle_filled(corner, vr, Color32::from_rgba_premultiplied(0,0,0,18));
    }

    // Bypass red tint overlay
    if bypass {
        painter.rect_filled(rect, 0.0, Color32::from_rgba_premultiplied(60,0,0,22));
    }
}

fn draw_title(painter: egui::Painter, rect: Rect, bypass: bool) {
    let bar = Rect::from_min_size(rect.min, Vec2::new(rect.width(), 46.0));
    painter.rect_filled(bar, 0.0, BG_PANEL);

    // Gradient bottom border
    let col = if bypass { RED_HOT } else { CYAN };
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(3.0, col));
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(10.0, ga(col, 28)));
    painter.line_segment([bar.left_bottom(), bar.right_bottom()], Stroke::new(20.0, ga(col, 10)));

    // Title text with shadow effect
    let tx = bar.min.x + 15.0; let ty = bar.center().y;
    painter.text(Pos2::new(tx+1.0, ty+1.0), egui::Align2::LEFT_CENTER,
        "NEBULA  DE-ESSER", FontId::new(18.0, FontFamily::Monospace), ga(CYAN, 25));
    painter.text(Pos2::new(tx, ty), egui::Align2::LEFT_CENTER,
        "NEBULA  DE-ESSER", FontId::new(18.0, FontFamily::Monospace), CYAN);

    painter.text(Pos2::new(bar.min.x + 238.0, ty + 1.5), egui::Align2::LEFT_CENTER,
        "ALIEN SPECTRUM PROCESSOR  //  64-BIT  CLAP",
        FontId::new(8.0, FontFamily::Monospace), TEXT_LO);

    // Version badge
    let vbx = bar.max.x - 12.0; let vby = ty;
    painter.text(Pos2::new(vbx, vby), egui::Align2::RIGHT_CENTER,
        "v2.2", FontId::new(8.5, FontFamily::Monospace), ga(PURPLE, 210));

    if bypass {
        let bx = bar.max.x - 74.0;
        let br = Rect::from_center_size(Pos2::new(bx, ty), Vec2::new(56.0, 16.0));
        painter.rect_filled(br, 4.0, ga(RED_HOT, 40));
        painter.rect_stroke(br, 4.0, Stroke::new(1.0, RED_HOT), egui::StrokeKind::Outside);
        painter.text(br.center(), egui::Align2::CENTER_CENTER,
            "BYPASSED", FontId::new(7.5, FontFamily::Monospace), RED_HOT);
    }
}

// ─── Toolbar ─────────────────────────────────────────────────────────────────
fn draw_toolbar(ui: &mut Ui, rect: Rect, params: &GuiParams, gui: &mut NebulaGui, ch: &mut GuiChanges) {
    { let p = ui.painter_at(rect);
      p.rect_filled(rect, 0.0, Color32::from_rgb(5, 3, 16));
      p.line_segment([rect.left_bottom(), rect.right_bottom()], Stroke::new(1.0, ga(PURPLE, 55))); }

    let cy = rect.center().y; let bh = 19.0;
    let mut cx = rect.min.x + 8.0;

    macro_rules! tbtn {
        ($label:expr, $active:expr, $col:expr, $w:expr) => {{
            let r = Rect::from_min_max(Pos2::new(cx, cy-bh*0.5), Pos2::new(cx+$w, cy+bh*0.5));
            cx += $w + 5.0;
            let resp = ui.allocate_rect(r, Sense::click());
            let hov  = resp.hovered();
            let c    = if $active { $col } else if hov { ga($col, 180) } else { ga($col, 95) };
            let bg   = if $active { ga($col, 32) } else if hov { ga($col, 12) } else { BG_WIDGET };
            { let p = ui.painter_at(rect);
              p.rect_filled(r, 4.0, bg);
              p.rect_stroke(r, 4.0, Stroke::new(if $active { 1.3 } else { 0.8 }, c), egui::StrokeKind::Outside);
              if $active { p.rect_stroke(r, 4.0, Stroke::new(5.0, ga(c, 25)), egui::StrokeKind::Outside); }
              p.text(r.center(), egui::Align2::CENTER_CENTER, $label, FontId::new(7.5, FontFamily::Monospace), c); }
            resp
        }};
    }

    // BYPASS
    if tbtn!(if params.bypass {"⊗ BYPASSED"} else {"⊗ BYPASS"}, params.bypass, RED_HOT, 72.0).clicked() {
        ch.bypass = Some(!params.bypass);
    }
    cx += 3.0;

    // PRESET dropdown
    let pw = 138.0;
    { let pr = Rect::from_min_max(Pos2::new(cx, cy-bh*0.5), Pos2::new(cx+pw, cy+bh*0.5));
      cx += pw + 5.0;
      let resp = ui.allocate_rect(pr, Sense::click());
      let hov  = resp.hovered();
      let lbl  = if gui.presets.is_empty() { "PRESET  ─".to_string() }
          else { let n = &gui.presets[gui.selected_preset.min(gui.presets.len()-1)].0;
                 format!("▾ {}", if n.len()>15 { &n[..15] } else { n }) };
      { let p = ui.painter_at(rect);
        p.rect_filled(pr, 4.0, BG_WIDGET);
        p.rect_stroke(pr, 4.0, Stroke::new(if hov {1.2} else {0.8}, if hov {CYAN} else {ga(CYAN,60)}), egui::StrokeKind::Outside);
        p.text(pr.center(), egui::Align2::CENTER_CENTER, &lbl, FontId::new(7.5, FontFamily::Monospace), if hov {CYAN} else {ga(CYAN,150)}); }
      gui.preset_anchor = Pos2::new(pr.min.x, pr.max.y + 2.0);  // save for dropdown
      if resp.clicked() { gui.preset_dropdown_open = !gui.preset_dropdown_open; }
    }

    // SAVE / DEL
    if tbtn!("SAVE", false, GOLD, 42.0).clicked() {
        gui.preset_name_buf.clear(); gui.preset_save_popup=true; gui.preset_dropdown_open=false;
    }
    if tbtn!("DEL", false, MAGENTA, 34.0).clicked() && !gui.presets.is_empty() {
        gui.presets.remove(gui.selected_preset.min(gui.presets.len()-1));
        if gui.selected_preset > 0 { gui.selected_preset -= 1; }
    }
    cx += 3.0;

    // UNDO / REDO
    let can_undo = !gui.undo_stack.is_empty();
    let can_redo = !gui.redo_stack.is_empty();
    { let r = Rect::from_min_max(Pos2::new(cx,cy-bh*0.5), Pos2::new(cx+46.0,cy+bh*0.5)); cx+=51.0;
      let resp = ui.allocate_rect(r, Sense::click());
      let c = if can_undo { if resp.hovered() {CYAN} else {ga(CYAN,150)} } else {ga(TEXT_LO,70)};
      { let p = ui.painter_at(rect);
        p.rect_filled(r, 4.0, BG_WIDGET);
        p.rect_stroke(r, 4.0, Stroke::new(0.8, ga(c,120)), egui::StrokeKind::Outside);
        p.text(r.center(), egui::Align2::CENTER_CENTER, "◄ UNDO", FontId::new(7.5, FontFamily::Monospace), c); }
      if resp.clicked() && can_undo {
          let snap = gui.undo_stack.pop().unwrap();
          gui.redo_stack.push(ParamSnapshot::from_params(params)); gui.redo_stack.truncate(50);
          snap.apply_to(ch);
      }
    }
    { let r = Rect::from_min_max(Pos2::new(cx,cy-bh*0.5), Pos2::new(cx+46.0,cy+bh*0.5)); cx+=51.0;
      let resp = ui.allocate_rect(r, Sense::click());
      let c = if can_redo { if resp.hovered() {CYAN} else {ga(CYAN,150)} } else {ga(TEXT_LO,70)};
      { let p = ui.painter_at(rect);
        p.rect_filled(r, 4.0, BG_WIDGET);
        p.rect_stroke(r, 4.0, Stroke::new(0.8, ga(c,120)), egui::StrokeKind::Outside);
        p.text(r.center(), egui::Align2::CENTER_CENTER, "REDO ►", FontId::new(7.5, FontFamily::Monospace), c); }
      if resp.clicked() && can_redo {
          let snap = gui.redo_stack.pop().unwrap();
          gui.undo_stack.push(ParamSnapshot::from_params(params)); gui.undo_stack.truncate(50);
          snap.apply_to(ch);
      }
    }
    cx += 3.0;

    // A/B STATE
    let ab_label = if gui.active_state == 'A' { "A/B  A" } else { "A/B  B" };
    let ab_active = gui.state_a.is_some() || gui.state_b.is_some();
    let ab_resp = tbtn!(ab_label, ab_active, GREEN_NEON, 58.0);
    if ab_resp.clicked() {
        // Save current state to the slot we're leaving
        let current_snap = ParamSnapshot::from_params(params);
        match gui.active_state {
            'A' => gui.state_a = Some(current_snap),
            'B' => gui.state_b = Some(current_snap),
            _ => {}
        }
        
        // Toggle between A and B
        gui.active_state = if gui.active_state == 'A' { 'B' } else { 'A' };
        
        // Apply the active state if it exists
        match (gui.active_state, &gui.state_a, &gui.state_b) {
            ('A', Some(state_a), _) => state_a.apply_to(ch),
            ('B', _, Some(state_b)) => state_b.apply_to(ch),
            _ => {}
        }
    }
    if ab_resp.secondary_clicked() {
        // Right-click: store current state to active slot (overwrites existing)
        let snap = ParamSnapshot::from_params(params);
        match gui.active_state {
            'A' => gui.state_a = Some(snap),
            'B' => gui.state_b = Some(snap),
            _ => {}
        }
    }
    cx += 3.0;

    // MIDI LEARN
    let learning = gui.midi_learn.learning_target.load(std::sync::atomic::Ordering::Relaxed) >= 0;
    let midi_btn = tbtn!(if learning {"● LEARNING"} else {"MIDI LEARN"}, learning, MAGENTA, 86.0);
    if midi_btn.clicked() {
        if learning { gui.midi_learn.learning_target.store(-1, std::sync::atomic::Ordering::Release); }
        else { gui.midi_popup = true; }
    }
    if midi_btn.secondary_clicked() {
        // Right-click: show MIDI context menu
        gui.midi_context_menu = true;
        gui.midi_context_anchor = Pos2::new(midi_btn.rect.min.x, midi_btn.rect.max.y + 2.0);
    }
    cx += 3.0;

    // OVERSAMPLING — button only; dropdown rendered as floating Area in draw()
    let os_labels = ["OFF", "2×", "4×", "6×", "8×"];
    let cur = os_labels.get(params.oversampling as usize).copied().unwrap_or("OFF");
    let os_w = 94.0;
    { let or_ = Rect::from_min_max(Pos2::new(cx,cy-bh*0.5), Pos2::new(cx+os_w,cy+bh*0.5));
      gui.os_anchor = Pos2::new(or_.min.x, or_.max.y + 2.0);  // save for dropdown
      let resp = ui.allocate_rect(or_, Sense::click());
      let active = params.oversampling > 0;
      let c = if active {GOLD} else if resp.hovered() {ga(GOLD,180)} else {ga(GOLD,85)};
      { let p = ui.painter_at(rect);
        p.rect_filled(or_, 4.0, if active {ga(GOLD,18)} else {BG_WIDGET});
        p.rect_stroke(or_, 4.0, Stroke::new(if active {1.2} else {0.8}, c), egui::StrokeKind::Outside);
        if active { p.rect_stroke(or_, 4.0, Stroke::new(5.0, ga(c,20)), egui::StrokeKind::Outside); }
        p.text(or_.center(), egui::Align2::CENTER_CENTER,
            format!("OS  {}  ▾", cur), FontId::new(7.5, FontFamily::Monospace), c); }
      if resp.clicked() { gui.os_dropdown = !gui.os_dropdown; }
    }
}

// ─── Detection Meter Panel ────────────────────────────────────────────────────
fn draw_det_panel(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 6.0, BG_PANEL);
      pa.rect_stroke(rect, 6.0, Stroke::new(1.0, ga(CYAN,45)), egui::StrokeKind::Outside);
      pa.rect_stroke(rect, 6.0, Stroke::new(4.0, ga(CYAN,12)), egui::StrokeKind::Outside);
      pa.text(Pos2::new(rect.center().x, rect.min.y+10.0), egui::Align2::CENTER_CENTER,
          "DETECT", FontId::new(8.0, FontFamily::Monospace), ga(CYAN,200)); }

    let cx = rect.center().x;
    let mt = rect.min.y + 44.0; let mh = rect.height() - 82.0;
    let mw = 14.0; let sw = 14.0;
    let sx = cx - (mw+sw+4.0)*0.5;

    let max_r = Rect::from_center_size(Pos2::new(cx, rect.min.y+28.0), Vec2::new(rect.width()-14.0, 13.0));
    if ui.allocate_rect(max_r, Sense::click()).clicked() { ch.detection_max_reset = true; }

    let sl_r = Rect::from_min_size(Pos2::new(sx+mw+4.0, mt), Vec2::new(sw, mh));
    let sr   = ui.allocate_rect(sl_r, Sense::drag());
    if sr.dragged() {
        let n = (((p.threshold+60.0)/60.0) as f32 - sr.drag_delta().y/mh).clamp(0.0,1.0);
        ch.threshold = Some(-60.0 + n as f64 * 60.0);
    }

    { let pa = ui.painter_at(rect);
      // Max field
      pa.rect_filled(max_r, 3.0, BG_DEEP);
      pa.rect_stroke(max_r, 3.0, Stroke::new(0.8, ga(M_YELLOW,110)), egui::StrokeKind::Outside);
      pa.text(max_r.center(), egui::Align2::CENTER_CENTER, format!("{:.1}", p.detection_max_db),
          FontId::new(7.5, FontFamily::Monospace), M_YELLOW);
      // Meter
      let mr = Rect::from_min_size(Pos2::new(sx, mt), Vec2::new(mw, mh));
      fancy_meter(&pa, mr, p.detection_db, -60.0, 0.0);
      // Scale ticks
      for db in [-60_i32,-48,-36,-24,-12,0] {
          let y = mt + mh*(1.0-((db as f32+60.0)/60.0));
          pa.line_segment([Pos2::new(mr.min.x-3.0,y), Pos2::new(mr.min.x,y)], Stroke::new(0.6, ga(CYAN,40)));
          pa.text(Pos2::new(mr.min.x-5.0,y), egui::Align2::RIGHT_CENTER,
              if db==0{"0"}else{""}, FontId::new(6.5, FontFamily::Monospace), TEXT_LO);
      }
      // Threshold slider
      pa.rect_filled(sl_r, 3.0, BG_DEEP);
      pa.rect_stroke(sl_r, 3.0, Stroke::new(0.8, ga(CYAN,45)), egui::StrokeKind::Outside);
      let tn = ((p.threshold+60.0)/60.0).clamp(0.0,1.0) as f32;
      let ty = mt + mh*(1.0-tn);
      // Glow handle
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, ty), Vec2::new(sw-2.0, 6.0)), 2.0, ga(CYAN,60));
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, ty), Vec2::new(sw-4.0, 4.0)), 1.0, CYAN);
      // Label
      pa.text(Pos2::new(cx, mt+mh+11.0), egui::Align2::CENTER_CENTER,
          format!("T {:.1}", p.threshold), FontId::new(7.5, FontFamily::Monospace), ga(CYAN,190));
    }
}

fn draw_red_panel(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 6.0, BG_PANEL);
      pa.rect_stroke(rect, 6.0, Stroke::new(1.0, ga(MAGENTA,45)), egui::StrokeKind::Outside);
      pa.rect_stroke(rect, 6.0, Stroke::new(4.0, ga(MAGENTA,10)), egui::StrokeKind::Outside);
      pa.text(Pos2::new(rect.center().x, rect.min.y+10.0), egui::Align2::CENTER_CENTER,
          "REDUCE", FontId::new(8.0, FontFamily::Monospace), ga(MAGENTA,200)); }

    let cx = rect.center().x;
    let mt = rect.min.y + 44.0; let mh = rect.height() - 82.0;
    let mw = 14.0; let sw = 14.0;
    let sx = cx - (mw+sw+4.0)*0.5;

    let max_r = Rect::from_center_size(Pos2::new(cx, rect.min.y+28.0), Vec2::new(rect.width()-14.0, 13.0));
    if ui.allocate_rect(max_r, Sense::click()).clicked() { ch.reduction_max_reset = true; }

    let sl_r = Rect::from_min_size(Pos2::new(sx, mt), Vec2::new(sw, mh));
    let sr   = ui.allocate_rect(sl_r, Sense::drag());
    if sr.dragged() {
        let n = ((p.max_reduction/40.0) as f32 - sr.drag_delta().y/mh).clamp(0.0,1.0);
        ch.max_reduction = Some(n as f64 * 40.0);
    }

    { let pa = ui.painter_at(rect);
      pa.rect_filled(max_r, 3.0, BG_DEEP);
      pa.rect_stroke(max_r, 3.0, Stroke::new(0.8, ga(MAGENTA,110)), egui::StrokeKind::Outside);
      pa.text(max_r.center(), egui::Align2::CENTER_CENTER, format!("{:.1}", p.reduction_max_db),
          FontId::new(7.5, FontFamily::Monospace), MAGENTA);
      // Slider
      pa.rect_filled(sl_r, 3.0, BG_DEEP);
      pa.rect_stroke(sl_r, 3.0, Stroke::new(0.8, ga(MAGENTA,45)), egui::StrokeKind::Outside);
      let mrn = (p.max_reduction/40.0).clamp(0.0,1.0) as f32;
      let mry = mt + mh*(1.0-mrn);
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, mry), Vec2::new(sw-2.0, 6.0)), 2.0, ga(MAGENTA,55));
      pa.rect_filled(Rect::from_center_size(Pos2::new(sl_r.center().x, mry), Vec2::new(sw-4.0, 4.0)), 1.0, MAGENTA);
      // Meter
      let mr2 = Rect::from_min_size(Pos2::new(sx+sw+4.0, mt), Vec2::new(mw, mh));
      pa.rect_filled(mr2, 3.0, BG_DEEP);
      pa.rect_stroke(mr2, 3.0, Stroke::new(0.8, ga(MAGENTA,35)), egui::StrokeKind::Outside);
      let rn = (-p.reduction_db/40.0).clamp(0.0,1.0);
      let fh = mr2.height()*rn;
      if fh > 0.5 {
          let fr = Rect::from_min_size(mr2.min, Vec2::new(mr2.width(), fh));
          let intensity = rn;
          let col = lerp_c(MAGENTA, Color32::from_rgb(255,80,80), intensity);
          pa.rect_filled(fr, 1.0, ga(col, 180));
          pa.rect_filled(fr, 1.0, ga(col, 35));
      }
      for db in [0_i32,-10,-20,-30,-40] {
          let y = mt + mh*(-db as f32/40.0);
          pa.text(Pos2::new(mr2.max.x+4.0, y), egui::Align2::LEFT_CENTER,
              format!("{db}"), FontId::new(6.5, FontFamily::Monospace), TEXT_LO);
      }
      pa.text(Pos2::new(cx, mt+mh+11.0), egui::Align2::CENTER_CENTER,
          format!("MAX {:.1}", p.max_reduction), FontId::new(7.5, FontFamily::Monospace), ga(MAGENTA,190));
    }
}

fn fancy_meter(pa: &egui::Painter, rect: Rect, db: f32, min_db: f32, max_db: f32) {
    pa.rect_filled(rect, 3.0, BG_DEEP);
    pa.rect_stroke(rect, 3.0, Stroke::new(0.8, ga(CYAN,30)), egui::StrokeKind::Outside);
    let n = ((db-min_db)/(max_db-min_db)).clamp(0.0,1.0);
    let fh = rect.height()*n;
    if fh > 0.5 {
        let fr = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y-fh), Vec2::new(rect.width(), fh));
        let col = if db > -12.0 { M_RED } else if db > -24.0 { M_YELLOW } else { M_BLUE };
        pa.rect_filled(fr, 1.0, ga(col, 200));
        pa.rect_filled(fr, 1.0, ga(col, 40));
        // Peak sparkle at top
        if fh > 3.0 {
            let pk = Rect::from_min_size(Pos2::new(rect.min.x, rect.max.y-fh), Vec2::new(rect.width(), 2.0));
            pa.rect_filled(pk, 0.0, col);
        }
    }
}

// ─── Controls Panel ──────────────────────────────────────────────────────────
fn draw_controls(ui: &mut Ui, rect: Rect, p: &GuiParams, ch: &mut GuiChanges, gui: &mut NebulaGui) {
    { let pa = ui.painter_at(rect);
      pa.rect_filled(rect, 6.0, BG_PANEL);
      pa.rect_stroke(rect, 6.0, Stroke::new(1.0, ga(PURPLE,45)), egui::StrokeKind::Outside); }

    let inner  = rect.shrink(7.0);
    let top_h  = 62.0;
    let kh     = 74.0;
    let btn_h  = 23.0;
    let gap    = 4.0;

    // ── Selector row ──────────────────────────────────────────────────────
    let cw = inner.width() / 5.0;
    let cols: Vec<Rect> = (0..5).map(|i|
        Rect::from_min_size(Pos2::new(inner.min.x + i as f32 * cw, inner.min.y), Vec2::new(cw-4.0, top_h))).collect();
    if let Some(i) = sec2(ui, cols[0], "MODE",      ["RELATIVE","ABSOLUTE"], if p.mode_relative{0}else{1}) { push_undo(gui,p); ch.mode_relative=Some(i==0); }
    if let Some(i) = sec2(ui, cols[1], "RANGE",     ["SPLIT","WIDE"], if p.use_wide_range{1}else{0}) { push_undo(gui,p); ch.use_wide_range=Some(i==1); }
    if let Some(i) = sec2(ui, cols[2], "FILTER",    ["LOWPASS","PEAK"], if p.use_peak_filter{1}else{0}) { push_undo(gui,p); ch.use_peak_filter=Some(i==1); }
    if let Some(i) = sec2(ui, cols[3], "SIDECHAIN", ["INTERNAL","EXTERNAL"], if p.sidechain_external{1}else{0}) { push_undo(gui,p); ch.sidechain_external=Some(i==1); }
    if let Some(i) = sec2(ui, cols[4], "MODE",      ["VOCAL","ALLROUND"], if p.vocal_mode{0}else{1}) { push_undo(gui,p); ch.vocal_mode=Some(i==0); }

    // ── Main knob row ─────────────────────────────────────────────────────
    let y2 = inner.min.y + top_h + gap;
    let main_k: &[(&str, f64, f64, f64, &str, NumTarget)] = &[
        ("THRESH",   p.threshold,    -60.0,  0.0, "dB", NumTarget::Threshold),
        ("MAX RED",  p.max_reduction,  0.0, 40.0, "dB", NumTarget::MaxReduction),
        ("MIN FREQ", p.min_freq,    1000.0,16000.0,"Hz", NumTarget::MinFreq),
        ("MAX FREQ", p.max_freq,    1000.0,20000.0,"Hz", NumTarget::MaxFreq),
        ("LOOKAHD",  p.lookahead_ms,  0.0, 20.0, "ms", NumTarget::Lookahead),
        ("ST LINK",  p.stereo_link,   0.0,  1.0, "%",  NumTarget::StereoLink),
    ];
    knob_row(ui, rect, inner, y2, kh, main_k, ch, gui, p, CYAN);

    // ── Cut shape knob row (Width + Depth) ───────────────────────────────
    let y2b = y2 + kh + gap;
    { let pa = ui.painter_at(rect);
      pa.line_segment([Pos2::new(inner.min.x+20.0, y2b-1.0), Pos2::new(inner.max.x-20.0, y2b-1.0)],
          Stroke::new(0.5, ga(PURPLE, 35))); }
    let cut_k: &[(&str, f64, f64, f64, &str, NumTarget)] = &[
        ("CUT WIDTH", p.cut_width, 0.0, 1.0, "%", NumTarget::CutWidth),
        ("CUT DEPTH", p.cut_depth, 0.0, 1.0, "%", NumTarget::CutDepth),
        ("MIX",       p.mix,       0.0, 1.0, "%", NumTarget::Mix),
    ];
    let cut_inner = Rect::from_min_size(
        Pos2::new(inner.min.x + inner.width()*0.2, y2b),
        Vec2::new(inner.width()*0.6, kh));
    knob_row(ui, rect, cut_inner, y2b, kh, cut_k, ch, gui, p, PURPLE);

    // ── I/O knob row ──────────────────────────────────────────────────────
    let y3 = y2b + kh + gap;
    { let pa = ui.painter_at(rect);
      pa.line_segment([Pos2::new(inner.min.x+20.0, y3-1.0), Pos2::new(inner.max.x-20.0, y3-1.0)],
          Stroke::new(0.5, ga(GREEN_NEON, 35))); }
    let io_k: &[(&str, f64, f64, f64, &str, NumTarget)] = &[
        ("IN LEVEL",  p.input_level,  -60.0, 12.0, "dB",  NumTarget::InputLevel),
        ("IN PAN",    p.input_pan,     -1.0,  1.0, "pan", NumTarget::InputPan),
        ("OUT LEVEL", p.output_level, -60.0, 12.0, "dB",  NumTarget::OutputLevel),
        ("OUT PAN",   p.output_pan,   -1.0,  1.0, "pan", NumTarget::OutputPan),
    ];
    let io_inner = Rect::from_min_size(
        Pos2::new(inner.min.x + inner.width()*0.1, y3),
        Vec2::new(inner.width()*0.8, kh));
    knob_row(ui, rect, io_inner, y3, kh, io_k, ch, gui, p, GREEN_NEON);

    // ── Toggle buttons ────────────────────────────────────────────────────
    let y4 = y3 + kh + gap;
    let btns: &[(&str, bool)] = &[
        ("FILTER SOLO",  p.filter_solo),
        ("TRIGGER HEAR", p.trigger_hear),
        ("LOOKAHEAD ON", p.lookahead_enabled),
        ("MID / SIDE",   p.stereo_mid_side),
    ];
    let bw = inner.width() / btns.len() as f32 - 4.0;
    for (i, (lbl, active)) in btns.iter().enumerate() {
        let bx = inner.min.x + (bw+4.0) * i as f32;
        let br = Rect::from_min_size(Pos2::new(bx, y4), Vec2::new(bw, btn_h));
        let r  = ui.allocate_rect(br, Sense::click());
        let hov = r.hovered();
        { let pa = ui.painter_at(rect);
          let c  = if *active {CYAN} else if hov {ga(CYAN,160)} else {TEXT_LO};
          let bg = if *active {Color32::from_rgb(0,34,50)} else if hov {ga(CYAN,8)} else {BG_WIDGET};
          pa.rect_filled(br, 4.0, bg);
          pa.rect_stroke(br, 4.0, Stroke::new(if *active {1.2} else {0.7}, c), egui::StrokeKind::Outside);
          if *active { pa.rect_stroke(br, 4.0, Stroke::new(5.0, ga(c,22)), egui::StrokeKind::Outside); }
          pa.text(br.center(), egui::Align2::CENTER_CENTER, *lbl, FontId::new(7.0, FontFamily::Monospace), c); }
        if r.clicked() {
            push_undo(gui, p);
            match *lbl {
                "FILTER SOLO"  => ch.filter_solo      = Some(!active),
                "TRIGGER HEAR" => ch.trigger_hear     = Some(!active),
                "LOOKAHEAD ON" => ch.lookahead_enabled = Some(!active),
                "MID / SIDE"   => ch.stereo_mid_side  = Some(!active),
                _ => {}
            }
        }
    }
}

fn sec2(ui: &mut Ui, rect: Rect, hdr: &str, labs: [&str;2], ai: usize) -> Option<usize> {
    { let pa = ui.painter_at(rect);
      pa.text(Pos2::new(rect.center().x, rect.min.y+7.0), egui::Align2::CENTER_CENTER,
          hdr, FontId::new(6.5, FontFamily::Monospace), ga(PURPLE,210)); }
    let bh = 15.0; let mut res = None;
    for (i, lbl) in labs.iter().enumerate() {
        let br = Rect::from_min_size(
            Pos2::new(rect.min.x, rect.min.y+13.0+i as f32*(bh+2.0)),
            Vec2::new(rect.width(), bh));
        let r  = ui.allocate_rect(br, Sense::click());
        let ia = i == ai; let hov = r.hovered();
        { let pa = ui.painter_at(rect);
          let c = if ia{CYAN} else if hov{ga(CYAN,160)} else{TEXT_LO};
          pa.rect_filled(br, 2.0, if ia{Color32::from_rgb(0,34,50)} else if hov{ga(CYAN,8)} else{BG_WIDGET});
          pa.rect_stroke(br, 2.0, Stroke::new(if ia{1.0} else{0.6}, c), egui::StrokeKind::Outside);
          pa.text(br.center(), egui::Align2::CENTER_CENTER, *lbl, FontId::new(6.5, FontFamily::Monospace), c); }
        if r.clicked() { res = Some(i); }
    }
    res
}

// Workaround: use a plain function for section drawing that takes GuiChanges directly
fn knob_row(
    ui: &mut Ui, rect: Rect, inner: Rect, y: f32, _h: f32,
    defs: &[(&str, f64, f64, f64, &str, NumTarget)],
    ch: &mut GuiChanges, gui: &mut NebulaGui, p: &GuiParams, col: Color32,
) {
    let n = defs.len(); let kw = inner.width() / n as f32;
    let ks = (kw * 0.66).min(36.0);
    for (i, (lbl, val, min, max, unit, tgt)) in defs.iter().enumerate() {
        let kx  = inner.min.x + kw * i as f32 + kw * 0.5;
        let kc  = Pos2::new(kx, y + 12.0 + ks * 0.5);
        let kr  = Rect::from_center_size(kc, Vec2::splat(ks));
        let fr  = Rect::from_center_size(Pos2::new(kx, kr.max.y + 9.0), Vec2::new(kw-10.0, 13.0));

        { let pa = ui.painter_at(rect);
          pa.text(Pos2::new(kx, y+5.0), egui::Align2::CENTER_CENTER,
              *lbl, FontId::new(6.5, FontFamily::Monospace), ga(col, 180)); }

        let resp = ui.allocate_rect(kr, Sense::drag().union(Sense::click()));
        if resp.drag_started() { gui.drag_snap = Some(ParamSnapshot::from_params(p)); }
        if resp.dragged() {
            let n = ((*val - *min) / (*max - *min)) as f32;
            let nv = (*min + (n - resp.drag_delta().y * 0.006).clamp(0.0, 1.0) as f64 * (*max - *min)).clamp(*min, *max);
            apply_ch(tgt, nv, ch);
        }
        if resp.drag_stopped() {
            if let Some(s) = gui.drag_snap.take() {
                gui.undo_stack.push(s); gui.undo_stack.truncate(50); gui.redo_stack.clear();
            }
        }
        if resp.hovered() {
            let sc = ui.input(|i| i.smooth_scroll_delta.y);
            if sc != 0.0 {
                let n = ((*val - *min) / (*max - *min)) as f32;
                let nv = (*min + (n + sc*0.008).clamp(0.0,1.0) as f64 * (*max-*min)).clamp(*min,*max);
                apply_ch(tgt, nv, ch);
            }
        }
        if resp.secondary_clicked() {
            gui.num_input = NumInput { open:true, label:lbl.to_string(),
                value_str:format!("{:.2}",val), target:tgt.clone(), min:*min, max:*max };
        }
        if ui.allocate_rect(fr, Sense::click()).secondary_clicked() {
            gui.num_input = NumInput { open:true, label:lbl.to_string(),
                value_str:format!("{:.2}",val), target:tgt.clone(), min:*min, max:*max };
        }

        { let pa = ui.painter_at(rect);
          draw_premium_knob(&pa, kc, ks*0.5, *val, *min, *max, col);
          let disp = fmt_knob(*val, *unit);
          draw_value_field(&pa, fr, &disp, col); }
    }
}

fn fmt_knob(v: f64, unit: &str) -> String {
    match unit {
        "Hz"  => if v >= 1000.0 { format!("{:.1}k", v/1000.0) } else { format!("{:.0}", v) },
        "%"   => format!("{:.0}%", v*100.0),
        "pan" => if v.abs()<0.01 {"C".into()} else if v>0.0 {format!("R{:.0}",v*100.0)} else {format!("L{:.0}",-v*100.0)},
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

// ─── Premium Knob ────────────────────────────────────────────────────────────
fn draw_premium_knob(pa: &egui::Painter, c: Pos2, r: f32, val: f64, min: f64, max: f64, col: Color32) {
    let norm  = ((val-min)/(max-min)).clamp(0.0,1.0) as f32;
    let start = std::f32::consts::PI * 0.75;
    let sweep = std::f32::consts::PI * 1.5;
    let angle = start + norm * sweep;

    // Multi-layer body: outer ring glow → body → inner cap
    pa.circle_filled(c, r+1.5, ga(col, 12));
    pa.circle_filled(c, r,     BG_DEEP);
    pa.circle_stroke(c, r,     Stroke::new(1.5, ga(col, 55)));

    // Track arc
    arc(pa, c, r*0.76, start, start+sweep, ga(col,35), 2.5);
    // Value arc (with glow)
    if norm > 0.005 {
        arc(pa, c, r*0.76, start, angle, ga(col, 55), 4.5);
        arc(pa, c, r*0.76, start, angle, col,         2.0);
    }

    // Indicator dot
    let ix = c.x + r*0.52 * angle.cos();
    let iy = c.y + r*0.52 * angle.sin();
    pa.circle_filled(Pos2::new(ix, iy), 2.5, col);
    pa.circle_filled(Pos2::new(ix, iy), 1.5, Color32::WHITE);
    // Centre dot
    pa.circle_filled(c, 2.0, ga(col, 180));
}

fn arc(pa: &egui::Painter, c: Pos2, r: f32, a0: f32, a1: f32, col: Color32, w: f32) {
    let steps = 32; let span = a1 - a0;
    let pts: Vec<Pos2> = (0..=steps).map(|i| {
        let a = a0 + i as f32 / steps as f32 * span;
        Pos2::new(c.x + r*a.cos(), c.y + r*a.sin())
    }).collect();
    for i in 0..pts.len()-1 { pa.line_segment([pts[i], pts[i+1]], Stroke::new(w, col)); }
}

fn draw_value_field(pa: &egui::Painter, rect: Rect, text: &str, col: Color32) {
    pa.rect_filled(rect, 3.0, BG_DEEP);
    pa.rect_stroke(rect, 3.0, Stroke::new(0.7, ga(col,70)), egui::StrokeKind::Outside);
    pa.text(rect.center(), egui::Align2::CENTER_CENTER, text, FontId::new(7.5, FontFamily::Monospace), ga(col,220));
}

// ─── Spectrum Analyzer ───────────────────────────────────────────────────────
fn freq_to_x(freq: f32, w: f32) -> f32 {
    let lmin = 20.0_f32.log10(); let lmax = 22000.0_f32.log10();
    (freq.clamp(20.0, 22000.0).log10() - lmin) / (lmax - lmin) * w
}
fn x_to_freq(x: f32, w: f32) -> f32 {
    let lmin = 20.0_f32.log10(); let lmax = 22000.0_f32.log10();
    10.0_f32.powf(lmin + (x/w)*(lmax-lmin))
}

fn draw_spectrum(ui: &mut Ui, rect: Rect, gui: &mut NebulaGui, p: &GuiParams, ch: &mut GuiChanges) {
    if rect.height() < 24.0 { return; }

    // ── Panel background ─────────────────────────────────────────────────────
    let pa = ui.painter_at(rect);
    pa.rect_filled(rect, 6.0, BG_DEEP);
    pa.rect_stroke(rect, 6.0, Stroke::new(1.0, ga(PURPLE, 65)), egui::StrokeKind::Outside);

    let inner = rect.shrink(5.0);
    let ph    = (inner.height() - 16.0).max(10.0);
    let sr    = 44100.0_f32;

    // ── dB grid lines ────────────────────────────────────────────────────────
    for &db in &[-80.0_f32, -60.0, -40.0, -20.0, -10.0] {
        // Map with -90 to 0 range (matches what we display)
        let ny = 1.0 - (db - (-90.0)) / 90.0;
        let y  = inner.min.y + ny * ph;
        pa.line_segment(
            [Pos2::new(inner.min.x, y), Pos2::new(inner.max.x, y)],
            Stroke::new(0.4, ga(PURPLE, 28)));
        pa.text(Pos2::new(inner.min.x + 3.0, y - 2.0),
            egui::Align2::LEFT_BOTTOM,
            format!("{}", db as i32),
            FontId::new(5.5, FontFamily::Monospace), ga(TEXT_LO, 140));
    }

    // ── Frequency grid lines ──────────────────────────────────────────────────
    for &freq in &[100.0_f32, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0] {
        let x = inner.min.x + freq_to_x(freq, inner.width());
        pa.line_segment(
            [Pos2::new(x, inner.min.y), Pos2::new(x, inner.min.y + ph)],
            Stroke::new(0.4, ga(CYAN, 16)));
        let lbl = if freq >= 1000.0 {
            format!("{}k", (freq / 1000.0) as i32)
        } else {
            format!("{}", freq as i32)
        };
        pa.text(Pos2::new(x, inner.max.y - 3.0),
            egui::Align2::CENTER_CENTER, lbl,
            FontId::new(6.0, FontFamily::Monospace), TEXT_LO);
    }

    // ── Update smoothed magnitudes from shared analyzer data ──────────────────
    // Use lock() not try_lock() — GUI thread is not real-time so it's safe
    // to wait the few microseconds the audio thread holds the mutex.
    {
        let spec = gui.spectrum.lock();
        let mags = &spec.magnitudes;
        let nb   = mags.len();
        if gui.smooth_mags.len() != nb {
            gui.smooth_mags = vec![-90.0_f32; nb];
        }
        // Fast attack (0.3 → 70% of new value), slow release (0.85 → 15% of new value)
        let atk = 0.30_f32;
        let rel = 0.85_f32;
        for (i, &mag) in mags.iter().enumerate().take(nb) {
            let m = mag.clamp(-90.0, 0.0);
            gui.smooth_mags[i] = if m > gui.smooth_mags[i] {
                gui.smooth_mags[i] * atk + m * (1.0 - atk)   // attack: mostly new
            } else {
                gui.smooth_mags[i] * rel + m * (1.0 - rel)    // release: mostly old
            };
        }
    }

    // ── Build log-warped display points (one per pixel column) ────────────────
    // Correct bin formula: bin = freq * FFT_SIZE / sr = freq * (2 * nb - 2) / sr
    let nb   = gui.smooth_mags.len();
    let fft_size = (nb - 1) * 2;   // FFT_SIZE = 2*(NUM_BINS-1) = 2*1024 = 2048
    let db_min  = -90.0_f32;
    let db_max  =   0.0_f32;
    let db_rng  = db_max - db_min;

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

    // ── Draw spectrum — fill then line (using PathShape for correct fill) ──────
    if pts.len() >= 2 {
        let bottom_y = inner.min.y + ph;

        // Filled area: build a closed path going along spectrum top then back
        // along the bottom. Use many small triangles via vertical strips — this
        // avoids any convexity requirement.
        let fill_col = ga(CYAN, 22);
        for i in 0..pts.len().saturating_sub(1) {
            let tl = pts[i];
            let tr = pts[i + 1];
            let bl = Pos2::new(tl.x, bottom_y);
            let br = Pos2::new(tr.x, bottom_y);
            // Two triangles per strip — always convex by construction
            pa.add(egui::Shape::convex_polygon(
                vec![tl, tr, br, bl], fill_col, Stroke::NONE));
        }

        // Outer glow — wide, low alpha
        for i in 0..pts.len() - 1 {
            pa.line_segment([pts[i], pts[i + 1]], Stroke::new(4.0, ga(CYAN, 20)));
        }
        // Inner glow
        for i in 0..pts.len() - 1 {
            pa.line_segment([pts[i], pts[i + 1]], Stroke::new(2.0, ga(CYAN, 70)));
        }
        // Crisp main line — 1.5px so it's visible at any DPI
        for i in 0..pts.len() - 1 {
            pa.line_segment([pts[i], pts[i + 1]], Stroke::new(1.5, Color32::from_rgba_premultiplied(0, 210, 240, 230)));
        }
    }

    // ── Detection band overlay ────────────────────────────────────────────────
    let min_x = inner.min.x + freq_to_x(p.min_freq as f32, inner.width());
    let max_x = inner.min.x + freq_to_x(p.max_freq as f32, inner.width());
    if max_x > min_x {
        let br = Rect::from_min_max(
            Pos2::new(min_x, inner.min.y),
            Pos2::new(max_x, inner.min.y + ph));
        pa.rect_filled(br, 0.0, ga(PURPLE, 22));
        pa.line_segment([Pos2::new(min_x, inner.min.y), Pos2::new(min_x, inner.min.y + ph)],
            Stroke::new(1.5, ga(MAGENTA, 200)));
        pa.line_segment([Pos2::new(min_x, inner.min.y), Pos2::new(min_x, inner.min.y + ph)],
            Stroke::new(5.0, ga(MAGENTA, 38)));
        pa.line_segment([Pos2::new(max_x, inner.min.y), Pos2::new(max_x, inner.min.y + ph)],
            Stroke::new(1.5, ga(GOLD, 200)));
        pa.line_segment([Pos2::new(max_x, inner.min.y), Pos2::new(max_x, inner.min.y + ph)],
            Stroke::new(5.0, ga(GOLD, 38)));
    }

    // ── Draggable frequency nodes ─────────────────────────────────────────────
    let node_y = inner.min.y + ph * 0.5;

    let min_hit = Rect::from_center_size(Pos2::new(min_x, node_y), Vec2::splat(22.0));
    let mr = ui.allocate_rect(min_hit, Sense::drag());
    if mr.dragged() {
        let nx = (min_x + mr.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        ch.min_freq = Some(
            (x_to_freq(nx, inner.width()) as f64).clamp(1000.0, p.max_freq - 100.0));
    }

    let max_hit = Rect::from_center_size(Pos2::new(max_x, node_y), Vec2::splat(22.0));
    let xr = ui.allocate_rect(max_hit, Sense::drag());
    if xr.dragged() {
        let nx = (max_x + xr.drag_delta().x - inner.min.x).clamp(0.0, inner.width());
        ch.max_freq = Some(
            (x_to_freq(nx, inner.width()) as f64).clamp(p.min_freq + 100.0, 20000.0));
    }

    freq_node(&pa, Pos2::new(min_x, node_y), MAGENTA, "MIN");
    freq_node(&pa, Pos2::new(max_x, node_y), GOLD,    "MAX");

    pa.text(Pos2::new(inner.min.x + 6.0, inner.min.y + 7.0),
        egui::Align2::LEFT_CENTER,
        "SPECTRUM ANALYZER",
        FontId::new(6.5, FontFamily::Monospace), ga(PURPLE, 170));

    // Force continuous repaint so the spectrum animates
    ui.ctx().request_repaint();
}

fn freq_node(pa: &egui::Painter, c: Pos2, col: Color32, lbl: &str) {
    pa.circle_filled(c, 9.0, ga(col, 22));
    pa.circle_filled(c, 6.5, ga(col, 140));
    pa.circle_stroke(c, 6.5, Stroke::new(1.5, Color32::WHITE));
    pa.text(Pos2::new(c.x, c.y-14.0), egui::Align2::CENTER_CENTER, lbl, FontId::new(6.5, FontFamily::Monospace), col);
}

// ─── Numeric Popup ───────────────────────────────────────────────────────────
fn draw_num_popup(ctx: &Context, gui: &mut NebulaGui, ch: &mut GuiChanges) {
    let sc  = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(220.0, 110.0));
    let fr  = Rect::from_center_size(Pos2::new(pop.center().x, pop.center().y-4.0), Vec2::new(180.0, 23.0));
    let ok  = Rect::from_center_size(Pos2::new(pop.center().x-44.0, pop.max.y-15.0), Vec2::new(68.0, 18.0));
    let cx_ = Rect::from_center_size(Pos2::new(pop.center().x+44.0, pop.max.y-15.0), Vec2::new(68.0, 18.0));
    let lbl = gui.num_input.label.clone();
    egui::Area::new(egui::Id::new("neb_num")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        { let p = ui.painter();
          p.rect_filled(sc, 0.0, Color32::from_black_alpha(170));
          p.rect_filled(pop, 7.0, BG_PANEL);
          p.rect_stroke(pop, 7.0, Stroke::new(2.0, CYAN), egui::StrokeKind::Outside);
          p.rect_stroke(pop, 7.0, Stroke::new(9.0, ga(CYAN,28)), egui::StrokeKind::Outside);
          p.text(Pos2::new(pop.center().x, pop.min.y+16.0), egui::Align2::CENTER_CENTER,
              format!("SET  {}", lbl), FontId::new(9.5, FontFamily::Monospace), CYAN);
          p.rect_filled(fr, 3.0, BG_DEEP);
          p.rect_stroke(fr, 3.0, Stroke::new(1.2, ga(PURPLE,200)), egui::StrokeKind::Outside);
          p.rect_filled(ok, 3.0, Color32::from_rgb(0,50,70)); p.rect_stroke(ok, 3.0, Stroke::new(1.0, CYAN), egui::StrokeKind::Outside);
          p.text(ok.center(), egui::Align2::CENTER_CENTER, "OK", FontId::new(8.5, FontFamily::Monospace), CYAN);
          p.rect_filled(cx_, 3.0, Color32::from_rgb(55,0,0)); p.rect_stroke(cx_, 3.0, Stroke::new(1.0, MAGENTA), egui::StrokeKind::Outside);
          p.text(cx_.center(), egui::Align2::CENTER_CENTER, "CANCEL", FontId::new(8.5, FontFamily::Monospace), MAGENTA); }
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(fr), |ui| {
            let te = egui::TextEdit::singleline(&mut gui.num_input.value_str)
                .font(FontId::new(10.0, FontFamily::Monospace))
                .text_color(CYAN).frame(false).desired_width(178.0);
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
fn draw_preset_save(ctx: &Context, gui: &mut NebulaGui, p: &GuiParams, _ch: &mut GuiChanges) {
    let sc  = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(250.0, 112.0));
    let fr  = Rect::from_center_size(Pos2::new(pop.center().x, pop.center().y-4.0), Vec2::new(210.0, 23.0));
    let ok  = Rect::from_center_size(Pos2::new(pop.center().x-48.0, pop.max.y-15.0), Vec2::new(74.0, 18.0));
    let cx_ = Rect::from_center_size(Pos2::new(pop.center().x+48.0, pop.max.y-15.0), Vec2::new(74.0, 18.0));
    egui::Area::new(egui::Id::new("neb_prsave")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        { let pa = ui.painter();
          pa.rect_filled(sc, 0.0, Color32::from_black_alpha(170));
          pa.rect_filled(pop, 7.0, BG_PANEL);
          pa.rect_stroke(pop, 7.0, Stroke::new(2.0, GOLD), egui::StrokeKind::Outside);
          pa.rect_stroke(pop, 7.0, Stroke::new(9.0, ga(GOLD,25)), egui::StrokeKind::Outside);
          pa.text(Pos2::new(pop.center().x, pop.min.y+16.0), egui::Align2::CENTER_CENTER,
              "SAVE PRESET", FontId::new(9.5, FontFamily::Monospace), GOLD);
          pa.rect_filled(fr, 3.0, BG_DEEP);
          pa.rect_stroke(fr, 3.0, Stroke::new(1.2, ga(GOLD,180)), egui::StrokeKind::Outside);
          pa.rect_filled(ok, 3.0, Color32::from_rgb(40,32,0)); pa.rect_stroke(ok, 3.0, Stroke::new(1.0, GOLD), egui::StrokeKind::Outside);
          pa.text(ok.center(), egui::Align2::CENTER_CENTER, "SAVE", FontId::new(8.5, FontFamily::Monospace), GOLD);
          pa.rect_filled(cx_, 3.0, Color32::from_rgb(55,0,0)); pa.rect_stroke(cx_, 3.0, Stroke::new(1.0, MAGENTA), egui::StrokeKind::Outside);
          pa.text(cx_.center(), egui::Align2::CENTER_CENTER, "CANCEL", FontId::new(8.5, FontFamily::Monospace), MAGENTA); }
        ui.allocate_new_ui(egui::UiBuilder::new().max_rect(fr), |ui| {
            let te = egui::TextEdit::singleline(&mut gui.preset_name_buf)
                .font(FontId::new(10.0, FontFamily::Monospace))
                .text_color(GOLD).frame(false).desired_width(208.0).hint_text("Preset name…");
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
    if let Some(idx) = gui.presets.iter().position(|(n,_)| n==&name) {
        gui.presets[idx].1 = snap; gui.selected_preset = idx;
    } else {
        gui.presets.push((name, snap)); gui.selected_preset = gui.presets.len()-1;
    }
    gui.preset_save_popup = false;
}

// ─── MIDI Learn Popup ────────────────────────────────────────────────────────
fn draw_midi_popup(ctx: &Context, gui: &mut NebulaGui) {
    let sc  = ctx.screen_rect();
    let pop = Rect::from_center_size(sc.center(), Vec2::new(280.0, 310.0));
    egui::Area::new(egui::Id::new("neb_midi")).fixed_pos(Pos2::ZERO).order(egui::Order::Foreground).show(ctx, |ui| {
        { let pa = ui.painter();
          pa.rect_filled(sc, 0.0, Color32::from_black_alpha(170));
          pa.rect_filled(pop, 7.0, BG_PANEL);
          pa.rect_stroke(pop, 7.0, Stroke::new(2.0, MAGENTA), egui::StrokeKind::Outside);
          pa.rect_stroke(pop, 7.0, Stroke::new(9.0, ga(MAGENTA,25)), egui::StrokeKind::Outside);
          pa.text(Pos2::new(pop.center().x, pop.min.y+16.0), egui::Align2::CENTER_CENTER,
              "MIDI LEARN", FontId::new(10.0, FontFamily::Monospace), MAGENTA);
          pa.text(Pos2::new(pop.center().x, pop.min.y+32.0), egui::Align2::CENTER_CENTER,
              "Click parameter → move CC knob", FontId::new(7.0, FontFamily::Monospace), TEXT_MID); }

        let learning = gui.midi_learn.learning_target.load(std::sync::atomic::Ordering::Relaxed);
        let mappings = gui.midi_learn.mappings.lock().clone();

        for (idx, &name) in MIDI_PARAM_NAMES.iter().enumerate().take(MIDI_PARAM_COUNT) {
            let cc_s: String = mappings.iter()
                .find(|(_,&v)| v==idx as u8)
                .map(|(&cc,_)| format!("CC{}", cc))
                .unwrap_or_else(|| "─".to_string());
            let ih = 21.0;
            let rr = Rect::from_min_size(
                Pos2::new(pop.min.x+10.0, pop.min.y+46.0+idx as f32*ih),
                Vec2::new(pop.width()-20.0, ih-2.0));
            let resp = ui.allocate_rect(rr, Sense::click());
            let isl  = learning == idx as i32;
            let hov  = resp.hovered();
            { let pa = ui.painter_at(Rect::EVERYTHING);
              pa.rect_filled(rr, 3.0, if isl{ga(MAGENTA,38)} else if hov{ga(MAGENTA,15)} else{BG_WIDGET});
              pa.rect_stroke(rr, 3.0, Stroke::new(if isl{1.2} else{0.6}, if isl{MAGENTA} else {ga(MAGENTA,50)}), egui::StrokeKind::Outside);
              pa.text(Pos2::new(rr.min.x+8.0, rr.center().y), egui::Align2::LEFT_CENTER,
                  name, FontId::new(7.5, FontFamily::Monospace), if isl{MAGENTA} else{TEXT_HI});
              pa.text(Pos2::new(rr.max.x-8.0, rr.center().y), egui::Align2::RIGHT_CENTER,
                  &cc_s, FontId::new(7.5, FontFamily::Monospace), if isl{MAGENTA} else{CYAN}); }
            if resp.clicked() {
                let t = if isl { -1 } else { idx as i32 };
                gui.midi_learn.learning_target.store(t, std::sync::atomic::Ordering::Release);
            }
        }

        let clr = Rect::from_center_size(Pos2::new(pop.center().x-52.0, pop.max.y-16.0), Vec2::new(82.0, 20.0));
        let cls = Rect::from_center_size(Pos2::new(pop.center().x+52.0, pop.max.y-16.0), Vec2::new(82.0, 20.0));
        { let pa = ui.painter_at(Rect::EVERYTHING);
          pa.rect_filled(clr, 4.0, Color32::from_rgb(55,0,0)); pa.rect_stroke(clr, 4.0, Stroke::new(1.0, RED_HOT), egui::StrokeKind::Outside);
          pa.text(clr.center(), egui::Align2::CENTER_CENTER, "CLEAR ALL", FontId::new(7.5, FontFamily::Monospace), RED_HOT);
          pa.rect_filled(cls, 4.0, Color32::from_rgb(0,34,50)); pa.rect_stroke(cls, 4.0, Stroke::new(1.0, CYAN), egui::StrokeKind::Outside);
          pa.text(cls.center(), egui::Align2::CENTER_CENTER, "CLOSE", FontId::new(7.5, FontFamily::Monospace), CYAN); }
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

// ─── MIDI Context Menu (Right‑Click) ─────────────────────────────────────────
fn draw_midi_context_menu(ctx: &Context, gui: &mut NebulaGui) {
    let menu_w = 180.0;
    let ih = 22.0;
    let menu_h = 5.0 * ih + 10.0; // 5 items + padding
    let anchor = gui.midi_context_anchor;
    let menu_rect = Rect::from_min_size(anchor, Vec2::new(menu_w, menu_h));

    // Click outside to close
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("neb_midi_ctx_bg"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let full = ui.allocate_rect(screen, Sense::click());
            if full.clicked() { gui.midi_context_menu = false; }
        });

    egui::Area::new(egui::Id::new("neb_midi_ctx"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            // Background panel
            { let p = ui.painter();
              p.rect_filled(menu_rect, 5.0, BG_PANEL);
              p.rect_stroke(menu_rect, 5.0, Stroke::new(1.2, ga(MAGENTA, 80)), egui::StrokeKind::Outside);
              p.rect_stroke(menu_rect, 5.0, Stroke::new(5.0, ga(MAGENTA, 18)), egui::StrokeKind::Outside); }

            let items = [
                ("MIDI On/Off", 0),
                ("Clean Up...", 1),
                ("Roll Back", 2),
                ("Save", 3),
                ("Close", 4),
            ];

            for (i, (label, idx)) in items.iter().enumerate() {
                let item_rect = Rect::from_min_size(
                    Pos2::new(anchor.x + 5.0, anchor.y + 5.0 + i as f32 * ih),
                    Vec2::new(menu_w - 10.0, ih - 2.0));
                let resp = ui.allocate_rect(item_rect, Sense::click());
                let hov = resp.hovered();
                
                { let p = ui.painter();
                  if hov { p.rect_filled(item_rect, 3.0, ga(MAGENTA, 15)); }
                  p.text(Pos2::new(item_rect.min.x + 10.0, item_rect.center().y),
                      egui::Align2::LEFT_CENTER, *label,
                      FontId::new(8.0, FontFamily::Monospace),
                      if hov { MAGENTA } else { TEXT_HI }); }
                
                if resp.clicked() {
                    match idx {
                        0 => { // MIDI On/Off
                            let current = gui.midi_learn.midi_enabled.load(std::sync::atomic::Ordering::Relaxed);
                            gui.midi_learn.midi_enabled.store(!current, std::sync::atomic::Ordering::Release);
                        }
                        1 => { // Clean Up - show submenu
                            gui.midi_cleanup_menu = true;
                            gui.midi_cleanup_anchor = Pos2::new(item_rect.max.x + 2.0, item_rect.min.y);
                        }
                        2 => { // Roll Back
                            let saved = gui.midi_learn.saved_mappings.lock().clone();
                            *gui.midi_learn.mappings.lock() = saved;
                        }
                        3 => { // Save
                            let current = gui.midi_learn.mappings.lock().clone();
                            *gui.midi_learn.saved_mappings.lock() = current;
                        }
                        4 => { // Close
                            gui.midi_context_menu = false;
                        }
                        _ => {}
                    }
                    if *idx != 1 { // Don't close if opening submenu
                        gui.midi_context_menu = false;
                    }
                }
            }

            // Close on Escape
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.midi_context_menu = false;
            }
        });
    
    // Render Clean Up submenu if open
    if gui.midi_cleanup_menu {
        draw_midi_cleanup_menu(ctx, gui);
    }
}

// ─── MIDI Clean Up Submenu ───────────────────────────────────────────────────
fn draw_midi_cleanup_menu(ctx: &Context, gui: &mut NebulaGui) {
    let mappings = gui.midi_learn.mappings.lock().clone();
    let item_count = mappings.len() + 1; // +1 for "Clear All"
    let sub_w = 220.0;
    let ih = 22.0;
    let sub_h = item_count as f32 * ih + 10.0;
    let anchor = gui.midi_cleanup_anchor;
    let sub_rect = Rect::from_min_size(anchor, Vec2::new(sub_w, sub_h));

    // Click outside to close
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("neb_midi_clean_bg"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let full = ui.allocate_rect(screen, Sense::click());
            if full.clicked() { gui.midi_cleanup_menu = false; }
        });

    egui::Area::new(egui::Id::new("neb_midi_clean"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            // Background panel
            { let p = ui.painter();
              p.rect_filled(sub_rect, 5.0, BG_PANEL);
              p.rect_stroke(sub_rect, 5.0, Stroke::new(1.2, ga(CYAN, 80)), egui::StrokeKind::Outside);
              p.rect_stroke(sub_rect, 5.0, Stroke::new(5.0, ga(CYAN, 18)), egui::StrokeKind::Outside); }

            // Header
            { let p = ui.painter();
              let header_rect = Rect::from_min_size(
                  Pos2::new(anchor.x + 5.0, anchor.y + 5.0),
                  Vec2::new(sub_w - 10.0, ih - 2.0));
              p.rect_filled(header_rect, 3.0, ga(CYAN, 20));
              p.text(Pos2::new(header_rect.center().x, header_rect.center().y),
                  egui::Align2::CENTER_CENTER, "MIDI Associations",
                  FontId::new(7.5, FontFamily::Monospace), CYAN); }

            // List associations
            let mut sorted_mappings: Vec<(u8, u8)> = mappings.iter().map(|(&cc, &param)| (cc, param)).collect();
            sorted_mappings.sort_by_key(|&(cc, _)| cc);

            for (i, &(cc, param_idx)) in sorted_mappings.iter().enumerate() {
                let y_pos = anchor.y + 5.0 + (i + 1) as f32 * ih;
                let item_rect = Rect::from_min_size(
                    Pos2::new(anchor.x + 5.0, y_pos),
                    Vec2::new(sub_w - 10.0, ih - 2.0));
                let resp = ui.allocate_rect(item_rect, Sense::click());
                let hov = resp.hovered();
                
                let param_name = if param_idx < MIDI_PARAM_COUNT as u8 {
                    MIDI_PARAM_NAMES[param_idx as usize]
                } else {
                    "Unknown"
                };
                
                { let p = ui.painter();
                  if hov { p.rect_filled(item_rect, 3.0, ga(RED_HOT, 15)); }
                  p.text(Pos2::new(item_rect.min.x + 10.0, item_rect.center().y),
                      egui::Align2::LEFT_CENTER,
                      format!("CC{} → {}", cc, param_name),
                      FontId::new(7.5, FontFamily::Monospace),
                      if hov { RED_HOT } else { TEXT_HI }); }
                
                if resp.clicked() {
                    gui.midi_learn.mappings.lock().remove(&cc);
                }
            }

            // Clear All button
            let clear_y = anchor.y + 5.0 + (sorted_mappings.len() + 1) as f32 * ih;
            let clear_rect = Rect::from_min_size(
                Pos2::new(anchor.x + 5.0, clear_y),
                Vec2::new(sub_w - 10.0, ih - 2.0));
            let resp = ui.allocate_rect(clear_rect, Sense::click());
            let hov = resp.hovered();
            
            { let p = ui.painter();
              if hov { p.rect_filled(clear_rect, 3.0, ga(RED_HOT, 20)); }
              p.text(Pos2::new(clear_rect.center().x, clear_rect.center().y),
                  egui::Align2::CENTER_CENTER, "Clear All",
                  FontId::new(7.5, FontFamily::Monospace),
                  if hov { RED_HOT } else { TEXT_HI }); }
            
            if resp.clicked() {
                gui.midi_learn.mappings.lock().clear();
                gui.midi_cleanup_menu = false;
                gui.midi_context_menu = false;
            }

            // Close on Escape
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.midi_cleanup_menu = false;
            }
        });
}

// ─── Floating OS Dropdown ─────────────────────────────────────────────────────
// Rendered as a top-level Area so it is never clipped by the toolbar rect.
fn draw_os_dropdown(ctx: &Context, gui: &mut NebulaGui, params: &GuiParams, ch: &mut GuiChanges) {
    let os_labels = ["OFF", "2×", "4×", "6×", "8×"];
    let os_w = 94.0;
    let ih   = 20.0;
    let drop_h = os_labels.len() as f32 * ih + 6.0;
    let anchor = gui.os_anchor;
    let dr = Rect::from_min_size(anchor, Vec2::new(os_w, drop_h));

    // Click outside to close
    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("neb_os_bg"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let full = ui.allocate_rect(screen, Sense::click());
            if full.clicked() { gui.os_dropdown = false; }
        });

    egui::Area::new(egui::Id::new("neb_os_drop"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)   // above the bg-dismiss layer
        .show(ctx, |ui| {
            // Background panel
            { let p = ui.painter();
              p.rect_filled(dr, 5.0, BG_PANEL);
              p.rect_stroke(dr, 5.0, Stroke::new(1.2, ga(GOLD, 80)), egui::StrokeKind::Outside);
              p.rect_stroke(dr, 5.0, Stroke::new(5.0, ga(GOLD, 18)), egui::StrokeKind::Outside); }

            for (i, &lbl) in os_labels.iter().enumerate() {
                let ir = Rect::from_min_size(
                    Pos2::new(anchor.x + 3.0, anchor.y + 3.0 + i as f32 * ih),
                    Vec2::new(os_w - 6.0, ih - 2.0));
                let resp = ui.allocate_rect(ir, Sense::click());
                let isel = i == params.oversampling as usize;
                let hov  = resp.hovered();
                let c    = if isel { GOLD } else if hov { ga(GOLD, 200) } else { TEXT_MID };
                { let p = ui.painter();
                  if isel { p.rect_filled(ir, 3.0, ga(GOLD, 28)); }
                  else if hov { p.rect_filled(ir, 3.0, ga(GOLD, 12)); }
                  if isel {
                      p.rect_stroke(ir, 3.0, Stroke::new(0.8, ga(GOLD, 80)), egui::StrokeKind::Outside);
                  }
                  p.text(Pos2::new(ir.min.x + 10.0, ir.center().y),
                      egui::Align2::LEFT_CENTER, lbl,
                      FontId::new(8.5, FontFamily::Monospace), c); }
                if resp.clicked() {
                    ch.oversampling = Some(i as u32);
                    gui.os_dropdown = false;
                }
            }

            // Close on Escape
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                gui.os_dropdown = false;
            }
        });
}

// ─── Floating Preset Dropdown ─────────────────────────────────────────────────
fn draw_preset_dropdown(ctx: &Context, gui: &mut NebulaGui, ch: &mut GuiChanges) {
    if gui.presets.is_empty() { gui.preset_dropdown_open = false; return; }

    let pw   = 148.0;
    let ih   = 20.0;
    let drop_h = gui.presets.len() as f32 * ih + 6.0;
    let anchor = gui.preset_anchor;
    let dr = Rect::from_min_size(anchor, Vec2::new(pw, drop_h));

    let screen = ctx.screen_rect();
    egui::Area::new(egui::Id::new("neb_pr_bg"))
        .fixed_pos(Pos2::ZERO)
        .order(egui::Order::Foreground)
        .show(ctx, |ui| {
            let full = ui.allocate_rect(screen, Sense::click());
            if full.clicked() { gui.preset_dropdown_open = false; }
        });

    let presets_clone = gui.presets.clone();
    egui::Area::new(egui::Id::new("neb_pr_drop"))
        .fixed_pos(anchor)
        .order(egui::Order::Tooltip)
        .show(ctx, |ui| {
            { let p = ui.painter();
              p.rect_filled(dr, 5.0, BG_PANEL);
              p.rect_stroke(dr, 5.0, Stroke::new(1.2, ga(CYAN, 80)), egui::StrokeKind::Outside);
              p.rect_stroke(dr, 5.0, Stroke::new(5.0, ga(CYAN, 16)), egui::StrokeKind::Outside); }

            for (i, (name, snap)) in presets_clone.iter().enumerate() {
                let ir = Rect::from_min_size(
                    Pos2::new(anchor.x + 3.0, anchor.y + 3.0 + i as f32 * ih),
                    Vec2::new(pw - 6.0, ih - 2.0));
                let resp = ui.allocate_rect(ir, Sense::click());
                let isel = i == gui.selected_preset;
                let hov  = resp.hovered();
                let c    = if isel { CYAN } else if hov { ga(CYAN, 200) } else { TEXT_MID };
                { let p = ui.painter();
                  if isel { p.rect_filled(ir, 3.0, ga(CYAN, 24)); }
                  else if hov { p.rect_filled(ir, 3.0, ga(CYAN, 10)); }
                  if isel {
                      p.rect_stroke(ir, 3.0, Stroke::new(0.8, ga(CYAN, 80)), egui::StrokeKind::Outside);
                  }
                  let display = if name.len() > 20 { &name[..20] } else { name };
                  p.text(Pos2::new(ir.min.x + 10.0, ir.center().y),
                      egui::Align2::LEFT_CENTER, display,
                      FontId::new(8.0, FontFamily::Monospace), c); }
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
