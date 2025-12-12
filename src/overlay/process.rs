use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::core::*;
use std::sync::{Arc, Mutex, Once};
use std::collections::HashMap;
use image::{ImageBuffer, Rgba};

use crate::api::{translate_image_streaming, translate_text_streaming};
use crate::config::{Config, Preset};
use super::utils::{copy_to_clipboard, get_error_message};
use super::result::{create_result_window, update_window_text, WindowType, link_windows, RefineContext, RetranslationConfig, WINDOW_STATES};

// --- PROCESSING WINDOW STATIC STATE ---
static REGISTER_PROC_CLASS: Once = Once::new();
const MAX_GLOW_BUFFER_DIM: i32 = 1280;

struct ProcessingState {
    animation_offset: f32,
    is_fading_out: bool,
    alpha: u8,
    cache_hbm: HBITMAP,
    cache_bits: *mut core::ffi::c_void,
    cache_w: i32,
    cache_h: i32,
    scaled_w: i32,
    scaled_h: i32,
    timer_killed: bool,
    graphics_mode: String,
}

unsafe impl Send for ProcessingState {}

impl ProcessingState {
    fn new(graphics_mode: String) -> Self {
        Self {
            animation_offset: 0.0,
            is_fading_out: false,
            alpha: 255,
            cache_hbm: HBITMAP(0),
            cache_bits: std::ptr::null_mut(),
            cache_w: 0,
            cache_h: 0,
            scaled_w: 0,
            scaled_h: 0,
            timer_killed: false,
            graphics_mode,
        }
    }
    
    fn cleanup(&mut self) {
        if self.cache_hbm.0 != 0 {
            unsafe { DeleteObject(self.cache_hbm); }
            self.cache_hbm = HBITMAP(0);
            self.cache_bits = std::ptr::null_mut();
        }
    }
}

lazy_static::lazy_static! {
    static ref PROC_STATES: Mutex<HashMap<isize, ProcessingState>> = Mutex::new(HashMap::new());
}

