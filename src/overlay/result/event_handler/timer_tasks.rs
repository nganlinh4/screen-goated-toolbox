use super::super::logic;
use crate::overlay::result::markdown_view;

use crate::overlay::result::state::WINDOW_STATES;
use crate::overlay::utils::to_wstring;
use std::time::{SystemTime, UNIX_EPOCH};
use windows::core::PCWSTR;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::InvalidateRect;
use windows::Win32::UI::WindowsAndMessaging::*;

pub unsafe fn handle_timer(hwnd: HWND, wparam: WPARAM) -> LRESULT {
    let timer_id = wparam.0;

    // Timer ID 2: Markdown hover polling (The Authority on WebView Sizing)
    if timer_id == 2 {
        let mut cursor_pos = POINT::default();
        let _ = GetCursorPos(&mut cursor_pos);
        let mut window_rect = RECT::default();
        let _ = GetWindowRect(hwnd, &mut window_rect);

        // Check if cursor is geometrically inside the window rect
        let cursor_inside = cursor_pos.x >= window_rect.left
            && cursor_pos.x < window_rect.right
            && cursor_pos.y >= window_rect.top
            && cursor_pos.y < window_rect.bottom;

        let (is_markdown_mode, current_hover_state) = {
            let states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get(&(hwnd.0 as isize)) {
                (state.is_markdown_mode, state.is_hovered)
            } else {
                (false, false)
            }
        };

        if is_markdown_mode {
            // State change detection
            if cursor_inside && !current_hover_state {
                // Enter: Mark hovered -> Shrink WebView -> Buttons visible
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        state.is_hovered = true;
                    }
                }
                markdown_view::resize_markdown_webview(hwnd, true);
                markdown_view::fit_font_to_window(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
            } else if !cursor_inside && current_hover_state {
                // Leave: Mark unhovered -> Expand WebView -> Clean look
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        state.is_hovered = false;
                        state.on_copy_btn = false;
                        state.on_undo_btn = false;
                        state.on_markdown_btn = false;
                        state.on_download_btn = false;
                        state.on_back_btn = false;
                        state.on_forward_btn = false;
                    }
                }
                markdown_view::resize_markdown_webview(hwnd, false);
                markdown_view::fit_font_to_window(hwnd);
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
        }

        return LRESULT(0);
    }

    // Timer ID 1 and other timers: existing logic
    let mut need_repaint = false;

    let mut pending_update: Option<String> = None;
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u32)
        .unwrap_or(0);

    {
        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
            // Handle animation updates if refining
            if state.is_refining {
                state.animation_offset -= 8.0;
                if state.animation_offset < -3600.0 {
                    state.animation_offset += 3600.0;
                }

                // Refresh markdown WebView when refinement starts to show the context quote
                if state.is_markdown_mode && state.font_cache_dirty {
                    state.font_cache_dirty = false;
                    markdown_view::update_markdown_content_ex(
                        hwnd,
                        &state.full_text,
                        true,
                        &state.preset_prompt,
                        &state.input_text,
                    );
                }

                need_repaint = true;
            }

            // Detect streaming end transition to force flush remaining text
            // When streaming was active but is now inactive, we must render any leftover text
            let streaming_just_ended = state.was_streaming_active && !state.is_streaming_active;
            if streaming_just_ended {
                state.was_streaming_active = false;
            }

            // Safety: If streaming is NOT active, always process pending text immediately
            // This ensures any leftover text is rendered even if streaming_just_ended was missed
            let not_streaming = !state.is_streaming_active;

            // Track if we need to force font cache dirty (bypass 200ms throttle)
            // This is critical for rendering the final text after streaming ends

            // Throttle - but bypass if:
            // 1. streaming_just_ended (transition detection)
            // 2. not_streaming (any pending text after streaming should render immediately)
            // 3. first update (last_text_update_time == 0)
            // 4. throttle expired (>16ms since last update)
            if state.pending_text.is_some()
                && (streaming_just_ended
                    || not_streaming
                    || state.last_text_update_time == 0
                    || now.wrapping_sub(state.last_text_update_time) > 16)
            {
                pending_update = state.pending_text.take();
                state.last_text_update_time = now;

                // CRITICAL: When streaming ends, force font recalculation
                // to ensure the final text is properly rendered (bypass 200ms throttle)
                if not_streaming {
                    state.font_cache_dirty = true;
                }
            }

            // Note: Native EDIT control handling removed - both plain text and markdown modes
            // now use WebView-based refine input. Polling happens outside the lock below.
        }
    }

    if let Some(txt) = pending_update {
        let (maybe_markdown_update, is_hovered, is_markdown_streaming, is_streaming) = {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                // 200ms font recalc throttling during streaming/text updates
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u32)
                    .unwrap_or(0);
                let time_since_last_calc = now.wrapping_sub(state.last_font_calc_time);
                if time_since_last_calc >= 200 || state.last_font_calc_time == 0 {
                    state.font_cache_dirty = true;
                    state.last_font_calc_time = now;
                }
                state.full_text = txt.clone();

                if state.is_markdown_mode && !state.is_refining {
                    (
                        Some(state.full_text.clone()),
                        state.is_hovered,
                        state.is_markdown_streaming,
                        state.is_streaming_active,
                    )
                } else {
                    (None, false, false, false)
                }
            } else {
                (None, false, false, false)
            }
        };

        if let Some(md_text) = maybe_markdown_update {
            // MARKDOWN MODE - OPTIMIZED PATH
            // Skip SetWindowTextW and InvalidateRect to prevent double-rendering lag behind WebView

            // Use streaming-optimized update for markdown_stream mode during active streaming
            if is_markdown_streaming && is_streaming {
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map(|d| d.as_millis() as u32)
                    .unwrap_or(0);

                let mut should_update_webview = false;
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        let time_since_last_webview =
                            now.wrapping_sub(state.last_webview_update_time);
                        if time_since_last_webview >= 80 || state.last_webview_update_time == 0 {
                            state.last_webview_update_time = now;
                            should_update_webview = true;
                        }
                    }
                }

                if should_update_webview {
                    markdown_view::stream_markdown_content(hwnd, &md_text);
                    // Register with button canvas (may already be registered, that's fine)
                    crate::overlay::result::button_canvas::register_markdown_window(hwnd);
                }
            } else if is_markdown_streaming && !is_streaming {
                // Streaming just ended in markdown_stream mode
                // Render the FINAL content first (in case last chunks weren't rendered due to throttling)
                markdown_view::stream_markdown_content(hwnd, &md_text);
                // Initialize Grid.js on any tables
                markdown_view::init_gridjs(hwnd);
                // Ensure WebView bounds are set (forces layout recalculation)
                markdown_view::resize_markdown_webview(hwnd, false);
                // Fit font to fill any unfilled space
                markdown_view::fit_font_to_window(hwnd);
                // Now reset for next session
                markdown_view::reset_stream_counter(hwnd);
                // Reset throttle for next time
                {
                    let mut states = WINDOW_STATES.lock().unwrap();
                    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                        state.last_webview_update_time = 0;
                    }
                }
                // Register with button canvas
                crate::overlay::result::button_canvas::register_markdown_window(hwnd);
            } else {
                // Regular markdown mode (not streaming) - full render
                markdown_view::reset_stream_counter(hwnd);
                markdown_view::create_markdown_webview(hwnd, &md_text, is_hovered);
                // Ensure WebView bounds are set (forces layout recalculation)
                markdown_view::resize_markdown_webview(hwnd, false);
                // Fit font to fill any unfilled space
                markdown_view::fit_font_to_window(hwnd);
                // Register with button canvas
                crate::overlay::result::button_canvas::register_markdown_window(hwnd);
            }

            // Do NOT set need_repaint = true here.
            // The WebView handles the display. Repainting parent window is wasteful and causes lag.
        } else {
            // PLAIN TEXT MODE (or Refining) - LEGACY PATH
            // Must update window text and trigger GDI repaint
            let wide_text = to_wstring(&txt);
            let _ = SetWindowTextW(hwnd, PCWSTR(wide_text.as_ptr()));
            need_repaint = true;
        }
    }

    logic::handle_timer(hwnd, wparam);
    if need_repaint {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
    LRESULT(0)
}
