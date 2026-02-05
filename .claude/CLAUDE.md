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

## File Size Limits
- **Maximum 600 lines per file** - if a file approaches this limit, split it into a module directory
- When splitting: `foo.rs` → `foo/mod.rs` + `foo/submodule.rs`
- Keep public API in `mod.rs`, move implementation details to submodules
- Prefer logical splits (e.g., `paint.rs`, `messages.rs`, `window.rs`) over arbitrary line-based splits
- Each submodule should have a clear, single responsibility

## Testing
- Always run `cargo clippy --all-targets` before commits
- Test on Windows 10/11 for compatibility

## Claude Code Rules
- **Never run `cargo build --release`** - the user will build manually when ready
- Use `cargo check` or `cargo clippy` for verification instead
