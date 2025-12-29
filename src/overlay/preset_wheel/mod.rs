// Preset Wheel Overlay - Modern WebView2 implementation
// Shows a beautiful wheel of preset options for MASTER presets

mod html;
mod window;

pub use window::{dismiss_wheel, is_wheel_active, show_preset_wheel, warmup};
