// --- SCREEN CAPTURE ---
// GDI-based fast screen capture for screenshot overlays.

use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::Threading::{
    GR_GDIOBJECTS, GR_GDIOBJECTS_PEAK, GR_USEROBJECTS, GR_USEROBJECTS_PEAK, GetCurrentProcess,
    GetGuiResources,
};
use windows::Win32::UI::WindowsAndMessaging::*;

#[derive(Clone, Copy, Debug)]
pub struct GuiResourcesSnapshot {
    pub gdi_objects: u32,
    pub user_objects: u32,
    pub gdi_peak: u32,
    pub user_peak: u32,
}

pub fn gui_resources_snapshot() -> GuiResourcesSnapshot {
    unsafe {
        let process = GetCurrentProcess();
        GuiResourcesSnapshot {
            gdi_objects: GetGuiResources(process, GR_GDIOBJECTS),
            user_objects: GetGuiResources(process, GR_USEROBJECTS),
            gdi_peak: GetGuiResources(process, GR_GDIOBJECTS_PEAK),
            user_peak: GetGuiResources(process, GR_USEROBJECTS_PEAK),
        }
    }
}

pub fn format_gui_resources(snapshot: GuiResourcesSnapshot) -> String {
    format!(
        "gdi={} user={} gdi_peak={} user_peak={}",
        snapshot.gdi_objects, snapshot.user_objects, snapshot.gdi_peak, snapshot.user_peak
    )
}

fn gdi_error(context: &str) -> anyhow::Error {
    let error_code = unsafe { windows::Win32::Foundation::GetLastError().0 };
    let error = windows::core::Error::from_thread();
    let resources = format_gui_resources(gui_resources_snapshot());
    anyhow::anyhow!(
        "GDI Error: {} (GetLastError={}, {}, {})",
        context,
        error_code,
        error,
        resources
    )
}

/// Wrapper for GDI Handle to ensure cleanup on drop.
pub struct GdiCapture {
    pub hbitmap: HBITMAP,
    pub width: i32,
    pub height: i32,
}

// Make it safe to send between threads (Handles are process-global in Windows GDI)
unsafe impl Send for GdiCapture {}
unsafe impl Sync for GdiCapture {}

impl Drop for GdiCapture {
    fn drop(&mut self) {
        unsafe {
            if !self.hbitmap.is_invalid() {
                let _ = DeleteObject(self.hbitmap.into());
            }
        }
    }
}

/// Capture the entire virtual screen using GDI.
pub fn capture_screen_fast() -> anyhow::Result<GdiCapture> {
    unsafe {
        let x = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let y = GetSystemMetrics(SM_YVIRTUALSCREEN);
        let width = GetSystemMetrics(SM_CXVIRTUALSCREEN);
        let height = GetSystemMetrics(SM_CYVIRTUALSCREEN);

        // Validate dimensions
        if width <= 0 || height <= 0 {
            return Err(anyhow::anyhow!(
                "GDI Error: Invalid screen dimensions ({} x {})",
                width,
                height
            ));
        }

        let hdc_screen = GetDC(None);
        if hdc_screen.is_invalid() {
            return Err(gdi_error("Failed to get screen device context"));
        }

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() {
            let _ = ReleaseDC(None, hdc_screen);
            return Err(gdi_error("Failed to create compatible device context"));
        }

        let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);

        if hbitmap.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return Err(gdi_error("Failed to create compatible bitmap"));
        }

        let old_obj = SelectObject(hdc_mem, hbitmap.into());

        // This is the only "heavy" part, but it's purely GPU/GDI memory move. Very fast.
        let blit_result = BitBlt(
            hdc_mem,
            0,
            0,
            width,
            height,
            Some(hdc_screen),
            x,
            y,
            SRCCOPY,
        );

        if let Err(err) = blit_result {
            let _ = SelectObject(hdc_mem, old_obj);
            let _ = DeleteObject(hbitmap.into());
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            let resources = format_gui_resources(gui_resources_snapshot());
            return Err(anyhow::anyhow!(
                "GDI Error: Failed to copy screen into bitmap: {} ({})",
                err,
                resources
            ));
        }

        // Cleanup DCs, but KEEP the HBITMAP
        let _ = SelectObject(hdc_mem, old_obj);
        let _ = DeleteDC(hdc_mem);
        let _ = ReleaseDC(None, hdc_screen);

        Ok(GdiCapture {
            hbitmap,
            width,
            height,
        })
    }
}
