mod state;
pub mod paint;
mod logic;
mod layout;
mod window;
mod event_handler;

use state::{WINDOW_STATES, WindowState, CursorPhysics, AnimationMode, InteractionMode, ResizeEdge};

pub use state::{WindowType, link_windows, RefineContext};
pub use window::{create_result_window, update_window_text};
