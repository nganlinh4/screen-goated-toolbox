use crate::overlay::result::layout::calculate_next_window_rect;
use std::collections::HashMap;
use std::sync::Mutex;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

pub const MAX_GLOW_BUFFER_DIM: i32 = 1280;

pub struct ProcessingState {
    pub animation_offset: f32,
    pub is_fading_out: bool,
    pub alpha: u8,
    pub cache_hbm: HBITMAP,
    pub cache_bits: *mut core::ffi::c_void,
    pub scaled_w: i32,
    pub scaled_h: i32,
    pub timer_killed: bool,
    pub graphics_mode: String,
}

unsafe impl Send for ProcessingState {}
unsafe impl Sync for ProcessingState {}

impl ProcessingState {
    pub fn new(graphics_mode: String) -> Self {
        Self {
            animation_offset: 0.0,
            is_fading_out: false,
            alpha: 255,
            cache_hbm: HBITMAP::default(),
            cache_bits: std::ptr::null_mut(),
            scaled_w: 0,
            scaled_h: 0,
            timer_killed: false,
            graphics_mode,
        }
    }

    pub fn cleanup(&mut self) {
        if !self.cache_hbm.is_invalid() {
            unsafe {
                let _ = DeleteObject(self.cache_hbm.into());
            }
            self.cache_hbm = HBITMAP::default();
            self.cache_bits = std::ptr::null_mut();
        }
    }
}

lazy_static::lazy_static! {
    // Per-chain window position tracking - ensures snake pattern only applies within the same chain
    // Key: chain_id (UUID string), Value: last window RECT for that chain
    static ref CHAIN_WINDOW_POSITIONS: Mutex<HashMap<String, RECT>> = Mutex::new(HashMap::new());
}

/// Get the corrected rect using saved preset geometry (if available).
/// For non-image presets, uses the saved window_geometry from the preset.
/// Returns the original rect if no saved geometry exists or if it's an image preset.
pub fn get_rect_with_saved_geometry(preset_id: &str, original_rect: RECT) -> RECT {
    if let Ok(app) = crate::APP.lock() {
        if let Some(preset) = app.config.presets.iter().find(|p| p.id == preset_id) {
            // Only apply saved geometry for non-image presets
            let is_image_category = preset.preset_type == "image";
            if !is_image_category {
                if let Some(geom) = &preset.window_geometry {
                    return RECT {
                        left: geom.x,
                        top: geom.y,
                        right: geom.x + geom.width,
                        bottom: geom.y + geom.height,
                    };
                }
            }
        }
    }
    original_rect
}

/// Generate a new unique chain ID for a processing chain
pub fn generate_chain_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos() as u64;
    let count = COUNTER.fetch_add(1, Ordering::Relaxed);

    format!("chain-{}-{}", timestamp, count)
}

/// Get the next window position using snake algorithm (first-come-first-serve)
/// This is mutex-protected so parallel branches within the SAME chain get sequential positions
/// Different chains (different chain_id) are completely independent
pub fn get_next_window_position_for_chain(chain_id: &str, initial_rect: RECT) -> RECT {
    let mut positions = CHAIN_WINDOW_POSITIONS.lock().unwrap();

    let monitor_rect = unsafe {
        let h_monitor = MonitorFromRect(&initial_rect, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO::default();
        mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;
        if GetMonitorInfoW(h_monitor, &mut mi).as_bool() {
            mi.rcMonitor
        } else {
            RECT {
                left: 0,
                top: 0,
                right: GetSystemMetrics(SM_CXSCREEN),
                bottom: GetSystemMetrics(SM_CYSCREEN),
            }
        }
    };

    let next_rect = match positions.get(chain_id) {
        None => {
            // First window in this chain: use initial rect
            initial_rect
        }
        Some(&prev) => {
            // Subsequent windows in this chain: use snake algorithm from last position
            calculate_next_window_rect(prev, monitor_rect)
        }
    };

    // Update last position for this chain
    positions.insert(chain_id.to_string(), next_rect);

    next_rect
}
