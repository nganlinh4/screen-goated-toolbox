use eframe::egui;
use crate::config::{Config, Preset, get_all_languages};
use crate::gui::locale::LocaleText;
use crate::gui::icons::{Icon, icon_button, draw_icon_static};
use crate::model_config::{get_all_models, ModelType, get_model_by_id};
use crate::updater::{Updater, UpdateStatus};
use std::collections::HashMap;
use auto_launch::AutoLaunch;

#[derive(PartialEq, Clone, Copy)]
pub enum ViewMode {
    Global,
    Preset(usize),
}

// --- Sidebar ---
pub fn render_sidebar(
    ui: &mut egui::Ui,
    config: &mut Config,
    view_mode: &mut ViewMode,
    text: &LocaleText,
) -> bool {
    let mut changed = false;

    // Theme & Language Controls
    ui.horizontal(|ui| {
        let theme_icon = if config.dark_mode { Icon::Moon } else { Icon::Sun };
        if icon_button(ui, theme_icon).on_hover_text("Toggle Theme").clicked() {
            config.dark_mode = !config.dark_mode;
            changed = true;
        }
        
        let original_lang = config.ui_language.clone();
        let lang_display = match config.ui_language.as_str() {
            "vi" => "VI",
            "ko" => "KO",
            _ => "EN",
        };
        egui::ComboBox::from_id_source("header_lang_switch")
            .width(60.0)
            .selected_text(lang_display)
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut config.ui_language, "en".to_string(), "English");
                ui.selectable_value(&mut config.ui_language, "vi".to_string(), "Vietnamese");
                ui.selectable_value(&mut config.ui_language, "ko".to_string(), "Korean");
            });
        if original_lang != config.ui_language {
            changed = true;
        }
    });
    ui.add_space(5.0);

    // Global Settings Button
    let is_global = matches!(view_mode, ViewMode::Global);
    ui.horizontal(|ui| {
        draw_icon_static(ui, Icon::Settings, None);
        if ui.selectable_label(is_global, text.global_settings).clicked() {
            *view_mode = ViewMode::Global;
        }
    });
    
    ui.add_space(10.0);
    ui.label(egui::RichText::new(text.presets_section).strong());
    
    let mut preset_idx_to_delete = None;

    for (idx, preset) in config.presets.iter().enumerate() {
        ui.horizontal(|ui| {
            let is_selected = matches!(view_mode, ViewMode::Preset(i) if *i == idx);
            
            let icon_type = if preset.preset_type == "audio" { Icon::Microphone }
            else if preset.preset_type == "video" { Icon::Video }
            else { Icon::Image };
            
            if preset.is_upcoming {
                ui.add_enabled_ui(false, |ui| {
                    ui.horizontal(|ui| {
                        draw_icon_static(ui, icon_type, None);
                        let _ = ui.selectable_label(is_selected, &preset.name);
                    });
                });
            } else {
                ui.horizontal(|ui| {
                    draw_icon_static(ui, icon_type, None);
                    if ui.selectable_label(is_selected, &preset.name).clicked() {
                        *view_mode = ViewMode::Preset(idx);
                    }
                });
                // Delete button (X icon)
                if config.presets.len() > 1 {
                    if icon_button(ui, Icon::Delete).clicked() {
                        preset_idx_to_delete = Some(idx);
                    }
                }
            }
        });
    }
    
    ui.add_space(5.0);
    if ui.button(text.add_preset_btn).clicked() {
        let mut new_preset = Preset::default();
        new_preset.name = format!("Preset {}", config.presets.len() + 1);
        config.presets.push(new_preset);
        *view_mode = ViewMode::Preset(config.presets.len() - 1);
        changed = true;
    }

    if let Some(idx) = preset_idx_to_delete {
        config.presets.remove(idx);
        if let ViewMode::Preset(curr) = *view_mode {
            if curr >= idx && curr > 0 {
                *view_mode = ViewMode::Preset(curr - 1);
            } else if config.presets.is_empty() {
                *view_mode = ViewMode::Global;
            } else {
                *view_mode = ViewMode::Preset(0);
            }
        }
        changed = true;
    }

    changed
}

