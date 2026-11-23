use crate::overlay::result::layout::calculate_next_window_rect;
use crate::overlay::utils::bounded_cache::BoundedCache;
use std::sync::{LazyLock, Mutex};
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;

// Per-chain window position tracking - ensures snake pattern only applies within the same chain
// Key: chain_id, Value: last window RECT for that chain. Only recent chains can
// still open windows, so older ones are evicted instead of growing forever.
const MAX_TRACKED_CHAINS: usize = 64;
static CHAIN_WINDOW_POSITIONS: LazyLock<Mutex<BoundedCache<String, RECT>>> =
    LazyLock::new(|| Mutex::new(BoundedCache::new(MAX_TRACKED_CHAINS)));

/// Get the corrected rect using saved preset geometry (if available).
/// For non-image presets, uses the saved window_geometry from the preset.
/// Returns the original rect if no saved geometry exists or if it's an image preset.
pub fn get_rect_with_saved_geometry(preset_id: &str, original_rect: RECT) -> RECT {
    if let Ok(app) = crate::APP.lock()
        && let Some(preset) = app.config.presets.iter().find(|p| p.id == preset_id)
    {
        // Only apply saved geometry for non-image presets
        let is_image_category = preset.preset_type == "image";
        if !is_image_category && let Some(geom) = &preset.window_geometry {
            return crate::overlay::display_placement::fit_rect_to_work_area(RECT {
                left: geom.x,
                top: geom.y,
                right: geom.x + geom.width,
                bottom: geom.y + geom.height,
            });
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
    let mut positions = CHAIN_WINDOW_POSITIONS
        .lock()
        .unwrap_or_else(|error| error.into_inner());

    let monitor_rect = unsafe {
        let h_monitor = MonitorFromRect(&initial_rect, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(h_monitor, &mut mi).as_bool() {
            mi.rcWork
        } else {
            RECT {
                left: 0,
                top: 0,
                right: GetSystemMetrics(SM_CXSCREEN),
                bottom: GetSystemMetrics(SM_CYSCREEN),
            }
        }
    };

    let next_rect = match positions.get(&chain_id.to_string()) {
        None => {
            // First window in this chain: use initial rect
            initial_rect
        }
        Some(&prev) => {
            // Subsequent windows in this chain: use snake algorithm from last position
            calculate_next_window_rect(prev, monitor_rect)
        }
    };
    let next_rect = crate::overlay::display_placement::fit_rect_to_work_area(next_rect);

    // Update last position for this chain
    positions.insert(chain_id.to_string(), next_rect);

    next_rect
}

#[cfg(test)]
mod tests {
    use super::{
        CHAIN_WINDOW_POSITIONS, MAX_TRACKED_CHAINS, generate_chain_id,
        get_next_window_position_for_chain,
    };
    use windows::Win32::Foundation::RECT;

    #[test]
    fn chain_positions_stay_bounded_over_many_chains() {
        let rect = RECT {
            left: 10,
            top: 10,
            right: 210,
            bottom: 110,
        };
        let mut latest = None;
        for _ in 0..MAX_TRACKED_CHAINS * 3 {
            let chain = generate_chain_id();
            get_next_window_position_for_chain(&chain, rect);
            latest = Some(chain);
        }
        let latest = latest.unwrap();
        let positions = CHAIN_WINDOW_POSITIONS.lock().unwrap();
        assert!(positions.len() <= MAX_TRACKED_CHAINS);
        assert!(positions.get(&latest).is_some());
    }
}
