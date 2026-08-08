use super::state::WINDOW_STATES;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::UI::WindowsAndMessaging::{GetWindowRect, IsWindow, IsWindowVisible};

/// Check if two RECTs overlap (with a gap margin)
fn rects_overlap(a: &RECT, b: &RECT, gap: i32) -> bool {
    // Expand both rects by gap/2 to account for minimum gap between windows
    let half_gap = gap / 2;
    !(a.right + half_gap <= b.left - half_gap
        || b.right + half_gap <= a.left - half_gap
        || a.bottom + half_gap <= b.top - half_gap
        || b.bottom + half_gap <= a.top - half_gap)
}

/// Get RECTs of all currently visible result overlay windows
/// This provides intelligent detection of existing windows for collision avoidance
fn get_all_active_window_rects() -> Vec<RECT> {
    let mut rects = Vec::new();

    // Lock WINDOW_STATES to get all tracked overlay windows
    if let Ok(states) = WINDOW_STATES.lock() {
        for (&hwnd_key, _state) in states.iter() {
            let hwnd = HWND(hwnd_key as *mut std::ffi::c_void);
            unsafe {
                // Verify window is still valid and VISIBLE
                // We check visibility because windows being closed are hidden immediately
                // but might take a few milliseconds to be removed from WINDOW_STATES.
                if IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool() {
                    let mut rect = RECT::default();
                    if GetWindowRect(hwnd, &mut rect).is_ok() {
                        rects.push(rect);
                    }
                }
            }
        }
    }

    rects
}

/// Check if a proposed RECT overlaps with any existing window
fn would_overlap_existing(proposed: &RECT, existing: &[RECT], gap: i32) -> bool {
    existing.iter().any(|r| rects_overlap(proposed, r, gap))
}

/// Calculate the next window position with intelligent collision detection.
///
/// This improved algorithm:
/// 1. Collects all active overlay windows from WINDOW_STATES
/// 2. Tries positions in order: Right -> Bottom -> Left -> Top
/// 3. Checks each candidate against ALL existing windows (not just the previous one)
/// 4. Falls back to cascade positioning if all directions are blocked
///
/// Similar to the intelligent layout in node_graph.rs blocks_to_snarl()
pub fn calculate_next_window_rect(prev: RECT, monitor_rect: RECT) -> RECT {
    let gap = 15;
    let w = (prev.right - prev.left).abs();
    let h = (prev.bottom - prev.top).abs();

    // Get all active window RECTs for collision detection
    let existing_windows = get_all_active_window_rects();

    // 1. Try RIGHT
    let right_candidate = RECT {
        left: prev.right + gap,
        top: prev.top,
        right: prev.right + gap + w,
        bottom: prev.bottom,
    };
    if right_candidate.right <= monitor_rect.right
        && !would_overlap_existing(&right_candidate, &existing_windows, gap)
    {
        return right_candidate;
    }

    // 2. Try BOTTOM
    let bottom_candidate = RECT {
        left: prev.left,
        top: prev.bottom + gap,
        right: prev.right,
        bottom: prev.bottom + gap + h,
    };
    if bottom_candidate.bottom <= monitor_rect.bottom
        && !would_overlap_existing(&bottom_candidate, &existing_windows, gap)
    {
        return bottom_candidate;
    }

    // 3. Try LEFT
    let left_candidate = RECT {
        left: prev.left - gap - w,
        top: prev.top,
        right: prev.left - gap,
        bottom: prev.bottom,
    };
    if left_candidate.left >= monitor_rect.left
        && !would_overlap_existing(&left_candidate, &existing_windows, gap)
    {
        return left_candidate;
    }

    // 4. Try TOP
    let top_candidate = RECT {
        left: prev.left,
        top: prev.top - gap - h,
        right: prev.right,
        bottom: prev.top - gap,
    };
    if top_candidate.top >= monitor_rect.top
        && !would_overlap_existing(&top_candidate, &existing_windows, gap)
    {
        return top_candidate;
    }

    // 5. Try diagonals if cardinal directions are blocked
    let diagonals = [
        // Bottom-Right
        RECT {
            left: prev.right + gap,
            top: prev.bottom + gap,
            right: prev.right + gap + w,
            bottom: prev.bottom + gap + h,
        },
        // Bottom-Left
        RECT {
            left: prev.left - gap - w,
            top: prev.bottom + gap,
            right: prev.left - gap,
            bottom: prev.bottom + gap + h,
        },
        // Top-Right
        RECT {
            left: prev.right + gap,
            top: prev.top - gap - h,
            right: prev.right + gap + w,
            bottom: prev.top - gap,
        },
        // Top-Left
        RECT {
            left: prev.left - gap - w,
            top: prev.top - gap - h,
            right: prev.left - gap,
            bottom: prev.top - gap,
        },
    ];

    for diag in diagonals {
        if diag.left >= monitor_rect.left
            && diag.right <= monitor_rect.right
            && diag.top >= monitor_rect.top
            && diag.bottom <= monitor_rect.bottom
            && !would_overlap_existing(&diag, &existing_windows, gap)
        {
            return diag;
        }
    }

    // 6. Cascade fallback: find a non-overlapping cascade position
    for cascade_mult in 1..10 {
        let offset = 40 * cascade_mult;
        let cascade = RECT {
            left: prev.left + offset,
            top: prev.top + offset,
            right: prev.left + offset + w,
            bottom: prev.top + offset + h,
        };

        // Clamp to screen bounds
        if cascade.right <= monitor_rect.right
            && cascade.bottom <= monitor_rect.bottom
            && !would_overlap_existing(&cascade, &existing_windows, gap)
        {
            return cascade;
        }
    }

    // 7. Ultimate fallback: just use the simple cascade (may overlap)
    RECT {
        left: prev.left + 40,
        top: prev.top + 40,
        right: prev.left + 40 + w,
        bottom: prev.top + 40 + h,
    }
}
