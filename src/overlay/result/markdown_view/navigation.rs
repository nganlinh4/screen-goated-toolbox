//! Navigation functions for markdown view (back/forward, content updates)

use windows::Win32::Foundation::*;

use super::webview::{create_markdown_webview, create_markdown_webview_ex, destroy_markdown_webview};
use super::conversion::markdown_to_html;
use super::{SKIP_NEXT_NAVIGATION, WEBVIEWS};

/// Navigate back in browser history
pub fn go_back(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    // Determine if we need to recreate the webview (returning to original content)
    // or just go back in browser history.
    let (returning_to_original, markdown_text, is_hovered) = {
        let mut states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            if state.navigation_depth > 0 {
                state.navigation_depth -= 1;
            }

            // If depth is now 0, we are returning to the starting result content.
            // We recreate the WebView to ensure a clean state and avoid "white screen"
            // issues that happen when document.write is blocked by website CSP.
            if state.navigation_depth == 0 {
                state.is_browsing = false;
                state.max_navigation_depth = 0; // History is reset on recreation
                (true, state.full_text.clone(), state.is_hovered)
            } else {
                (false, String::new(), false)
            }
        } else {
            (false, String::new(), false)
        }
    };

    if returning_to_original {
        // Full recreation of the WebView with the desired content
        create_markdown_webview(parent_hwnd, &markdown_text, is_hovered);

        // Trigger repaint to hide navigation buttons
        unsafe {
            let _ = windows::Win32::Graphics::Gdi::InvalidateRect(Some(parent_hwnd), None, false);
        }
        crate::overlay::result::button_canvas::update_window_position(parent_hwnd);
    } else {
        // Normal browser history back for deeper navigation
        // Set skip flag to prevent navigation_handler from re-incrementing depth
        {
            let mut skip_map = SKIP_NEXT_NAVIGATION.lock().unwrap();
            skip_map.insert(hwnd_key, true);
        }

        WEBVIEWS.with(|webviews| {
            if let Some(webview) = webviews.borrow().get(&hwnd_key) {
                let _ = webview.evaluate_script("history.back();");
            }
        });
        crate::overlay::result::button_canvas::update_window_position(parent_hwnd);
    }
}

/// Navigate forward in browser history
pub fn go_forward(parent_hwnd: HWND) {
    let hwnd_key = parent_hwnd.0 as isize;

    // Set skip flag to prevent navigation_handler from incrementing depth
    {
        let mut skip_map = SKIP_NEXT_NAVIGATION.lock().unwrap();
        skip_map.insert(hwnd_key, true);
    }

    // Increment navigation depth since we're going forward
    {
        let mut states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&hwnd_key) {
            if state.navigation_depth < state.max_navigation_depth {
                state.navigation_depth += 1;
                state.is_browsing = true;
            } else {
                return; // Cannot go forward
            }
        }
    }

    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            let _ = webview.evaluate_script("history.forward();");
        }
    });
    crate::overlay::result::button_canvas::update_window_position(parent_hwnd);
}

/// Update the markdown content in an existing WebView
pub fn update_markdown_content(parent_hwnd: HWND, markdown_text: &str) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let (is_refining, preset_prompt, input_text) = {
        let states = crate::overlay::result::state::WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get(&hwnd_key) {
            (
                state.is_refining,
                state.preset_prompt.clone(),
                state.input_text.clone(),
            )
        } else {
            (false, String::new(), String::new())
        }
    };
    update_markdown_content_ex(
        parent_hwnd,
        markdown_text,
        is_refining,
        &preset_prompt,
        &input_text,
    )
}

/// Update the markdown content in an existing WebView (Raw version, does not fetch state)
/// For interactive HTML with scripts: recreates WebView to get proper origin
/// For simple content: uses fast inline update
pub fn update_markdown_content_ex(
    parent_hwnd: HWND,
    markdown_text: &str,
    is_refining: bool,
    preset_prompt: &str,
    input_text: &str,
) -> bool {
    let hwnd_key = parent_hwnd.0 as isize;
    let html = markdown_to_html(markdown_text, is_refining, preset_prompt, input_text);

    // Check if this content has scripts that need full browser capabilities
    // If so, we must recreate the WebView to get proper origin access
    if super::html_utils::content_needs_recreation(&html) {
        // Destroy existing WebView and create fresh one
        destroy_markdown_webview(parent_hwnd);

        // Get hover state for sizing
        let is_hovered = {
            if let Ok(states) = crate::overlay::result::state::WINDOW_STATES.lock() {
                states.get(&hwnd_key).map(|s| s.is_hovered).unwrap_or(false)
            } else {
                false
            }
        };

        // Recreate WebView with fresh content (will use with_html for proper origin)
        return create_markdown_webview_ex(
            parent_hwnd,
            markdown_text,
            is_hovered,
            is_refining,
            preset_prompt,
            input_text,
        );
    }

    // Fast path for simple content without scripts
    WEBVIEWS.with(|webviews| {
        if let Some(webview) = webviews.borrow().get(&hwnd_key) {
            // For simple markdown, update body content via DOM manipulation
            // This is safe because we verified there are no conflicting scripts
            let escaped_html = html
                .replace('\\', "\\\\")
                .replace('`', "\\`")
                .replace("${", "\\${");
            let script = format!(
                "document.open(); document.write(`{}`); document.close();",
                escaped_html
            );
            let _ = webview.evaluate_script(&script);
            return true;
        }
        false
    })
}
