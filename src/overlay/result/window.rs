use std::sync::Once;
use windows::Win32::Foundation::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use super::event_handler::result_wnd_proc;
use super::state::{RefineContext, WINDOW_STATES, WindowState, WindowType};

pub const CHAIN_PALETTE: [u32; 5] = [
    0x001a1a1c, // Slate Gray (Primary)
    0x00113832, // Deep Teal
    0x00162a4d, // Royal Navy
    0x00311b3e, // Deep Plum
    0x004a2c22, // Deep Sienna
];

pub const CHAIN_PALETTE_LIGHT: [u32; 5] = [
    0x00f5f5f7, // Off White (Primary)
    0x00e0f2f1, // Light Teal
    0x00e3f2fd, // Light Blue
    0x00f3e5f5, // Light Purple
    0x00fbe9e7, // Light Orange
];

pub fn get_chain_color(visible_index: usize) -> u32 {
    let is_dark = crate::overlay::is_dark_mode();
    let palette = if is_dark {
        &CHAIN_PALETTE
    } else {
        &CHAIN_PALETTE_LIGHT
    };

    if visible_index == 0 {
        palette[0]
    } else {
        let cycle_idx = (visible_index - 1) % (palette.len() - 1);
        palette[cycle_idx + 1]
    }
}

pub fn remap_chain_color_for_theme(color: u32, is_dark: bool) -> u32 {
    CHAIN_PALETTE
        .iter()
        .chain(CHAIN_PALETTE_LIGHT.iter())
        .position(|candidate| *candidate == color)
        .map(|position| position % CHAIN_PALETTE.len())
        .map(|slot| {
            if is_dark {
                CHAIN_PALETTE[slot]
            } else {
                CHAIN_PALETTE_LIGHT[slot]
            }
        })
        .unwrap_or(color)
}

static REGISTER_RESULT_CLASS: Once = Once::new();

fn result_window_styles() -> (WINDOW_EX_STYLE, WINDOW_STYLE) {
    let ex_style = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    (ex_style, WS_POPUP)
}

pub struct ResultWindowParams {
    pub target_rect: RECT,
    pub win_type: WindowType,
    pub context: RefineContext,
    pub model_id: String,
    pub provider: String,
    pub streaming_enabled: bool,
    pub start_editing: bool,
    pub preset_prompt: String,
    pub custom_bg_color: u32,
    pub initial_text: String,
    pub preset_id: Option<String>,
    pub is_chain_root: bool,
}

pub fn create_result_window(params: ResultWindowParams) -> HWND {
    let ResultWindowParams {
        target_rect,
        win_type: _win_type,
        context,
        model_id,
        provider,
        streaming_enabled,
        start_editing,
        preset_prompt,
        custom_bg_color,
        initial_text,
        preset_id,
        is_chain_root,
    } = params;
    unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class_name = w!("TranslationResult");

        REGISTER_RESULT_CLASS.call_once(|| {
            let wc = WNDCLASSW {
                lpfnWndProc: Some(result_wnd_proc),
                hInstance: instance.into(),
                hCursor: LoadCursorW(None, IDC_ARROW).unwrap(),
                lpszClassName: class_name,
                style: CS_HREDRAW | CS_VREDRAW | CS_DBLCLKS,
                hbrBackground: Default::default(),
                ..Default::default()
            };
            let _ = RegisterClassW(&wc);
        });

        let width = (target_rect.right - target_rect.left).abs();
        let height = (target_rect.bottom - target_rect.top).abs();
        let favorite_overlay_opacity = {
            let app = crate::APP.lock().unwrap();
            app.config.favorite_overlay_opacity.clamp(10, 100)
        };

        // WindowType logic essentially just sets color now, but we override it via custom_bg_color usually
        let (x, y) = (target_rect.left, target_rect.top);

        let (ex_style, base_style) = result_window_styles();

        let hwnd = CreateWindowExW(
            ex_style,
            class_name,
            w!(""),
            base_style,
            x,
            y,
            width,
            height,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap_or_default();

        {
            let mut states = WINDOW_STATES.lock().unwrap();
            states.insert(
                hwnd.0 as isize,
                WindowState {
                    copy_success: false,
                    is_editing: start_editing,
                    context_data: context,
                    full_text: initial_text.clone(),
                    text_history: Vec::new(),
                    redo_history: Vec::new(),
                    is_refining: false,
                    is_streaming_active: streaming_enabled,
                    was_streaming_active: streaming_enabled,
                    model_id,
                    provider,
                    streaming_enabled,
                    bg_color: custom_bg_color,
                    linked_windows: Vec::new(),
                    pending_text: Some(initial_text),
                    last_text_update_time: 0,
                    preset_prompt,
                    input_text: String::new(),
                    cancellation_token: None,
                    chain_id: None,
                    is_browsing: false,
                    navigation_depth: 0,
                    max_navigation_depth: 0,
                    tts_request_id: 0,
                    tts_loading: false,
                    opacity_percent: favorite_overlay_opacity,
                    preset_id: preset_id.clone(),
                    is_chain_root,
                },
            );
        }

        super::scene_compositor::register_window(hwnd);

        // The HWND owns geometry and resize hit-testing only. Rendering and
        // opacity belong exclusively to the scene compositor.
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), 1, LWA_ALPHA);

        if start_editing {
            // Just activate the window, let the button canvas handle the UI
            let _ = SetForegroundWindow(hwnd);
        }

        // Always register window with button canvas so floating buttons are available
        super::button_canvas::register_markdown_window(hwnd);

        hwnd
    }
}

pub fn update_window_text(hwnd: HWND, text: &str) {
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return;
    }

    let sync_immediately = {
        let mut states = WINDOW_STATES.lock().unwrap();
        let Some(state) = states.get_mut(&(hwnd.0 as isize)) else {
            return;
        };
        if text_update_waits_for_stream_timer(state.is_streaming_active) {
            state.pending_text = Some(text.to_string());
            false
        } else {
            state.pending_text = None;
            state.full_text = text.to_string();
            true
        }
    };
    if sync_immediately {
        let visible = unsafe { IsWindowVisible(hwnd).as_bool() };
        super::scene_compositor::sync_window(hwnd, visible);
    }
}

fn text_update_waits_for_stream_timer(is_streaming_active: bool) -> bool {
    is_streaming_active
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn result_window_stays_hidden_until_the_caller_shows_it() {
        let (_, style) = result_window_styles();

        assert_eq!(style.0 & WS_VISIBLE.0, 0);
    }

    #[test]
    fn streaming_updates_have_one_timer_owned_sync_path() {
        assert!(text_update_waits_for_stream_timer(true));
        assert!(!text_update_waits_for_stream_timer(false));
    }

    #[test]
    fn theme_palette_colors_preserve_their_slot_across_theme_changes() {
        for slot in 0..CHAIN_PALETTE.len() {
            assert_eq!(
                remap_chain_color_for_theme(CHAIN_PALETTE[slot], false),
                CHAIN_PALETTE_LIGHT[slot]
            );
            assert_eq!(
                remap_chain_color_for_theme(CHAIN_PALETTE_LIGHT[slot], true),
                CHAIN_PALETTE[slot]
            );
        }
        assert_eq!(remap_chain_color_for_theme(0x00123456, true), 0x00123456);
    }
}