// --- Global Settings Panel ---
pub fn render_global_settings(
    ui: &mut egui::Ui,
    config: &mut Config,
    show_api_key: &mut bool,
    show_gemini_api_key: &mut bool,
    usage_stats: &HashMap<String, String>,
    updater: &Option<Updater>,
    update_status: &UpdateStatus,
    run_at_startup: &mut bool,
    auto_launcher: &Option<AutoLaunch>,
    text: &LocaleText,
) -> bool {
    let mut changed = false;

    ui.add_space(10.0);
    
    // API Keys
    ui.group(|ui| {
        ui.label(egui::RichText::new(text.api_section).strong());
        ui.horizontal(|ui| {
            ui.label(text.api_key_label);
            if ui.link(text.get_key_link).clicked() { let _ = open::that("https://console.groq.com/keys"); }
        });
        ui.horizontal(|ui| {
            if ui.add(egui::TextEdit::singleline(&mut config.api_key).password(!*show_api_key).desired_width(320.0)).changed() {
                changed = true;
            }
            let eye_icon = if *show_api_key { Icon::EyeOpen } else { Icon::EyeClosed };
            if icon_button(ui, eye_icon).clicked() { *show_api_key = !*show_api_key; }
        });
        
        ui.add_space(5.0);
        ui.horizontal(|ui| {
            ui.label(text.gemini_api_key_label);
            if ui.link(text.gemini_get_key_link).clicked() { let _ = open::that("https://aistudio.google.com/app/apikey"); }
        });
        ui.horizontal(|ui| {
            if ui.add(egui::TextEdit::singleline(&mut config.gemini_api_key).password(!*show_gemini_api_key).desired_width(320.0)).changed() {
                changed = true;
            }
            let eye_icon = if *show_gemini_api_key { Icon::EyeOpen } else { Icon::EyeClosed };
            if icon_button(ui, eye_icon).clicked() { *show_gemini_api_key = !*show_gemini_api_key; }
        });
    });

    ui.add_space(10.0);
    
    // Usage Statistics
    render_usage_statistics(ui, usage_stats, text, &config.ui_language);

    ui.add_space(10.0);

    // Software Update
    render_update_section(ui, updater, update_status, text);

    ui.add_space(10.0);

    ui.horizontal(|ui| {
        if let Some(launcher) = auto_launcher {
            if ui.checkbox(run_at_startup, text.startup_label).clicked() {
                if *run_at_startup { let _ = launcher.enable(); } else { let _ = launcher.disable(); }
            }
        }
        if ui.button(text.reset_defaults_btn).clicked() {
            let saved_groq_key = config.api_key.clone();
            let saved_gemini_key = config.gemini_api_key.clone();
            
            *config = Config::default();
            
            config.api_key = saved_groq_key;
            config.gemini_api_key = saved_gemini_key;
            changed = true;
        }
    });

    changed
}

fn render_usage_statistics(
    ui: &mut egui::Ui, 
    usage_stats: &HashMap<String, String>, 
    text: &LocaleText,
    _lang_code: &str
) {
    ui.group(|ui| {
        ui.horizontal(|ui| {
            draw_icon_static(ui, Icon::Statistics, None);
            ui.label(egui::RichText::new(text.usage_statistics_title).strong());
            icon_button(ui, Icon::Info).on_hover_text(text.usage_statistics_tooltip);
        });
        
        egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
            egui::Grid::new("usage_grid").striped(true).show(ui, |ui| {
                ui.label(egui::RichText::new(text.usage_model_column).strong());
                ui.label(egui::RichText::new(text.usage_remaining_column).strong());
                ui.end_row();

                let mut shown_models = std::collections::HashSet::new();
                
                for model in get_all_models() {
                    if !model.enabled { continue; }
                    
                    if shown_models.contains(&model.full_name) { continue; }
                    shown_models.insert(model.full_name.clone());
                    
                    ui.label(model.full_name.clone());
                    
                    if model.provider == "groq" {
                        let status = usage_stats.get(&model.full_name).cloned().unwrap_or_else(|| "??? / ?".to_string());
                        ui.label(status);
                    } else if model.provider == "google" {
                        ui.hyperlink_to(text.usage_check_link, "https://aistudio.google.com/usage?timeRange=last-1-day&tab=rate-limit");
                    }
                    ui.end_row();
                }
            });
        });
    });
}

