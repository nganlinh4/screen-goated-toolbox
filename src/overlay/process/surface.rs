use std::sync::Once;

use windows::Win32::Foundation::{
    COLORREF, HINSTANCE, HWND, LPARAM, LRESULT, POINT, RECT, SIZE, WPARAM,
};
use windows::Win32::Graphics::Gdi::{
    AC_SRC_ALPHA, AC_SRC_OVER, BI_RGB, BITMAPINFO, BITMAPINFOHEADER, BLENDFUNCTION, BeginPaint,
    CreateCompatibleDC, CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, EndPaint, GetDC,
    HBITMAP, HDC, HGDIOBJ, PAINTSTRUCT, ReleaseDC, SelectObject,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, HTTRANSPARENT, RegisterClassW,
    SW_SHOWNOACTIVATE, ShowWindow, ULW_ALPHA, UpdateLayeredWindow, WM_NCHITTEST, WM_PAINT,
    WNDCLASSW, WS_EX_LAYERED, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_EX_TOPMOST, WS_EX_TRANSPARENT,
    WS_POPUP,
};
use windows::core::w;

use super::glow::{GlowMask, SurfaceSpec, animation_phase, surface_specs};

static REGISTER_SURFACE_CLASS: Once = Once::new();

pub(super) struct SurfaceSet {
    surfaces: Vec<LayerSurface>,
    pixel_count: usize,
}

unsafe impl Send for SurfaceSet {}

impl SurfaceSet {
    pub unsafe fn create(owner: HWND, instance: HINSTANCE, rect: RECT) -> Option<Self> {
        unsafe { register_surface_class(instance) };
        let width = (rect.right - rect.left).abs();
        let height = (rect.bottom - rect.top).abs();
        let mut surfaces = Vec::new();
        let mut pixel_count = 0usize;
        for spec in surface_specs(width, height) {
            let surface =
                unsafe { LayerSurface::create(owner, instance, rect, spec, width, height) }?;
            pixel_count = pixel_count.saturating_add(surface.pixel_count());
            surfaces.push(surface);
        }
        (!surfaces.is_empty()).then_some(Self {
            surfaces,
            pixel_count,
        })
    }

    pub fn pixel_count(&self) -> usize {
        self.pixel_count
    }

    pub unsafe fn present(&mut self, elapsed: std::time::Duration, alpha: u8, recolor: bool) {
        let phase = animation_phase(elapsed);
        for surface in &mut self.surfaces {
            unsafe { surface.present(phase, alpha, recolor) };
        }
    }
}

struct LayerSurface {
    hwnd: HWND,
    bitmap: HBITMAP,
    memory_dc: HDC,
    previous_bitmap: HGDIOBJ,
    bits: *mut u32,
    screen_origin: POINT,
    size: SIZE,
    mask: GlowMask,
    shown: bool,
}

impl LayerSurface {
    unsafe fn create(
        owner: HWND,
        instance: HINSTANCE,
        outer: RECT,
        spec: SurfaceSpec,
        full_width: i32,
        full_height: i32,
    ) -> Option<Self> {
        let screen_origin = POINT {
            x: outer.left + spec.x,
            y: outer.top + spec.y,
        };
        let hwnd = unsafe {
            CreateWindowExW(
                WS_EX_LAYERED
                    | WS_EX_TOPMOST
                    | WS_EX_TOOLWINDOW
                    | WS_EX_TRANSPARENT
                    | WS_EX_NOACTIVATE,
                w!("SGTProcessingGlowEdge"),
                w!(""),
                WS_POPUP,
                screen_origin.x,
                screen_origin.y,
                spec.width,
                spec.height,
                Some(owner),
                None,
                Some(instance),
                None,
            )
        }
        .ok()?;
        let screen_dc = unsafe { GetDC(None) };
        let memory_dc = unsafe { CreateCompatibleDC(Some(screen_dc)) };
        let mut raw_bits = std::ptr::null_mut();
        let bitmap_info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: spec.width,
                biHeight: -spec.height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let bitmap = unsafe {
            CreateDIBSection(
                Some(screen_dc),
                &bitmap_info,
                DIB_RGB_COLORS,
                &mut raw_bits,
                None,
                0,
            )
        }
        .ok();
        unsafe { ReleaseDC(None, screen_dc) };
        let Some(bitmap) = bitmap.filter(|bitmap| !bitmap.is_invalid()) else {
            unsafe {
                let _ = DeleteDC(memory_dc);
                let _ = DestroyWindow(hwnd);
            }
            return None;
        };
        if raw_bits.is_null() {
            unsafe {
                let _ = DeleteObject(bitmap.into());
                let _ = DeleteDC(memory_dc);
                let _ = DestroyWindow(hwnd);
            }
            return None;
        }
        let previous_bitmap = unsafe { SelectObject(memory_dc, bitmap.into()) };
        Some(Self {
            hwnd,
            bitmap,
            memory_dc,
            previous_bitmap,
            bits: raw_bits.cast(),
            screen_origin,
            size: SIZE {
                cx: spec.width,
                cy: spec.height,
            },
            mask: GlowMask::new(spec, full_width, full_height),
            shown: false,
        })
    }

    fn pixel_count(&self) -> usize {
        (self.size.cx as usize).saturating_mul(self.size.cy as usize)
    }

    unsafe fn present(&mut self, phase: usize, alpha: u8, recolor: bool) {
        if recolor {
            unsafe { self.mask.render(self.bits, phase) };
        }
        let source = POINT::default();
        let blend = BLENDFUNCTION {
            BlendOp: AC_SRC_OVER as u8,
            BlendFlags: 0,
            SourceConstantAlpha: alpha,
            AlphaFormat: AC_SRC_ALPHA as u8,
        };
        let _ = unsafe {
            UpdateLayeredWindow(
                self.hwnd,
                None,
                Some(&self.screen_origin),
                Some(&self.size),
                Some(self.memory_dc),
                Some(&source),
                COLORREF(0),
                Some(&blend),
                ULW_ALPHA,
            )
        };
        if !self.shown {
            let _ = unsafe { ShowWindow(self.hwnd, SW_SHOWNOACTIVATE) };
            self.shown = true;
        }
    }
}

impl Drop for LayerSurface {
    fn drop(&mut self) {
        unsafe {
            let _ = SelectObject(self.memory_dc, self.previous_bitmap);
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.memory_dc);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

unsafe fn register_surface_class(instance: HINSTANCE) {
    REGISTER_SURFACE_CLASS.call_once(|| {
        let class = WNDCLASSW {
            lpfnWndProc: Some(surface_wnd_proc),
            hInstance: instance,
            lpszClassName: w!("SGTProcessingGlowEdge"),
            ..Default::default()
        };
        unsafe {
            let _ = RegisterClassW(&class);
        }
    });
}

unsafe extern "system" fn surface_wnd_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    unsafe {
        match message {
            WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
            WM_PAINT => {
                let mut paint = PAINTSTRUCT::default();
                let _ = BeginPaint(hwnd, &mut paint);
                let _ = EndPaint(hwnd, &paint);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, message, wparam, lparam),
        }
    }
}
