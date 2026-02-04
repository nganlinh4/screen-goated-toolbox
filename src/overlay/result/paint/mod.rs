// --- RESULT PAINT MODULE ---
// Window painting for result overlay with text, effects, buttons, and broom.

mod effects;
mod helpers;

use super::state::{ResizeEdge, WINDOW_STATES};
use crate::overlay::broom_assets::{render_procedural_broom, BroomRenderParams, BROOM_H, BROOM_W};
use effects::{render_particles, render_refinement_glow};
use helpers::{create_bitmap_from_pixels, measure_text_bounds};
use std::mem::size_of;
use windows::core::w;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::UI::WindowsAndMessaging::*;


pub fn paint_window(hwnd: HWND) {
    unsafe {
        let mut ps = PAINTSTRUCT::default();
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut rect = RECT::default();
        let _ = GetClientRect(hwnd, &mut rect);
        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        // --- PHASE 1: STATE SNAPSHOT & CACHE MANAGEMENT ---
        let state_snapshot = collect_state_snapshot(hwnd, hdc, width, height);
        let Some(state_snapshot) = state_snapshot else {
            let _ = EndPaint(hwnd, &mut ps);
            return;
        };

        // --- PHASE 2: COMPOSITOR SETUP ---
        let mem_dc = CreateCompatibleDC(Some(hdc));

        let bmi_scratch = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut scratch_bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let scratch_bitmap = CreateDIBSection(
            Some(hdc),
            &bmi_scratch,
            DIB_RGB_COLORS,
            &mut scratch_bits,
            None,
            0,
        )
        .unwrap();
        let old_scratch = SelectObject(mem_dc, scratch_bitmap.into());

        // Copy Background
        if !state_snapshot.cached_bg_bm.is_invalid() {
            let cache_dc = CreateCompatibleDC(Some(hdc));
            let old_cbm = SelectObject(cache_dc, state_snapshot.cached_bg_bm.into());
            let _ = BitBlt(mem_dc, 0, 0, width, height, Some(cache_dc), 0, 0, SRCCOPY).ok();
            SelectObject(cache_dc, old_cbm);
            let _ = DeleteDC(cache_dc);
        }

        // --- PHASE 3: TEXT RENDERING ---
        let cached_text_bm = render_text_content(
            hwnd,
            hdc,
            mem_dc,
            width,
            height,
            &state_snapshot,
        );

        // --- PHASE 4: PIXEL MANIPULATION ---
        if !scratch_bits.is_null() {
            let raw_pixels = std::slice::from_raw_parts_mut(
                scratch_bits as *mut u32,
                (width * height) as usize,
            );

            // Refinement glow
            if state_snapshot.is_refining {
                render_refinement_glow(
                    raw_pixels,
                    width,
                    height,
                    state_snapshot.anim_offset,
                    &state_snapshot.graphics_mode,
                );
            }

            // Particles
            render_particles(raw_pixels, width, height, &state_snapshot.particles);
        }

        // --- PHASE 5: DYNAMIC BROOM ---
        if let Some((bx, by, params)) = state_snapshot.broom_data {
            let pixels = render_procedural_broom(params);
            let hbm = create_bitmap_from_pixels(&pixels, BROOM_W, BROOM_H);
            if !hbm.is_invalid() {
                let broom_dc = CreateCompatibleDC(Some(hdc));
                let old_hbm_broom = SelectObject(broom_dc, hbm.into());
                let mut bf = BLENDFUNCTION::default();
                bf.BlendOp = AC_SRC_OVER as u8;
                bf.SourceConstantAlpha = 255;
                bf.AlphaFormat = AC_SRC_ALPHA as u8;
                let draw_x = bx as i32 - (BROOM_W / 2);
                let draw_y = by as i32 - (BROOM_H as f32 * 0.65) as i32;
                let _ = GdiAlphaBlend(
                    mem_dc, draw_x, draw_y, BROOM_W, BROOM_H, broom_dc, 0, 0, BROOM_W, BROOM_H, bf,
                );
                SelectObject(broom_dc, old_hbm_broom);
                let _ = DeleteDC(broom_dc);
                let _ = DeleteObject(hbm.into());
            }
        }

        // --- PHASE 6: FINAL BLIT ---
        let _ = BitBlt(hdc, 0, 0, width, height, Some(mem_dc), 0, 0, SRCCOPY).ok();

        SelectObject(mem_dc, old_scratch);
        let _ = DeleteObject(scratch_bitmap.into());
        let _ = DeleteDC(mem_dc);

        // Store updated bitmap back
        if !cached_text_bm.is_invalid() {
            let mut states = WINDOW_STATES.lock().unwrap();
            if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
                if state.content_bitmap != cached_text_bm {
                    state.content_bitmap = cached_text_bm;
                }
            }
        }

        let _ = EndPaint(hwnd, &mut ps);
    }
}

