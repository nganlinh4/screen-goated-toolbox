//! App selection popup for per-app audio capture

use windows::Win32::Foundation::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::Graphics::Gdi::{GetStockObject, BLACK_BRUSH, HBRUSH};
use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::core::w;
use std::sync::atomic::Ordering;
use super::state::*;
/// Enumerate visible windows with titles for app selection
/// Returns a list of (PID, Window Title) for apps that likely emit audio
pub fn enumerate_audio_apps() -> Vec<(u32, String)> {
    use windows::Win32::System::Threading::{OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION};
    
    let mut apps: Vec<(u32, String)> = Vec::new();
    let mut seen_pids: std::collections::HashSet<u32> = std::collections::HashSet::new();
    
    unsafe {
        // Callback to collect window info
        let mut callback_data = (&mut apps, &mut seen_pids);
        
        extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> windows_core::BOOL {
            unsafe {
                // Skip invisible windows
                if !IsWindowVisible(hwnd).as_bool() {
                    return windows_core::BOOL(1);
                }
                
                // Get window title
                let mut title_buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut title_buf);
                if len == 0 {
                    return windows_core::BOOL(1);
                }
                
                let title = String::from_utf16_lossy(&title_buf[..len as usize]);
                
                // Skip empty/system windows
                if title.is_empty() || title == "Program Manager" || title == "Settings" {
                    return windows_core::BOOL(1);
                }
                
                // Get process ID
                let mut pid: u32 = 0;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                
                if pid == 0 {
                    return windows_core::BOOL(1);
                }
                
                // Get callback data from lparam
                let data = &mut *(lparam.0 as *mut (&mut Vec<(u32, String)>, &mut std::collections::HashSet<u32>));
                let (apps, seen_pids) = data;
                
                // Skip if we've already seen this PID (one entry per app)
                if seen_pids.contains(&pid) {
                    return windows_core::BOOL(1);
                }
                seen_pids.insert(pid);
                
                // Skip our own process
                let our_pid = std::process::id();
                if pid == our_pid {
                    return windows_core::BOOL(1);
                }
                
                apps.push((pid, title));
                
                windows_core::BOOL(1)
            }
        }
        
        let _ = EnumWindows(
            Some(enum_callback),
            LPARAM(&mut callback_data as *mut _ as isize)
        );
    }
    
    // Sort by title for better UX
    apps.sort_by(|a, b| a.1.to_lowercase().cmp(&b.1.to_lowercase()));
    
    apps
}

