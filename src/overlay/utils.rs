use windows::Win32::Foundation::*;
use windows::Win32::System::DataExchange::*;
use windows::Win32::System::Memory::*;

pub fn to_wstring(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

// --- CLIPBOARD SUPPORT ---
pub fn copy_to_clipboard(text: &str, hwnd: HWND) {
    unsafe {
        if OpenClipboard(hwnd).as_bool() {
            EmptyClipboard();
            
            // Convert text to UTF-16
            let wide_text: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
            let mem_size = wide_text.len() * 2;
            
            // Allocate global memory
            if let Ok(h_mem) = GlobalAlloc(GMEM_MOVEABLE, mem_size) {
                let ptr = GlobalLock(h_mem) as *mut u16;
                std::ptr::copy_nonoverlapping(wide_text.as_ptr(), ptr, wide_text.len());
                GlobalUnlock(h_mem);
                
                // Set clipboard data (CF_UNICODETEXT = 13)
                let h_mem_handle = HANDLE(h_mem.0);
                let _ = SetClipboardData(13u32, h_mem_handle);
            }
            
            CloseClipboard();
        }
    }
}

pub fn get_error_message(error: &str, lang: &str) -> String {
    match error {
        "NO_API_KEY" => {
            match lang {
                "vi" => "Bạn chưa nhập API key!".to_string(),
                _ => "You haven't entered an API key!".to_string(),
            }
        }
        "INVALID_API_KEY" => {
            match lang {
                "vi" => "API key không hợp lệ!".to_string(),
                _ => "Invalid API key!".to_string(),
            }
        }
        _ => {
            match lang {
                "vi" => format!("Lỗi: {}", error),
                _ => format!("Error: {}", error),
            }
        }
    }
}
