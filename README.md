# NEBULA DEESSER v2.2.0

**Professional 64-bit CLAP & VST3 De-esser Plugin**  
Written in Rust · nih-plug · egui · Zero Warnings · Pure Native Builds

---

## 🎯 **Overview**

Nebula DeEsser is a state-of-the-art de-esser plugin that combines professional-grade audio processing with a stunning synthwave/alien aesthetic. Built entirely in Rust with 64-bit double-precision processing, it delivers studio-quality results while maintaining zero compilation warnings and pure native builds across all platforms.

Version 2.2.0 is a significant refinement release focused on **phase transparency**, **surgical cut control**, **parallel processing**, and **fully resizable UI** — addressing the most requested improvements from users.

<img width="1440" height="900" alt="Image" src="https://github.com/user-attachments/assets/6037962b-317f-4f83-8644-8edf1e940a4a" />

---

# 💰 Support the Project

If you find this open-source software helpful and would like to support its development, you can buy this plugin on Gumroad:

<p align="left">
  <a href="https://subhankar42.gumroad.com/l/adounr">
    <img src="https://img.shields.io/badge/Buy_on-Gumroad-FF4D4D?style=for-the-badge&logo=gumroad&logoColor=white" alt="Buy on Gumroad">
  </a>
</p>

---

## ✨ **What's New in v2.2.0**

### 🔧 **Phase Transparency Fix**

The most impactful change in this release. The previous split-band mode used separate LP and HP biquad chains on the audio path, then subtracted to recover the mid band. Because LP and HP filters have different phase responses, `lo + hi ≠ signal` — this caused audible phase artifacts that undermined the plugin's transparency claim.

**The fix:** complementary LP split. The audio path now computes `lo = LP(x)` and `hi = x − lo`. Since `lo + hi = x` exactly at every sample, recombination is mathematically perfect with zero phase error. Wide mode now uses a cascaded bell EQ applied directly to the signal — a phase-coherent gain change rather than a wideband multiply.

### 🎛️ **Three New Control Knobs**

| Knob | Range | What it does |
|------|-------|--------------|
| **Cut Width** | 0–100% | Controls the Q (bandwidth) of the de-essing notch. 0% = broadest cut, 100% = narrowest surgical notch |
| **Cut Depth** | 0–100% | Scales how deep the cut goes relative to Max Reduction. Lets you dial in a gentler treatment without touching the threshold |
| **Mix** | 0–100% | Dry/wet blend between the raw input and the processed signal. Parallel de-essing — bring back organic texture at a controlled amount |

All three sit in a dedicated row between the main knobs and the I/O section, styled in purple to visually group them as cut-shape controls.

### 📐 **Fully Resizable Window**

The plugin window is now freely resizable by dragging the corner handle (bottom-right). The entire UI — knobs, meters, spectrum analyzer, text, panels — scales proportionally using egui's zoom system. The window size is persisted between sessions.

- Drag the corner handle to resize
- Minimum size: 400×300 (nothing gets unusably small)
- Maximum size: limited only by your screen
- Window size saved and restored with the plugin state

---

## ✨ **Features from v2.1.0**

### 🆕 **Exclusive Features**

| Feature | Description |
|---------|-------------|
| **A/B State Comparison** | Instant switching between two plugin states |
| **Enhanced MIDI Learn** | Right-click context menu with Clean Up, Roll Back, Save |
| **Zero Warnings Build** | Clean compilation with all Clippy warnings addressed |
| **Pure Native Builds** | No external dependencies on any platform |

### ✅ **Complete Feature Set**

#### **Professional Preset System**
- Dropdown menu for preset management
- Save/Load/Delete presets
- 50-step undo/redo history
- Right-click numeric input for precise parameter editing

#### **Enhanced MIDI Control**
- MIDI On/Off global toggle
- Clean Up submenu showing all CC associations
- Roll Back to last saved mapping
- Save current mapping

#### **Audio Processing Suite**
- FX Bypass with visual feedback
- Input/Output Level (−60 to +12 dB)
- Input/Output Pan
- Oversampling: Off / 2× / 4× / 6× / 8×
- Live FFT Spectrum Analyzer

---

## 🎛️ **Interface Tour**

### **Toolbar**
- **⊗ BYPASS** — Soft bypass; title bar turns red when active
- **A/B** — Toggle between State A and State B
- **PRESET** — Preset dropdown
- **SAVE / DEL** — Preset operations
- **◄ UNDO / REDO ►** — 50-step history
- **MIDI LEARN** — Right-click for context menu
- **OS** — Oversampling selector

