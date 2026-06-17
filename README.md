# NEBULA DE-ESSER

**Specialist 64-bit De-esser Plugin**
Written in Rust · nih-plug · egui (macOS & Linux) · Direct2D (Windows) · Pure Native Builds

---

## 🎯 **Overview**

Nebula De-Esser is a specialist de-esser built for transparent, air-preserving vocal control. It is not intended to be the simplest possible modern clamp-style de-esser. Its purpose is closer to the polished late-80s and early-90s studio approach: bright vocals stay open, expensive, and harmonically alive while sharp sibilants are tucked back with as little damage as possible to the vocal air and upper harmonics.

The plugin uses a hybrid adaptive spectral design. Conventional control stages such as filtered detection, envelope following, threshold-style reduction targets, smoothing, psychoacoustic weighting, and formant protection decide when and how strongly the processor should act. The actual reduction is then handled by an OSP-style spectral projection/residual stage, which is designed to attenuate the harsh sibilant component without simply dulling the whole top end.

In practical terms, Nebula De-Esser is a niche finishing tool for engineers who want meticulous transparent de-essing, mastering-style Mid/Side control, and high-gloss vocal polish. If the desired sound is obvious clamp, lisp, saturation, or aggressive modern compression, this plugin can still be pushed, but that is not its main identity.

**Important Announcement** - The AUv2 build is being discontinued because while it works on all DAWs it is unstable on Logic Pro, which is the main target for the AUv2 build. So, at this point there's no reason to keep the AUv2 build. Hence it's being discontinued.

Version 3.3.0 keeps the transparent de-essing engine intact while changing release packaging: macOS now ships separate Apple Silicon and Intel builds instead of one universal binary, and Windows now ships both x86_64 and Windows 11 ARM64 VST3 builds.

---

## 🎚️ **Who This Plugin Is For**

Nebula De-Esser is for users who want obsessive, transparent control rather than fast one-knob cleanup. It is especially suited to:

- polished arena rock, metal, pop, AOR, and glossy backing-vocal production
- vocals where the air band and harmonic shine must survive de-essing
- mastering or mix-bus cases where only the Mid, only the Side, or the full Stereo image needs de-essing
- deliberate producer-driven sibilance riding using MIDI notes instead of relying only on automatic detection
- bright stacked vocals, falsettos, breathy singers, and dense harmonies where normal de-essers can dull the performance
- archival or stereo-mix cleanup where the vocal cannot be separated cleanly from the instrumental

It is deliberately more complex than a utility de-esser because the target is not just "less S." The target is controlled sibilance while preserving the impression of an open, expensive top end.

---

## ✨ **What's New in v3.3.0**

### 🏗️ **Separate native builds and discontinuation of CLAP format**

- **Separate macOS binaries** - macOS releases are now split into Apple Silicon (`aarch64-apple-darwin`) and Intel (`x86_64-apple-darwin`) binaries instead of being merged into one large universal binary.
- **Windows 11 ARM64 build** - The native Windows ARM64 VST3 build is now available.
- **Discontinuation of CLAP format** - It was observed that the CLAP format of the plugin caused major stuttering in the audio thread. So for the time being it's being discontinued till the CLAP audio thread stutter issue of nih-plug is fixed.

---

## ✨ **What's New in v3.2.0**

### 🎛️ **Purpose clarification, Mid/Side behavior, and GUI size persistence**

- **Clearer plugin identity** - The README now states the intended use case directly: Nebula De-Esser is a specialist transparent de-esser for air-preserving, high-gloss vocal control, not a generic hard-clamp de-esser.
- **Corrected Mid/Side de-essing behavior** - Stereo Link now works from 0-100% in every mode. In Stereo mode it controls stereo linking across the full stereo image. In Mid mode it controls Mid-only de-essing amount. In Side mode it controls Side-only de-essing amount. Slider to switch between modes is provided in the lowermost row. The Direct2D variant has ambient highlighting system for the slider, the EGUI variant has a plain slider switch.
- **Mode-aware monitoring and analysis** - Filter Solo, Internal Trigger Hear, and the spectrum analyzer follow the selected Stereo/Mid/Side mode, so the user hears and sees the same component that is being targeted.
- **New MIDI sidechain trigger** - The Sidechain selector now includes a MIDI mode. In this mode MIDI notes can deliberately drive de-essing amount, allowing phrase-by-phrase or section-by-section reduction control from the DAW.
- **Freely resizeable plugin window now available on the the Direct2D variant too** - The free resizeability feature that was earlier available only in the EGUI variant is now available for the Direct2D variant too. Again, due to the nature of Direct2D the overall resizing is smoother on it than EGUI.
- **Persistent GUI size** - The EGUI and Direct2D editors now remember the user-set window size and reopen at that size in later DAW sessions.
- **Unified bundle filenames** - CLAP and VST3 bundles now use the same filename convention: Nebula De-Esser.

