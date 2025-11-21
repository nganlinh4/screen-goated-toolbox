#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod config;
mod api;
mod gui;
mod overlay;
mod icon_gen;
mod model_config;

use std::sync::{Arc, Mutex};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::HiDpi::*;
use windows::Win32::System::Threading::*;
use windows::core::*;
use lazy_static::lazy_static;
use image::ImageBuffer;
use config::{Config, load_config};
use tray_icon::{TrayIconBuilder, menu::{Menu, MenuItem}};

// Global event for inter-process restore signaling (manual-reset event)
lazy_static! {
    pub static ref RESTORE_EVENT: Option<windows::Win32::Foundation::HANDLE> = unsafe {
        CreateEventW(None, true, false, w!("ScreenGroundedTranslatorRestoreEvent")).ok()
    };
}


pub struct AppState {
    pub config: Config,
    pub original_screenshot: Option<ImageBuffer<image::Rgba<u8>, Vec<u8>>>,
    pub hotkeys_updated: bool,
    pub model_selector: model_config::ModelSelector,
}

lazy_static! {
    pub static ref APP: Arc<Mutex<AppState>> = Arc::new(Mutex::new(AppState {
        config: load_config(),
        original_screenshot: None,
        hotkeys_updated: false,
        model_selector: model_config::ModelSelector::new(model_config::USE_MODEL_ROTATION),
    }));
}

fn main() -> eframe::Result<()> {
    // Ensure the named event exists (for first instance, for second instance to signal)
    let _ = RESTORE_EVENT.as_ref();
    
    // Keep the handle alive for the duration of the program
    let _single_instance_mutex = unsafe {
        let instance = CreateMutexW(None, true, w!("ScreenGroundedTranslatorSingleInstanceMutex"));
        if let Ok(handle) = instance {
            if GetLastError() == ERROR_ALREADY_EXISTS {
                // Another instance is running - signal it to restore
                if let Some(event) = RESTORE_EVENT.as_ref() {
                    let _ = SetEvent(*event);
                }
                return Ok(());
            }
            Some(handle)
        } else {
            None
        }
    };

    unsafe { let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2); }

    std::thread::spawn(|| {
        run_hotkey_listener();
    });

    let tray_menu = Menu::new();
    // Added "Settings" option so users can explicitly click to restore
    // Ensure IDs match what we handle in gui.rs
    let settings_i = MenuItem::with_id("1002", "Settings", true, None);
    let quit_i = MenuItem::with_id("1001", "Quit", true, None);
    let _ = tray_menu.append(&settings_i);
    let _ = tray_menu.append(&quit_i);

    let icon = icon_gen::generate_icon();
    let tray_icon = TrayIconBuilder::new()
        .with_menu(Box::new(tray_menu.clone()))
        .with_tooltip("Screen Grounded Translator (nganlinh4)")
        .with_icon(icon)
        .build()
        .unwrap();

    let mut viewport_builder = eframe::egui::ViewportBuilder::default()
        .with_inner_size([600.0, 500.0])
        .with_resizable(true);
    
    // Set window icon - embedded in binary
    let app_icon_bytes = include_bytes!("../assets/app-icon-small.png");
    if let Ok(img) = image::load_from_memory(app_icon_bytes) {
        let img_rgba = img.to_rgba8();
        let (width, height) = img_rgba.dimensions();
        let icon_data = eframe::egui::IconData {
            rgba: img_rgba.to_vec(),
            width,
            height,
        };
        viewport_builder = viewport_builder.with_icon(std::sync::Arc::new(icon_data));
    }
    
    let options = eframe::NativeOptions {
        viewport: viewport_builder,
        ..Default::default()
    };
    
    let initial_config = APP.lock().unwrap().config.clone();
    
    eframe::run_native(
        "Screen Grounded Translator",
        options,
        Box::new(move |cc| {
            gui::configure_fonts(&cc.egui_ctx);
            Box::new(gui::SettingsApp::new(initial_config, APP.clone(), tray_icon, tray_menu, cc.egui_ctx.clone()))
        }),
    )
}



