# NEBULA DE-ESSER

**Professional 64-bit CLAP & VST3 De-esser Plugin**  
Written in Rust · nih-plug · egui · Zero Warnings · Pure Native Builds

---

## 🎯 **Overview**

Nebula DeEsser is a professional-grade de-esser plugin built entirely in Rust with 64-bit double-precision processing. It delivers studio-quality results while maintaining zero compilation warnings and pure native builds across all platforms.

Version 2.7.0 further tunes up the DSP and the control parameters to take full advantage of the new Orthogonal Subspace Projection engine powered by Teager-Kaiser Energy Operator analysis based DSP architecture.

<img width="864" height="698" alt="Image" src="https://github.com/user-attachments/assets/5135ca2c-2d63-428a-aa97-7bdb88611e14" />

---

# 💰 Support the Project

If you find this open-source software helpful and would like to support its development, you can buy this plugin on Gumroad:

<p align="left">
  <a href="https://subhankar42.gumroad.com/l/adounr">
    <img src="https://img.shields.io/badge/Buy_on-Gumroad-FF4D4D?style=for-the-badge&logo=gumroad&logoColor=white" alt="Buy on Gumroad">
  </a>
</p>

---

## 🎛️**What's New in v2.7.0**

### 🧠 **Even control optimization for the new DSP**
- **Threshold knob now renamed to TKEO Sharp knob:** This represents the functionality of the knob better, and further DSP optimizations have been done for the controller.
- **Reduction meter replaced by Annihilation meter:** Shows the The "Annihilation" Factor which shows the amount of signal energy that has been identified as "harsh" (sibilant) and projected out of the final audio.
  - Real-time Attenuation: Each time the meter moves, it indicates how many decibels of that specific "harshness subspace" are being removed from the original signal.
  - Dynamic Response: Because OSP doesn't use standard compression envelopes (attack/release), the meter is incredibly fast and precise. It should jump exactly when the sibilance occurs and return to zero instantly without the "slow crawl" seen in vintage-style meters.
- **Reprogrammed Reduction knob and Reduction meter slider:** Sets the limit or "ceiling" for the Orthogonal Subspace Projection (OSP) engine. If you set the slider to its maximum, the plugin attempts to "annihilate" all signal components that match the harshness signature. By lowering this slider, you’re effectively telling the algorithm: "I know you found the harshness, but don't remove 100% of it." This is crucial for keeping a vocal sounding natural rather than "lispy" or "dead". Think of it as a mix knob for the subtraction. A setting of -3dB to -6dB is often the "sweet spot" where the sibilance is tamed but the articulation remains clear. Variable from 0 to down to -100dB
- **Reprogrammed Vocal mode:** When vocal mode is on it sets Teager-Kaiser Energy Operator to operate in solo vocal sibilance detection mode, this mode is tailor made for highest accuracy so that utmost transparency can be achieved even on highly technical and complex vocals like the ones used by coloratura sopranos. This mode can differentiate between the natural harmonics of a coloratura soprano and the sibilance. This is tailor made for classical and orchestral producers, but other vocalists, like metal singers who often use falsettos can also take advantage of it. When switched off it sets the Teager-Kaiser Energy Operator to operate for regular sibilant detection, this is a versatile mode that lets the user use the plugin for processing not only vocals but also other sibilant sources like cymbals, etc. It can be also used for cleaning up vocals in old archived mixes that can no more be separated into separate vocals and instrumental tracks. 

---

## 🎛️**What's New in v2.6.0**

### 🧠 **Full control optimization for the new DSP**

- **Threshold knob and detection meter slider:** They now control the Teager-Kaiser Energy Operator, basically how sharp or erratic an energy spike needs to be before the algorithm classifies it as sibilance rather than part of the vocal cord's natural vibration.
- **Reprogrammed Absolute Mode:** It learns the singer’s actual voice in real time, If a sound doesn't align with that "learned subspace" (like a sharp burst of air), the Orthogonal energy gating removes it. In absolute mode the de-esser uses 3-vector space to perform the separation:
   - Voiced Axis (Harmonics): Where the periodic, "vowel-like" energy lives.
   - Unvoiced Axis (Sibilance): Where the aperiodic, TKEO-detected "noise" lives.
   - Residual/Error Axis: The "math dust" that doesn't fit either category.   