fn render_update_section(ui: &mut egui::Ui, updater: &Option<Updater>, status: &UpdateStatus, text: &LocaleText) {
    match status {
        UpdateStatus::Idle => {
            ui.horizontal(|ui| {
                ui.label(format!("{} v{}", text.current_version_label, env!("CARGO_PKG_VERSION")));
                if ui.button(text.check_for_updates_btn).clicked() {
                    if let Some(u) = updater { u.check_for_updates(); }
                }
            });
        },
        UpdateStatus::Checking => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(text.checking_github);
            });
        },
        UpdateStatus::UpToDate(ver) => {
            ui.horizontal(|ui| {
                ui.label(egui::RichText::new(format!("{} (v{})", text.up_to_date, ver)).color(egui::Color32::from_rgb(34, 139, 34)));
                if ui.button(text.check_again_btn).clicked() {
                    if let Some(u) = updater { u.check_for_updates(); }
                }
            });
        },
        UpdateStatus::UpdateAvailable { version, body } => {
            ui.colored_label(egui::Color32::YELLOW, format!("{} {}", text.new_version_available, version));
            ui.collapsing(text.release_notes_label, |ui| {
                ui.label(body);
            });
            ui.add_space(5.0);
            if ui.button(egui::RichText::new(text.download_update_btn).strong()).clicked() {
                if let Some(u) = updater { u.perform_update(); }
            }
        },
        UpdateStatus::Downloading => {
            ui.horizontal(|ui| {
                ui.spinner();
                ui.label(text.downloading_update);
            });
        },
        UpdateStatus::Error(e) => {
            ui.colored_label(egui::Color32::RED, format!("{} {}", text.update_failed, e));
            ui.label(egui::RichText::new(text.app_folder_writable_hint).size(11.0));
            if ui.button(text.retry_btn).clicked() {
                if let Some(u) = updater { u.check_for_updates(); }
            }
        },
        UpdateStatus::UpdatedAndRestartRequired => {
            ui.label(egui::RichText::new(text.update_success).color(egui::Color32::GREEN).heading());
            ui.label(text.restart_to_use_new_version);
            if ui.button(text.restart_app_btn).clicked() {
                if let Ok(exe_path) = std::env::current_exe() {
                    if let Some(exe_dir) = exe_path.parent() {
                        if let Ok(entries) = std::fs::read_dir(exe_dir) {
                            if let Some(newest_exe) = entries.filter_map(|e| e.ok()).filter(|e| {
                                    let name = e.file_name();
                                    let name_str = name.to_string_lossy();
                                    name_str.starts_with("ScreenGroundedTranslator_v") && name_str.ends_with(".exe")
                                }).max_by_key(|e| e.metadata().ok().and_then(|m| m.modified().ok()))
                            {
                                let _ = std::process::Command::new(newest_exe.path()).spawn();
                            }
                        }
                    }
                    std::process::exit(0);
                }
            }
        }
    }
}

