// --- RENDERING MODULE ---
// Main application rendering: layout, title bar, footer, and overlays.

mod computer_control;
mod footer;
mod live_translate;
mod overlays;
mod preset_model_update;
mod screen_translate;
mod title_bar;

use super::types::{DetailPane, SettingsApp};
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::node_graph::{blocks_to_snarl, snarl_to_graph};
use crate::gui::settings_ui::{
    PresetIndexMove, ViewMode, cached_grid_width, render_global_settings, render_history_panel,
    render_preset_editor, render_sidebar,
};
use eframe::egui;
use std::sync::atomic::Ordering;

// Min widths driving the responsive detail layout (in egui POINTS, i.e. after
// display scaling — a 1920px monitor at 125% is only ~1536 points wide).
// W_EDITOR_MIN is how much room the editor needs before an aux column reveals;
// W_GLOBAL / W_HISTORY double as those columns' fixed panel widths.
const W_EDITOR_MIN: f32 = 480.0;
const W_GLOBAL: f32 = 500.0;
const W_HISTORY: f32 = 480.0;

/// Empty gutter docked at the window's right edge so the outermost detail column
/// isn't flush against it (the right side previously had no padding).
/// Inset between the body's content and the window edge, on both sides.
///
/// The left came from the sidebar frame's own margin and the right from the
/// width of an empty gutter panel, so they were written independently — 8px
/// against 18px, which reads as the whole body sitting off-centre. One
/// constant now feeds both.
///
/// Wider than [`crate::gui::theme::WINDOW_EDGE_INSET`], which the bars use: a
/// bar is a band of controls that reads as part of the frame, while the body
/// is content and wants air around it. It matches the gutter between the
/// columns, so the body keeps one rhythm from edge to edge.
const BODY_EDGE_INSET: i8 = crate::gui::theme::space::CARD;

/// Width of the empty gutter docked at the right, keeping the outermost detail
/// column off the window edge.
const RIGHT_PAD: f32 = BODY_EDGE_INSET as f32;
const SIDEBAR_GRID_TRAILING_PAD: f32 = 4.0;
const SIDEBAR_MIN_WIDTH: f32 = 440.0;
const SIDEBAR_MAX_WIDTH: f32 = 760.0;

fn sidebar_width() -> f32 {
    sidebar_width_for(cached_grid_width())
}

/// Panel width that leaves the preset grid its measured width.
///
/// `Panel::left::exact_size` sizes the whole panel, frame included, so the
/// left inset comes out of the grid's room. It was not accounted for, which
/// left the grid short by exactly the inset — invisible while that was small,
/// but it clipped the favourite star off the last column as soon as the inset
/// grew.
fn sidebar_width_for(grid_width: f32) -> f32 {
    (grid_width + SIDEBAR_GRID_TRAILING_PAD + f32::from(BODY_EDGE_INSET))
        .clamp(SIDEBAR_MIN_WIDTH, SIDEBAR_MAX_WIDTH)
}

/// Fixed width for an aux (non-first) detail column.
fn detail_pane_width(p: DetailPane) -> f32 {
    match p {
        DetailPane::Editor => W_EDITOR_MIN,
        DetailPane::Global => W_GLOBAL,
        DetailPane::History => W_HISTORY,
    }
}

/// Replace a non-finite (inf/NaN) value with a finite fallback. egui's sizing
/// pass can hand us inf for `available_*`/`max_rect`; passing that into an
/// allocate/panel rect makes egui panic on a NaN rect (layout.rs:662).
fn san(v: f32, fallback: f32) -> f32 {
    if v.is_finite() { v } else { fallback }
}

