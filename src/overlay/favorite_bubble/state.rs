use std::cell::RefCell;
use std::sync::{
    atomic::{AtomicBool, AtomicI32, AtomicIsize, AtomicU8},
    Once,
};
use wry::{WebContext, WebView};

// Constants
pub static BUBBLE_SIZE: AtomicI32 = AtomicI32::new(40);
pub const PANEL_WIDTH: i32 = 260;
pub const DRAG_THRESHOLD: i32 = 5; // Pixels of movement before counting as a drag

// Smooth opacity animation state
pub const OPACITY_TIMER_ID: usize = 1;
pub const OPACITY_STEP: u8 = 25; // Opacity change per frame (~150ms total animation)
pub const OPACITY_INACTIVE: u8 = 80; // ~31% opacity when not hovered
pub const OPACITY_ACTIVE: u8 = 255; // 100% opacity when hovered/expanded

pub const PHYSICS_TIMER_ID: usize = 2;

// Statics / Atomics
pub static REGISTER_BUBBLE_CLASS: Once = Once::new();
pub static REGISTER_PANEL_CLASS: Once = Once::new();
pub static BUBBLE_ACTIVE: AtomicBool = AtomicBool::new(false);
pub static BUBBLE_HWND: AtomicIsize = AtomicIsize::new(0);
pub static PANEL_HWND: AtomicIsize = AtomicIsize::new(0);
pub static IS_EXPANDED: AtomicBool = AtomicBool::new(false);
pub static IS_HOVERED: AtomicBool = AtomicBool::new(false);
pub static IS_DRAGGING: AtomicBool = AtomicBool::new(false);
pub static IS_DRAGGING_MOVED: AtomicBool = AtomicBool::new(false);
pub static DRAG_START_X: AtomicIsize = AtomicIsize::new(0);
pub static DRAG_START_Y: AtomicIsize = AtomicIsize::new(0);

// Animation state
pub static CURRENT_OPACITY: AtomicU8 = AtomicU8::new(80); // Start at inactive opacity
pub static BLINK_STATE: AtomicU8 = AtomicU8::new(0); // 0=None, 1..4=Blink Phases
pub static FADE_OUT_STATE: AtomicBool = AtomicBool::new(false); // True = fading out before close

// Focus restoration: Track the foreground window before any bubble interaction
// This is critical for text-select presets, which need to send Ctrl+C to the original window
pub static LAST_FOREGROUND_HWND: AtomicIsize = AtomicIsize::new(0);

// Track recursive theme updates
pub static LAST_THEME_IS_DARK: AtomicBool = AtomicBool::new(true);

// Thread Locals
thread_local! {
    pub static PANEL_WEBVIEW: RefCell<Option<WebView>> = RefCell::new(None);
    pub static PHYSICS_STATE: RefCell<(f32, f32)> = RefCell::new((0.0, 0.0));
    // Shared WebContext for this thread using common data directory
    pub static PANEL_WEB_CONTEXT: RefCell<Option<WebContext>> = RefCell::new(None);

    // Icon cache: (size, data)
    static CACHED_ICON: RefCell<(i32, Vec<u8>)> = RefCell::new((0, vec![]));
    static CACHED_ICON_LIGHT: RefCell<(i32, Vec<u8>)> = RefCell::new((0, vec![]));
}

// App icon embedded at compile time
const ICON_PNG_BYTES: &[u8] = include_bytes!("../../../assets/app-icon-small.png");
const ICON_LIGHT_PNG_BYTES: &[u8] = include_bytes!("../../../assets/app-icon-small-light.png");

// Cached decoded RGBA pixels - Removed lazy_static to support dynamic sizing

pub fn get_icon_data(size: i32, is_dark: bool) -> Vec<u8> {
    if is_dark {
        CACHED_ICON.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.0 != size {
                *cache = (size, decode_icon(ICON_PNG_BYTES, size));
            }
            cache.1.clone()
        })
    } else {
        CACHED_ICON_LIGHT.with(|cache| {
            let mut cache = cache.borrow_mut();
            if cache.0 != size {
                *cache = (size, decode_icon(ICON_LIGHT_PNG_BYTES, size));
            }
            cache.1.clone()
        })
    }
}

fn decode_icon(bytes: &[u8], size: i32) -> Vec<u8> {
    if let Ok(img) = image::load_from_memory(bytes) {
        let resized = img.resize_exact(
            size as u32,
            size as u32,
            image::imageops::FilterType::Lanczos3,
        );
        resized.to_rgba8().into_raw()
    } else {
        vec![]
    }
}
