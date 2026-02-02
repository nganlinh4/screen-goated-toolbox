# Screen Goated Toolbox

Windows AI productivity automation tool built with Rust.

## Project Context
- **Type**: Native Windows desktop application
- **GUI**: egui/eframe with glow renderer
- **Audio**: WASAPI, cpal, symphonia for multi-format playback
- **GPU**: wgpu for rendering, DirectML for ML inference
- **AI**: Parakeet for speech recognition, multiple AI provider integrations

## Build Commands
```bash
cargo build --release     # Production build
cargo run                 # Debug run
cargo clippy             # Lint check
cargo fmt                # Format code
cargo test               # Run tests
```

## Key Dependencies
- `egui` / `eframe`: Immediate mode GUI
- `parakeet-rs`: Local speech recognition with DirectML
- `ort`: ONNX Runtime for ML inference
- `windows-capture`: Screen capture API
- `wry`: WebView for markdown rendering
- `tray-icon`: System tray integration

## Code Patterns
- Use `anyhow::Result` for error handling
- Windows API via `windows` crate (0.62)
- Async audio processing with `parking_lot` mutexes
- Node graph workflows via `egui-snarl`

## Testing
- Always run `cargo clippy --all-targets` before commits
- Test on Windows 10/11 for compatibility