impl SettingsApp {
    pub(crate) fn render_main_layout(&mut self, root_ui: &mut egui::Ui) {
        let text = LocaleText::get(&self.config.ui_language);
        let ctx = root_ui.ctx().clone();
        let ctx = &ctx;
        let panel_fill = root_ui.visuals().panel_fill;
        let panes = self.detail_panes.clone();

        // Paint the parent-owned content area before child panels advance its cursor. Any spacing
        // that egui allocates between docked panels then inherits the same themed background.
        root_ui
            .painter()
            .rect_filled(root_ui.available_rect_before_wrap(), 0.0, panel_fill);

        // NATIVE egui panels: egui sizes + clips each panel itself, so columns can
        // never overflow the window or cut each other off (the manual width math
        // kept mismatching the display scaling). Sidebar docks left (sized to its
        // grid), the aux columns dock right at fixed widths, and the main pane (the
        // editor, usually) fills the CentralPanel between them — bounded by the
        // central rect, so the node graph can't paint over its neighbours either.
        let col_frame = |left: i8| {
            egui::Frame::NONE
                .fill(panel_fill)
                .inner_margin(egui::Margin {
                    left,
                    right: 0,
                    top: 0,
                    bottom: 0,
                })
        };

        // Column 1: preset controls (left, sized to its grid).
        egui::Panel::left("sgt_sidebar")
            .resizable(false)
            .exact_size(sidebar_width())
            .show_separator_line(false)
            .frame(
                egui::Frame::NONE
                    .fill(panel_fill)
                    .inner_margin(egui::Margin {
                        left: BODY_EDGE_INSET,
                        right: 0,
                        top: crate::gui::theme::space::HAIR,
                        bottom: 0,
                    }),
            )
            .show(root_ui, |ui| {
                let sidebar_response =
                    render_sidebar(ui, &mut self.config, &mut self.view_mode, &text);
                if let Some(index_move) = sidebar_response.preset_index_move {
                    self.remap_preset_ui_indices(index_move);
                }
                if sidebar_response.changed {
                    self.save_and_sync();
                    if sidebar_response.refresh_favorites {
                        crate::overlay::favorite_bubble::update_favorites_panel();
                    }
                    if sidebar_response.blink_favorite {
                        crate::overlay::favorite_bubble::trigger_blink_animation();
                    }
                }
                self.update_sr_hotkey_recording(ctx);
            });

        // Right-edge padding: an empty gutter docked first (outermost right) so the
        // outermost detail column doesn't sit flush against the window edge.
        egui::Panel::right("sgt_right_pad")
            .resizable(false)
            .exact_size(RIGHT_PAD)
            .show_separator_line(false)
            .frame(egui::Frame::NONE.fill(panel_fill))
            .show(root_ui, |_ui| {});

        // Aux columns dock from the right — History outermost, then Global — so the
        // visual order is: controls | editor | global | history.
        for pane in panes.iter().skip(1).rev() {
            let id = match pane {
                DetailPane::History => "sgt_col_history",
                DetailPane::Global => "sgt_col_global",
                DetailPane::Editor => "sgt_col_editor",
            };
            egui::Panel::right(id)
                .resizable(false)
                .show_separator_line(false)
                .exact_size(detail_pane_width(*pane))
                .frame(col_frame(crate::gui::theme::space::CARD))
                .show(root_ui, |ui| {
                    let cb = san(ui.max_rect().bottom(), 600.0);
                    let cr = san(ui.max_rect().right(), 1200.0);
                    self.render_detail_pane(ui, ctx, *pane, &text, cb, cr);
                });
        }

        // Central: the main / first pane (the editor, or the focused single pane).
        egui::CentralPanel::default()
            .frame(col_frame(crate::gui::theme::space::CARD))
            .show(root_ui, |ui| {
                let cb = san(ui.max_rect().bottom(), 600.0);
                let cr = san(ui.max_rect().right(), 1200.0);
                if let Some(main) = panes.first() {
                    self.render_detail_pane(ui, ctx, *main, &text, cb, cr);
                }
            });
    }
}