// --- TEXT PROCESSING (Select/Type Mode) ---
pub fn start_text_processing(
    text_content: String,
    screen_rect: RECT,
    config: Config,
    preset: Preset
) {
    println!("[DEBUG] start_text_processing called");
    let hide_overlay = preset.hide_overlay;
    let model_id = preset.model.clone();
    let model_config = crate::model_config::get_model_by_id(&model_id);
    let provider = model_config.map(|m| m.provider).unwrap_or("groq".to_string());
    
    // Config for retranslation
    let retrans_config = if preset.retranslate {
        Some(RetranslationConfig {
            enabled: true,
            target_lang: preset.retranslate_to.clone(),
            model_id: preset.retranslate_model.clone(),
            provider: "groq".to_string(), // Default, refined logic inside result window
            streaming: preset.retranslate_streaming_enabled,
            auto_copy: preset.retranslate_auto_copy,
        })
    } else {
        None
    };

    // Prepare prompt
    let mut final_prompt = preset.prompt.clone();
    for (key, value) in &preset.language_vars {
        final_prompt = final_prompt.replace(&format!("{{{}}}", key), value);
    }
    final_prompt = final_prompt.replace("{language}", &preset.selected_language);

    // 1. TYPE MODE: Open Result Window Directly
    if preset.text_input_mode == "type" {
        std::thread::spawn(move || {
            let hwnd = create_result_window(
                screen_rect,
                WindowType::Primary,
                RefineContext::None,
                model_id,
                provider,
                preset.streaming_enabled,
                true, // start_editing
                final_prompt, 
                retrans_config
            );
            unsafe { ShowWindow(hwnd, SW_SHOW); }
            
            // Message Loop
            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    if !IsWindow(hwnd).as_bool() { break; }
                }
            }
        });
        return;
    }

    // 2. SELECT MODE: Standard Processing Pipeline
    let graphics_mode = config.graphics_mode.clone();
    
    println!("[DEBUG] Creating processing overlay...");
    let processing_hwnd = unsafe { create_processing_window(screen_rect, graphics_mode) };
    // Force initial update to prevent black box
    unsafe { SendMessageW(processing_hwnd, WM_TIMER, WPARAM(1), LPARAM(0)); }

    // Prepare API Data
    let model_name = crate::model_config::get_model_by_id(&model_id)
        .map(|m| m.full_name).unwrap_or(model_id.clone());
    
    let groq_api_key = config.api_key.clone();
    let gemini_api_key = config.gemini_api_key.clone();
    let streaming_enabled = preset.streaming_enabled;
    let auto_copy = preset.auto_copy;
    let auto_paste = preset.auto_paste;
    let do_retranslate = preset.retranslate;
    let retranslate_to = preset.retranslate_to.clone();
    let retranslate_model_id = preset.retranslate_model.clone();
    let retranslate_streaming_enabled = preset.retranslate_streaming_enabled;
    let retranslate_auto_copy = preset.retranslate_auto_copy;

    let target_window_for_paste = if let Ok(app) = crate::APP.lock() {
        app.last_active_window
    } else { None };

    // API Thread
    std::thread::spawn(move || {
        let accumulated_text = Arc::new(Mutex::new(String::new()));
        let acc_text_clone = accumulated_text.clone();
        let mut first_chunk_received = false;
        
        let (tx_hwnd, rx_hwnd) = std::sync::mpsc::channel();
        let streaming_hwnd = Arc::new(Mutex::new(None));
        let streaming_hwnd_cb = streaming_hwnd.clone();

        let api_res = translate_text_streaming(
            &groq_api_key, &gemini_api_key, text_content, preset.selected_language.clone(), 
            model_name, provider.clone(), streaming_enabled, false,
            |chunk| {
                let mut t = acc_text_clone.lock().unwrap();
                t.push_str(chunk);
                
                if !first_chunk_received {
                    first_chunk_received = true;
                    // Close processing overlay
                    if processing_hwnd.0 != 0 { unsafe { PostMessageW(processing_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); } }
                    
                    // Create Result Window
                    let rect_copy = screen_rect;
                    let mid_copy = model_id.clone();
                    let prov_copy = provider.clone();
                    let tx_clone = tx_hwnd.clone();
                    let st_hwnd = streaming_hwnd.clone();
                    let pr_copy = final_prompt.clone();
                    let re_cfg = retrans_config.clone();
                    
                    std::thread::spawn(move || {
                        let hwnd = create_result_window(
                            rect_copy, WindowType::Primary, RefineContext::None, 
                            mid_copy, prov_copy, streaming_enabled, false, 
                            pr_copy, re_cfg
                        );
                        if !hide_overlay { unsafe { ShowWindow(hwnd, SW_SHOW); } }
                        
                        if let Ok(mut g) = st_hwnd.lock() { *g = Some(hwnd); }
                        let _ = tx_clone.send(hwnd);
                        
                        unsafe {
                            let mut msg = MSG::default();
                            while GetMessageW(&mut msg, None, 0, 0).into() {
                                TranslateMessage(&msg); DispatchMessageW(&msg);
                                if !IsWindow(hwnd).as_bool() { break; }
                            }
                        }
                    });
                }
                
                if !hide_overlay {
                    if let Ok(g) = streaming_hwnd_cb.lock() {
                        if let Some(hwnd) = *g { update_window_text(hwnd, &t); }
                    }
                }
            }
        );

        // Fallback if no streaming chunks (e.g. error or instant response)
        let result_hwnd = if first_chunk_received { 
            rx_hwnd.recv().ok() 
        } else {
            if processing_hwnd.0 != 0 { unsafe { PostMessageW(processing_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); } }
            let rect_copy = screen_rect;
            let mid_copy = model_id.clone();
            let prov_copy = provider.clone();
            let tx_clone = tx_hwnd.clone();
            
            std::thread::spawn(move || {
                let hwnd = create_result_window(
                    rect_copy, WindowType::Primary, RefineContext::None, 
                    mid_copy, prov_copy, streaming_enabled, false, 
                    final_prompt, retrans_config
                );
                if !hide_overlay { unsafe { ShowWindow(hwnd, SW_SHOW); } }
                let _ = tx_clone.send(hwnd);
                
                unsafe {
                    let mut msg = MSG::default();
                    while GetMessageW(&mut msg, None, 0, 0).into() {
                        TranslateMessage(&msg); DispatchMessageW(&msg);
                        if !IsWindow(hwnd).as_bool() { break; }
                    }
                }
            });
            rx_hwnd.recv().ok()
        };

        if let Some(r_hwnd) = result_hwnd {
            match api_res {
                Ok(full_text) => {
                    if !hide_overlay { update_window_text(r_hwnd, &full_text); }
                    
                    // Auto Copy/Paste
                    if auto_copy && !full_text.trim().is_empty() {
                        let txt_to_copy = full_text.clone();
                        let target_hwnd = target_window_for_paste;
                        
                        std::thread::spawn(move || {
                            std::thread::sleep(std::time::Duration::from_millis(200));
                            copy_to_clipboard(&txt_to_copy, HWND(0));
                            if auto_paste && hide_overlay { 
                                if let Some(hwnd) = target_hwnd { crate::overlay::utils::force_focus_and_paste(hwnd); }
                            }
                        });
                    }

                    // --- RETRANSLATION LOGIC (TEXT PRESET) ---
                    if do_retranslate && !full_text.trim().is_empty() {
                        let text_to_retrans = full_text.clone();
                        let g_key = groq_api_key.clone();
                        let gm_key = gemini_api_key.clone();
                        
                        std::thread::spawn(move || {
                            // Calculate Split Position
                            let gap = 20; 
                            let w = (screen_rect.right - screen_rect.left).abs();
                            let total_w = w * 2 + gap; 
                            let s_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
                            let start_x = (s_w - total_w) / 2;
                            
                            unsafe { SetWindowPos(r_hwnd, HWND_TOP, start_x, screen_rect.top, 0, 0, SWP_NOSIZE | SWP_NOACTIVATE); }
                            
                            let sec_rect = RECT { 
                                left: start_x + w + gap, 
                                top: screen_rect.top, 
                                right: start_x + w + gap + w, 
                                bottom: screen_rect.bottom 
                            };

                            let tm_config = crate::model_config::get_model_by_id(&retranslate_model_id);
                            let (tm_id, tm_name, tm_provider) = match tm_config {
                                Some(m) => (m.id, m.full_name, m.provider),
                                None => ("fast_text".to_string(), "openai/gpt-oss-20b".to_string(), "groq".to_string())
                            };

                            let sec_hwnd = create_result_window(
                                sec_rect, WindowType::SecondaryExplicit, RefineContext::None, 
                                tm_id, tm_provider.clone(), retranslate_streaming_enabled, false, "".to_string(), None
                            );
                            
                            // --- FIX: Enable Loading Animation Immediately ---
                            {
                                let mut states = WINDOW_STATES.lock().unwrap();
                                if let Some(s) = states.get_mut(&(sec_hwnd.0 as isize)) { 
                                    s.is_refining = true; 
                                }
                            }

                            link_windows(r_hwnd, sec_hwnd);
                            if !hide_overlay { 
                                unsafe { ShowWindow(sec_hwnd, SW_SHOW); } 
                                update_window_text(sec_hwnd, ""); 
                            }

                            std::thread::spawn(move || {
                                let acc = Arc::new(Mutex::new(String::new()));
                                let acc_c = acc.clone();
                                let mut first = true;
                                
                                let res = translate_text_streaming(
                                    &g_key, &gm_key, text_to_retrans, retranslate_to, tm_name, tm_provider, retranslate_streaming_enabled, false,
                                    |chunk| {
                                        // --- FIX: Disable Loading on First Chunk ---
                                        if first {
                                            let mut states = WINDOW_STATES.lock().unwrap();
                                            if let Some(s) = states.get_mut(&(sec_hwnd.0 as isize)) { s.is_refining = false; }
                                            first = false;
                                        }

                                        let mut t = acc_c.lock().unwrap();
                                        t.push_str(chunk);
                                        if !hide_overlay { update_window_text(sec_hwnd, &t); }
                                    }
                                );

                                if let Ok(fin) = res {
                                    if !hide_overlay { update_window_text(sec_hwnd, &fin); }
                                    if retranslate_auto_copy { 
                                        std::thread::spawn(move || { 
                                            std::thread::sleep(std::time::Duration::from_millis(100)); 
                                            copy_to_clipboard(&fin, HWND(0)); 
                                        }); 
                                    }
                                } else if let Err(e) = res {
                                    if !hide_overlay { update_window_text(sec_hwnd, &format!("Error: {}", e)); }
                                }
                                
                                // Ensure loading state is off if API finished (in case no chunks came)
                                {
                                    let mut states = WINDOW_STATES.lock().unwrap();
                                    if let Some(s) = states.get_mut(&(sec_hwnd.0 as isize)) { s.is_refining = false; }
                                }
                            });

                            unsafe { 
                                let mut m = MSG::default(); 
                                while GetMessageW(&mut m, None, 0, 0).into() { 
                                    TranslateMessage(&m); DispatchMessageW(&m); 
                                    if !IsWindow(sec_hwnd).as_bool() { break; } 
                                } 
                            }
                        });
                    }
                },
                Err(e) => { update_window_text(r_hwnd, &format!("Error: {}", e)); }
            }
        }
    });

    // Message loop for processing window
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).into() {
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
            if !IsWindow(processing_hwnd).as_bool() { break; }
        }
    }
}


