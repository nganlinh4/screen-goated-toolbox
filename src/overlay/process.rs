use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use std::sync::{Arc, Mutex};
use image::GenericImageView;

use crate::{AppState, api::translate_image_streaming};
use super::utils::{copy_to_clipboard, get_error_message};
use super::result::{create_result_window, update_result_window};

pub fn process_and_close(app: Arc<Mutex<AppState>>, rect: RECT, overlay_hwnd: HWND) {
    let (img, config, model_name, provider) = {
        let guard = app.lock().unwrap();
        let model_id = &guard.config.preferred_model;
        let model_config = crate::model_config::get_model_by_id(model_id);
        let model = guard.model_selector.get_model();
        let provider = model_config.map(|m| m.provider.clone()).unwrap_or_else(|| "groq".to_string());
        (guard.original_screenshot.clone().unwrap(), guard.config.clone(), model, provider)
    };

    let x_virt = unsafe { GetSystemMetrics(SM_XVIRTUALSCREEN) };
    let y_virt = unsafe { GetSystemMetrics(SM_YVIRTUALSCREEN) };
    
    let crop_x = (rect.left - x_virt).max(0) as u32;
    let crop_y = (rect.top - y_virt).max(0) as u32;
    let crop_w = (rect.right - rect.left).abs() as u32;
    let crop_h = (rect.bottom - rect.top).abs() as u32;
    
    let img_w = img.width();
    let img_h = img.height();
    let crop_w = crop_w.min(img_w.saturating_sub(crop_x));
    let crop_h = crop_h.min(img_h.saturating_sub(crop_y));

    if crop_w > 0 && crop_h > 0 {
        let cropped = img.view(crop_x, crop_y, crop_w, crop_h).to_image();
        
        // Store settings before config is moved
        let auto_copy = config.auto_copy;
        let groq_api_key = config.api_key.clone();
        let gemini_api_key = config.gemini_api_key.clone();
        let ui_language = config.ui_language.clone();
        let target_lang = config.target_language.clone();
        let streaming_enabled = config.streaming_enabled;
        
        // NOTE: We do NOT close the overlay_hwnd here. We keep it open (and scanning)
        // until the first chunk of data arrives.
        
        // Spawn a dedicated UI thread for the result window
        std::thread::spawn(move || {
            // Create result window immediately but HIDDEN
            let result_hwnd = create_result_window(rect);
            
            // Spawn a worker thread for the blocking API call
            std::thread::spawn(move || {
                // Accumulate text for final result and auto-copy
                let accumulated = Arc::new(Mutex::new(String::new()));
                let accumulated_clone = accumulated.clone();
                let mut first_chunk_received = false;
                
                // Blocking call with callback for real-time updates
                let res = translate_image_streaming(&groq_api_key, &gemini_api_key, target_lang, model_name, provider, cropped, streaming_enabled, |chunk| {
                    let mut text = accumulated_clone.lock().unwrap();
                    text.push_str(chunk);
                    
                    // On first chunk, switch windows
                    if !first_chunk_received {
                        first_chunk_received = true;
                        unsafe {
                            // Close the selection/scanning overlay
                            PostMessageW(overlay_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                            // Show the result window
                            ShowWindow(result_hwnd, SW_SHOW);
                        }
                    }
                    
                    // Update the window in real-time
                    update_result_window(&text);
                });

                match res {
                    Ok(text) => {
                        if !text.trim().is_empty() {
                            // Apply auto-copy if enabled
                            if auto_copy {
                                std::thread::spawn(move || {
                                    std::thread::sleep(std::time::Duration::from_millis(100));
                                    copy_to_clipboard(&text, HWND(0));
                                });
                            }
                        }
                    }
                    Err(e) => {
                        // If we error out before showing the window, show it now
                        if !first_chunk_received {
                            unsafe {
                                PostMessageW(overlay_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0));
                                ShowWindow(result_hwnd, SW_SHOW);
                            }
                        }
                        let error_msg = get_error_message(&e.to_string(), &ui_language);
                        update_result_window(&error_msg);
                    }
                }
            });

            // Run message loop on this thread to keep the result window responsive
            unsafe {
                let mut msg = MSG::default();
                while GetMessageW(&mut msg, None, 0, 0).into() {
                    TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    if !IsWindow(result_hwnd).as_bool() { break; }
                }
            }
        });

    } else {
        unsafe { PostMessageW(overlay_hwnd, WM_CLOSE, WPARAM(0), LPARAM(0)); }
    }
}
