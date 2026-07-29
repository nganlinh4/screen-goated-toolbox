use std::mem::size_of;
use std::sync::Once;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::*;

use super::event_handler::result_wnd_proc;
use super::state::{
    CursorPhysics, InteractionMode, RefineContext, ResizeEdge, WINDOW_STATES, WindowState,
    WindowType,
};

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

static REGISTER_RESULT_CLASS: Once = Once::new();

fn result_window_styles(is_markdown_mode: bool) -> (WINDOW_EX_STYLE, WINDOW_STYLE) {
    let ex_style = WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE;
    let base_style = result_window_style_for_mode(WS_POPUP, is_markdown_mode);

    (ex_style, base_style)
}

fn result_window_style_for_mode(
    current_style: WINDOW_STYLE,
    is_markdown_mode: bool,
) -> WINDOW_STYLE {
    if is_markdown_mode {
        // The markdown WebView is a child HWND. The parent is invalidated by the
        // animation timer, so its GDI paint must exclude the child surface.
        current_style | WS_CLIPCHILDREN
    } else {
        // Native text painting owns the full client area.
        WINDOW_STYLE(current_style.0 & !WS_CLIPCHILDREN.0)
    }
}

pub(super) unsafe fn set_markdown_parent_clipping(hwnd: HWND, enabled: bool) {
    unsafe {
        let current_style = WINDOW_STYLE(GetWindowLongW(hwnd, GWL_STYLE) as u32);
        let updated_style = result_window_style_for_mode(current_style, enabled);
        if updated_style == current_style {
            return;
        }

        SetWindowLongW(hwnd, GWL_STYLE, updated_style.0 as i32);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_FRAMECHANGED | SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn rgb_to_colorref(rgb_color: u32) -> COLORREF {
    COLORREF(
        ((rgb_color & 0x0000_00ff) << 16)
            | (rgb_color & 0x0000_ff00)
            | ((rgb_color & 0x00ff_0000) >> 16),
    )
}

unsafe fn prime_markdown_background(hwnd: HWND, rgb_color: u32) {
    unsafe {
        let window_dc = GetDC(Some(hwnd));
        if window_dc.is_invalid() {
            return;
        }

        let mut client_rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut client_rect);
        let brush = CreateSolidBrush(rgb_to_colorref(rgb_color));
        if !brush.is_invalid() {
            FillRect(window_dc, &client_rect, brush);
            let _ = DeleteObject(brush.into());
        }
        let _ = ReleaseDC(Some(hwnd), window_dc);
    }
}

pub(super) unsafe fn prepare_markdown_parent(hwnd: HWND, rgb_color: u32) {
    unsafe {
        // A transparent WebView exposes whatever the parent last painted. Clear
        // the native text while the full client area is still writable, then
        // exclude the child from all later parent paints.
        set_markdown_parent_clipping(hwnd, false);
        prime_markdown_background(hwnd, rgb_color);
        set_markdown_parent_clipping(hwnd, true);
    }
}

pub struct ResultWindowParams<'a> {
    pub target_rect: RECT,
    pub win_type: WindowType,
    pub context: RefineContext,
    pub model_id: String,
    pub provider: String,
    pub streaming_enabled: bool,
    pub start_editing: bool,
    pub preset_prompt: String,
    pub custom_bg_color: u32,
    pub render_mode: &'a str,
    pub initial_text: String,
    pub preset_id: Option<String>,
    pub is_chain_root: bool,
}