// --- Preset Editor Panel ---
pub fn render_preset_editor(
    ui: &mut egui::Ui,
    config: &mut Config,
    preset_idx: usize,
    search_query: &mut String,
    cached_monitors: &mut Vec<String>,
    recording_hotkey_for_preset: &mut Option<usize>,
    hotkey_conflict_msg: &Option<String>,
    text: &LocaleText,
) -> bool {
    // Safety check
    if preset_idx >= config.presets.len() { return false; }

    let mut preset = config.presets[preset_idx].clone();
    let mut changed = false;

    ui.add_space(5.0);

    // 1. Name
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.preset_name_label).heading());
        if ui.add(egui::TextEdit::singleline(&mut preset.name).font(egui::TextStyle::Heading)).changed() {
            changed = true;
        }
    });
    
    // Type Dropdown
    ui.horizontal(|ui| {
         ui.label(text.preset_type_label);
         let image_label = text.preset_type_image;
         let audio_label = text.preset_type_audio;
         let video_label = text.preset_type_video;
         
         let selected_text = match preset.preset_type.as_str() {
             "audio" => audio_label,
             "video" => video_label,
             _ => image_label,
         };
         
         egui::ComboBox::from_id_source("preset_type_combo")
             .selected_text(selected_text)
             .show_ui(ui, |ui| {
                 if ui.selectable_value(&mut preset.preset_type, "image".to_string(), image_label).clicked() {
                     preset.model = "scout".to_string(); 
                     changed = true;
                 }
                 if ui.selectable_value(&mut preset.preset_type, "audio".to_string(), audio_label).clicked() {
                     preset.model = "whisper-fast".to_string(); 
                     changed = true;
                 }
                 ui.add_enabled_ui(false, |ui| {
                     let _ = ui.selectable_value(&mut preset.preset_type, "video".to_string(), video_label);
                 });
             });
    });

    let is_audio = preset.preset_type == "audio";
    let is_video = preset.preset_type == "video";

    if is_video {
        // Video Placeholder UI
        ui.group(|ui| {
             ui.label(egui::RichText::new(text.capture_method_label).strong());
             ui.horizontal(|ui| {
                 if icon_button(ui, Icon::Refresh).on_hover_text("Refresh Monitors").clicked() {
                     *cached_monitors = crate::gui::utils::get_monitor_names();
                 }

                 egui::ComboBox::from_id_source("video_cap_method")
                     .selected_text(if preset.video_capture_method == "region" {
                         text.region_capture.to_string()
                     } else {
                         preset.video_capture_method.strip_prefix("monitor:").unwrap_or("Unknown").to_string()
                     })
                     .show_ui(ui, |ui| {
                         if ui.selectable_value(&mut preset.video_capture_method, "region".to_string(), text.region_capture).clicked() {
                             changed = true;
                         }
                         for monitor in cached_monitors.iter() {
                             let val = format!("monitor:{}", monitor);
                             let label = format!("Full screen ({})", monitor);
                             if ui.selectable_value(&mut preset.video_capture_method, val, label).clicked() {
                                 changed = true;
                             }
                         }
                     });
             });
        });
    } else {
        // Standard UI
        let show_prompt_controls = !is_audio || (is_audio && preset.model.contains("gemini"));

        if show_prompt_controls {
            ui.group(|ui| {
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(text.prompt_label).strong());
                    if ui.button(text.insert_lang_btn).clicked() {
                        let mut max_num = 0;
                        for i in 1..=10 {
                            if preset.prompt.contains(&format!("{{language{}}}", i)) {
                                max_num = i;
                            }
                        }
                        let next_num = max_num + 1;
                        preset.prompt.push_str(&format!(" {{language{}}} ", next_num));
                        let key = format!("language{}", next_num);
                        if !preset.language_vars.contains_key(&key) {
                            preset.language_vars.insert(key, "Vietnamese".to_string());
                        }
                        changed = true;
                    }
                });
                
                if ui.add(egui::TextEdit::multiline(&mut preset.prompt).desired_rows(3).desired_width(f32::INFINITY)).changed() {
                    changed = true;
                }
                
                if preset.prompt.trim().is_empty() {
                    ui.colored_label(egui::Color32::RED, text.empty_prompt_warning);
                }
                
                // Language tags
                let mut detected_langs = Vec::new();
                for i in 1..=10 {
                    let pattern = format!("{{language{}}}", i);
                    if preset.prompt.contains(&pattern) {
                        detected_langs.push(i);
                    }
                }
                
                for num in detected_langs {
                    let key = format!("language{}", num);
                    if !preset.language_vars.contains_key(&key) {
                        preset.language_vars.insert(key.clone(), "Vietnamese".to_string());
                    }
                    let label = match config.ui_language.as_str() {
                        "vi" => format!("Ngôn ngữ cho thẻ {{language{}}}:", num),
                        "ko" => format!("{{language{}}} 태그 언어:", num),
                        _ => format!("Language for {{language{}}} tag:", num),
                    };
                    ui.horizontal(|ui| {
                        ui.label(label);
                        let current_lang = preset.language_vars.get(&key).cloned().unwrap_or_else(|| "Vietnamese".to_string());
                        ui.menu_button(current_lang.clone(), |ui| {
                            ui.style_mut().wrap = Some(false);
                            ui.set_min_width(150.0);
                            ui.add(egui::TextEdit::singleline(search_query).hint_text(text.search_placeholder));
                            let q = search_query.to_lowercase();
                            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                for lang in get_all_languages().iter() {
                                    if q.is_empty() || lang.to_lowercase().contains(&q) {
                                        if ui.button(lang).clicked() {
                                            preset.language_vars.insert(key.clone(), lang.clone());
                                            changed = true;
                                            ui.close_menu();
                                        }
                                    }
                                }
                            });
                        });
                    });
                }
            });
        }

        if is_audio {
            ui.group(|ui| {
                ui.label(egui::RichText::new(text.audio_source_label).strong());
                ui.horizontal(|ui| {
                    if ui.radio_value(&mut preset.audio_source, "mic".to_string(), text.audio_src_mic).clicked() {
                        changed = true;
                    }
                    if ui.radio_value(&mut preset.audio_source, "device".to_string(), text.audio_src_device).clicked() {
                        changed = true;
                    }
                    if ui.checkbox(&mut preset.hide_recording_ui, text.hide_recording_ui_label).clicked() {
                        changed = true;
                    }
                });
            });
        }

        ui.group(|ui| {
            ui.label(egui::RichText::new(text.model_section).strong());
            
            ui.horizontal(|ui| {
                let selected_model = get_model_by_id(&preset.model);
                let display_label = selected_model.as_ref()
                    .map(|m| match config.ui_language.as_str() {
                        "vi" => &m.name_vi,
                        "ko" => &m.name_ko,
                        _ => &m.name_en,
                    })
                    .map(|s| s.as_str())
                    .unwrap_or(&preset.model);

                egui::ComboBox::from_id_source("model_selector")
                    .selected_text(display_label)
                    .show_ui(ui, |ui| {
                        let target_type = if is_audio { ModelType::Audio } else { ModelType::Vision };
                        for model in get_all_models() {
                            if model.enabled && model.model_type == target_type {
                                let dropdown_label = format!("{} ({}) - {}", 
                                    match config.ui_language.as_str() {
                                        "vi" => &model.name_vi,
                                        "ko" => &model.name_ko,
                                        _ => &model.name_en,
                                    },
                                    model.full_name,
                                    model.quota_limit
                                );
                                if ui.selectable_value(&mut preset.model, model.id.clone(), dropdown_label).clicked() {
                                     changed = true;
                                     if is_audio && preset.model.contains("gemini") && preset.prompt.trim().is_empty() {
                                         preset.prompt = "Transcribe the audio accurately.".to_string();
                                     } else if is_audio && !preset.model.contains("gemini") && preset.prompt == "Transcribe the audio accurately." {
                                         preset.prompt = "".to_string();
                                     }
                                }
                            }
                        }
                    });

                 if !preset.hide_overlay {
                     ui.label(text.streaming_label);
                     egui::ComboBox::from_id_source("stream_combo")
                         .selected_text(if preset.streaming_enabled { text.streaming_option_stream } else { text.streaming_option_wait })
                         .show_ui(ui, |ui| {
                             if ui.selectable_value(&mut preset.streaming_enabled, false, text.streaming_option_wait).clicked() { changed = true; }
                             if ui.selectable_value(&mut preset.streaming_enabled, true, text.streaming_option_stream).clicked() { changed = true; }
                         });
                 }
            });

            ui.horizontal(|ui| {
                if ui.checkbox(&mut preset.auto_copy, text.auto_copy_label).clicked() {
                    changed = true;
                    if preset.auto_copy { preset.retranslate_auto_copy = false; }
                }
                if preset.auto_copy {
                    if ui.checkbox(&mut preset.hide_overlay, text.hide_overlay_label).clicked() {
                        changed = true;
                    }
                }
            });
        });

        if !preset.hide_overlay {
            ui.group(|ui| {
                ui.label(egui::RichText::new(text.retranslate_section).strong());
                
                ui.horizontal(|ui| {
                    if ui.checkbox(&mut preset.retranslate, text.retranslate_checkbox).clicked() {
                        changed = true;
                    }
                    
                    if preset.retranslate {
                        ui.label(text.retranslate_to_label);
                        let retrans_label = preset.retranslate_to.clone();
                        ui.menu_button(retrans_label, |ui| {
                            ui.style_mut().wrap = Some(false);
                            ui.set_min_width(150.0);
                            ui.add(egui::TextEdit::singleline(search_query).hint_text(text.search_placeholder));
                            let q = search_query.to_lowercase();
                            egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                                for lang in get_all_languages().iter() {
                                    if q.is_empty() || lang.to_lowercase().contains(&q) {
                                        if ui.button(lang).clicked() {
                                            preset.retranslate_to = lang.clone();
                                            changed = true;
                                            ui.close_menu();
                                        }
                                    }
                                }
                            });
                        });
                    }
                });

                if preset.retranslate {
                    ui.horizontal(|ui| {
                        ui.label(text.retranslate_model_label);
                        let text_model = get_model_by_id(&preset.retranslate_model);
                        let text_display_label = text_model.as_ref()
                            .map(|m| match config.ui_language.as_str() {
                                "vi" => &m.name_vi,
                                "ko" => &m.name_ko,
                                _ => &m.name_en,
                            })
                            .map(|s| s.as_str())
                            .unwrap_or(&preset.retranslate_model);
                        
                        egui::ComboBox::from_id_source("text_model_selector")
                            .selected_text(text_display_label)
                            .show_ui(ui, |ui| {
                                for model in get_all_models() {
                                    if model.enabled && model.model_type == ModelType::Text {
                                        let dropdown_label = format!("{} ({}) - {}", 
                                            match config.ui_language.as_str() {
                                                "vi" => &model.name_vi,
                                                "ko" => &model.name_ko,
                                                _ => &model.name_en,
                                            },
                                            model.full_name,
                                            model.quota_limit
                                        );
                                        if ui.selectable_value(&mut preset.retranslate_model, model.id.clone(), dropdown_label).clicked() {
                                            changed = true;
                                        }
                                    }
                                }
                            });
                        
                        if ui.checkbox(&mut preset.retranslate_auto_copy, text.auto_copy_label).clicked() {
                                changed = true;
                                if preset.retranslate_auto_copy { preset.auto_copy = false; }
                        }
                    });

                    if !preset.hide_overlay {
                        ui.horizontal(|ui| {
                            ui.label(text.streaming_label);
                            egui::ComboBox::from_id_source("retrans_stream_combo")
                                .selected_text(if preset.retranslate_streaming_enabled { text.streaming_option_stream } else { text.streaming_option_wait })
                                .show_ui(ui, |ui| {
                                    if ui.selectable_value(&mut preset.retranslate_streaming_enabled, false, text.streaming_option_wait).clicked() { changed = true; }
                                    if ui.selectable_value(&mut preset.retranslate_streaming_enabled, true, text.streaming_option_stream).clicked() { changed = true; }
                                });
                        });
                    }
                }
            });
        }

        ui.group(|ui| {
           ui.label(egui::RichText::new(text.hotkeys_section).strong());
           
           let mut hotkey_to_remove = None;
           for (h_idx, hotkey) in preset.hotkeys.iter().enumerate() {
               ui.horizontal(|ui| {
                   ui.label(&hotkey.name);
                   if ui.small_button("x").clicked() {
                       hotkey_to_remove = Some(h_idx);
                   }
               });
           }
           if let Some(h_idx) = hotkey_to_remove {
               preset.hotkeys.remove(h_idx);
               changed = true;
           }

           if *recording_hotkey_for_preset == Some(preset_idx) {
               ui.horizontal(|ui| {
                   ui.colored_label(egui::Color32::YELLOW, text.press_keys);
                   if ui.button(text.cancel_label).clicked() {
                       *recording_hotkey_for_preset = None;
                   }
               });
               if let Some(msg) = hotkey_conflict_msg {
                   ui.colored_label(egui::Color32::RED, msg);
               }
           } else {
               if ui.button(text.add_hotkey_button).clicked() {
                   *recording_hotkey_for_preset = Some(preset_idx);
               }
           }
       });
    }

    if changed {
        config.presets[preset_idx] = preset;
    }
    
    changed
}

// --- Footer ---
pub fn render_footer(ui: &mut egui::Ui, text: &LocaleText) {
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text.footer_admin_text).size(11.0).color(ui.visuals().weak_text_color()));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let version_text = format!("{} v{}", text.footer_version, env!("CARGO_PKG_VERSION"));
            ui.label(egui::RichText::new(version_text).size(11.0).color(ui.visuals().weak_text_color()));
        });
    });
}