// --- AUDIO PROCESSING ---
pub fn show_audio_result(
    preset: Preset,
    transcription_text: String,
    rect: RECT,
    retranslate_rect: Option<RECT>,
) {
    let model_id = preset.model.clone();
    let model_config = crate::model_config::get_model_by_id(&model_id);
    let provider = model_config.map(|m| m.provider).unwrap_or("groq".to_string());

    let retrans_config = if preset.retranslate {
        Some(RetranslationConfig {
            enabled: true,
            target_lang: preset.retranslate_to.clone(),
            model_id: preset.retranslate_model.clone(),
            provider: "groq".to_string(),
            streaming: preset.retranslate_streaming_enabled,
            auto_copy: preset.retranslate_auto_copy,
        })
    } else {
        None
    };

    let mut final_prompt = preset.prompt.clone();
    for (key, value) in &preset.language_vars {
        final_prompt = final_prompt.replace(&format!("{{{}}}", key), value);
    }
    final_prompt = final_prompt.replace("{language}", &preset.selected_language);

    std::thread::spawn(move || {
        let hwnd = create_result_window(rect, WindowType::Primary, RefineContext::None, model_id.clone(), provider.clone(), preset.streaming_enabled, false, final_prompt.clone(), retrans_config.clone());
        unsafe { ShowWindow(hwnd, SW_SHOW); }
        update_window_text(hwnd, &transcription_text);

        // --- AUDIO RETRANSLATION ---
        if preset.retranslate && !transcription_text.trim().is_empty() {
            if let Some(sec_rect) = retranslate_rect {
                let groq_api_key = { let app = crate::APP.lock().unwrap(); app.config.api_key.clone() };
                let gemini_api_key = { let app = crate::APP.lock().unwrap(); app.config.gemini_api_key.clone() };
                let text_to_retrans = transcription_text.clone();
                let tm_config = crate::model_config::get_model_by_id(&preset.retranslate_model);
                let (tm_id, tm_name, tm_provider) = match tm_config {
                    Some(m) => (m.id, m.full_name, m.provider),
                    None => ("fast_text".to_string(), "openai/gpt-oss-20b".to_string(), "groq".to_string()),
                };
                let sec_hwnd = create_result_window(sec_rect, WindowType::SecondaryExplicit, RefineContext::None, tm_id, tm_provider.clone(), preset.retranslate_streaming_enabled, false, "".to_string(), None);
                
                // SET LOADING EFFECT
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(s) = states.get_mut(&(sec_hwnd.0 as isize)) { s.is_refining = true; }
                }

                link_windows(hwnd, sec_hwnd);
                unsafe { ShowWindow(sec_hwnd, SW_SHOW); }
                update_window_text(sec_hwnd, "");
                
                std::thread::spawn(move || {
                    let acc = Arc::new(Mutex::new(String::new()));
                    let acc_c = acc.clone();
                    let mut first = true;
                    
                    let res = translate_text_streaming(&groq_api_key, &gemini_api_key, text_to_retrans, preset.retranslate_to.clone(), tm_name, tm_provider, preset.retranslate_streaming_enabled, false, |chunk| {
                        // UNSET LOADING EFFECT
                        if first { 
                            let mut s = WINDOW_STATES.lock().unwrap();
                            if let Some(st) = s.get_mut(&(sec_hwnd.0 as isize)) { st.is_refining = false; }
                            first = false; 
                        }
                        let mut t = acc_c.lock().unwrap(); t.push_str(chunk);
                        update_window_text(sec_hwnd, &t);
                    });
                    
                    if let Ok(fin) = res { 
                        update_window_text(sec_hwnd, &fin); 
                        if preset.retranslate_auto_copy { std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(100)); copy_to_clipboard(&fin, HWND(0)); }); } 
                    }
                    
                    // Final cleanup
                    {
                        let mut s = WINDOW_STATES.lock().unwrap();
                        if let Some(st) = s.get_mut(&(sec_hwnd.0 as isize)) { st.is_refining = false; }
                    }
                });
                
                unsafe { let mut m = MSG::default(); while GetMessageW(&mut m, None, 0, 0).into() { TranslateMessage(&m); DispatchMessageW(&m); if !IsWindow(sec_hwnd).as_bool() { break; } } }
            }
        }
        unsafe { let mut m = MSG::default(); while GetMessageW(&mut m, None, 0, 0).into() { TranslateMessage(&m); DispatchMessageW(&m); if !IsWindow(hwnd).as_bool() { break; } } }
    });
}