// --- STATE SNAPSHOT ---
// Minimal state needed for painting (buttons handled by WebView overlay)

struct StateSnapshot {
    bg_color_u32: u32,
    is_markdown_mode: bool,
    broom_data: Option<(f32, f32, BroomRenderParams)>,
    particles: Vec<(f32, f32, f32, f32, u32)>,
    cached_text_bm: HBITMAP,
    cache_dirty: bool,
    cached_bg_bm: HBITMAP,
    is_refining: bool,
    anim_offset: f32,
    graphics_mode: String,
    preset_prompt: String,
    input_text: String,
}

unsafe fn collect_state_snapshot(
    hwnd: HWND,
    hdc: HDC,
    width: i32,
    height: i32,
) -> Option<StateSnapshot> {
    let mut states = WINDOW_STATES.lock().unwrap();
    let state = states.get_mut(&(hwnd.0 as isize))?;

    // Update Background Cache if needed
    if state.bg_bitmap.is_invalid() || state.bg_w != width || state.bg_h != height {
        if !state.bg_bitmap.is_invalid() {
            let _ = DeleteObject(state.bg_bitmap.into());
        }

        let bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: width,
                biHeight: -height,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0 as u32,
                ..Default::default()
            },
            ..Default::default()
        };

        let mut p_bg_bits: *mut core::ffi::c_void = std::ptr::null_mut();
        let hbm_bg =
            CreateDIBSection(Some(hdc), &bmi, DIB_RGB_COLORS, &mut p_bg_bits, None, 0).unwrap();

        if !p_bg_bits.is_null() {
            let pixels =
                std::slice::from_raw_parts_mut(p_bg_bits as *mut u32, (width * height) as usize);
            let r = (state.bg_color >> 16) & 0xFF;
            let g = (state.bg_color >> 8) & 0xFF;
            let b = state.bg_color & 0xFF;
            let col = (255 << 24) | (r << 16) | (g << 8) | b;
            pixels.fill(col);
        }
        state.bg_bitmap = hbm_bg;
        state.bg_w = width;
        state.bg_h = height;
    }

    if state.last_w != width || state.last_h != height {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u32)
            .unwrap_or(0);

        let time_since_last_resize = now.wrapping_sub(state.last_resize_time);
        if time_since_last_resize > 100 || state.last_resize_time == 0 {
            state.font_cache_dirty = true;
        }
        state.last_resize_time = now;
        state.last_w = width;
        state.last_h = height;
    }

    let particles_vec: Vec<(f32, f32, f32, f32, u32)> = state
        .physics
        .particles
        .iter()
        .map(|p| (p.x, p.y, p.life, p.size, p.color))
        .collect();

    let is_closing = false;
    let show_broom = !is_closing
        && !state.is_markdown_mode
        && (state.is_hovered
            && !state.on_copy_btn
            && !state.on_edit_btn
            && !state.on_undo_btn
            && !state.on_redo_btn
            && !state.on_markdown_btn
            && !state.on_back_btn
            && !state.on_forward_btn
            && !state.on_download_btn
            && !state.on_speaker_btn
            && state.current_resize_edge == ResizeEdge::None);

    let broom_info = if show_broom {
        Some((
            state.physics.x,
            state.physics.y,
            BroomRenderParams {
                tilt_angle: state.physics.current_tilt,
                squish: state.physics.squish_factor,
                bend: state.physics.bristle_bend,
                opacity: 1.0,
            },
        ))
    } else {
        None
    };

    Some(StateSnapshot {
        bg_color_u32: state.bg_color,
        is_markdown_mode: state.is_markdown_mode,
        broom_data: broom_info,
        particles: particles_vec,
        cached_text_bm: state.content_bitmap,
        cache_dirty: state.font_cache_dirty,
        cached_bg_bm: state.bg_bitmap,
        is_refining: state.is_refining,
        anim_offset: state.animation_offset,
        graphics_mode: state.graphics_mode.clone(),
        preset_prompt: state.preset_prompt.clone(),
        input_text: state.input_text.clone(),
    })
}

// --- TEXT RENDERING ---