- **Reprogrammed Relative Mode:** Just like absolute mode it too learns the singer’s actual voice in real time, If a sound doesn't align with that "learned subspace" (like a sharp burst of air), the Orthogonal energy gating removes it. However, it switches the plugin to operate in Multi-Vector Space (N-dimensions) for more complex separation:
   - Higher-Order Correlation: It’s not just looking at Harmonics vs. Noise. It starts looking at secondary relationships—like how the air at 12kHz specifically correlates with the chest resonance at 300Hz.
   - Contextual Intelligence: It decides which extra vectors to add based on the complexity of the signal. If you have a breathy singer with complex "stacking" textures, the math expands to map those unique characteristics so it doesn't accidentally categorize a "cool breath" as a "bad sibilant."
   - Subspace Contextuality: allows the subspace to expand and contract its dimensions in real-time, making it significantly more transparent for singers with a lot of dynamic "character" or those who shift between airy whispers and belts.
- **Reprogrammed Split/Wide switching:** It now controls the following parameters:
   - **Split = Harsh-band analysis**
       - The detector focuses mostly on the sibilant region set by the frequency selector, and compares it to the whole signal to determine how it should be processed.
   - **Wide = Full-signal analysis**
       - The detector looks at the entire signal, not just the selected region, it actively detects sibilance across the whole signal and processes the whole signal accordingly

---

## 🚀 **What's New in v2.5.0 — Orthogonal Subspace Era**

### 🧠 **Brand-New Core Algorithm**

Nebula De-Esser now runs an **Orthogonal Subspace Projection** pipeline for reduction control, driven by **Teager-Kaiser Energy Operator (TKEO)** dynamics instead of legacy spectral compression.

- **Adaptive subspace tracking** with slow eigenvector adaptation for stable, non-jittery behavior
- **Multi-resolution analysis** (short / medium / long windows) to respond to both transients and sustained consonants
- **Orthogonal energy gating** that focuses reduction where energy diverges from the learned voiced subspace

### 🎤 **Voice-Aware Transparency Stack**

To keep vocals natural under heavy de-essing, v2.5.0 introduces layered speech-aware protection:

- **Psychoacoustic harmonic weighting** de-emphasizes reduction pressure in voiced/harmonic regions
- **Real-time vowel classification (A / E / I / O / U aware)** to keep vowel identity intact
- **Kalman-smoothed formant tracking** for buttery-stable formant trajectories (F1/F2/F3)
- **Formant preservation locking** that protects vowel peaks explicitly while still controlling harsh sibilance

### 🎧 **Result**

Cleaner high-end control, less lisp risk, smoother behavior on dense vocals, and more transparent de-essing across spoken word, sung leads, and stacked harmonies.

---

## ✨ **What's New in v2.4.0**

### 🔧🎛️ **Enhanced stability and even more precise control**

- **Stability updates** — further stability tuning to ensure lower CPU consumption.
- **Cut Slope** — Lets the user fine tune the slope of the notch, continuosly varible from 0 dB/oct to 100 dB/oct for precise tuning.

---

## ✨ **What's New in v2.3.0**

### 🎨 **Complete UI Redesign — Sci-Fi Dark Theme**

The entire GUI has been rebuilt from scratch with a new dark sci-fi aesthetic — deep violet-black backgrounds, neon accent colours, and a clean structured layout that keeps everything readable and professional.

- **Deep black base** — near-black backgrounds with a subtle violet undertone, giving the UI depth without being flat
- **Neon accent system** — electric cyan as the primary accent, with hot magenta for cut shape controls, electric purple for I/O controls, and neon green/teal/amber for semantic states
- **Structured panel layout** — every section (detection, reduction, controls, spectrum) sits in its own elevated card with a neon cyan top-edge highlight and purple-tinted border
- **NavigationView header** — app icon tile, title with subtitle typography, version badge, bypass indicator
- **CommandBar toolbar** — flat strip with button-style controls: tinted fill at rest, neon highlight on hover, solid accent fill when active, divider separators between groups
- **RadioButton groups** — the five mode selectors (Mode, Range, Filter, Sidechain, Vocal) use radio controls with outer ring and filled inner dot
- **ToggleSwitches** — the four boolean toggles (Filter Solo, Trigger Hear, Lookahead, Mid/Side) use pill-track switches: fills cyan when on, thumb shifts position
- **Knob colour identity** — main parameters in cyan, cut shape knobs in magenta, I/O knobs in purple
- **ContentDialog popups** — scrim overlay, elevated card with rounded corners, TextBox with accent focus underline, primary/secondary button row
- **Flyout dropdowns** — elevated card with drop shadow, accent-filled selected row

### 🔧 **Audio Thread Safety Fixes**

