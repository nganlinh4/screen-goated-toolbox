mod confirm_dialog;
pub mod download_manager;
mod footer;
mod global;
pub mod help_assistant;
mod history;
pub mod node_graph;
pub mod pointer_gallery;
mod preset;
mod sidebar;

pub use confirm_dialog::{ConfirmModal, ConfirmResult};
pub use footer::{FooterToggles, render_footer};
pub use global::render_global_settings;
pub use history::render_history_panel;
pub use preset::render_preset_editor;
pub(crate) use sidebar::cached_grid_width;
pub use sidebar::get_localized_preset_name;
pub use sidebar::render_sidebar;

#[derive(PartialEq, Clone, Copy)]
pub enum ViewMode {
    Global,
    History,
    Preset(usize),
}
