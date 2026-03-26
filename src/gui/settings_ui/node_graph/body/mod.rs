mod model_selector;

use super::node::ChainNode;
use super::viewer::ChainViewer;
use crate::gui::icons::{Icon, icon_button};
use crate::model_config::ModelType;
use eframe::egui;
use egui_snarl::{NodeId, Snarl};

pub fn show_body(
    viewer: &mut ChainViewer,
    node_id: NodeId,
    ui: &mut egui::Ui,
    snarl: &mut Snarl<ChainNode>,
) {
    #[allow(deprecated)]
    {
        let mut auto_copy_triggered = false;
        let current_node_uuid = snarl
            .get_node(node_id)
            .map(|n| n.id().to_string())
            .unwrap_or_default();

        // Render Node UI
        {
            let node = snarl.get_node_mut(node_id).unwrap();

            ui.vertical(|ui| {
                ui.set_max_width(320.0);

                match node {
                    ChainNode::Input {
                        block_type,
                        auto_copy,
                        auto_speak,
                        show_overlay,
                        render_mode,
                        ..
                    } => {
                        ui.set_min_width(173.0);

                        // Determine actual input type
                        let actual_type = if block_type == "input_adapter" {
                            viewer.preset_type.as_str()
                        } else {
                            block_type.as_str()
                        };

                        // Eye button + Display mode row for Input nodes
                        ui.horizontal(|ui| {
                            // Eye icon toggle
                            let icon = if *show_overlay {
                                Icon::EyeOpen
                            } else {
                                Icon::EyeClosed
                            };
                            if icon_button(ui, icon).clicked() {
                                *show_overlay = !*show_overlay;
                                viewer.changed = true;

                                // When turning ON, auto-set render_mode based on input type
                                if *show_overlay {
                                    *render_mode = if actual_type == "text" {
                                        "plain".to_string()
                                    } else {
                                        "markdown".to_string()
                                    };
                                }
                            }

                            if *show_overlay {
                                // Render Mode Dropdown for input display
                                let current_mode_label = if render_mode == "markdown"
                                    || render_mode == "markdown_stream"
                                {
                                    // Normalize markdown_stream to markdown for Input nodes (they don't stream)
                                    if render_mode == "markdown_stream" {
                                        *render_mode = "markdown".to_string();
                                    }
                                    match viewer.ui_language.as_str() {
                                        "vi" => "Đẹp",
                                        "ko" => "마크다운",
                                        _ => "Markdown",
                                    }
                                } else {
                                    // Normalize stream to plain for Input nodes
                                    if render_mode == "stream" {
                                        *render_mode = "plain".to_string();
                                    }
                                    match viewer.ui_language.as_str() {
                                        "vi" => "Thường",
                                        "ko" => "일반",
                                        _ => "Normal",
                                    }
                                };

                                let popup_id = ui.make_persistent_id(format!(
                                    "input_render_mode_popup_{:?}",
                                    node_id
                                ));
                                let btn_bg = if ui.visuals().dark_mode {
                                    egui::Color32::from_rgba_unmultiplied(80, 80, 80, 180)
                                } else {
                                    egui::Color32::from_rgba_unmultiplied(220, 220, 220, 200)
                                };
                                let btn = ui.add(
                                    egui::Button::new(current_mode_label)
                                        .fill(btn_bg)
                                        .corner_radius(4.0),
                                );
                                if btn.clicked() {
                                    ui.memory_mut(|mem| mem.toggle_popup(popup_id));
                                }
                                egui::popup_below_widget(
                                    ui,
                                    popup_id,
                                    &btn,
                                    egui::PopupCloseBehavior::CloseOnClickOutside,
                                    |ui| {
                                        ui.set_min_width(60.0);
                                        let (lbl_norm, lbl_md) = match viewer.ui_language.as_str() {
                                            "vi" => ("Thường", "Đẹp"),
                                            "ko" => ("일반", "마크다운"),
                                            _ => ("Normal", "Markdown"),
                                        };

                                        if ui
                                            .selectable_label(render_mode == "plain", lbl_norm)
                                            .clicked()
                                        {
                                            *render_mode = "plain".to_string();
                                            viewer.changed = true;
                                            ui.memory_mut(|mem| mem.close_popup(popup_id));
                                        }
                                        if ui
                                            .selectable_label(render_mode == "markdown", lbl_md)
                                            .clicked()
                                        {
                                            *render_mode = "markdown".to_string();
                                            viewer.changed = true;
                                            ui.memory_mut(|mem| mem.close_popup(popup_id));
                                        }
                                    },
                                );
                            }
                        });

                        // Copy/Speak toggles for Input - Conditional based on Type
                        ui.horizontal(|ui| {
                            let show_copy = actual_type != "audio";
                            let show_speak = actual_type == "text";

                            if show_copy {
                                let is_text_input = actual_type == "text";

                                if is_text_input {
                                    if !*auto_copy {
                                        *auto_copy = true;
                                        viewer.changed = true;
                                    }
                                    let _ = icon_button(ui, Icon::Copy)
                                        .on_hover_text(viewer.text.input_auto_copy_tooltip);
                                } else {
                                    let copy_icon = if *auto_copy {
                                        Icon::Copy
                                    } else {
                                        Icon::CopyDisabled
                                    };
                                    if icon_button(ui, copy_icon)
                                        .on_hover_text(viewer.text.input_auto_copy_tooltip)
                                        .clicked()
                                    {
                                        *auto_copy = !*auto_copy;
                                        viewer.changed = true;
                                        if *auto_copy {
                                            auto_copy_triggered = true;
                                        }
                                    }
                                }
                            }

                            if show_speak {
                                let speak_icon = if *auto_speak {
                                    Icon::Speaker
                                } else {
                                    Icon::SpeakerDisabled
                                };
                                if icon_button(ui, speak_icon)
                                    .on_hover_text(viewer.text.input_auto_speak_tooltip)
                                    .clicked()
                                {
                                    *auto_speak = !*auto_speak;
                                    viewer.changed = true;
                                }
                            }
                        });
                    }
                    ChainNode::Special {
                        model,
                        prompt,
                        language_vars,
                        show_overlay,
                        streaming_enabled,
                        render_mode,
                        auto_copy,
                        auto_speak,
                        ..
                    } => {
                        let target_model_type = match viewer.preset_type.as_str() {
                            "image" => ModelType::Vision,
                            "audio" => ModelType::Audio,
                            _ => ModelType::Text,
                        };

                        if model_selector::show_model_and_settings(
                            ui,
                            viewer,
                            target_model_type,
                            model,
                            prompt,
                            language_vars,
                            show_overlay,
                            streaming_enabled,
                            render_mode,
                            auto_copy,
                            auto_speak,
                        ) {
                            auto_copy_triggered = true;
                        }
                    }
                    ChainNode::Process {
                        model,
                        prompt,
                        language_vars,
                        show_overlay,
                        streaming_enabled,
                        render_mode,
                        auto_copy,
                        auto_speak,
                        ..
                    } => {
                        let target_model_type = ModelType::Text;

                        if model_selector::show_model_and_settings(
                            ui,
                            viewer,
                            target_model_type,
                            model,
                            prompt,
                            language_vars,
                            show_overlay,
                            streaming_enabled,
                            render_mode,
                            auto_copy,
                            auto_speak,
                        ) {
                            auto_copy_triggered = true;
                        }
                    }
                }
            });
        }

        // Enforce auto-copy exclusivity
        if auto_copy_triggered {
            for node in snarl.nodes_mut() {
                if node.id() != current_node_uuid {
                    node.set_auto_copy(false);
                }
            }
        }
    }
}
