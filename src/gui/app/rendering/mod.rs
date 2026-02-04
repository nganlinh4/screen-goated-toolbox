// --- RENDERING MODULE ---
// Main application rendering: layout, title bar, footer, and overlays.

mod footer;
mod overlays;
mod title_bar;

use super::types::SettingsApp;
use crate::gui::locale::LocaleText;
use crate::gui::settings_ui::node_graph::{blocks_to_snarl, snarl_to_graph};
use crate::gui::settings_ui::{
    render_global_settings, render_history_panel, render_preset_editor, render_sidebar, ViewMode,
};
use eframe::egui;

impl SettingsApp {
    pub(crate) fn render_main_layout(&mut self, ctx: &egui::Context) {
        let text = LocaleText::get(&self.config.ui_language);
        let _is_dark = ctx.style().visuals.dark_mode;

        egui::CentralPanel::default()
            .frame(
                egui::Frame::NONE
                    .fill(ctx.style().visuals.panel_fill)
                    .corner_radius(egui::CornerRadius {
                        nw: 0,
                        ne: 0,
                        sw: 0, // Footer handles bottom corners now
                        se: 0, // Footer handles bottom corners now
                    }),
            )
            .show(ctx, |ui| {
                let available_width = ui.available_width();
                let left_width = available_width * 0.35;
                let right_width = available_width * 0.65;

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
                                    top: 8,
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
                            self.render_detail_view(ui, ctx, &text);
                        },
                    );
                });
            });
    }

    fn render_detail_view(&mut self, ui: &mut egui::Ui, ctx: &egui::Context, text: &LocaleText) {
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
                ) {
                    self.save_and_sync();
                }
            }
            ViewMode::Preset(idx) => {
                self.render_preset_view(ui, ctx, idx, text);
            }
        }
    }

    fn render_preset_view(
        &mut self,
        ui: &mut egui::Ui,
        _ctx: &egui::Context,
        idx: usize,
        text: &LocaleText,
    ) {
        // Sync snarl state if switching presets or first load
        if self.last_edited_preset_idx != Some(idx) {
            if idx < self.config.presets.len() {
                self.snarl = Some(blocks_to_snarl(
                    &self.config.presets[idx].blocks,
                    &self.config.presets[idx].block_connections,
                    &self.config.presets[idx].preset_type,
                ));
                self.last_edited_preset_idx = Some(idx);
            }
        }

        if let Some(snarl) = &mut self.snarl {
            if render_preset_editor(
                ui,
                &mut self.config,
                idx,
                &mut self.search_query,
                &mut self.cached_monitors,
                &mut self.recording_hotkey_for_preset,
                &self.hotkey_conflict_msg,
                text,
                snarl,
            ) {
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
}