---

## 🎹 **MIDI Sidechain Trigger**

The MIDI sidechain trigger is a new feature in Nebula De-Esser. It is different from MIDI Learn: MIDI Learn maps CC controls to parameters, while **Sidechain = MIDI** uses MIDI notes as the actual de-essing trigger.

This is useful when the producer or engineer wants to choose exactly where de-essing happens instead of leaving every decision to automatic sibilance detection. In a DAW, route or send MIDI notes to Nebula De-Esser, switch the Sidechain selector to MIDI, then place notes only on the phrases, words, or sections that need extra control. Note velocity controls trigger strength, so a quiet verse, loud chorus, backing-vocal stack, or aggressive ad-lib can each drive a different amount of reduction.

This workflow mirrors deliberate studio-console de-essing moves: an engineer can ride the de-esser only when the performance needs it, push harder on exposed esses, back off when the vocal needs more bite, or treat different song sections with different intent. It is especially useful when the goal is polished transparent control without flattening the whole vocal track.

When **Trigger Hear** is enabled in MIDI sidechain mode, the plugin outputs an audible MIDI trigger monitor signal so the user can confirm where and how strongly the MIDI trigger is firing. Since MIDI is not audio, this monitor tone represents the MIDI trigger envelope rather than the vocal or external sidechain input.

---
**Screenshot - macOS and Linux variant that uses EGUI:**
<img width="924" height="669" alt="image" src="https://github.com/user-attachments/assets/48968118-b907-4544-b5c9-68a461fdbb30" />
---
**Screenshot - Windows variant that uses Direct2D:**
<img width="689" height="526" alt="image" src="https://github.com/user-attachments/assets/1a9bbde7-6727-4496-bbd3-82c87a3af373" />
---

## ✨ **What's New in v3.1.0**

### 🎛️ **Mono mode, CLAP format is hereby discontinued for Windows variant, Return of pay what you want system**

- **Mono Mode** - The plugin now features a mono mode, so henceforth it shall work with mono signals too. That being said due to the nature of its algorithm, internally it splits the signal into two signals and the processing is done in stereo. The two signals and then merged into the resultant output mono signal.
- **Discontinuation of the AUv2 build** - The AUv2 build is being discontinued because while it works on all DAWs it is unstable on Logic Pro, which is the main target for the AUv2 build. So, at this point there's no reason to keep the AUv2 build. Hence it's being discontinued.
- **Discontinuation of the CLAP format on the Windows variant** - Windows releases are now VST3-only. The Windows Direct2D editor works through VST3, while the current upstream nih-plug CLAP wrapper reports failure from embedded GUI show/hide callbacks on CLAP hosts that enforce the GUI lifecycle strictly. There was only two options, one to discontinue the Windows variant all together, or drop the CLAP format.
- **Return of pay what you want system** - The plugin now once again operates in pay what you want model. The whole point of this plugin was to bring high quality de-essers to new studio start-ups. The pay what you want structure works better for that. That being said, any requests to simplify the features won't be entertained. This plugin is made for users who are either well versed with audio engineering concepts or are willing to learn, no request for hand holding or making it beginner friendly will be entertained.

---

## ✨ **What's New in v3.0.0**

### 🎛️ **Reprogrammed Bypass switch, and hardcoded DPI-awareness in the Windows variant**

