// --- SELECTION MAGNIFICATION ---
// Windows Magnification API loading and helpers.

use super::state::*;
use windows::core::*;
use windows::Win32::System::LibraryLoader::*;

#[allow(static_mut_refs)]
pub unsafe fn load_magnification_api() -> bool {
    let mag_dll = std::ptr::addr_of!(MAG_DLL).read();
    if !mag_dll.is_invalid() {
        return true; // Already loaded
    }

    let dll_name = w!("Magnification.dll");
    let dll = LoadLibraryW(dll_name);

    if let Ok(h) = dll {
        MAG_DLL = h;

        // Get function pointers
        if let Some(init) = GetProcAddress(h, s!("MagInitialize")) {
            MAG_INITIALIZE = Some(std::mem::transmute(init));
        }
        if let Some(uninit) = GetProcAddress(h, s!("MagUninitialize")) {
            MAG_UNINITIALIZE = Some(std::mem::transmute(uninit));
        }
        if let Some(transform) = GetProcAddress(h, s!("MagSetFullscreenTransform")) {
            MAG_SET_FULLSCREEN_TRANSFORM = Some(std::mem::transmute(transform));
        }

        let init_ptr = std::ptr::addr_of!(MAG_INITIALIZE).read();
        let trans_ptr = std::ptr::addr_of!(MAG_SET_FULLSCREEN_TRANSFORM).read();
        return init_ptr.is_some() && trans_ptr.is_some();
    }

    false
}