unsafe fn render_text_content(
    hwnd: HWND,
    hdc: HDC,
    mem_dc: HDC,
    width: i32,
    height: i32,
    snapshot: &StateSnapshot,
) -> HBITMAP {
    if snapshot.is_markdown_mode {
        return HBITMAP::default();
    }

    let mut cached_text_bm = snapshot.cached_text_bm;

    if snapshot.cache_dirty || cached_text_bm.is_invalid() {
        if !cached_text_bm.is_invalid() {
            let _ = DeleteObject(cached_text_bm.into());
        }

        cached_text_bm = CreateCompatibleBitmap(hdc, width, height);
        let cache_dc = CreateCompatibleDC(Some(hdc));
        let old_cache_bm = SelectObject(cache_dc, cached_text_bm.into());

        let dark_brush = CreateSolidBrush(COLORREF(snapshot.bg_color_u32));
        let fill_rect = RECT {
            left: 0,
            top: 0,
            right: width,
            bottom: height,
        };
        FillRect(cache_dc, &fill_rect, dark_brush);
        let _ = DeleteObject(dark_brush.into());

        SetBkMode(cache_dc, TRANSPARENT);
        let bg_r = (snapshot.bg_color_u32 >> 16) & 0xFF;
        let bg_g = (snapshot.bg_color_u32 >> 8) & 0xFF;
        let bg_b = snapshot.bg_color_u32 & 0xFF;
        let luminance = (0.299 * bg_r as f32) + (0.587 * bg_g as f32) + (0.114 * bg_b as f32);
        let text_col = if luminance > 140.0 { 0x00000000 } else { 0x00FFFFFF };
        SetTextColor(cache_dc, COLORREF(text_col));

        let mut buf = if snapshot.is_refining {
            if !crate::overlay::utils::SHOW_REFINING_CONTEXT_QUOTE {
                vec![0u16; 1]
            } else {
                let combined = if snapshot.input_text.is_empty() {
                    snapshot.preset_prompt.clone()
                } else {
                    format!("{}\n\n{}", snapshot.preset_prompt, snapshot.input_text)
                };
                let quote = crate::overlay::utils::get_context_quote(&combined);
                quote.encode_utf16().collect::<Vec<u16>>()
            }
        } else {
            let text_len = GetWindowTextLengthW(hwnd);
            let mut b = vec![0u16; text_len as usize + 1];
            let actual_len = GetWindowTextW(hwnd, &mut b);
            b.truncate(actual_len as usize);
            b
        };

        let h_padding = if snapshot.is_refining { 20 } else { 2 };
        let available_w = (width - (h_padding * 2)).max(1);
        let v_safety_margin = 0;
        let available_h = (height - v_safety_margin).max(1);

        let mut low = if snapshot.is_refining { 8 } else { 2 };
        let max_possible = if snapshot.is_refining {
            18.min(available_h)
        } else {
            available_h.max(2).min(150)
        };
        let mut high = max_possible;
        let mut best_fit = low;

        if high < low {
            best_fit = low;
        } else {
            while low <= high {
                let mid = (low + high) / 2;
                let (h, w) = measure_text_bounds(cache_dc, &mut buf, mid, available_w);
                if h <= available_h && w <= available_w {
                    best_fit = mid;
                    low = mid + 1;
                } else {
                    high = mid - 1;
                }
            }
        }
        let font_size_val = best_fit;

        let font_weight = if snapshot.is_refining { FW_NORMAL } else { FW_MEDIUM };
        let hfont = CreateFontW(
            font_size_val,
            0,
            0,
            0,
            font_weight.0 as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET,
            OUT_DEFAULT_PRECIS,
            CLIP_DEFAULT_PRECIS,
            CLEARTYPE_QUALITY,
            (VARIABLE_PITCH.0 | FF_SWISS.0) as u32,
            w!("Google Sans Flex"),
        );
        let old_font = SelectObject(cache_dc, hfont.into());

        let mut measure_rect = RECT {
            left: 0,
            top: 0,
            right: available_w,
            bottom: 0,
        };
        DrawTextW(
            cache_dc,
            &mut buf,
            &mut measure_rect,
            DT_CALCRECT | DT_WORDBREAK | DT_EDITCONTROL,
        );
        let text_h = measure_rect.bottom;

        let offset_y = ((height - text_h) / 2).max(0);
        let mut draw_rect = RECT {
            left: h_padding,
            top: offset_y,
            right: width - h_padding,
            bottom: height,
        };

        let draw_flags = if snapshot.is_refining {
            DT_CENTER | DT_WORDBREAK | DT_EDITCONTROL
        } else {
            DT_LEFT | DT_WORDBREAK | DT_EDITCONTROL
        };
        DrawTextW(cache_dc, &mut buf, &mut draw_rect as *mut _, draw_flags);

        SelectObject(cache_dc, old_font);
        let _ = DeleteObject(hfont.into());
        SelectObject(cache_dc, old_cache_bm);
        let _ = DeleteDC(cache_dc);

        let mut states = WINDOW_STATES.lock().unwrap();
        if let Some(state) = states.get_mut(&(hwnd.0 as isize)) {
            state.content_bitmap = cached_text_bm;
            state.cached_font_size = font_size_val;
            state.font_cache_dirty = false;
        }
    }

    if !cached_text_bm.is_invalid() {
        let cache_dc = CreateCompatibleDC(Some(hdc));
        let old_cbm = SelectObject(cache_dc, cached_text_bm.into());
        let _ = BitBlt(mem_dc, 0, 0, width, height, Some(cache_dc), 0, 0, SRCCOPY).ok();
        SelectObject(cache_dc, old_cbm);
        let _ = DeleteDC(cache_dc);
    }

    cached_text_bm
}