// --- IMAGE PROCESSING ---
pub fn start_processing_pipeline(
    cropped_img: ImageBuffer<Rgba<u8>, Vec<u8>>, 
    screen_rect: RECT, 
    config: Config, 
    preset: Preset
) {
    let hide_overlay = preset.hide_overlay;
    let model_id = preset.model.clone();
    let model_config = crate::model_config::get_model_by_id(&model_id).expect("Model config missing");
    let provider = model_config.provider.clone();
    let retrans_config = if preset.retranslate {
        Some(RetranslationConfig {
            enabled: true,
            target_lang: preset.retranslate_to.clone(),
            model_id: preset.retranslate_model.clone(),
            provider: "groq".to_string(),
            streaming: preset.retranslate_streaming_enabled,
            auto_copy: preset.retranslate_auto_copy,
        })
    } else { None };

    let mut final_prompt = preset.prompt.clone();
    for (key, value) in &preset.language_vars {
        final_prompt = final_prompt.replace(&format!("{{{}}}", key), value);
    }
    final_prompt = final_prompt.replace("{language}", &preset.selected_language);

    if preset.prompt_mode == "dynamic" {
        let mut png_data = Vec::new();
        let _ = cropped_img.write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png);
        std::thread::spawn(move || {
            let hwnd = create_result_window(screen_rect, WindowType::Primary, RefineContext::Image(png_data), model_id, provider, preset.streaming_enabled, true, final_prompt, retrans_config);
            unsafe { ShowWindow(hwnd, SW_SHOW); }
            unsafe { let mut m = MSG::default(); while GetMessageW(&mut m, None, 0, 0).into() { TranslateMessage(&m); DispatchMessageW(&m); if !IsWindow(hwnd).as_bool() { break; } } }
        });
        return;
    }

    let graphics_mode = config.graphics_mode.clone();
    let processing_hwnd = unsafe { create_processing_window(screen_rect, graphics_mode) };
    unsafe { SendMessageW(processing_hwnd, WM_TIMER, WPARAM(1), LPARAM(0)); }

    let model_name = model_config.full_name.clone();
    let groq_api_key = config.api_key.clone();
    let gemini_api_key = config.gemini_api_key.clone();
    let ui_language = config.ui_language.clone();
    let streaming_enabled = preset.streaming_enabled;
    let use_json_format = preset.id == "preset_translate";
    let auto_copy = preset.auto_copy;
    let auto_paste = preset.auto_paste; 
    let do_retranslate = preset.retranslate;
    let retranslate_to = preset.retranslate_to.clone();
    let retranslate_model_id = preset.retranslate_model.clone();
    let retranslate_streaming_enabled = preset.retranslate_streaming_enabled;
    let retranslate_auto_copy = preset.retranslate_auto_copy;
    let cropped_for_history = cropped_img.clone();
    let target_window_for_paste = if let Ok(app) = crate::APP.lock() { app.last_active_window } else { None };

    std::thread::spawn(move || {
        let mut png_data = Vec::new();
        let _ = cropped_img.write_to(&mut std::io::Cursor::new(&mut png_data), image::ImageFormat::Png);
        let refine_context = RefineContext::Image(png_data);
        let accumulated_vision = Arc::new(Mutex::new(String::new()));
        let acc_vis_clone = accumulated_vision.clone();
        let mut first_chunk_received = false;
        let streaming_hwnd = Arc::new(Mutex::new(None));
        let streaming_hwnd_cb = streaming_hwnd.clone();
        let (tx_hwnd, rx_hwnd) = std::sync::mpsc::channel();

        let api_res = translate_image_streaming(&groq_api_key, &gemini_api_key, final_prompt.clone(), model_name, provider.clone(), cropped_img, streaming_enabled, use_json_format, |chunk| {
            let mut text = acc_vis_clone.lock().unwrap(); text.push_str(chunk);
            if !first_chunk_received {
                first_chunk_received = true;
                if processing_hwnd.0 != 0 { unsafe { PostMessageW(processing_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); } }
                let rect_copy = screen_rect; let ctx_copy = refine_context.clone();
                let m_id = model_id.clone(); let prov = provider.clone(); let st_tx = tx_hwnd.clone();
                let st_h_inner = streaming_hwnd.clone(); let p_pr = final_prompt.clone(); let r_cf = retrans_config.clone();
                std::thread::spawn(move || {
                    let hwnd = create_result_window(rect_copy, WindowType::Primary, ctx_copy, m_id, prov, streaming_enabled, false, p_pr, r_cf);
                    if !hide_overlay { unsafe { ShowWindow(hwnd, SW_SHOW); } }
                    if let Ok(mut g) = st_h_inner.lock() { *g = Some(hwnd); }
                    let _ = st_tx.send(hwnd);
                    unsafe { let mut m = MSG::default(); while GetMessageW(&mut m, None, 0, 0).into() { TranslateMessage(&m); DispatchMessageW(&m); if !IsWindow(hwnd).as_bool() { break; } } }
                });
            }
            if !hide_overlay { if let Ok(guard) = streaming_hwnd_cb.lock() { if let Some(hwnd) = *guard { update_window_text(hwnd, &text); } } }
        });

        let result_hwnd = if first_chunk_received { rx_hwnd.recv().ok() } else {
             if processing_hwnd.0 != 0 { unsafe { PostMessageW(processing_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); } }
            let rect_c = screen_rect; let m_id = model_id.clone(); let prov = provider.clone();
            let st_tx = tx_hwnd.clone();
            std::thread::spawn(move || {
                let hwnd = create_result_window(rect_c, WindowType::Primary, refine_context, m_id, prov, streaming_enabled, false, final_prompt, retrans_config);
                if !hide_overlay { unsafe { ShowWindow(hwnd, SW_SHOW); } }
                let _ = st_tx.send(hwnd);
                unsafe { let mut m = MSG::default(); while GetMessageW(&mut m, None, 0, 0).into() { TranslateMessage(&m); DispatchMessageW(&m); if !IsWindow(hwnd).as_bool() { break; } } }
            });
            rx_hwnd.recv().ok()
        };

        if let Some(r_hwnd) = result_hwnd {
            match api_res {
                Ok(full_text) => {
                    if !hide_overlay { update_window_text(r_hwnd, &full_text); }
                    if let Ok(app_lock) = crate::APP.lock() { app_lock.history.save_image(cropped_for_history, full_text.clone()); }
                    if auto_copy && !full_text.trim().is_empty() {
                         let txt_to_copy = full_text.clone();
                         let target_h = target_window_for_paste;
                         std::thread::spawn(move || {
                             std::thread::sleep(std::time::Duration::from_millis(200)); copy_to_clipboard(&txt_to_copy, HWND(0));
                             if auto_paste && hide_overlay { if let Some(hwnd) = target_h { crate::overlay::utils::force_focus_and_paste(hwnd); } }
                         });
                    }

                    if do_retranslate && !full_text.trim().is_empty() {
                         let text_to_retrans = full_text.clone();
                         let g_key = groq_api_key.clone(); let gm_key = gemini_api_key.clone();
                         std::thread::spawn(move || {
                             let tm_config = crate::model_config::get_model_by_id(&retranslate_model_id);
                             let (tm_id, tm_name, tm_provider) = match tm_config {
                                 Some(m) => (m.id, m.full_name, m.provider),
                                 None => ("fast_text".to_string(), "openai/gpt-oss-20b".to_string(), "groq".to_string())
                             };
                             let sec_hwnd = create_result_window(screen_rect, WindowType::Secondary, RefineContext::None, tm_id, tm_provider.clone(), retranslate_streaming_enabled, false, "".to_string(), None);
                             
                             // SET LOADING EFFECT
                             {
                                 let mut states = WINDOW_STATES.lock().unwrap();
                                 if let Some(s) = states.get_mut(&(sec_hwnd.0 as isize)) { s.is_refining = true; }
                             }

                             link_windows(r_hwnd, sec_hwnd);
                             if !hide_overlay { unsafe { ShowWindow(sec_hwnd, SW_SHOW); } update_window_text(sec_hwnd, ""); }
                             std::thread::spawn(move || {
                                 let acc_r = Arc::new(Mutex::new(String::new())); let acc_r_c = acc_r.clone();
                                 let mut first = true;
                                 let res = translate_text_streaming(&g_key, &gm_key, text_to_retrans, retranslate_to, tm_name, tm_provider, retranslate_streaming_enabled, false, |chunk| {
                                     if first { 
                                         let mut s = WINDOW_STATES.lock().unwrap();
                                         if let Some(st) = s.get_mut(&(sec_hwnd.0 as isize)) { st.is_refining = false; }
                                         first = false; 
                                     }
                                     let mut t = acc_r_c.lock().unwrap(); t.push_str(chunk);
                                     if !hide_overlay { update_window_text(sec_hwnd, &t); }
                                 });
                                 if let Ok(fin) = res {
                                     if !hide_overlay { update_window_text(sec_hwnd, &fin); }
                                     if retranslate_auto_copy { std::thread::spawn(move || { std::thread::sleep(std::time::Duration::from_millis(100)); copy_to_clipboard(&fin, HWND(0)); }); }
                                 }
                                 // cleanup
                                 {
                                     let mut s = WINDOW_STATES.lock().unwrap();
                                     if let Some(st) = s.get_mut(&(sec_hwnd.0 as isize)) { st.is_refining = false; }
                                 }
                             });
                             unsafe { let mut m = MSG::default(); while GetMessageW(&mut m, None, 0, 0).into() { TranslateMessage(&m); DispatchMessageW(&m); if !IsWindow(sec_hwnd).as_bool() { break; } } }
                         });
                    }
                },
                Err(e) => { let err_msg = get_error_message(&e.to_string(), &ui_language); update_window_text(r_hwnd, &err_msg); }
            }
        }
    });

    unsafe { let mut m = MSG::default(); while GetMessageW(&mut m, None, 0, 0).into() { TranslateMessage(&m); DispatchMessageW(&m); if !IsWindow(processing_hwnd).as_bool() { break; } } }
}

