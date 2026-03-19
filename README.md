# NEBULA DEESSER v2.0.0

**Hyper-optimized 64-bit CLAP De-esser Plugin**  
Written in Rust · nih-plug · egui · Alien Synthwave UI

---

## Overview

Nebula DeEsser is a high-performance, pure 64-bit CLAP de-esser plugin built for professionals who demand precision. Its DSP engine runs entirely in double-precision (f64), exploiting the full dynamic range of 64-bit IEEE 754 arithmetic. The GUI is styled with a deep synthwave/alien aesthetic — dark voids, neon cyan, magenta, and gold, animated scanline grids, and glowing interactive controls.

Version 2.0.0 adds Presets, Undo/Redo, MIDI Learn, FX Bypass, Input/Output Level and Pan controls, Oversampling up to 8x, and a fully working live Spectrum Analyzer.

<img width="1440" height="900" alt="Image" src="https://github.com/user-attachments/assets/209d32b5-6f0b-40d2-ac77-f35fc6dddf75" />
---

## What's New in v2.0.0

| Feature | Description |
|---------|-------------|
| **Preset Manager** | Save and load named envelope presets via dropdown |
| **Undo / Redo** | 50-step undo/redo history for all parameter changes |
| **MIDI Learn** | Assign any MIDI CC to threshold, reduction, frequencies, I/O level, pan, and more |
| **FX Bypass** | Soft bypass button — passes audio unmodified; title bar turns red when active |
| **Input Level** | Pre-processing gain control (−60 to +12 dB) |
| **Input Pan** | Pre-processing stereo pan (L100 – C – R100) |
| **Output Level** | Post-processing gain control (−60 to +12 dB) |
| **Output Pan** | Post-processing stereo pan (L100 – C – R100) |
| **Oversampling** | Off / 2× / 4× / 6× / 8× — reduces aliasing at high frequencies |
| **Spectrum Analyzer** | Fixed live FFT display with exponential smoothing and cyan sci-fi aesthetic |

---

## Features

### Toolbar (top strip)
- **⊗ BYPASS** — Toggles full soft bypass; audio passes through unchanged. Title bar border turns red.
- **PRESET dropdown** — Shows saved presets; click to load instantly.
- **SAVE** — Opens naming popup to save current settings as a new preset.
- **DEL** — Deletes the currently selected preset.
- **◄ UNDO / REDO ►** — Step through 50-level change history for all knob/button interactions.
- **MIDI LEARN** — Opens the MIDI Learn panel; click a parameter row then move a CC on your controller.
- **OS dropdown** — Oversampling: Off / 2× / 4× / 6× / 8×.

### Detection & Metering
- **Detection Meter** — Level of the filtered sidechain signal (blue → yellow as threshold is approached)
- **Detection Max Field** — Peak-hold display; click to reset
- **Detection Meter Slider** — Drag vertically to set threshold
- **Reduction Meter** — Real-time gain reduction display
- **Reduction Max Field** — Peak-hold GR display; click to reset
- **Reduction Meter Slider** — Drag vertically to set max reduction ceiling

### Core Parameters
| Parameter | Range | Description |
|-----------|-------|-------------|
| Threshold | −60 to 0 dB | Detection trigger level |
| Max Reduction | 0 to 40 dB | Maximum gain reduction applied |
| Min Frequency | 1000–16000 Hz | Low end of detection band |
| Max Frequency | 1000–20000 Hz | High end of detection band |
| Lookahead | 0–20 ms | Look-ahead delay (toggle on/off) |
| Stereo Link | 0–100% | Amount of channel coupling |

### I/O Controls (v2.0.0)
| Parameter | Range | Description |
|-----------|-------|-------------|
| Input Level | −60 to +12 dB | Pre-DSP input gain |
| Input Pan | L100 – C – R100 | Pre-DSP stereo pan |
| Output Level | −60 to +12 dB | Post-DSP output gain |
| Output Pan | L100 – C – R100 | Post-DSP stereo pan |

### Detection Modes
- **Relative** — Compares filtered signal to full bandwidth; transparent at all levels
- **Absolute** — Classic de-esser; triggers on absolute level crossing threshold

### Detection Filter
- **Lowpass** — Broad frequency reduction across band
- **Peak** — Narrow resonant detection; surgical control over a specific frequency

### Range
- **Split** — Only processes within the set frequency band
- **Wide** — Applies gain reduction across the entire frequency range

### Stereo
- **Stereo Link** — 0–100%; selectable Mid or Side coupling mode
- **Mid/Side** — M/S linking applies gain reduction symmetrically in mid or side domain

### Sidechain
- **Internal** — Uses main input as sidechain source
- **External** — Accepts a separate sidechain input (aux input in supporting hosts)

### Processing Mode
- **Vocal** — Optimized attack/release ballistics for voice de-essing
- **Allround** — General-purpose mode for instruments and mixed material

### Monitoring
- **Filter Solo** — Hear the isolated detection band
- **Trigger Hear** — Monitor the raw sidechain signal

### Spectrum Analyzer
- 2048-point real-time FFT with Hann windowing
- Logarithmic frequency scale (20 Hz – 22 kHz)
- Exponential magnitude smoothing (fast attack, slow release)
- Minimalist cyan glow line with filled area — sci-fi aesthetic
- Frequency grid lines at 100, 200, 500, 1k, 2k, 5k, 10k, 20k Hz
- **Interactive frequency nodes** — drag MIN (magenta) and MAX (gold) nodes to set detection band
- Highlighted detection band overlay shows active range