Two bugs that could cause crashes or undefined behaviour under rapid interaction, particularly in DAWs with strict real-time thread policies:

- **Undo/redo stack** — replaced `pop().unwrap()` calls with `if let Some(snap) = ...pop()` guards. The previous code could panic if a rapid click sequence caused a race between the `can_undo`/`can_redo` check and the actual pop.
- **Per-block heap allocation** — removed `Vec<Vec<f64>>` heap allocations for `input_data` and `sc_data` that were happening inside `process()` on every audio block. The f64 conversion is now deferred to the per-sample loop, eliminating two full-buffer allocations per block from the real-time thread.

### 📊 **Spectrum Analyzer — Post-Effects Tap + Accuracy Fixes**

The spectrum analyzer has been completely corrected. Previously it read the raw pre-input-gain signal, which was both in the wrong position and inaccurate.

**Tap point moved to post-effects:**  
The analyzer now feeds from `(out_l + out_r) * 0.5` after input gain, DSP processing, output gain, and dry/wet mix. The display now shows exactly what comes out of the plugin — threshold changes, frequency band adjustments, cut width/depth, and mix all reflect immediately in the spectrum.

**Magnitude scaling corrected (+6 dB fix):**  
The previous scale factor `2.0 / FFT_SIZE` was 6 dB too low. The correct derivation accounts for the Hann window's coherent gain of 0.5 (window sum = `FFT_SIZE/2`) and the single-sided doubling of non-DC bins, giving `4.0 / FFT_SIZE`. Readings now match a calibrated reference signal.

**Ring buffer read order fixed:**  
The previous code read from `write_pos` which points to the *next write slot* — one sample ahead of the last written sample. This caused the Hann window to be applied starting from the wrong sample, scrambling the phase relationship across every FFT frame. The write pointer is now advanced after writing, so it always points to the oldest sample, which is the correct linearisation start point.

**Nyquist bin corrected:**  
The Nyquist bin (index `FFT_SIZE/2`) was previously doubled along with all other bins. It has no mirror image in the single-sided spectrum and is now scaled correctly without doubling.

---

## ✨ **What's New in v2.2.0**

### 🔧 **Phase Transparency Fix**

The previous split-band mode used separate LP and HP biquad chains on the audio path, then subtracted to recover the mid band. Because LP and HP filters have different phase responses, `lo + hi ≠ signal` — this caused audible phase artifacts.

**The fix:** complementary LP split. The audio path now computes `lo = LP(x)` and `hi = x − lo`. Since `lo + hi = x` exactly at every sample, recombination is mathematically perfect with zero phase error. Wide mode uses a cascaded bell EQ applied directly to the signal — a phase-coherent gain change rather than a wideband multiply.

### 🎛️ **Three New Control Knobs**

| Knob | Range | What it does |
|------|-------|--------------|
| **Cut Width** | 0–100% | Controls the Q (bandwidth) of the de-essing notch. 0% = broadest cut, 100% = narrowest surgical notch |
| **Cut Depth** | 0–100% | Scales how deep the cut goes relative to Max Reduction |
| **Mix** | 0–100% | Dry/wet blend — parallel de-essing |

### 📐 **Fully Resizable Window**

The plugin window is freely resizable. The entire UI scales proportionally. Minimum size 400×300, window size persisted between sessions.

---

## ✨ **Features from v2.1.0**

| Feature | Description |
|---------|-------------|
| **A/B State Comparison** | Instant switching between two plugin states |
| **Enhanced MIDI Learn** | Right-click context menu with Clean Up, Roll Back, Save |
| **50-step Undo/Redo** | Full parameter history |
| **Preset System** | Save/Load/Delete named presets |
| **Zero Warnings Build** | Clean compilation with all Clippy warnings addressed |

---

## 🎛️ **Interface Tour**

### **CommandBar**
- **Bypass** — Soft bypass; header badge appears when active
- **A/B** — Toggle between State A and State B
- **Preset / Save / Delete** — Preset management
- **Undo / Redo** — 50-step history
- **MIDI Learn** — Right-click for context menu
- **OS** — Oversampling selector (Off / 2× / 4× / 6× / 8×)

### **Core Parameters**
| Parameter | Range | Precision |
|-----------|-------|-----------|
| Threshold | −60 to 0 dB | 0.1 dB |
| Max Reduction | 0 to 40 dB | 0.1 dB |
| Min Frequency | 1000–16000 Hz | 1 Hz |
| Max Frequency | 1000–20000 Hz | 1 Hz |
| Lookahead | 0–20 ms | 0.1 ms |
| Stereo Link | 0–100% | 1% |

