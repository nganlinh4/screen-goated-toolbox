use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use std::sync::Mutex;
use crate::overlay::result::layout::calculate_next_window_rect;

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
            unsafe { let _ = DeleteObject(self.cache_hbm.into()); }
            self.cache_hbm = HBITMAP::default();
            self.cache_bits = std::ptr::null_mut();
        }
    }
}

lazy_static::lazy_static! {
    // Global sequential window position queue - ensures snake pattern even with parallel processing
    // Stores Option<RECT> where None means reset to initial position
    static ref LAST_WINDOW_RECT: Mutex<Option<RECT>> = Mutex::new(None);
}

/// Reset the window position queue (call at start of new processing chain)
pub fn reset_window_position_queue() {
    let mut last = LAST_WINDOW_RECT.lock().unwrap();
    *last = None;
}

/// Get the next window position using snake algorithm (first-come-first-serve)
/// This is mutex-protected so parallel branches get sequential positions
pub fn get_next_window_position(initial_rect: RECT) -> RECT {
    let mut last = LAST_WINDOW_RECT.lock().unwrap();
    
    let s_w = unsafe { GetSystemMetrics(SM_CXSCREEN) };
    let s_h = unsafe { GetSystemMetrics(SM_CYSCREEN) };
    
    let next_rect = match *last {
        None => {
            // First window: use initial rect
            initial_rect
        }
        Some(prev) => {
            // Subsequent windows: use snake algorithm from last position
            calculate_next_window_rect(prev, s_w, s_h)
        }
    };
    
    // Update last position for next caller
    *last = Some(next_rect);
    
    next_rect
}