// --- WINDOW PROC FOR OVERLAY ---
unsafe fn create_processing_window(rect: RECT, graphics_mode: String) -> HWND {
    println!("[DEBUG] Creating processing window...");
    let instance = GetModuleHandleW(None).unwrap();
    let class_name = w!("SGTProcessingOverlay");

    REGISTER_PROC_CLASS.call_once(|| {
        let mut wc = WNDCLASSW::default();
        wc.lpfnWndProc = Some(processing_wnd_proc);
        wc.hInstance = instance;
        wc.hCursor = LoadCursorW(None, IDC_WAIT).unwrap();
        wc.lpszClassName = class_name;
        wc.style = CS_HREDRAW | CS_VREDRAW;
        wc.hbrBackground = HBRUSH(0); 
        RegisterClassW(&wc);
    });

    let w = (rect.right - rect.left).abs();
    let h = (rect.bottom - rect.top).abs();
    // Reduce frame rate for huge resolutions
    let pixels = (w as i64) * (h as i64);
    let timer_interval = if pixels > 5_000_000 { 50 } else if pixels > 2_000_000 { 32 } else { 16 };

    let hwnd = CreateWindowExW(WS_EX_LAYERED | WS_EX_TOPMOST | WS_EX_TOOLWINDOW | WS_EX_TRANSPARENT, class_name, w!("Processing"), WS_POPUP, rect.left, rect.top, w, h, None, None, instance, None);
    let mut states = PROC_STATES.lock().unwrap();
    states.insert(hwnd.0 as isize, ProcessingState::new(graphics_mode));
    drop(states);
    SetTimer(hwnd, 1, timer_interval, None);
    ShowWindow(hwnd, SW_SHOW);
    println!("[DEBUG] Processing window created. HWND: {:?}", hwnd);
    hwnd
}

