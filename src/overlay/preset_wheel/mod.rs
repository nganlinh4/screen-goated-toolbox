// Preset Wheel Overlay - Modern WebView2 implementation
// Shows a beautiful wheel of preset options for MASTER presets

mod html;
mod liveness;
mod runtime;
mod script;
mod state;
mod styles;
mod window;

pub(crate) use window::should_show_preset;

pub use window::{
    dismiss_wheel, is_wheel_active, show_custom_wheel, show_preset_wheel, show_preset_wheel_id,
    show_preset_wheel_with_extra,
};
