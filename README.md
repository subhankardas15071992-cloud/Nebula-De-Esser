# NEBULA DEESSER

**Hyper-optimized 64-bit CLAP De-esser Plugin**  
Written in Rust · nih-plug · egui · Alien Synthwave UI

---

## Overview

Nebula DeEsser is a high-performance, pure 64-bit CLAP de-esser plugin built for professionals who demand precision. Its DSP engine runs entirely in double-precision (f64), exploiting the full dynamic range of 64-bit IEEE 754 arithmetic. The GUI is styled with a deep synthwave/alien aesthetic — dark voids, neon cyan, magenta, and gold, animated scanline grids, and glowing interactive controls.

---

<img width="1440" height="900" alt="Image" src="https://github.com/user-attachments/assets/32db7293-7599-4178-9a99-fa1171af4ccd" />

## Features

### Detection & Metering
- **Detection Meter** — level of the filtered sidechain signal (blue = below threshold, yellow = active processing)
- **Detection Max Field** — peak-hold display, click to reset
- **Detection Meter Slider** — drag to set threshold
- **Reduction Meter** — real-time gain reduction amount
- **Reduction Max Field** — peak-hold GR display, click to reset
- **Reduction Meter Slider** — drag to set max reduction ceiling

### Parameters
| Parameter | Range | Description |
|-----------|-------|-------------|
| Threshold | -60 to 0 dB | Detection trigger level |
| Max Reduction | 0 to 40 dB | Maximum gain reduction applied |
| Min Frequency | 1000–16000 Hz | Low end of detection band |
| Max Frequency | 1000–20000 Hz | High end of detection band |
| Lookahead | 0–20 ms | Look-ahead delay (toggle on/off) |
| Stereo Link | 0–100% | Amount of channel coupling |

### Modes
- **Relative** — compares filtered signal to full bandwidth; works at all levels; highly transparent
- **Absolute** — classic de-esser; triggers on absolute level crossing threshold

### Detection Filter
- **Lowpass** — broad frequency reduction across band
- **Peak** — narrow resonant detection; surgical control over a specific frequency

### Range
- **Split** — only processes within the set frequency band (transparent to rest of signal)
- **Wide** — applies gain reduction across entire frequency range

### Stereo
- **Stereo Link** — knob 0–100%, select Mid or Side coupling mode
- **Mid/Side** — M/S linking applies gain reduction symmetrically in mid or side domain

### Sidechain
- **Internal** — uses main input as sidechain source
- **External** — accepts a separate sidechain input (2-channel aux input in supporting hosts)

### Modes
- **Single Vocal** — optimized attack/release ballistics for voice de-essing
- **Allround** — general-purpose mode for instruments and mixed material

### Monitoring
- **Filter Solo** — hear the isolated detection band
- **Trigger Hear** — monitor the raw sidechain signal

### Spectrum Analyzer
- 2048-point real-time FFT with Hann windowing
- Logarithmic frequency scale (20 Hz – 22 kHz)
- **Interactive frequency nodes** — drag MIN and MAX nodes to set detection band frequencies
  - Dragging nodes automatically updates Min/Max Frequency knobs and their value fields
  - Highlighted band overlay shows active detection range

### Right-Click Numeric Input
Right-click any knob or value field to open a precise numeric entry popup. Type the exact value and press Enter or click OK.

---

## Building

### Prerequisites
- Rust stable toolchain (`rustup.rs`)
- `cargo-nih-plug` bundler (auto-installed by build scripts)
- On Linux: GCC or Clang, JACK development headers (`libjack-jackd2-dev`)
- On macOS: Xcode Command Line Tools
- On Windows: MSVC (Visual Studio 2022) or GNU toolchain

### Linux (JACK / ALSA / PipeWire)
```bash
chmod +x build_linux.sh
./build_linux.sh
# Output: target/bundled/nebula_desser.clap
# Install: cp target/bundled/nebula_desser.clap ~/.clap/
```

### macOS (Core Audio, Universal Binary)
```bash
chmod +x build_mac.sh
./build_mac.sh
# Output: target/bundled/Nebula DeEsser.clap (universal arm64 + x86_64)
# Install: cp -r "target/bundled/Nebula DeEsser.clap" ~/Library/Audio/Plug-Ins/CLAP/
```

### Windows (ASIO / WASAPI / WaveRT)
```powershell
# PowerShell:
.\build_windows.ps1

# Or Command Prompt:
build_windows.bat
# Output: target\bundled\nebula_desser.clap
# Install: Copy to %COMMONPROGRAMFILES%\CLAP\
```

---

## Audio Engine Optimizations

### 64-bit DSP
All sample processing uses `f64` (IEEE 754 double precision):
- 53-bit mantissa vs 23-bit in f32 → dramatically lower quantization noise in filter cascades
- Biquad filter coefficients computed in f64 to prevent coefficient quantization at high frequencies
- Envelope follower, gain computation, and smoothing all in f64

### Platform-Specific
| Platform | Audio API | Optimization |
|----------|-----------|--------------|
| Windows | ASIO | Native kernel bypass, ~1ms latency |
| Windows | WASAPI Exclusive | Shared-mode bypass, ~3ms latency |
| Windows | WaveRT | Low-latency kernel streaming |
| macOS | Core Audio | HAL direct access, <5ms latency |
| Linux | JACK | Real-time priority, sub-ms latency |
| Linux | PipeWire | Modern low-latency audio graph |

### Compiler Flags
```
opt-level = 3          # Maximum LLVM optimization
lto = "fat"            # Full link-time optimization across crates
codegen-units = 1      # Single compilation unit for better vectorization
target-cpu = x86-64-v2 # AVX/SSE4.2 vectorization
panic = "abort"        # No unwinding overhead
```

### Lock-Free Design
- Meter values shared via `AtomicU32` (bit-cast f32) — zero contention between audio and GUI
- Spectrum analyzer uses `try_lock()` — drops frames rather than blocking audio thread
- All GUI → DSP communication goes through nih-plug's thread-safe parameter system

---

## CLAP Format

This plugin is CLAP-only. CLAP (CLever Audio Plugin) is an open-source, Steinberg-independent plugin format offering:
- Better thread safety guarantees than VST3
- Native support for non-destructive parameter modulation
- First-class sidechain support
- Designed for modern multi-core CPUs

Compatible hosts include: Bitwig Studio, Reaper, Ardour, Zrythm, MultitrackStudio, and others.

---

## Architecture

```
lib.rs              — Plugin struct, parameter definitions, CLAP export
├── dsp.rs          — Pure f64 DSP engine (biquad, envelope, gain, lookahead)
├── analyzer.rs     — FFT spectrum analyzer (2048-point Hann-windowed)
└── gui.rs          — egui synthwave GUI (knobs, meters, analyzer, popups)
```

---

## License

MIT License — free to use, modify, and distribute.

---

*"In the neon void between frequencies, Nebula listens."*
