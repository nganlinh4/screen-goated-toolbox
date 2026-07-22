# egui-wgpu

[![Latest version](https://img.shields.io/crates/v/egui-wgpu.svg)](https://crates.io/crates/egui-wgpu)
[![Documentation](https://docs.rs/egui-wgpu/badge.svg)](https://docs.rs/egui-wgpu)
![MIT](https://img.shields.io/badge/license-MIT-blue.svg)
![Apache](https://img.shields.io/badge/license-Apache-blue.svg)

This crates provides bindings between [`egui`](https://github.com/emilk/egui) and [wgpu](https://crates.io/crates/wgpu).

This was originally hosted at https://github.com/hasenbanck/egui_wgpu_backend

## SGT patch

This vendored 0.34.3 copy adds in-process wgpu device-loss recovery for Screen Goated Toolbox.
It recreates the painter's device-owned resources and restores live managed textures while keeping
the existing winit event loop and application process alive. The upstream source remains licensed
under MIT or Apache-2.0; see `LICENSE-MIT` and `LICENSE-APACHE`.