- **Reprogrammed Bypass Switch** - The Bypass switch now programmed to hard bypass the plugin instead of soft bypass. So when toggled off it now completely removes the plugin from the signal path. This ensures easier comparison between the on and off state.
- **Hardcoded DPI-awareness in the Windows variant** - The Windows variant gets DPI-awareness hardcoded into its GUI because it relies on Direct2D due to which unlike the macOS and Linux variants its GUI scaling is not abstracted from the OS. This fixes the scaling issues on DAWs like Reaper that don't employ internal graphics abstraction layer. The Linux and macOS variant do not require it because all the scaling is taken care of by EGUI, which ensures perfect scaling regardless of the system DPI and scaling settings. EGUI is still not stable on Windows, that's why it will continue to use Direct2D.

---

## ✨ **What's New in v2.9.0**

### 🎛️ **More DSP tuning and optimizations, new control menu and plugin now available in AUv2 format**

- **More DSP tuning** - The plugin now uses a hybrid adaptive spectral OSP-style reduction stage with multi-frame covariance tracking. Filtered detection, envelope following, TKEO-style transient support, and smoothing still provide control stability, while the audio reduction itself is performed by spectral projection/residual attenuation rather than a simple static high-band gain cut.
- **Improved voiced/sibilant training gates** - The new gate now measures spectral flatness for noise/sibilance likelihood, spectral flux for unstable/transient frames, centroid and upper-band energy bias for hissy/sibilant pressure, peak concentration for tonal/voiced evidence, TKEO-driven reduction amount as a clean-frame veto.
- **Filter Menu replaced with new Basis menu** - It now has three options, "Odd" estimates the “voiced” basis from odd non-spiky frames, "Even" estimates the “voiced” basis from even non-spiky frames, "Both" estimates the “voiced” basis from non-spiky frames. For most purposes using both is recommended.
- **Preset Manager Fixed** - The Preset Manager is now fully functional.
- **AUv2 Format** - The plugin is now available in AUv2 format using clap-wrapper.
- **Windows variant** - The Windows variant has been fixed, the problem was with EGUI. For Windows variant EGUI has been replaced by native Direct2D based GUI. Its not only stable now, but it consumes far less resources than the macOS and Linux variants, and it looks far more polished than the macOS and Linux variants. In easy words, the Windows variant is now the best variant of Nebula De-Esser.

---

## ✨ **What's New in v2.8.0**

### 🔧🎛️ **Enhanced stability even on non Steinberg compliant hosts**

- **Stability update for Cakewalk NXT** - The plugin is now perfectly stable on Cakewalk NXT running on macOS.

---

## 🎛️**What's New in v2.7.0**

### 🧠 **Even more control optimization for the new DSP**
- **Threshold knob now renamed to TKEO Sharp knob:** This represents the functionality of the knob better, and further DSP optimizations have been done for the controller.
- **Reduction meter replaced by Annihilation meter:** Shows the The "Annihilation" Factor which shows the amount of signal energy that has been identified as "harsh" (sibilant) and projected out of the final audio.
  - Real-time Attenuation: Each time the meter moves, it indicates how many decibels of that specific "harshness subspace" are being removed from the original signal.
  - Dynamic Response: The meter follows the spectral reduction control path quickly so the user can see when the processor is reacting to sibilance, while the audio path still uses smoothing and protection logic to avoid unstable or clicky behavior.
- **Reprogrammed Reduction knob and Reduction meter slider:** Sets the limit or "ceiling" for the Orthogonal Subspace Projection (OSP)-style engine. If you set the slider to its maximum, the plugin can remove more of the signal components that match the harshness signature. By lowering this slider, you’re effectively telling the algorithm: "I know you found the harshness, but don't remove 100% of it." This is crucial for keeping a vocal sounding natural rather than "lispy" or "dead". Think of it as a mix knob for the subtraction. A setting of -3dB to -6dB is often the "sweet spot" where the sibilance is tamed but the articulation remains clear. Variable from 0 to down to -100dB
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

### Objective Metrics

The Rust test suite includes deterministic objective audio metrics in `src/metrics.rs` and `tests/objective_metrics.rs`:

- target-band attenuation in the configured sibilance band
- low-band leakage outside the reduction band
- residual spectral focus
- bypass/null transparency SNR
- reported-vs-measured latency
- output peak and finite-sample safety

---

## 🏗️ **Build Instructions (Pure Manual Code Build)**

Build directly with Cargo and the `xtask` bundle command (no helper scripts).

