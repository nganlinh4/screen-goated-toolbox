use eframe::egui;
use crate::config::{Config, save_config, get_all_languages};
use std::sync::{Arc, Mutex};
use tray_icon::{TrayIcon, TrayIconEvent, MouseButton, menu::{Menu, MenuEvent}};
use auto_launch::AutoLaunch;
use std::sync::mpsc::{Receiver, channel};
use std::sync::atomic::{AtomicBool, Ordering};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::System::Threading::*;
use windows::Win32::Foundation::{CloseHandle, WAIT_OBJECT_0};
use windows::core::*;

use crate::gui::locale::LocaleText;
use crate::gui::key_mapping::egui_key_to_vk;

// Windows Modifier Constants
const MOD_ALT: u32 = 0x0001;
const MOD_CONTROL: u32 = 0x0002;
const MOD_SHIFT: u32 = 0x0004;
const MOD_WIN: u32 = 0x0008;

enum UserEvent {
    Tray(TrayIconEvent),
    Menu(MenuEvent),
}

// Global signal for window restoration
lazy_static::lazy_static! {
    static ref RESTORE_SIGNAL: Arc<AtomicBool> = Arc::new(AtomicBool::new(false));
}

pub struct SettingsApp {
    config: Config,
    app_state_ref: Arc<Mutex<crate::AppState>>,
    search_query: String,
    tray_icon: Option<TrayIcon>,
    _tray_menu: Menu,
    event_rx: Receiver<UserEvent>,
    is_quitting: bool,
    run_at_startup: bool,
    auto_launcher: Option<AutoLaunch>,
    show_api_key: bool,
    show_gemini_api_key: bool,
    recording_hotkey: bool,
}

impl SettingsApp {
    pub fn new(config: Config, app_state: Arc<Mutex<crate::AppState>>, tray_icon: TrayIcon, tray_menu: Menu, ctx: egui::Context) -> Self {
        let app_name = "ScreenGroundedTranslator";
        let app_path = std::env::current_exe().unwrap();
        let args: &[&str] = &[];
        
        let auto = AutoLaunch::new(app_name, app_path.to_str().unwrap(), args);
        let run_at_startup = auto.is_enabled().unwrap_or(false);
        let (tx, rx) = channel();

        // Tray thread
        let tx_tray = tx.clone();
        let ctx_tray = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(event) = TrayIconEvent::receiver().recv() {
                let _ = tx_tray.send(UserEvent::Tray(event));
                ctx_tray.request_repaint();
            }
        });

        // Restore signal listener
        let ctx_restore = ctx.clone();
        std::thread::spawn(move || {
            loop {
                unsafe {
                    match OpenEventW(EVENT_ALL_ACCESS, false, w!("ScreenGroundedTranslatorRestoreEvent")) {
                        Ok(event_handle) => {
                            let result = WaitForSingleObject(event_handle, INFINITE);
                            if result == WAIT_OBJECT_0 {
                                let class_name = w!("eframe");
                                let mut hwnd = FindWindowW(PCWSTR(class_name.as_ptr()), None);
                                if hwnd.0 == 0 {
                                    let title = w!("Screen Grounded Translator");
                                    hwnd = FindWindowW(None, PCWSTR(title.as_ptr()));
                                }
                                if hwnd.0 != 0 {
                                    ShowWindow(hwnd, SW_RESTORE);
                                    ShowWindow(hwnd, SW_SHOW);
                                    SetForegroundWindow(hwnd);
                                    SetFocus(hwnd);
                                }
                                RESTORE_SIGNAL.store(true, Ordering::SeqCst);
                                ctx_restore.request_repaint();
                                let _ = ResetEvent(event_handle);
                            }
                            let _ = CloseHandle(event_handle);
                        }
                        Err(_) => std::thread::sleep(std::time::Duration::from_millis(100)),
                    }
                }
            }
        });

        // Menu thread
        let tx_menu = tx.clone();
        let ctx_menu = ctx.clone();
        std::thread::spawn(move || {
            while let Ok(event) = MenuEvent::receiver().recv() {
                match event.id.0.as_str() {
                    "1001" => std::process::exit(0),
                    "1002" => {
                        unsafe {
                            let class_name = w!("eframe");
                            let mut hwnd = FindWindowW(PCWSTR(class_name.as_ptr()), None);
                            if hwnd.0 == 0 {
                                let title = w!("Screen Grounded Translator");
                                hwnd = FindWindowW(None, PCWSTR(title.as_ptr()));
                            }
                            if hwnd.0 != 0 {
                                ShowWindow(hwnd, SW_RESTORE);
                                ShowWindow(hwnd, SW_SHOW);
                                SetForegroundWindow(hwnd);
                                SetFocus(hwnd);
                            }
                        }
                        RESTORE_SIGNAL.store(true, Ordering::SeqCst);
                        let _ = tx_menu.send(UserEvent::Menu(event.clone()));
                        ctx_menu.request_repaint();
                    }
                    _ => { let _ = tx_menu.send(UserEvent::Menu(event)); ctx_menu.request_repaint(); }
                }
            }
        });

        Self {
            config,
            app_state_ref: app_state,
            search_query: String::new(),
            tray_icon: Some(tray_icon),
            _tray_menu: tray_menu,
            event_rx: rx,
            is_quitting: false,
            run_at_startup,
            auto_launcher: Some(auto),
            show_api_key: false,
            show_gemini_api_key: false,
            recording_hotkey: false,
        }
    }

    fn save_and_sync(&mut self) {
        let mut state = self.app_state_ref.lock().unwrap();
        if state.config.hotkeys != self.config.hotkeys {
            state.hotkeys_updated = true;
        }
        state.config = self.config.clone();
        drop(state);
        save_config(&self.config);
    }
    
    fn restore_window(&self, ctx: &egui::Context) {
         ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(false));
         ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
         ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
         ctx.request_repaint();
    }
}

