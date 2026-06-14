use super::get_localized_preset_name;
use super::node_graph::{
    ChainNode, blocks_to_snarl, render_node_graph, request_node_graph_view_reset,
};
use crate::config::Config;
use crate::gui::locale::LocaleText;
use eframe::egui;
use egui_snarl::Snarl;

mod controller_description;
mod graph_helpers;
mod hotkeys;

use controller_description::render_controller_mode_description;
use graph_helpers::{create_default_block_for_type, sync_graph_type};
use hotkeys::render_hotkeys;

#[expect(
    clippy::too_many_arguments,
    reason = "preset editor wires several independent config handles plus the layout floor"
)]
pub fn render_preset_editor(
    ui: &mut egui::Ui,
    config: &mut Config,
    preset_idx: usize,
    recording_hotkey_for_preset: &mut Option<usize>,
    hotkey_conflict_msg: &Option<String>,
    text: &LocaleText,
    snarl: &mut Snarl<ChainNode>,
    content_bottom: f32,
    content_right: f32,
) -> bool {
    if preset_idx >= config.presets.len() {
        return false;
    }

    let mut preset = config.presets[preset_idx].clone();
    let mut changed = false;

    // Forms read best at a fixed width, but the node-graph canvas below should
    // use the full detail-view width (responsive) instead of leaving dead space
    // on wide windows. Cap the config sections here; restore full width before
    // the graph.
    let full_width = ui.available_width();
    ui.set_max_width(full_width.min(510.0));

    // Check if this is a default preset (ID starts with "preset_")
    let is_default_preset = preset.id.starts_with("preset_");

    // Get localized name for default presets
    let display_name = if is_default_preset {
        get_localized_preset_name(&preset.id, &config.ui_language)
    } else {
        preset.name.clone()
    };

    // --- HEADER CARD: Name, Type & Settings ---
    let is_dark = ui.visuals().dark_mode;
    let theme = crate::gui::theme::AppTheme::from_dark(is_dark);
    let header_bg = theme.card_bg();
    let header_stroke = theme.card_stroke();

    ui.add_space(5.0);
    egui::Frame::new()
        .fill(header_bg)
        .stroke(header_stroke)
        .inner_margin(12.0)
        .corner_radius(10.0)
        .show(ui, |ui| {
            // Row 1: Preset Name + Controller + Restore
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(text.preset_name_label).strong());

                if is_default_preset {
                    ui.label(egui::RichText::new(&display_name).strong().size(15.0));
                } else if ui
                    .add(egui::TextEdit::singleline(&mut preset.name).font(egui::TextStyle::Body))
                    .changed()
                {
                    changed = true;
                }

                ui.add_space(10.0);

                // Controller checkbox with subtle styling
                // Hide for realtime audio presets (they always use the realtime overlay)
                let is_realtime_audio =
                    preset.preset_type == "audio" && preset.audio_processing_mode == "realtime";
                if !is_realtime_audio
                    && ui
                        .checkbox(
                            &mut preset.show_controller_ui,
                            text.controller_checkbox_label,
                        )
                        .clicked()
                {
                    if !preset.show_controller_ui && preset.blocks.is_empty() {
                        preset
                            .blocks
                            .push(create_default_block_for_type(&preset.preset_type));
                        *snarl = blocks_to_snarl(
                            &preset.blocks,
                            &preset.block_connections,
                            &preset.preset_type,
                        );
                    }
                    changed = true;
                }

                if is_default_preset {
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        // Restore button with subtle styling (violet), rendered via the
                        // shared filled-button helper so it keeps Material hover/press
                        // state layers.
                        if crate::gui::widgets::filled_button(
                            ui,
                            text.restore_preset_btn,
                            theme.restore_fill(),
                            theme.on_accent(),
                            8,
                        )
                        .on_hover_text(text.restore_preset_tooltip)
                        .clicked()
                        {
                            let default_config = Config::default();
                            if let Some(default_p) =
                                default_config.presets.iter().find(|p| p.id == preset.id)
                            {
                                preset = default_p.clone();
                                *snarl = blocks_to_snarl(
                                    &preset.blocks,
                                    &preset.block_connections,
                                    &preset.preset_type,
                                );
                                request_node_graph_view_reset(ui.ctx());
                                changed = true;
                            }
                        }
                    });
                }
            });

            ui.add_space(6.0);

            // Row 2: Type + Mode selectors
            ui.horizontal(|ui| {
                ui.label(text.preset_type_label);
                let selected_text = match preset.preset_type.as_str() {
                    "audio" => text.preset_type_audio,
                    "video" => text.preset_type_video,
                    "text" => text.preset_type_text,
                    _ => text.preset_type_image,
                };

                crate::gui::widgets::combo("preset_type_combo")
                    .selected_text(selected_text)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_value(
                                &mut preset.preset_type,
                                "image".to_string(),
                                text.preset_type_image,
                            )
                            .clicked()
                        {
                            sync_graph_type(snarl, &preset.preset_type);
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut preset.preset_type,
                                "text".to_string(),
                                text.preset_type_text,
                            )
                            .clicked()
                        {
                            sync_graph_type(snarl, &preset.preset_type);
                            changed = true;
                        }
                        if ui
                            .selectable_value(
                                &mut preset.preset_type,
                                "audio".to_string(),
                                text.preset_type_audio,
                            )
                            .clicked()
                        {
                            sync_graph_type(snarl, &preset.preset_type);
                            changed = true;
                        }
                        ui.add_enabled_ui(false, |ui| {
                            let _ = ui.selectable_value(
                                &mut preset.preset_type,
                                "video".to_string(),
                                text.preset_type_video,
                            );
                        });
                    });

                ui.add_space(15.0);

                // Mode selectors based on type
                if preset.preset_type == "image" {
                    if !preset.show_controller_ui {
                        ui.label(text.command_mode_label);
                        crate::gui::widgets::combo("prompt_mode_combo")
                            .selected_text(if preset.prompt_mode == "dynamic" {
                                text.prompt_mode_dynamic
                            } else {
                                text.prompt_mode_fixed
                            })
                            .show_ui(ui, |ui| {
                                if ui
                                    .selectable_value(
                                        &mut preset.prompt_mode,
                                        "fixed".to_string(),
                                        text.prompt_mode_fixed,
                                    )
                                    .clicked()
                                {
                                    changed = true;
                                }
                                if ui
                                    .selectable_value(
                                        &mut preset.prompt_mode,
                                        "dynamic".to_string(),
                                        text.prompt_mode_dynamic,
                                    )
                                    .clicked()
                                {
                                    changed = true;
                                }
                            });
                    }
                } else if preset.preset_type == "text" {
                    ui.label(text.text_input_mode_label);
                    crate::gui::widgets::combo("text_input_mode_combo")
                        .selected_text(if preset.text_input_mode == "type" {
                            text.text_mode_type
                        } else {
                            text.text_mode_select
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut preset.text_input_mode,
                                    "select".to_string(),
                                    text.text_mode_select,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut preset.text_input_mode,
                                    "type".to_string(),
                                    text.text_mode_type,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                        });

                    if preset.text_input_mode == "type"
                        && !preset.show_controller_ui
                        && ui
                            .checkbox(&mut preset.continuous_input, text.continuous_input_label)
                            .clicked()
                    {
                        changed = true;
                    }
                } else if preset.preset_type == "audio" && !preset.show_controller_ui {
                    ui.label(text.audio_mode_label);

                    let mode_record = text.audio_mode_record_then_process;
                    let mode_realtime = text.audio_mode_realtime;

                    let selected_mode_text = if preset.audio_processing_mode == "realtime" {
                        mode_realtime
                    } else {
                        mode_record
                    };

                    crate::gui::widgets::combo("audio_operation_mode_combo")
                        .selected_text(selected_mode_text)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut preset.audio_processing_mode,
                                    "record_then_process".to_string(),
                                    mode_record,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut preset.audio_processing_mode,
                                    "realtime".to_string(),
                                    mode_realtime,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                        });
                }
            });

            // Row 2.5: Realtime Interface
            if preset.preset_type == "audio"
                && preset.audio_processing_mode == "realtime"
                && !preset.show_controller_ui
            {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.label(text.realtime_interface_label);

                    let mode_standard = text.realtime_interface_standard;
                    let mode_minimal = text.realtime_interface_minimal;

                    let selected_window_mode = if preset.realtime_window_mode == "minimal" {
                        mode_minimal
                    } else {
                        mode_standard
                    };

                    crate::gui::widgets::combo("realtime_window_mode_combo")
                        .selected_text(selected_window_mode)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut preset.realtime_window_mode,
                                    "standard".to_string(),
                                    mode_standard,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut preset.realtime_window_mode,
                                    "minimal".to_string(),
                                    mode_minimal,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                        });
                });
            }

            // Row 3: Audio source (if applicable) - Hide if Realtime mode
            if preset.preset_type == "audio" && preset.audio_processing_mode != "realtime" {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(text.audio_source_label);
                    let selected_text = if preset.audio_source == "mic" {
                        text.audio_src_mic
                    } else {
                        text.audio_src_device
                    };
                    crate::gui::widgets::combo("audio_source_combo")
                        .selected_text(selected_text)
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut preset.audio_source,
                                    "mic".to_string(),
                                    text.audio_src_mic,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut preset.audio_source,
                                    "device".to_string(),
                                    text.audio_src_device,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                        });
                    if !preset.show_controller_ui {
                        ui.add_space(10.0);
                        if ui
                            .checkbox(&mut preset.hide_recording_ui, text.hide_recording_ui_label)
                            .clicked()
                        {
                            changed = true;
                        }
                        ui.add_space(6.0);
                        if ui
                            .checkbox(
                                &mut preset.auto_stop_recording,
                                text.auto_stop_recording_label,
                            )
                            .clicked()
                        {
                            changed = true;
                        }
                    }
                });
            }

            // Row 3b: Command mode for text select presets (new row)
            if preset.preset_type == "text"
                && preset.text_input_mode == "select"
                && !preset.show_controller_ui
            {
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.label(text.command_mode_label);
                    crate::gui::widgets::combo("text_prompt_mode_combo")
                        .selected_text(if preset.prompt_mode == "dynamic" {
                            text.prompt_mode_dynamic
                        } else {
                            text.prompt_mode_fixed
                        })
                        .show_ui(ui, |ui| {
                            if ui
                                .selectable_value(
                                    &mut preset.prompt_mode,
                                    "fixed".to_string(),
                                    text.prompt_mode_fixed,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                            if ui
                                .selectable_value(
                                    &mut preset.prompt_mode,
                                    "dynamic".to_string(),
                                    text.prompt_mode_dynamic,
                                )
                                .clicked()
                            {
                                changed = true;
                            }
                        });
                });
            }
        });

    ui.add_space(8.0);

    // Determine visibility conditions: Auto Paste is visible if any NON-input_adapter block has auto_copy enabled
    let has_any_auto_copy = preset
        .blocks
        .iter()
        .any(|b| b.auto_copy && b.block_type != "input_adapter");

    // Show auto-paste control whenever any applicable block has auto_copy enabled AND controller UI is off
    if has_any_auto_copy && !preset.show_controller_ui {
        ui.horizontal(|ui| {
            if ui
                .checkbox(&mut preset.auto_paste, text.auto_paste_label)
                .clicked()
            {
                changed = true;
            }

            // Auto Newline: visible when any (non-input-adapter) block has auto_copy
            // Since has_any_auto_copy already excludes input_adapter, we can show it directly
            if ui
                .checkbox(
                    &mut preset.auto_paste_newline,
                    text.auto_paste_newline_label,
                )
                .clicked()
            {
                changed = true;
            }
        });
    } else if !has_any_auto_copy {
        // No auto_copy means auto_paste must be off
        if preset.auto_paste {
            preset.auto_paste = false;
            changed = true;
        }
    }

    ui.add_space(10.0);

    // Hotkeys - always visible, even when controller UI is enabled
    if render_hotkeys(
        ui,
        preset_idx,
        &mut preset.hotkeys,
        recording_hotkey_for_preset,
        hotkey_conflict_msg,
        text,
    ) {
        changed = true;
    }

    // --- PROCESSING CHAIN UI ---
    // Hide nodegraph when controller UI is enabled OR when in Realtime mode (no graph needed)
    if !(preset.show_controller_ui
        || preset.preset_type == "audio" && preset.audio_processing_mode == "realtime")
    {
        // Frame the node graph like the cards above it (same fill, border and
        // radius) so it reads as one consistent surface — not a panel with a
        // mismatched padded band around the canvas.
        let graph_theme = crate::gui::theme::AppTheme::from_ui(ui);

        // Width + height both come from the panel's true edges (`content_right`
        // / `content_bottom`), not the column's `available_*` which report
        // capped values here. Leave a clear margin from the window edge on the
        // right, and a gap above the footer at the bottom (so the canvas doesn't
        // literally touch either). The frame adds a 6+6 inner margin.
        const EDGE_GAP: f32 = 16.0;
        let graph_left = ui.cursor().left();
        let graph_top = ui.cursor().top();
        let graph_w = (content_right - graph_left - EDGE_GAP).max(320.0);
        let graph_min_h = (content_bottom - graph_top - EDGE_GAP - 12.0).max(300.0);
        ui.set_max_width(graph_w);
        ui.push_id("node_graph_area", |ui| {
            egui::Frame::new()
                .fill(graph_theme.card_bg())
                .stroke(graph_theme.card_stroke())
                .inner_margin(6.0)
                .corner_radius(10.0)
                .show(ui, |ui| {
                    if render_node_graph(
                        ui,
                        snarl,
                        &config.ui_language,
                        config.use_groq,
                        config.use_gemini,
                        config.use_openrouter,
                        config.use_ollama,
                        &preset.preset_type,
                        text,
                        graph_min_h,
                    ) {
                        changed = true;
                    }
                });
        });
    } else {
        render_controller_mode_description(
            ui,
            &config.ui_language,
            &preset.preset_type,
            &preset.audio_processing_mode,
        );
    }

    // Apply Logic Updates (Radio Button Sync & Auto Paste)
    if changed {
        config.presets[preset_idx] = preset;
    }

    changed
}