unsafe extern "system" fn processing_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CLOSE => {
            let mut states = PROC_STATES.lock().unwrap();
            let state = states.entry(hwnd.0 as isize).or_insert(ProcessingState::new("standard".to_string()));
            if !state.is_fading_out {
                state.is_fading_out = true;
                if !state.timer_killed {
                    KillTimer(hwnd, 1); state.timer_killed = true;
                    SetTimer(hwnd, 2, 25, None);
                }
            }
            LRESULT(0)
        }
        WM_TIMER => {
            let (should_destroy, anim_offset, alpha, is_fading) = {
                let mut states = PROC_STATES.lock().unwrap();
                let state = states.entry(hwnd.0 as isize).or_insert(ProcessingState::new("standard".to_string()));
                let mut destroy_flag = false;
                if state.is_fading_out {
                    if state.alpha > 20 { state.alpha -= 20; } else { state.alpha = 0; destroy_flag = true; }
                } else {
                    state.animation_offset += 5.0; if state.animation_offset > 360.0 { state.animation_offset -= 360.0; }
                }
                (destroy_flag, state.animation_offset, state.alpha, state.is_fading_out)
            };
            if should_destroy { KillTimer(hwnd, 1); KillTimer(hwnd, 2); DestroyWindow(hwnd); PostQuitMessage(0); return LRESULT(0); }
            let mut rect = RECT::default(); GetWindowRect(hwnd, &mut rect);
            let w = (rect.right - rect.left).abs(); let h = (rect.bottom - rect.top).abs();
            if w > 0 && h > 0 {
                let mut states = PROC_STATES.lock().unwrap();
                let state = states.get_mut(&(hwnd.0 as isize)).unwrap();
                // Downscale buffer for performance
                let scale_factor = if w > MAX_GLOW_BUFFER_DIM || h > MAX_GLOW_BUFFER_DIM {
                    (MAX_GLOW_BUFFER_DIM as f32 / w as f32).min(MAX_GLOW_BUFFER_DIM as f32 / h as f32).min(1.0)
                } else { 1.0 };
                let buf_w = ((w as f32) * scale_factor).ceil() as i32;
                let buf_h = ((h as f32) * scale_factor).ceil() as i32;
                if state.cache_hbm.0 == 0 || state.scaled_w != buf_w || state.scaled_h != buf_h {
                    state.cleanup();
                    let screen_dc = GetDC(None);
                    let bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: buf_w, biHeight: -buf_h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0 as u32, ..Default::default() }, ..Default::default() };
                    let res = CreateDIBSection(screen_dc, &bmi, DIB_RGB_COLORS, &mut state.cache_bits, None, 0);
                    ReleaseDC(None, screen_dc);
                    if let Ok(hbm) = res { if !hbm.is_invalid() && !state.cache_bits.is_null() { state.cache_hbm = hbm; state.scaled_w = buf_w; state.scaled_h = buf_h; } else { return LRESULT(0); } } else { return LRESULT(0); }
                }
                // Skip expensive drawing if fading out
                if !is_fading && !state.cache_bits.is_null() {
                    if state.graphics_mode == "minimal" { crate::overlay::paint_utils::draw_minimal_glow(state.cache_bits as *mut u32, state.scaled_w, state.scaled_h, anim_offset, 1.0, true); }
                    else { crate::overlay::paint_utils::draw_direct_sdf_glow(state.cache_bits as *mut u32, state.scaled_w, state.scaled_h, anim_offset, 1.0, true); }
                }
                let screen_dc = GetDC(None);
                let needs_scaling = state.scaled_w != w || state.scaled_h != h;
                // Scale up if needed
                let (final_hbm, final_w, final_h) = if needs_scaling {
                    let scaled_dc = CreateCompatibleDC(screen_dc); SelectObject(scaled_dc, state.cache_hbm);
                    let dest_bmi = BITMAPINFO { bmiHeader: BITMAPINFOHEADER { biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32, biWidth: w, biHeight: -h, biPlanes: 1, biBitCount: 32, biCompression: BI_RGB.0 as u32, ..Default::default() }, ..Default::default() };
                    let mut dest_bits: *mut core::ffi::c_void = std::ptr::null_mut();
                    let dest_hbm = CreateDIBSection(screen_dc, &dest_bmi, DIB_RGB_COLORS, &mut dest_bits, None, 0);
                    if let Ok(hbm) = dest_hbm {
                        if !hbm.is_invalid() {
                            let dest_dc = CreateCompatibleDC(screen_dc); SelectObject(dest_dc, hbm);
                            SetStretchBltMode(dest_dc, HALFTONE); StretchBlt(dest_dc, 0, 0, w, h, scaled_dc, 0, 0, state.scaled_w, state.scaled_h, SRCCOPY);
                            DeleteDC(scaled_dc); (Some((dest_dc, hbm)), w, h)
                        } else { DeleteDC(scaled_dc); (None, state.scaled_w, state.scaled_h) }
                    } else { DeleteDC(scaled_dc); (None, state.scaled_w, state.scaled_h) }
                } else { (None, w, h) };
                
                let (mem_dc, old_hbm, temp_res) = if let Some((dc, hbm)) = final_hbm { (dc, HGDIOBJ::default(), Some(hbm)) } else { let dc = CreateCompatibleDC(screen_dc); let old = SelectObject(dc, state.cache_hbm); (dc, old, None) };
                let pt_src = POINT { x: 0, y: 0 }; let size = SIZE { cx: final_w, cy: final_h };
                let mut blend = BLENDFUNCTION::default(); blend.BlendOp = AC_SRC_OVER as u8; blend.SourceConstantAlpha = alpha; blend.AlphaFormat = AC_SRC_ALPHA as u8;
                UpdateLayeredWindow(hwnd, None, None, Some(&size), mem_dc, Some(&pt_src), COLORREF(0), Some(&blend), ULW_ALPHA);
                
                if temp_res.is_some() { DeleteDC(mem_dc); if let Some(hbm) = temp_res { DeleteObject(hbm); } } else { SelectObject(mem_dc, old_hbm); DeleteDC(mem_dc); }
                ReleaseDC(None, screen_dc);
            }
            LRESULT(0)
        }
        WM_PAINT => { let mut ps = PAINTSTRUCT::default(); BeginPaint(hwnd, &mut ps); EndPaint(hwnd, &mut ps); LRESULT(0) }
        WM_DESTROY => { let mut states = PROC_STATES.lock().unwrap(); if let Some(mut state) = states.remove(&(hwnd.0 as isize)) { state.cleanup(); } PostQuitMessage(0); LRESULT(0) }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}