impl SettingsApp {
    fn remap_preset_ui_indices(&mut self, index_move: PresetIndexMove) {
        if let Some(index) = &mut self.current_preset_idx {
            *index = index_move.remap(*index);
        }
        if let Some(index) = &mut self.recording_hotkey_for_preset {
            *index = index_move.remap(*index);
        }
        if let Some((index, _, _)) = &mut self.last_edited_preset_key {
            *index = index_move.remap(*index);
        }
    }

    /// Decide which detail columns are visible this frame from the window width,
    /// and remember which preset's editor is the main pane. Run once per frame
    /// (before the title bar, which hides tabs for already-visible panes).
    pub(crate) fn update_detail_layout(&mut self, ctx: &egui::Context) {
        // External requests to jump to a settings sub-panel (polled once/frame).
        if crate::overlay::translation_gummy::REQUEST_OPEN_TTS_SETTINGS
            .swap(false, Ordering::SeqCst)
        {
            self.view_mode = ViewMode::Global;
            self.show_tts_modal = true;
        }
        if super::types::REQUEST_OPEN_DOWNLOADED_TOOLS.swap(false, Ordering::SeqCst) {
            self.view_mode = ViewMode::Global;
            self.show_tools_modal = true;
        }

        // The editor's preset persists across Global/History focus so it can stay
        // open as the main pane; keep it valid against the current preset list.
        if let ViewMode::Preset(idx) = self.view_mode
            && idx < self.config.presets.len()
        {
            self.current_preset_idx = Some(idx);
        }
        if let Some(i) = self.current_preset_idx
            && i >= self.config.presets.len()
        {
            self.current_preset_idx = None;
        }

        // Detail width = window minus the preset-controls sidebar (+ margins).
        let total = ctx.content_rect().width();
        let detail = (total - sidebar_width() - 30.0).max(0.0);

        let focused = match self.view_mode {
            ViewMode::History => DetailPane::History,
            ViewMode::Global => DetailPane::Global,
            ViewMode::Preset(_) => DetailPane::Editor,
        };
        // When only one aux slot fits, it follows the focused tab (History if
        // chosen, else Global).
        let aux = if focused == DetailPane::History {
            DetailPane::History
        } else {
            DetailPane::Global
        };

        // Purely width-driven. With a preset selected the editor is the main column;
        // Global then History reveal to its right as the window widens.
        let mut panes = Vec::new();
        if self.current_preset_idx.is_some() {
            if detail >= W_EDITOR_MIN + W_GLOBAL + W_HISTORY {
                panes.extend([DetailPane::Editor, DetailPane::Global, DetailPane::History]);
            } else if detail >= W_EDITOR_MIN + W_GLOBAL {
                panes.push(DetailPane::Editor);
                panes.push(aux);
            } else {
                panes.push(focused);
            }
        } else if detail >= W_GLOBAL + W_HISTORY {
            panes.extend([DetailPane::Global, DetailPane::History]);
        } else {
            panes.push(aux);
        }
        self.detail_panes = panes;
    }

    fn render_detail_pane(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        pane: DetailPane,
        text: &LocaleText,
        content_bottom: f32,
        content_right: f32,
    ) {
        match pane {
            DetailPane::Editor => {
                if let Some(idx) = self.current_preset_idx {
                    self.render_preset_view(ui, ctx, idx, text, content_bottom, content_right);
                }
            }
            DetailPane::Global => {
                let usage_stats = {
                    let app = self.app_state_ref.lock().unwrap();
                    app.model_usage_stats.clone()
                };
                if render_global_settings(
                    ui,
                    &mut self.config,
                    &mut self.show_api_key,
                    &mut self.show_gemini_api_key,
                    &mut self.show_openrouter_api_key,
                    &mut self.show_nvidia_api_key,
                    &usage_stats,
                    &self.updater,
                    &self.update_status,
                    &mut self.run_at_startup,
                    &self.auto_launcher,
                    self.current_admin_state,
                    text,
                    &mut self.show_models_modal,
                    &mut self.models_tab,
                    &mut self.show_tts_modal,
                    &mut self.show_tools_modal,
                    &mut self.show_restore_defaults_modal,
                    &mut self.download_manager,
                    &self.cached_audio_devices,
                    &mut self.recording_sr_hotkey,
                ) {
                    self.save_and_sync();
                }
            }
            DetailPane::History => {
                let history_manager = {
                    let app = self.app_state_ref.lock().unwrap();
                    app.history.clone()
                };
                if render_history_panel(
                    ui,
                    &mut self.config,
                    &history_manager,
                    &mut self.search_query,
                    text,
                    content_bottom,
                ) {
                    self.save_and_sync();
                }
            }
        }
    }