impl eframe::App for SettingsApp {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        if RESTORE_SIGNAL.swap(false, Ordering::SeqCst) {
            self.restore_window(ctx);
        }

        // --- Handle Hotkey Recording (Support Combinations) ---
        if self.recording_hotkey {
            let mut key_to_record: Option<(u32, String)> = None;
            let mut modifiers_bitmap = 0;
            
            // Check modifiers and keys using egui state
            ctx.input(|i| {
                if i.modifiers.ctrl { modifiers_bitmap |= MOD_CONTROL; }
                if i.modifiers.alt { modifiers_bitmap |= MOD_ALT; }
                if i.modifiers.shift { modifiers_bitmap |= MOD_SHIFT; }
                // mac command key usually maps to Win on Windows/Linux for egui
                if i.modifiers.command { modifiers_bitmap |= MOD_WIN; } 

                // Check for pressed keys
                for event in &i.events {
                    if let egui::Event::Key { key, pressed: true, .. } = event {
                        if let Some(vk) = egui_key_to_vk(key) {
                            // Filter out keys that are just modifier triggers themselves
                            // (16=Shift, 17=Ctrl, 18=Alt, 91=Win, 92=RWin)
                            if !matches!(vk, 16 | 17 | 18 | 91 | 92) {
                                let key_name = format!("{:?}", key).trim_start_matches("Key").to_string();
                                key_to_record = Some((vk, key_name));
                            }
                        }
                    }
                }
            });

            // If a non-modifier key is pressed, record the combo
            if let Some((vk, key_name)) = key_to_record {
                // Build name string
                let mut name_parts = Vec::new();
                if (modifiers_bitmap & MOD_CONTROL) != 0 { name_parts.push("Ctrl".to_string()); }
                if (modifiers_bitmap & MOD_ALT) != 0 { name_parts.push("Alt".to_string()); }
                if (modifiers_bitmap & MOD_SHIFT) != 0 { name_parts.push("Shift".to_string()); }
                if (modifiers_bitmap & MOD_WIN) != 0 { name_parts.push("Win".to_string()); }
                name_parts.push(key_name);
                
                let new_hotkey = crate::config::Hotkey {
                    code: vk,
                    modifiers: modifiers_bitmap,
                    name: name_parts.join(" + "),
                };

                // Avoid duplicates
                if !self.config.hotkeys.iter().any(|h| h.code == vk && h.modifiers == modifiers_bitmap) {
                    self.config.hotkeys.push(new_hotkey);
                    self.recording_hotkey = false;
                    self.save_and_sync();
                }
            }
        }

