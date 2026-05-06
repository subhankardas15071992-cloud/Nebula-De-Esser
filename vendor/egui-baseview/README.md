# egui-baseview
![Test](https://github.com/BillyDM/egui-baseview/workflows/Rust/badge.svg)
[![License](https://img.shields.io/crates/l/egui-baseview.svg)](https://github.com/BillyDM/egui-baseview/blob/main/LICENSE)

A [`baseview`](https://github.com/RustAudio/baseview) backend for [`egui`](https://github.com/emilk/egui).

<div align="center">
    <img src="screenshot.png">
</div>

## Audio Plugins

This backend is officially supported by [`nih-plug`](https://github.com/robbert-vdh/nih-plug). The nih-plug example plugin can be found [here](https://github.com/robbert-vdh/nih-plug/tree/master/plugins/examples/gain_gui_egui).

There is also an (outdated) vst2 example plugin [here](https://github.com/DGriffin91/egui_baseview_test_vst2).

## Prerequisites

### Linux

Install dependencies, e.g.,

```sh
sudo apt-get install libx11-dev libxcursor-dev libxcb-dri2-0-dev libxcb-icccm4-dev libx11-xcb-dev mesa-common-dev libgl1-mesa-dev libglu1-mesa-dev
```
