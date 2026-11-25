# macOS AUv2 wrapper

This directory builds the Audio Unit v2 component for Nebula De-Esser using
Steinberg's official **VST3 → AUv2 wrapper** from the VST 3 SDK. It is not a
CLAP wrapper and does not contain a second DSP implementation.

## How it works

The component is a thin loadable bundle:

```
Nebula De-Esser.component/
└── Contents/
    ├── Info.plist                 # AudioComponents entry (aufx / NDeS / NbAu)
    ├── MacOS/
    │   └── Nebula De-Esser        # AU wrapper executable
    └── Resources/
        └── plugin.vst3            # the real nih-plug VST3, embedded
```

At load time the wrapper reads the `AudioComponents` entry from `Info.plist`,
finds the embedded `plugin.vst3` in its own `Resources` directory, and forwards
the AU API to the VST3. This is the deployment model described in Steinberg's
[AUv2 Wrapper documentation](https://steinbergmedia.github.io/vst3_dev_portal/pages/What+is+the+VST+3+SDK/Wrappers/AUv2+Wrapper.html).

The component is ad-hoc signed (`codesign --sign -`). Release builds still need
to be notarized with a Developer ID if they are distributed outside the App
Store.

## Building

`build_mac.sh` invokes this project once per architecture. To build it by hand:

```bash
cmake -S cmake/macos-au -B build-au \
    -DCMAKE_BUILD_TYPE=Release \
    -DCMAKE_OSX_ARCHITECTURES=arm64 \
    -DNEBULA_VST3_PATH="/path/to/Nebula De-Esser.vst3"
cmake --build build-au --parallel
```

The result is `build-au/bundled/Nebula De-Esser.component`.

## Codes

| Key | Value |
| --- | --- |
| Type | `aufx` (effect) |
| Subtype | `NDeS` |
| Manufacturer | `NbAu` |
| Factory function | `AUWrapperFactory` |

Validate an installed component with:

```bash
auval -v aufx NDeS NbAu
```