        // --- Handle Pending Events ---
        while let Ok(event) = self.event_rx.try_recv() {
            match event {
                UserEvent::Tray(tray_event) => {
                    if let TrayIconEvent::DoubleClick { button: MouseButton::Left, .. } = tray_event {
                        self.restore_window(ctx);
                    }
                }
                UserEvent::Menu(menu_event) => {
                    if menu_event.id.0 == "1002" {
                        self.restore_window(ctx);
                    }
                }
            }
        }

        if ctx.input(|i| i.viewport().close_requested()) {
            if !self.is_quitting {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(false));
            }
        }

        if self.config.dark_mode {
            ctx.set_visuals(egui::Visuals::dark());
        } else {
            ctx.set_visuals(egui::Visuals::light());
        }

        let text = LocaleText::get(&self.config.ui_language);

        egui::CentralPanel::default().show(ctx, |ui| {
            // --- HEADER ---
            ui.horizontal(|ui| {
                ui.heading("Made by ");
                ui.add(egui::Hyperlink::from_label_and_url(
                    egui::RichText::new("nganlinh4").heading(),
                    "https://github.com/nganlinh4/screen-grounded-translator"
                ));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    let theme_icon = if self.config.dark_mode { "🌙" } else { "☀" };
                    if ui.button(theme_icon).on_hover_text("Toggle Theme").clicked() {
                        self.config.dark_mode = !self.config.dark_mode;
                        self.save_and_sync();
                    }
                    ui.add_space(5.0);
                    let original_lang = self.config.ui_language.clone();
                    let lang_display = match self.config.ui_language.as_str() {
                        "vi" => "VI",
                        "ko" => "KO",
                        _ => "EN",
                    };
                    egui::ComboBox::from_id_source("header_lang_switch")
                        .width(60.0)
                        .selected_text(lang_display)
                        .show_ui(ui, |ui| {
                            ui.selectable_value(&mut self.config.ui_language, "en".to_string(), "English");
                            ui.selectable_value(&mut self.config.ui_language, "vi".to_string(), "Vietnamese");
                            ui.selectable_value(&mut self.config.ui_language, "ko".to_string(), "Korean");
                        });
                    if original_lang != self.config.ui_language {
                        self.save_and_sync();
                    }
                });
            });

            ui.add_space(15.0);

            // --- TWO COLUMN LAYOUT ---
            ui.columns(2, |cols| {
                // LEFT COLUMN: API Key and Language
                cols[0].group(|ui| {
                    ui.heading(text.api_section);
                    ui.label(text.api_key_label);
                    ui.horizontal(|ui| {
                        let available = ui.available_width() - 32.0;
                        if ui.add(egui::TextEdit::singleline(&mut self.config.api_key).password(!self.show_api_key).desired_width(available)).changed() {
                            self.save_and_sync();
                        }
                        let eye_icon = if self.show_api_key { "👁" } else { "🔒" };
                        if ui.button(eye_icon).clicked() { self.show_api_key = !self.show_api_key; }
                    });
                    if ui.link(text.get_key_link).clicked() { let _ = open::that("https://console.groq.com/keys"); }
                    
                    ui.add_space(8.0);
                    ui.label(text.gemini_api_key_label);
                    ui.horizontal(|ui| {
                        let available = ui.available_width() - 32.0;
                        if ui.add(egui::TextEdit::singleline(&mut self.config.gemini_api_key).password(!self.show_gemini_api_key).desired_width(available)).changed() {
                            self.save_and_sync();
                        }
                        let eye_icon = if self.show_gemini_api_key { "👁" } else { "🔒" };
                        if ui.button(eye_icon).clicked() { self.show_gemini_api_key = !self.show_gemini_api_key; }
                    });
                    if ui.link(text.gemini_get_key_link).clicked() { let _ = open::that("https://aistudio.google.com/app/apikey"); }
                });

                cols[0].add_space(10.0);

                cols[0].group(|ui| {
                    ui.heading(text.lang_section);
                    ui.add(egui::TextEdit::singleline(&mut self.search_query).hint_text(text.search_placeholder));
                    ui.add_space(5.0);
                    egui::ScrollArea::vertical().max_height(120.0).show(ui, |ui| {
                        let q = self.search_query.to_lowercase();
                        let all_languages = get_all_languages();
                        let filtered = all_languages.iter().filter(|l| l.to_lowercase().contains(&q));
                        for lang in filtered {
                            if ui.radio_value(&mut self.config.target_language, lang.clone(), lang).clicked() {
                                self.save_and_sync();
                            }
                        }
                    });
                    ui.label(format!("{} {}", text.current_language_label, self.config.target_language));
                });

                // RIGHT COLUMN: Model and Streaming
                cols[1].group(|ui| {
                    ui.heading(text.model_section);
                    ui.columns(2, |model_cols| {
                        // LEFT COLUMN: Model Selection
                        model_cols[0].label(text.model_label);
                        let original_model = self.config.preferred_model.clone();
                        let is_vietnamese = self.config.ui_language == "vi";
                        
                        // Get current model label for display
                        let current_label = crate::model_config::get_model_by_id(&self.config.preferred_model)
                            .map(|m| m.get_label_short(is_vietnamese))
                            .unwrap_or_else(|| "Nhanh".to_string());
                        
                        egui::ComboBox::from_id_source("model_selector")
                            .selected_text(current_label)
                            .show_ui(&mut model_cols[0], |ui| {
                                for model in crate::model_config::get_all_models() {
                                    if model.enabled {
                                        ui.selectable_value(
                                            &mut self.config.preferred_model,
                                            model.id.clone(),
                                            model.get_label(is_vietnamese),
                                        );
                                    } else {
                                        // Grayed out disabled model
                                        ui.add_enabled(false, egui::SelectableLabel::new(false, model.get_label(is_vietnamese)));
                                    }
                                }
                            });
                        
                        if original_model != self.config.preferred_model {
                            self.save_and_sync();
                            // Update the model selector in app state
                            {
                                let mut state = self.app_state_ref.lock().unwrap();
                                state.model_selector.set_preferred_model(self.config.preferred_model.clone());
                            }
                        }

                        // RIGHT COLUMN: Streaming Selection
                        model_cols[1].label(text.streaming_label);
                        egui::ComboBox::from_id_source("streaming_selector")
                            .selected_text(if self.config.streaming_enabled { text.streaming_option_stream } else { text.streaming_option_wait })
                            .show_ui(&mut model_cols[1], |ui| {
                                if ui.selectable_value(&mut self.config.streaming_enabled, true, text.streaming_option_stream).clicked() {
                                    self.save_and_sync();
                                }
                                if ui.selectable_value(&mut self.config.streaming_enabled, false, text.streaming_option_wait).clicked() {
                                    self.save_and_sync();
                                }
                            });
                    });
                });

                cols[1].add_space(10.0);

                cols[1].group(|ui| {
                    ui.heading(text.hotkey_section);
                    if let Some(launcher) = &self.auto_launcher {
                        if ui.checkbox(&mut self.run_at_startup, text.startup_label).clicked() {
                             if self.run_at_startup { let _ = launcher.enable(); } else { let _ = launcher.disable(); }
                        }
                    }
                    ui.add_space(8.0);
                    if ui.checkbox(&mut self.config.auto_copy, text.auto_copy_label).clicked() { self.save_and_sync(); }
                    ui.add_space(8.0);
                    ui.label(egui::RichText::new(text.hotkey_label).strong());
                    
                    // List Hotkeys in a grid layout
                    let hotkey_list: Vec<_> = self.config.hotkeys.iter().cloned().collect();
                    if !hotkey_list.is_empty() {
                        ui.label(text.active_hotkeys_label);
                        let mut grid_indices_to_remove = Vec::new();
                        egui::Grid::new("hotkey_grid")
                            .num_columns(2)
                            .spacing([8.0, 5.0])
                            .show(ui, |ui| {
                                for (idx, hotkey) in hotkey_list.iter().enumerate() {
                                    ui.strong(&hotkey.name);
                                    if ui.small_button("✖").on_hover_text("Remove").clicked() {
                                        grid_indices_to_remove.push(idx);
                                    }
                                    ui.end_row();
                                }
                            });
                        
                        // Remove hotkeys in reverse order to maintain correct indices
                        for idx in grid_indices_to_remove.iter().rev() {
                            self.config.hotkeys.remove(*idx);
                        }
                        if !grid_indices_to_remove.is_empty() {
                            self.save_and_sync();
                        }
                    }
                    
                    // Recorder
                    if self.recording_hotkey {
                        ui.horizontal(|ui| {
                            ui.colored_label(egui::Color32::YELLOW, text.press_keys);
                            if ui.button(text.cancel_label).clicked() {
                                self.recording_hotkey = false;
                            }
                        });
                    } else {
                        if ui.button(text.add_hotkey_button).clicked() {
                            self.recording_hotkey = true;
                        }
                    }
                      
                    let warn_color = if self.config.dark_mode { egui::Color32::YELLOW } else { egui::Color32::from_rgb(200, 0, 0) };
                    ui.small(egui::RichText::new(text.fullscreen_note).color(warn_color));
                });
            });

            ui.add_space(20.0);
            ui.with_layout(egui::Layout::bottom_up(egui::Align::Center), |ui| {
                ui.label(egui::RichText::new(text.footer_note).italics().weak());
            });
        });
        
        ctx.request_repaint_after(std::time::Duration::from_millis(100));
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.tray_icon = None;
    }
}

