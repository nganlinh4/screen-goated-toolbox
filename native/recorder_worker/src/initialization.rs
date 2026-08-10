pub fn init_com_and_dpi() {
    unsafe {
        use windows::Win32::System::Com::CoInitialize;
        use windows::Win32::System::LibraryLoader::{GetProcAddress, LoadLibraryW};
        use windows::core::{BOOL, s, w};

        let _ = CoInitialize(None);
        if let Ok(user32) = LoadLibraryW(w!("user32.dll"))
            && let Some(set_context) = GetProcAddress(user32, s!("SetProcessDpiAwarenessContext"))
        {
            let function: extern "system" fn(isize) -> BOOL = std::mem::transmute(set_context);
            let _ = function(-4);
        }
    }
}
