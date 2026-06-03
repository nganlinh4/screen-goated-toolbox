// --- RENDERING MODULE ---
// Main application rendering: layout, title bar, footer, and overlays.

mod footer;
mod overlays;
mod title_bar;

use super::types::SettingsApp;
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::node_graph::{blocks_to_snarl, snarl_to_graph};
use crate::gui::settings_ui::{
    ViewMode, render_global_settings, render_history_panel, render_preset_editor, render_sidebar,
};
use eframe::egui;
use std::sync::atomic::Ordering;

impl SettingsApp {
    pub(crate) fn render_main_layout(&mut self, root_ui: &mut egui::Ui) {
        let text = LocaleText::get(&self.config.ui_language);
        let ctx = root_ui.ctx().clone();
        let ctx = &ctx;
        let panel_fill = root_ui.visuals().panel_fill;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(panel_fill)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 0, // Footer handles bottom corners now
                        se: 0, // Footer handles bottom corners now
                    }),
            )
            .show_inside(root_ui, |ui| {
                // The panel's own max_rect edges are the real content bounds.
                // Nested columns can report a wrong `available_height`/`_width`,
                // so thread these absolute coords down to anything that must
                // fill to the bottom / right (the node-graph canvas).
                let content_bottom = ui.max_rect().bottom();
                let content_right = ui.max_rect().right();
                let available_width = ui.available_width();
                // Responsive split: the sidebar holds the fixed-width preset
                // grid, so clamp it to its content (never starve it on narrow
                // windows, never let it bloat on wide ones). The detail view
                // takes all remaining width — on wide monitors that means the
                // node-graph canvas fills the space instead of leaving it dead.
                let left_width = (available_width * 0.35).clamp(440.0, 500.0);
                let right_width = (available_width - left_width).max(0.0);

                ui.horizontal(|ui| {
                    // Left Sidebar
                    ui.allocate_ui_with_layout(
                        egui::vec2(left_width, ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            // Add Left Margin/Padding for Sidebar
                            egui::Frame::NONE
                                .inner_margin(egui::Margin {
                                    left: 8,
                                    right: 0,
                                    // Tighter top margin pulls the profile bar up
                                    // closer to the title bar (was 8).
                                    top: 2,
                                    bottom: 0,
                                })
                                .show(ui, |ui| {
                                    if render_sidebar(
                                        ui,
                                        &mut self.config,
                                        &mut self.view_mode,
                                        &text,
                                    ) {
                                        self.save_and_sync();
                                    }
                                    self.update_sr_hotkey_recording(ctx);
                                });
                        },
                    );

                    ui.add_space(10.0);

                    // Right Detail View
                    ui.allocate_ui_with_layout(
                        egui::vec2((right_width - 20.0).max(0.0), ui.available_height()),
                        egui::Layout::top_down(egui::Align::Min),
                        |ui| {
                            self.render_detail_view(ui, ctx, &text, content_bottom, content_right);
                        },
                    );
                });
            });
    }

    fn render_detail_view(
        &mut self,
        ui: &mut egui::Ui,
        ctx: &egui::Context,
        text: &LocaleText,
        content_bottom: f32,
        content_right: f32,
    ) {
        // Poll translation gummy request to open TTS settings
        if crate::overlay::translation_gummy::REQUEST_OPEN_TTS_SETTINGS
            .swap(false, std::sync::atomic::Ordering::SeqCst)
        {
            self.view_mode = ViewMode::Global;
            self.show_tts_modal = true;
        }
        if super::types::REQUEST_OPEN_DOWNLOADED_TOOLS.swap(false, Ordering::SeqCst) {
            self.view_mode = ViewMode::Global;
            self.show_tools_modal = true;
        }
        match self.view_mode {
            ViewMode::Global => {
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
                    &mut self.show_cerebras_api_key,
                    &usage_stats,
                    &self.updater,
                    &self.update_status,
                    &mut self.run_at_startup,
                    &self.auto_launcher,
                    self.current_admin_state,
                    text,
                    &mut self.show_usage_modal,
                    &mut self.show_tts_modal,
                    &mut self.show_tools_modal,
                    &mut self.show_model_priority_modal,
                    &mut self.download_manager,
                    &self.cached_audio_devices,
                    &mut self.recording_sr_hotkey,
                ) {
                    self.save_and_sync();
                }
            }
            ViewMode::History => {
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
            ViewMode::Preset(idx) => {
                self.render_preset_view(ui, ctx, idx, text, content_bottom, content_right);
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