    fn render_preset_view(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        idx: usize,
        text: &LocaleText,
        content_bottom: f32,
        content_right: f32,
    ) {
        let preset_key = self.config.presets.get(idx).map(|preset| {
            let profile_id = self
                .config
                .preset_profiles
                .get(self.config.active_preset_profile_idx)
                .map(|profile| profile.id.clone())
                .unwrap_or_default();
            (idx, profile_id, preset.id.clone())
        });

        // Sync snarl state if switching presets, profiles, or first load.
        if preset_key.as_ref() != self.last_edited_preset_key.as_ref()
            && idx < self.config.presets.len()
        {
            self.snarl = Some(blocks_to_snarl(
                &self.config.presets[idx].blocks,
                &self.config.presets[idx].block_connections,
                &self.config.presets[idx].preset_type,
            ));
            self.last_edited_preset_key = preset_key;
        }

        if let Some(snarl) = &mut self.snarl
            && render_preset_editor(
                ui,
                &mut self.config,
                idx,
                &mut self.recording_hotkey_for_preset,
                &self.hotkey_conflict_msg,
                text,
                snarl,
                content_bottom,
                content_right,
            )
        {
            // Sync back to blocks and connections
            if idx < self.config.presets.len() {
                let (blocks, connections) = snarl_to_graph(snarl);
                self.config.presets[idx].blocks = blocks;
                self.config.presets[idx].block_connections = connections;
            }
            self.save_and_sync();
        }
    }
}

#[cfg(test)]
mod body_inset_tests {
    use super::{
        BODY_EDGE_INSET, RIGHT_PAD, SIDEBAR_GRID_TRAILING_PAD, SIDEBAR_MAX_WIDTH,
        SIDEBAR_MIN_WIDTH, sidebar_width_for,
    };

    /// The preset grid must fit inside the sidebar with its trailing pad
    /// intact, whatever the body inset is — the star column lives in that pad.
    #[test]
    fn the_sidebar_leaves_its_grid_the_width_it_measured() {
        let inset = f32::from(BODY_EDGE_INSET);
        for grid in [440.0_f32, 500.0, 600.0, 700.0] {
            let content = sidebar_width_for(grid) - inset;
            assert!(
                content >= grid + SIDEBAR_GRID_TRAILING_PAD,
                "grid {grid} got {content} points of content"
            );
        }
        // Past the cap the panel stops growing, by design.
        assert_eq!(sidebar_width_for(SIDEBAR_MAX_WIDTH), SIDEBAR_MAX_WIDTH);
        assert_eq!(sidebar_width_for(0.0), SIDEBAR_MIN_WIDTH);
    }

    /// The body must sit the same distance from both window edges.
    ///
    /// The left inset lives in the sidebar panel's frame and the right in the
    /// width of a spacer panel — two unrelated places that drifted to 8px and
    /// 18px. Anyone changing one has to change the constant, which moves both.
    #[test]
    fn the_body_is_inset_equally_from_both_window_edges() {
        assert_eq!(f32::from(BODY_EDGE_INSET), RIGHT_PAD);
        assert_eq!(
            BODY_EDGE_INSET,
            crate::gui::theme::space::CARD,
            "the body inset must come from the spacing scale"
        );
        // Content sits wider from the frame than the bars do.
        const _: () = assert!(BODY_EDGE_INSET > crate::gui::theme::WINDOW_EDGE_INSET);
    }
}
