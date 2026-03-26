pub mod html;
pub mod panel;
mod panel_actions;
mod panel_window;
pub mod render;
pub mod state;
pub mod utils;
pub mod window;

pub use panel::update_favorites_panel;
pub use window::{
    WM_BUBBLE_THEME_UPDATE, hide_favorite_bubble, show_favorite_bubble, trigger_blink_animation,
};