#### **Linux (x86_64)**
```bash
cargo build --release
cargo run --release --package xtask -- bundle nebula_desser --release
# Output: target/bundled/Nebula De-Esser.clap
#         target/bundled/Nebula De-Esser.vst3
```

#### **Windows (x86_64 or ARM64 / MSVC)**
Requires the matching Rust target and the Visual Studio Build Tools C++ toolchain.

```powershell
rustup target add x86_64-pc-windows-msvc
cargo run --release --package xtask -- bundle nebula_desser --release --target x86_64-pc-windows-msvc
# Output: target\x86_64-pc-windows-msvc\bundled\Nebula De-Esser.vst3

rustup target add aarch64-pc-windows-msvc
cargo run --release --package xtask -- bundle nebula_desser --release --target aarch64-pc-windows-msvc
# Output: target\aarch64-pc-windows-msvc\bundled\Nebula De-Esser.vst3
```

Windows releases are VST3-only. The Windows Direct2D editor works through VST3,
while the current upstream nih-plug CLAP wrapper reports failure from embedded
GUI show/hide callbacks on CLAP hosts that enforce the GUI lifecycle strictly.

#### **macOS (Separate Apple Silicon and Intel builds)**
```bash
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo run --release --package xtask -- bundle nebula_desser --release --target aarch64-apple-darwin
cargo run --release --package xtask -- bundle nebula_desser --release --target x86_64-apple-darwin
# Output: target/aarch64-apple-darwin/bundled/Nebula De-Esser.clap
#         target/aarch64-apple-darwin/bundled/Nebula De-Esser.vst3
#         target/x86_64-apple-darwin/bundled/Nebula De-Esser.clap
#         target/x86_64-apple-darwin/bundled/Nebula De-Esser.vst3
```

> VST3 now uses a **single fixed bus layout** (`Stereo + optional Sidechain`) on all platforms to reduce host layout-switch instability while preserving external sidechain functionality.

---
**Pre-built binaries can be bought from Gumroad:**
https://subhankar42.gumroad.com/l/adounr

Note:
For users new to CLAP plugins, they can sometimes look like folders on macOS, but the name of the folder has ".clap" in it like a file extension. It's perfectly normal.

The macOS zips contain CLAP and VST3 plugins, split by Apple Silicon and Intel.
The Linux zip contains CLAP and VST3 plugins. The Windows zips contain VST3
plugins, split by x86_64 and ARM64.

Note for macOS users:
macOS Gatekeeper blocks the binary because it has no code signature. Locally-built binaries are trusted automatically; externally built ones are flagged as "from the internet".

To fix this problem after unzipping run the following command:
  xattr -dr com.apple.quarantine [path of the Nebula De-Esser.clap or Nebula De-Esser.vst3 file]

After that you can copy it to either /Library/Audio/Plug-Ins/CLAP or /Library/Audio/Plug-Ins/VST3 (if you want to install it for all users) or ~/Library/Audio/Plug-Ins/CLAP/ or ~/Library/Audio/Plug-Ins/VST3/ (if you want to install it for only the current user)

---

## ⚠️ N-Track Studio

**The stability issues on N-Track Studio was actually due to my TCP set-up, I forgot to enable expand to stereo mode on the TCP that's why the signal was not gong through the plugin. Enabled expand to stereo for the TCP and it works fine. For some reason I forgot that this plugin has dedicated panning controls. Other DAWs do this switch automatically, N-Track Studio doesn't do it automatically. It doesn't use dynamic channel count negotiations to ensure compatibility with old and obsolete DirectX (DX/DXi) plugins. It is not a bug, it is more like a minor inconvenience, and it makes sense because legacy backwards compatibility is a big thing in N-Track Studio**

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
└── Cargo.toml
```

---

## 📄 **License**

GNU Affero General Public License v3.0 (AGPL-3.0-or-later) — free to use, modify, and distribute under copyleft terms.

---

**Ready for professional use in major DAWs supporting VST3 on Windows, macOS and Linux.**

---

**Reporting Issues:**
For reporting any issues create an issue on the Github repository, and while creating the issue do mention your email ID in the issue. The issues of paid customers will be solved on priority basis (Minimum payment of $10). Free customers are expected to workout any issues on their own, no support will be provided to them.