/// Show a popup window for selecting which app to capture audio from
/// This is called when TTS is enabled in device mode
pub fn show_app_selection_popup() {
    use windows::Win32::UI::WindowsAndMessaging::*;
    use windows::Win32::Graphics::Gdi::*;
    use windows::core::*;
    use std::sync::atomic::Ordering;
    
    // Get apps list
    let apps = enumerate_audio_apps();
    if apps.is_empty() {
        eprintln!("No audio apps found for selection");
        return;
    }
    
    // Build HTML for app list
    let app_items: Vec<String> = apps.iter()
        .map(|(pid, name)| {
            let escaped_name = name.replace('\\', "\\\\").replace('"', "\\\"").replace('<', "&lt;").replace('>', "&gt;");
            // Truncate by characters, not bytes (for Unicode safety)
            let short_name = if escaped_name.chars().count() > 50 {
                let truncated: String = escaped_name.chars().take(47).collect();
                format!("{}...", truncated)
            } else { 
                escaped_name.clone() 
            };
            format!(
                r#"<div class="app-item" data-pid="{}" onclick="selectApp({}, '{}')">
                    <span class="app-icon">🎵</span>
                    <div class="app-info">
                        <span class="app-name" title="{}">{}</span>
                        <span class="app-pid">PID: {}</span>
                    </div>
                </div>"#,
                pid, pid, escaped_name.replace('\'', "\\'"), escaped_name, short_name, pid
            )
        })
        .collect();
    
    let html = format!(r#"<!DOCTYPE html>
<html>
<head>
    <style>
        * {{ margin: 0; padding: 0; box-sizing: border-box; }}
        body {{
            font-family: 'Segoe UI', system-ui, sans-serif;
            background: rgba(20, 20, 30, 0.98);
            color: #fff;
            padding: 20px;
            min-height: 100vh;
        }}
        h1 {{
            font-size: 18px;
            font-weight: 500;
            margin-bottom: 8px;
            color: #fff;
        }}
        .hint {{
            font-size: 12px;
            color: #888;
            margin-bottom: 16px;
        }}
        .app-list {{
            display: flex;
            flex-direction: column;
            gap: 8px;
            max-height: calc(100vh - 100px);
            overflow-y: auto;
        }}
        .app-item {{
            display: flex;
            align-items: center;
            gap: 12px;
            padding: 12px 16px;
            background: rgba(255, 255, 255, 0.05);
            border-radius: 8px;
            cursor: pointer;
            transition: all 0.15s ease;
            border: 1px solid transparent;
        }}
        .app-item:hover {{
            background: rgba(255, 255, 255, 0.1);
            border-color: rgba(100, 180, 255, 0.5);
        }}
        .app-icon {{
            font-size: 24px;
            width: 40px;
            height: 40px;
            display: flex;
            align-items: center;
            justify-content: center;
            background: rgba(100, 180, 255, 0.2);
            border-radius: 8px;
        }}
        .app-info {{
            flex: 1;
            min-width: 0;
        }}
        .app-name {{
            display: block;
            font-size: 14px;
            font-weight: 500;
            white-space: nowrap;
            overflow: hidden;
            text-overflow: ellipsis;
        }}
        .app-pid {{
            font-size: 11px;
            color: #888;
        }}
        .app-list::-webkit-scrollbar {{
            width: 6px;
        }}
        .app-list::-webkit-scrollbar-track {{
            background: transparent;
        }}
        .app-list::-webkit-scrollbar-thumb {{
            background: rgba(255, 255, 255, 0.2);
            border-radius: 3px;
        }}
    </style>
</head>
<body>
    <h1>🎧 Select App to Capture</h1>
    <p class="hint">Choose the app whose audio you want to transcribe (TTS will be isolated)</p>
    <div class="app-list">
        {}
    </div>
    <script>
        function selectApp(pid, name) {{
            window.ipc.postMessage('selectApp:' + pid + ':' + name);
        }}
    </script>
</body>
</html>"#, app_items.join("\n"));
    
    // Create popup window
    std::thread::spawn(move || {
        unsafe {
            use windows::Win32::Graphics::Dwm::{DwmSetWindowAttribute, DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUND};
            use windows::Win32::UI::WindowsAndMessaging::{WS_CLIPCHILDREN, ShowWindow, SW_HIDE};
            
            // Register window class
            let class_name = w!("AppSelectPopup");
            let h_instance = GetModuleHandleW(None).unwrap_or_default();
            
            let wc = WNDCLASSEXW {
                cbSize: std::mem::size_of::<WNDCLASSEXW>() as u32,
                style: CS_HREDRAW | CS_VREDRAW,
                lpfnWndProc: Some(app_select_wndproc),
                hInstance: h_instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
                hbrBackground: HBRUSH(GetStockObject(BLACK_BRUSH).0),
                lpszClassName: class_name,
                ..Default::default()
            };
            RegisterClassExW(&wc);
            
            // Center the window on screen
            let screen_width = GetSystemMetrics(SM_CXSCREEN);
            let screen_height = GetSystemMetrics(SM_CYSCREEN);
            let win_width = 400;
            let win_height = 500;
            let x = (screen_width - win_width) / 2;
            let y = (screen_height - win_height) / 2;
            
            let hwnd = CreateWindowExW(
                WS_EX_TOPMOST | WS_EX_TOOLWINDOW,
                class_name,
                w!("Select App"),
                WS_POPUP | WS_VISIBLE | WS_CLIPCHILDREN,
                x, y, win_width, win_height,
                None,
                None,
                Some(h_instance.into()),
                None,
            ).unwrap();
            
            // Store handle for external closing
            APP_SELECTION_HWND.store(hwnd.0 as isize, Ordering::SeqCst);
            
            // Apply rounded corners
            let preference = DWMWCP_ROUND;
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &preference as *const _ as *const _,
                std::mem::size_of::<u32>() as u32,
            );
            
            // Create WebView2
            let html_clone = html.clone();
            let hwnd_val = hwnd.0 as isize;
            
            let result = wry::WebViewBuilder::new()
                .with_bounds(wry::Rect {
                    position: wry::dpi::Position::Physical(wry::dpi::PhysicalPosition::new(0, 0)),
                    size: wry::dpi::Size::Physical(wry::dpi::PhysicalSize::new(win_width as u32, win_height as u32)),
                })
                .with_html(&html_clone)
                .with_transparent(true)
                .with_ipc_handler(move |req| {
                    let body = req.body();
                    if body.starts_with("selectApp:") {
                        let rest = &body[10..];
                        if let Some((pid_str, name)) = rest.split_once(':') {
                            if let Ok(pid) = pid_str.parse::<u32>() {
                                // Store selected app
                                SELECTED_APP_PID.store(pid, Ordering::SeqCst);
                                if let Ok(mut app_name) = SELECTED_APP_NAME.lock() {
                                    *app_name = name.to_string();
                                }
                                
                                // Set audio source to trigger restart (must set this for restart to work!)
                                if let Ok(mut new_source) = NEW_AUDIO_SOURCE.lock() {
                                    *new_source = "device".to_string();
                                }
                                AUDIO_SOURCE_CHANGE.store(true, Ordering::SeqCst);
                                
                                let hwnd = HWND(hwnd_val as *mut std::ffi::c_void);
                                unsafe {
                                    // Close native popup
                                    let _ = ShowWindow(hwnd, SW_HIDE);
                                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                                    
                                    // Close TTS Modal in translation window (if exists)
                                    if !TRANSLATION_HWND.is_invalid() {
                                        let _ = PostMessageW(Some(TRANSLATION_HWND), WM_CLOSE_TTS_MODAL, WPARAM(0), LPARAM(0));
                                    }
                                }
                            }
                        }
                    }
                })
                .build_as_child(&HwndWrapper(hwnd));
            
            if result.is_err() {
                eprintln!("Failed to create WebView for app selection");
                let _ = DestroyWindow(hwnd);
                return;
            }
            
            // Keep WebView alive
            let _webview = result.unwrap();
            
            // Message loop
            let mut msg = MSG::default();
            while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
    });
}

pub unsafe extern "system" fn app_select_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    use windows::Win32::UI::WindowsAndMessaging::*;
    
    match msg {
        WM_CLOSE => {
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }
        WM_DESTROY => {
            APP_SELECTION_HWND.store(0, std::sync::atomic::Ordering::SeqCst);
            PostQuitMessage(0);
            LRESULT(0)
        }
        WM_SIZE => {
            // Resize child (WebView) to match parent
            let width = (lparam.0 & 0xFFFF) as i32;
            let height = ((lparam.0 >> 16) & 0xFFFF) as i32;
            if let Ok(child) = GetWindow(hwnd, GW_CHILD) {
                if child.0 != std::ptr::null_mut() {
                    let _ = MoveWindow(child, 0, 0, width, height, true);
                }
            }
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}