pub fn create_result_window(params: ResultWindowParams<'_>) -> HWND {
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
        render_mode,
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
                hbrBackground: HBRUSH::default(),
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
        let favorite_overlay_alpha =
            ((favorite_overlay_opacity as f32 / 100.0) * 255.0).round() as u8;

        // WindowType logic essentially just sets color now, but we override it via custom_bg_color usually
        let (x, y) = (target_rect.left, target_rect.top);

        let markdown_requested = render_mode == "markdown" || render_mode == "markdown_stream";
        let (ex_style, base_style) = result_window_styles(markdown_requested);

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

        let mut physics = CursorPhysics::default();

        // Initialize physics with current cursor position to prevent (0,0) glitch
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = ScreenToClient(hwnd, &mut pt);
        physics.x = pt.x as f32;
        physics.y = pt.y as f32;

        // Get graphics mode from config
        let graphics_mode = {
            let app = crate::APP.lock().unwrap();
            app.config.graphics_mode.clone()
        };

        {
            let mut states = WINDOW_STATES.lock().unwrap();
            states.insert(
                hwnd.0 as isize,
                WindowState {
                    is_hovered: false,
                    on_copy_btn: false,
                    copy_success: false,
                    on_edit_btn: false,
                    on_undo_btn: false,
                    on_redo_btn: false,
                    is_editing: start_editing,
                    context_data: context,
                    full_text: initial_text.clone(),
                    text_history: Vec::new(),
                    redo_history: Vec::new(),
                    is_refining: false,
                    animation_offset: 0.0,
                    is_streaming_active: streaming_enabled,
                    was_streaming_active: streaming_enabled,
                    model_id,
                    provider,
                    streaming_enabled,
                    bg_color: custom_bg_color,
                    linked_windows: Vec::new(),
                    physics,
                    interaction_mode: InteractionMode::None,
                    current_resize_edge: ResizeEdge::None,
                    drag_start_mouse: POINT { x: 0, y: 0 },
                    drag_start_window_rect: RECT::default(),
                    has_moved_significantly: false,
                    font_cache_dirty: true,
                    cached_font_size: 72,
                    content_bitmap: HBITMAP::default(),
                    last_w: 0,
                    last_h: 0,
                    pending_text: Some(initial_text),
                    last_text_update_time: 0,
                    last_resize_time: 0,
                    last_font_calc_time: 0,
                    markdown_settle_retry_until_ms: 0,
                    markdown_next_settle_fit_ms: 0,
                    last_webview_update_time: 0,
                    bg_bitmap: HBITMAP::default(),
                    bg_w: 0,
                    bg_h: 0,
                    preset_prompt,
                    input_text: String::new(),
                    graphics_mode,
                    cancellation_token: None,
                    chain_id: None,
                    // Markdown mode state
                    is_markdown_mode: markdown_requested,
                    is_markdown_streaming: render_mode == "markdown_stream",
                    on_markdown_btn: false,
                    is_browsing: false,
                    navigation_depth: 0,
                    max_navigation_depth: 0,
                    on_back_btn: false,
                    on_forward_btn: false,
                    on_download_btn: false,
                    on_speaker_btn: false,
                    tts_request_id: 0,
                    tts_loading: false,
                    opacity_percent: favorite_overlay_opacity,
                    preset_id: preset_id.clone(),
                    is_chain_root,
                },
            );
        }

        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), favorite_overlay_alpha, LWA_ALPHA);

        let corner_preference = 2u32;
        let _ = DwmSetWindowAttribute(
            hwnd,
            DWMWINDOWATTRIBUTE(33),
            &corner_preference as *const _ as *const _,
            size_of::<u32>() as u32,
        );

        if start_editing {
            // Just activate the window, let the button canvas handle the UI
            let _ = SetForegroundWindow(hwnd);
        }

        let _ = InvalidateRect(Some(hwnd), None, false);
        let _ = UpdateWindow(hwnd);

        // Always register window with button canvas so floating buttons are available
        super::button_canvas::register_markdown_window(hwnd);

        hwnd
    }
}

pub fn update_window_text(hwnd: HWND, text: &str) {
    if !unsafe { IsWindow(Some(hwnd)).as_bool() } {
        return;
    }

    let mut states = WINDOW_STATES.lock().unwrap();
    if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
        state.pending_text = Some(text.to_string());
        state.full_text = text.to_string();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markdown_parent_paint_excludes_webview_child() {
        let (_, style) = result_window_styles(true);

        assert_ne!(style.0 & WS_CLIPCHILDREN.0, 0);
    }

    #[test]
    fn plain_text_parent_keeps_full_client_painting() {
        let (_, style) = result_window_styles(false);

        assert_eq!(style.0 & WS_CLIPCHILDREN.0, 0);
    }

    #[test]
    fn mode_transition_preserves_unrelated_window_styles() {
        let original = WS_POPUP | WS_VISIBLE;
        let markdown = result_window_style_for_mode(original, true);
        let plain = result_window_style_for_mode(markdown, false);

        assert_ne!(markdown.0 & WS_CLIPCHILDREN.0, 0);
        assert_eq!(plain, original);
    }

    #[test]
    fn markdown_background_rgb_is_converted_to_colorref() {
        assert_eq!(rgb_to_colorref(0x0011_2233).0, 0x0033_2211);
    }

    #[test]
    fn result_window_stays_hidden_until_the_caller_shows_it() {
        let (_, style) = result_window_styles(true);

        assert_eq!(style.0 & WS_VISIBLE.0, 0);
    }
}