### **Core Parameters**
| Parameter | Range | Precision |
|-----------|-------|-----------|
| Threshold | −60 to 0 dB | 0.1 dB |
| Max Reduction | 0 to 40 dB | 0.1 dB |
| Min Frequency | 1000–16000 Hz | 1 Hz |
| Max Frequency | 1000–20000 Hz | 1 Hz |
| Lookahead | 0–20 ms | 0.1 ms |
| Stereo Link | 0–100% | 1% |

### **Cut Shape Parameters (New in v2.2)**
| Parameter | Range | Description |
|-----------|-------|-------------|
| Cut Width | 0–100% | Notch bandwidth (Q) |
| Cut Depth | 0–100% | Cut depth relative to Max Reduction |
| Mix | 0–100% | Dry/wet parallel blend |

### **I/O Controls**
| Parameter | Range | Description |
|-----------|-------|-------------|
| Input Level | −60 to +12 dB | Pre-DSP gain |
| Input Pan | L100 – C – R100 | Pre-DSP pan |
| Output Level | −60 to +12 dB | Post-DSP gain |
| Output Pan | L100 – C – R100 | Post-DSP pan |

---

## 🔬 **Technical Excellence**

### **64-bit DSP Engine**
- All processing in `f64` double precision
- Phase-transparent complementary LP/HP split (`hi = x − lo`)
- Cascaded bell EQ for musical notch shape
- 6th-order Butterworth detection filters
- 3-stage gain smoother for artifact-free gain changes

### **Lock-Free Architecture**
- Meter values via `AtomicU32` — zero contention
- Spectrum analyzer uses `try_lock()` — never blocks audio thread
- MIDI CC via `AtomicU32` + `AtomicBool` dirty flags

### **Performance**
| Metric | Value |
|--------|-------|
| Latency | < 5ms typical |
| CPU | < 1% per instance |
| Memory | < 50MB |
| Sample Rates | 44.1–192 kHz |
| Oversampling | 1×–8× |

---

## 🏗️ **Build Instructions**

#### **Linux (x86_64)**
```bash
chmod +x build_linux.sh && ./build_linux.sh
# Output: target/bundled/Nebula De-Esser.clap
#         target/bundled/Nebula De-Esser.vst3
```

#### **macOS (Universal Binary)**
```bash
chmod +x build_mac.sh && ./build_mac.sh
# Output: target/bundled/Nebula De-Esser.clap  (arm64 + x86_64)
#         target/bundled/Nebula De-Esser.vst3  (arm64 + x86_64)
```

#### **Windows (x86_64)**
```powershell
.\build_windows.ps1
# Output: target\bundled\Nebula De-Esser.clap
#         target\bundled\Nebula De-Esser.vst3
```

---

## 📦 **Project Structure**

```
Nebula-De-Esser/
├── src/
│   ├── lib.rs          # Plugin core, parameters, MIDI learn, VST3 export
│   ├── dsp.rs          # 64-bit DSP, phase-transparent split, bell EQ
│   ├── gui.rs          # Synthwave UI, resizable window, new knobs
│   └── analyzer.rs     # Lock-free FFT spectrum analyzer
├── tests/
│   ├── audio_tests.rs
│   ├── dsp_validation.rs
│   └── benchmark_comparison.rs
├── xtask/
│   ├── Cargo.toml      # nih_plug_xtask dependency
│   └── src/
│       └── main.rs     # cargo xtask entry point
├── .cargo/
│   └── config.toml     # Defines the `cargo xtask` alias
├── .github/
│   ├── workflows/
│   │   └── build.yml   # CI: builds CLAP + VST3 for all platforms
│   └── scripts/
│       ├── patch_vst3.py         # Injects VST3 export into lib.rs + Cargo.toml
│       └── patch_cargo_toml.py   # Ensures xtask is a workspace member
├── bundler.toml        # nih_plug_xtask bundle display name config
├── build_linux.sh
├── build_mac.sh
├── build_windows.ps1
└── Cargo.toml
```

---

## 📄 **License**

MIT License — free to use, modify, and distribute.

The VST3 plugin format is also MIT licensed. Steinberg re-licensed the VST3 SDK under the MIT License, meaning VST3 plugins carry the same permissive terms as this project with no additional obligations.

---

*"In the neon void between frequencies, where precision meets artistry, Nebula listens — and now remembers."* 🪐✨

---

**Ready for professional use in all major DAWs supporting the CLAP and VST3 formats.**

**Download includes source code with zero warnings and pure native build scripts.**