fn run_hotkey_listener() {
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("HotkeyListenerClass");
        
        let wc = WNDCLASSW {
            lpfnWndProc: Some(hotkey_proc),
            hInstance: instance,
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);
        
        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class_name,
            w!("Listener"),
            WS_OVERLAPPEDWINDOW,
            0, 0, 0, 0,
            None, None, instance, None
        );

        let mut current_hotkeys: Vec<(u32, u32)> = APP.lock().unwrap().config.hotkeys.iter().map(|h| (h.code, h.modifiers)).collect();
        for (id, (hotkey_code, modifiers)) in current_hotkeys.iter().enumerate() {
            RegisterHotKey(hwnd, (id + 1) as i32, HOT_KEY_MODIFIERS(*modifiers), *hotkey_code);
        }
        
        // Set a timer to check for hotkey updates periodically
        SetTimer(hwnd, 999, 500, None); // Check every 500ms

        let mut msg = MSG::default();
        loop {
            if GetMessageW(&mut msg, None, 0, 0).into() {
                match msg.message {
                    WM_TIMER => {
                        // Check if hotkeys were updated
                        let mut app = APP.lock().unwrap();
                        if app.hotkeys_updated {
                            let new_hotkeys: Vec<(u32, u32)> = app.config.hotkeys.iter().map(|h| (h.code, h.modifiers)).collect();
                            if new_hotkeys != current_hotkeys {
                                // Unregister old hotkeys
                                for id in 1..=current_hotkeys.len() as i32 {
                                    UnregisterHotKey(hwnd, id);
                                }
                                // Register new hotkeys
                                for (id, (hotkey_code, modifiers)) in new_hotkeys.iter().enumerate() {
                                    RegisterHotKey(hwnd, (id + 1) as i32, HOT_KEY_MODIFIERS(*modifiers), *hotkey_code);
                                }
                                current_hotkeys = new_hotkeys;
                            }
                            app.hotkeys_updated = false;
                        }
                    }
                    _ => {
                        TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                }
            }
        }
    }
}

unsafe extern "system" fn hotkey_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_HOTKEY => {
            // Handle any registered hotkey (wparam is the hotkey ID, which can be 1, 2, 3, etc.)
            if wparam.0 > 0 {
                // Check if selection overlay is already active, dismiss it instead of opening a new one
                if overlay::is_selection_overlay_active_and_dismiss() {
                    // Successfully dismissed the active overlay, don't create a new one
                    return LRESULT(0);
                }
                
                // No overlay active, proceed with normal capture flow
                match capture_full_screen() {
                    Ok(img) => {
                        {
                            let mut app = APP.lock().unwrap();
                            app.original_screenshot = Some(img);
                        }
                        std::thread::spawn(|| {
                           overlay::show_selection_overlay(); 
                        });
                    },
                    Err(e) => println!("Capture Error: {}", e),
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}



fn capture_full_screen() -> anyhow::Result<ImageBuffer<image::Rgba<u8>, Vec<u8>>> {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        let hdc_screen = GetDC(None);
        let hdc_mem = CreateCompatibleDC(hdc_screen);
        let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);
        SelectObject(hdc_mem, hbitmap);

        BitBlt(hdc_mem, 0, 0, width, height, hdc_screen, x, y, SRCCOPY).ok()?;

        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut buffer: Vec<u8> = vec![0; (width * height * 4) as usize];
        GetDIBits(hdc_mem, hbitmap, 0, height as u32, Some(buffer.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);

        for chunk in buffer.chunks_exact_mut(4) {
            chunk.swap(0, 2);
            chunk[3] = 255;
        }

        DeleteObject(hbitmap);
        DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        let img = ImageBuffer::from_raw(width as u32, height as u32, buffer)
            .ok_or_else(|| anyhow::anyhow!("Buffer creation failed"))?;
        
        Ok(img)
    }
}