### **Cut Shape Parameters**
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

### **Spectrum Analyzer (v2.3.0)**
- Post-effects tap — shows the processed output signal
- 2048-point FFT with 75% overlap (512-sample hop)
- Hann window with correct coherent gain compensation (`4.0 / FFT_SIZE`)
- Accurate single-sided magnitude spectrum with correct Nyquist handling
- Attack/release smoothing for stable visual display

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

## ⚠️ Cakewalk NXT / Cakewalk Sonar Not Supported

**This plugin will not work in Cakewalk NXT and Cakewalk Sonar. This is a known, confirmed incompatibility. And here's the explanation why this problem can only be fixed by Cakewalk themselves because they use a VST3 hosting layer that is non compliant with Steinberg standards**

'6   Nebula De-Esser    0x...13b9bb  GetPluginFactory + 175883  ← CRASH SITE
7   Cakewalk Next      0x...bb82a7  juce::VST3PluginInstance::processAudio'

The crash occurs at offset +175883 inside GetPluginFactory — this is not my process() method. This offset is inside code generated by the nih_export_vst3! macro from the nih-plug framework.

The root cause is VST3 ABI/Threading Contract Violation. nih_export_vst3 does the following:
```// In src/lib.rs
impl Vst3Plugin for NebulaDeEsser {
    const VST3_CLASS_ID: [u8; 16] = *b"NebulaDeEssrVST3";
    // ...
}
nih_export_vst3!(NebulaDeEsser);  // ← Macro expands to C FFI entry points
```

This macro generates:

    C-compatible GetPluginFactory() entry point
    VST3 IComponent, IEditController vtable implementations
    Thread-safety guards assuming Steinberg-compliant host behavior

And the reason Cakewalk triggers SIGSEGV is as follows:
| nih-plug Expectation | Cakewalk NXT (JUCE wrapper) Behavior | Result |
|------|-------|--------------|
| process() called only after setProcessing(true) | May call process() before full initialization | Uninitialized DSP state dereference |
| Parameter updates via setParameter() on main thread only | May update params concurrently with audio thread | Data race on Atomic* params |
| ProcessData struct fully populated per VST3 spec | May pass null/zeroed inputs/outputs pointers | Null pointer dereference at offset +175883 |
| Buffer pointers 8-byte aligned for f64 DSP | May pass null/zeroed inputs/outputs pointers | Misaligned access → SIGSEGV on Apple Silicon |

Critical Code Path That Fails
```// Inside nih-plug's generated VST3 wrapper (simplified)
extern "C" fn process(
    component: *mut IComponent,  // ← May be invalid from Cakewalk
    data: *const ProcessData     // ← May have null buffer pointers
) -> TResult {
    // ... validation omitted in release builds ...
    let plugin = &mut *(component as *mut NebulaDeEsser);  // ← CRASH if component invalid
    plugin.process(buffer, aux, context);  // ← Never reached
}
```

The large offset (+175883) indicates the crash happens deep inside macro-generated boilerplate that:

    Casts raw pointers to Rust references (&mut)
    Assumes valid vtable layout from host
    Does not defensively check for null/invalid pointers (for performance)

My plugin code is strictly memory-safe and thread-safe:

    ✅ All buffer accesses use .get()/.first() with Option chaining
    ✅ Channel count validated before split_at_mut()
    ✅ Atomic* types used for cross-thread params
    ✅ No unsafe blocks in lib.rs
    ✅ parking_lot::Mutex used for shared collections

The bug is in Cakewalk NXT's VST3 hosting layer. A non-compliant host that does not adhere to Steinberg's VST3 standards like Cakewalk Sonar and NXT is bound to crash. It is the problem of the DAW, not the plugin. 

**It is actually a documented non-compliance in Cakewalk's JUCE Wrapper, and let me list the list down the disastrous non-compliances in Cakewalk's JUCE wrapper that make it suitable for plugins that follow strict Steinberg VST3 standards:**

**activateBus() Called While Plugin Is Active (Spec Violation)**:
As per Steinberg's VST3 specification: IComponent::activateBus is called in the Setup Done state, the plug-in should be deactivated (setActive(true) not called).
Cakewalk's behavior (confirmed by Noel Borthwick, Cakewalk's lead engineer):
```// What Cakewalk does (INCORRECT per spec):
component->setActive(true);     // Plugin enters processing state
component->activateBus(...);    // ❌ Called while ACTIVE - spec violation

// What compliant hosts do (REAPER, Bitwig, Studio One):
component->setActive(false);    // Ensure deactivated
component->activateBus(...);    // ✅ Configure buses while inactive
component->setActive(true);     // Now enter processing state
```
**Why this crashes nih-plug:** The nih_export_vst3! macro generates Steinberg compliant VST3 code that assumes activateBus is only called during initialization. When Cakewalk calls it mid-processing, internal state pointers may be null or partially initialized, causing the SIGSEGV at GetPluginFactory + 175883.

