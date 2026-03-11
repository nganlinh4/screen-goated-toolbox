// Preset Wheel Overlay - Modern WebView2 implementation
// Shows a beautiful wheel of preset options for MASTER presets

mod html;
mod runtime;
mod script;
mod state;
mod styles;
mod window;

pub use window::{
    WheelOption, dismiss_wheel, is_wheel_active, show_option_wheel, show_preset_wheel, warmup,
};