### Right-Click Numeric Input
Right-click any knob or value field to open a precise numeric entry popup.

### MIDI Learn
Click **MIDI LEARN** in the toolbar to open the mapping panel. Click a parameter row to arm it (shown in magenta), then move a CC on your MIDI controller. The CC is bound instantly. **CLEAR ALL** removes all mappings.

MIDI-learnable parameters: Threshold, Max Reduction, Stereo Link, Input Level, Input Pan, Output Level, Output Pan, Min Freq, Max Freq, Lookahead.

---

## Building

### Prerequisites
- Rust stable toolchain (`rustup.rs`)
- `cargo-nih-plug` bundler — auto-installed by build scripts
- **Linux**: GCC or Clang, `libjack-jackd2-dev`
- **macOS**: Xcode Command Line Tools (`xcode-select --install`)
- **Windows**: MSVC (Visual Studio 2022 Build Tools) or GNU toolchain; 64-bit OS required

### Linux — 64-bit x86_64 (JACK / ALSA / PipeWire)
```bash
chmod +x build_linux.sh
./build_linux.sh
# Output: target/bundled/nebula_desser.clap
# Install: mkdir -p ~/.clap && cp target/bundled/nebula_desser.clap ~/.clap/
```

### macOS — Universal Binary (Apple Silicon + Intel)
```bash
chmod +x build_mac.sh
./build_mac.sh
# Output: target/bundled/Nebula DeEsser.clap  (arm64 + x86_64)
# Install: cp -r "target/bundled/Nebula DeEsser.clap" ~/Library/Audio/Plug-Ins/CLAP/
```

### Windows — 64-bit (ASIO / WASAPI / WaveRT)
```powershell
# PowerShell (recommended — includes optional auto-install):
.\build_windows.ps1

# Or Command Prompt:
build_windows.bat
# Output: target\bundled\nebula_desser.clap
# Install: copy to %COMMONPROGRAMFILES%\CLAP\
```

---

## Audio Engine

### 64-bit DSP
All sample processing uses `f64` (IEEE 754 double precision):
- 53-bit mantissa vs 23-bit in f32 → dramatically lower quantization noise in filter cascades
- Biquad filter coefficients computed in f64 to prevent coefficient quantization at high frequencies
- Envelope follower, gain computation, smoothing, and oversampling all in f64

### Oversampling
When oversampling is active, input samples are linearly interpolated to the target rate, processed, and averaged back down. Factors of 2×/4×/6×/8× help suppress aliasing artefacts from hard-clipping-style gain reduction at high frequencies.

### Platform-Specific Optimizations
| Platform | Audio API | Typical Latency |
|----------|-----------|----------------|
| Windows | ASIO | ~1 ms |
| Windows | WASAPI Exclusive | ~3 ms |
| Windows | WaveRT | Low-latency kernel streaming |
| macOS | Core Audio | < 5 ms |
| Linux | JACK | < 1 ms |
| Linux | PipeWire | ~2–5 ms |

### Compiler Flags
```
opt-level = 3           # Maximum LLVM optimization
lto = "fat"             # Full link-time optimization across crates
codegen-units = 1       # Single compilation unit for best vectorization
target-cpu = x86-64-v2  # AVX/SSE4.2 vectorization (Linux/Windows)
target-cpu = apple-m1   # ARM NEON vectorization (macOS Apple Silicon)
panic = "abort"         # No unwinding overhead
```

### Lock-Free Design
- Meter values shared via `AtomicU32` (bit-cast f32) — zero contention between audio and GUI
- Spectrum analyzer uses `try_lock()` — drops frames rather than blocking the audio thread
- MIDI CC values communicated via `AtomicU32` + `AtomicBool` dirty flags
- All GUI → DSP parameter communication via nih-plug's thread-safe parameter system

---

## CLAP Format

This plugin is CLAP-only. CLAP (CLever Audio Plugin) offers:
- Better thread-safety guarantees than VST3
- Native support for non-destructive parameter modulation
- First-class sidechain support
- Designed for modern multi-core CPUs

Compatible hosts: Bitwig Studio, Reaper, Ardour, Zrythm, MultitrackStudio, and others.

---

## Architecture

```
Cargo.toml          — Package definition, version 2.0.0
src/
├── lib.rs          — Plugin struct, all parameters (incl. v2), CLAP export,
│                     MIDI event processing, oversampling engine,
│                     I/O gain/pan application, bypass routing
├── dsp.rs          — Pure f64 DSP engine (biquad, envelope, gain, lookahead)
├── analyzer.rs     — FFT spectrum analyzer (2048-point Hann-windowed,
│                     corrected magnitude scaling, fresh flag)
└── gui.rs          — egui synthwave GUI:
                        toolbar (bypass, presets, undo/redo, MIDI learn, OS),
                        detection/reduction meters, 6 core knobs,
                        4 I/O knobs (level+pan), toggle buttons,
                        spectrum analyzer panel (smoothed, glow),
                        numeric input popup, preset save popup,
                        MIDI learn panel
```

---

## License

MIT License — free to use, modify, and distribute.

---

*"In the neon void between frequencies, Nebula listens."*
