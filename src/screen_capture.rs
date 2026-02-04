// --- SCREEN CAPTURE ---
// GDI-based fast screen capture for screenshot overlays.

use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

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
            return Err(anyhow::anyhow!(
                "GDI Error: Failed to get screen device context"
            ));
        }

        let hdc_mem = CreateCompatibleDC(Some(hdc_screen));
        if hdc_mem.is_invalid() {
            let _ = ReleaseDC(None, hdc_screen);
            return Err(anyhow::anyhow!(
                "GDI Error: Failed to create compatible device context"
            ));
        }

        let hbitmap = CreateCompatibleBitmap(hdc_screen, width, height);

        if hbitmap.is_invalid() {
            let _ = DeleteDC(hdc_mem);
            let _ = ReleaseDC(None, hdc_screen);
            return Err(anyhow::anyhow!(
                "GDI Error: Failed to create compatible bitmap."
            ));
        }

        SelectObject(hdc_mem, hbitmap.into());

        // This is the only "heavy" part, but it's purely GPU/GDI memory move. Very fast.
        BitBlt(
            hdc_mem,
            0,
            0,
            width,
            height,
            Some(hdc_screen),
            x,
            y,
            SRCCOPY,
        )?;

        // Cleanup DCs, but KEEP the HBITMAP
        let _ = DeleteDC(hdc_mem);
        ReleaseDC(None, hdc_screen);

        Ok(GdiCapture {
            hbitmap,
            width,
            height,
        })
    }
}