// --- Font Configuration (Unchanged) ---
pub fn configure_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let viet_font_name = "segoe_ui";
    let viet_font_path = "C:\\Windows\\Fonts\\segoeui.ttf";
    let viet_fallback_path = "C:\\Windows\\Fonts\\arial.ttf";
    let viet_data = std::fs::read(viet_font_path).or_else(|_| std::fs::read(viet_fallback_path));

    let korean_font_name = "malgun_gothic";
    let korean_font_path = "C:\\Windows\\Fonts\\malgun.ttf";
    let korean_data = std::fs::read(korean_font_path);

    if let Ok(data) = viet_data {
        fonts.font_data.insert(viet_font_name.to_owned(), egui::FontData::from_owned(data));
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) { vec.insert(0, viet_font_name.to_owned()); }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) { vec.insert(0, viet_font_name.to_owned()); }
    }
    if let Ok(data) = korean_data {
        fonts.font_data.insert(korean_font_name.to_owned(), egui::FontData::from_owned(data));
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Proportional) { 
            let idx = if vec.contains(&viet_font_name.to_string()) { 1 } else { 0 };
            vec.insert(idx, korean_font_name.to_owned()); 
        }
        if let Some(vec) = fonts.families.get_mut(&egui::FontFamily::Monospace) { 
             let idx = if vec.contains(&viet_font_name.to_string()) { 1 } else { 0 };
             vec.insert(idx, korean_font_name.to_owned()); 
        }
    }
    ctx.set_fonts(fonts);
}