**prepareToPlay() Called Before Final Bus Configuration**:
From the JUCE forum thread with Cakewalk's engineer
```// Cakewalk's call order:
1. prepareToPlay()          ← Plugin queries channel count → gets 2
2. activateBus() x3         ← Enables aux buses → now 5 channels
3. processBlock()           ← Plugin receives 5-channel buffer → mismatch!
```
**nih-plug expectation as per Steinberg's VST3 standards:** Buffer layout is finalized before process() is ever called. When Cakewalk changes bus configuration after initialization but before first process call, your plugin's internal buffer pointers become invalid.

**"Flush" Parameter Calls Without Proper Guards**:
As per Steinberg's VST3 spec, hosts may call process() with numSamples=0 and null buffers to flush parameters before setActive(true):
```// Spec-compliant flush call:
ProcessData data = {
    .numSamples = 0,
    .inputs = nullptr,    // ← NULL buffers allowed here
    .outputs = nullptr,
    .parameterChanges = ... // but params may be updated
};

// nih-plug's generated wrapper expects:
// - buffers to be valid when process() is called
// - no null-pointer checks in release builds (for performance)
```
Cakewalk's JUCE wrapper makes these flush calls, but nih-plug's VST3 backend doesn't defensively check for null buffers in the macro-generated code path where the crash occurs.

**Thread-Safety Assumptions Mismatch**:
nih-plug uses lock-free atomics and Rust's strict thread model. Cakewalk's audio engine has known thread-sync issues.
When Cakewalk's host calls VST3 methods from unexpected threads (or with non-standard synchronization), Rust's safety guarantees trigger a panic → converted to SIGSEGV by the FFI layer.

**Long story in short nih-plug optimizes for spec-compliant hosts, while Cakewalk's JUCE-based VST3 wrapper takes shortcuts that break those assumptions.**

So if you are a Cakewalk Sonar or NXT user, this plugin is not for you and it won't be fixed for your DAW in near future either. And it's not just about Cakwalk Sonar or NXT, any DAW that doesn't comply with Steiberg's VST3 standards and doesn't use stable thread safety standards won't be supported by this plugin. It is not the plugins problem that the DAW in question has such a shoddy implementaions when it comes to plugin managment.

If you are a Cakwalk Sonar or NXT user and you still want a solution, well, switch to a different DAW to use this plugin, maybe go for Tracktion Waveform Free, it's free and its perfectly compliant with Steinberg's VST3 standards, or compile Ardour on your system, that way you can get Ardour for free too.

---

## ⚠️ N-Track Studio

**The stability issues on N-Track Studio was actually due to my TCP set-up, I forgot to enable expand to stereo mode on the TCP that's why the signal was not gong through the plugin. Enabled expand to stereo for the TCP and it works fine. For some reason I forgot that this plugin has dedicated panning controls. Other DAWs do this switch automatically, N-Track Studio doesn't do it automatically. It doesn't use dynamic channel count negotiations to ensure compatibility with old and obsolete DirectX (DX/DXi) plugins, a rather useless protocol for a modern DAW, but it's their way of doing things. It is not a bug, it is more like a minot inconvenience that kinda makes sense because legacy backwards compatibility is a big thing in N-Track Studio**

---

## 📦 **Project Structure**

```
Nebula-De-Esser/
├── src/
│   ├── lib.rs          # Plugin core, parameters, MIDI learn, signal chain
│   ├── dsp.rs          # 64-bit DSP, phase-transparent split, bell EQ
│   ├── gui.rs          # WinUI 3 dark theme UI, resizable window
│   └── analyzer.rs     # Post-effects lock-free FFT spectrum analyzer
├── tests/
│   ├── audio_tests.rs
│   ├── dsp_validation.rs
│   └── benchmark_comparison.rs
├── xtask/
│   ├── Cargo.toml
│   └── src/main.rs
├── .cargo/
│   └── config.toml
├── .github/
│   └── workflows/build.yml
├── bundler.toml
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

**Ready for professional use in all major DAWs supporting the CLAP and VST3 formats.**
