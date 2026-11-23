//! Shared low-level session primitives for the Computer Control runtime and
//! probe: API-key loading, WebSocket connect (endpoint version overridable via
//! `CC_WS_BASE` in debug builds only), JSON send, and screenshot capture.

use anyhow::Result;
use tungstenite::Message;

mod connect;
pub(super) use connect::connect_ws;
mod errors;
mod gpu_patch;
pub(super) use errors::{
    SessionFailureKind, SessionStopped, check_connection_attempt, failure_kind,
};
mod setup;
pub(super) use setup::setup_frame_ready;
mod view;
pub(super) use view::View;

pub(super) type Sock = tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>;

/// Marks an uncertain WebSocket write, independently of local validation errors.
#[derive(Debug)]
pub(super) struct SocketWriteFailure;

impl std::fmt::Display for SocketWriteFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WebSocket write failed; delivery is uncertain")
    }
}

impl std::error::Error for SocketWriteFailure {}

pub(super) fn is_socket_write_failure(error: &anyhow::Error) -> bool {
    error.chain().any(|cause| cause.is::<SocketWriteFailure>())
}

/// Use the selected runtime's credential policy and configuration.
pub(super) fn load_key() -> Result<String> {
    let configured = super::app_config::read(|config| config.gemini_api_key.clone());
    let key = crate::api::provider_credentials::resolve("GEMINI_API_KEY", &configured);
    if key.is_empty() {
        anyhow::bail!("NO_API_KEY:gemini");
    }
    Ok(key)
}

/// Classify only explicit Gemini credential failures. Network, quota, model,
/// and generic setup failures must not be mislabeled as a bad key.
pub(super) fn api_key_error_code(error: &anyhow::Error) -> Option<&'static str> {
    error
        .chain()
        .find_map(|cause| api_key_error_code_text(&cause.to_string()))
}

fn api_key_error_code_text(message: &str) -> Option<&'static str> {
    let lower = message.to_ascii_lowercase();
    if lower.contains("no_api_key") {
        return Some("NO_API_KEY:gemini");
    }
    if lower.contains("invalid_api_key")
        || lower.contains("api key not valid")
        || lower.contains("api key is not valid")
        || lower.contains("invalid api key")
        || lower.contains("api key is invalid")
    {
        return Some("INVALID_API_KEY:gemini");
    }
    None
}

pub(super) fn send(socket: &mut Sock, value: serde_json::Value) -> Result<()> {
    socket
        .write(Message::Text(value.to_string().into()))
        .map_err(anyhow::Error::from)
        .map_err(|error| error.context(SocketWriteFailure))?;
    socket
        .flush()
        .map_err(anyhow::Error::from)
        .map_err(|error| error.context(SocketWriteFailure))?;
    Ok(())
}

/// The whole virtual screen as RGB, with its top-left origin in screen pixels.
pub(super) struct Capture {
    pub rgb: image::RgbImage,
    pub origin_x: i32,
    pub origin_y: i32,
}

/// Capture the whole virtual desktop into an RGB image (GDI BitBlt + GetDIBits).
pub(super) fn capture_virtual() -> Result<Capture> {
    use windows::Win32::Graphics::Gdi::{
        BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, GetDC, GetDIBits, ReleaseDC,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetSystemMetrics, SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN,
    };

    let capture = crate::screen_capture::capture_screen_fast()?;
    let (w, h) = (capture.width, capture.height);
    let mut bgra = vec![0u8; (w as usize) * (h as usize) * 4];

    let lines = unsafe {
        let hdc = GetDC(None);
        let mut bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h, // negative => top-down rows
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let lines = GetDIBits(
            hdc,
            capture.hbitmap,
            0,
            h as u32,
            Some(bgra.as_mut_ptr().cast()),
            &mut bmi,
            DIB_RGB_COLORS,
        );
        let _ = ReleaseDC(None, hdc);
        lines
    };
    if lines == 0 {
        anyhow::bail!("GetDIBits returned 0 lines");
    }

    let mut rgb = Vec::with_capacity((w as usize) * (h as usize) * 3);
    for px in bgra.as_chunks::<4>().0 {
        rgb.push(px[2]); // R
        rgb.push(px[1]); // G
        rgb.push(px[0]); // B
    }
    let mut img = image::RgbImage::from_raw(w as u32, h as u32, rgb)
        .ok_or_else(|| anyhow::anyhow!("rgb buffer size mismatch"))?;
    let (ox, oy) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    };
    // GDI BitBlt returns BLACK for GPU-composited content (browser canvas/WebGL
    // games, hardware video). Best-effort: patch the ACTIVE window's real pixels in.
    gpu_patch::patch_foreground_gpu(&mut img, ox, oy);
    Ok(Capture {
        rgb: img,
        origin_x: ox,
        origin_y: oy,
    })
}

/// Average channel value over a sparse sample of the image rect (image coords).
/// Returns a high value (not-black) when the rect is mostly off-image.
fn region_avg(rgb: &image::RgbImage, x: i32, y: i32, w: i32, h: i32) -> u32 {
    let (iw, ih) = (rgb.width() as i32, rgb.height() as i32);
    let step = (w.max(h) / 24).max(1);
    let (mut sum, mut n) = (0u64, 0u64);
    let mut yy = 0;
    while yy < h {
        let py = y + yy;
        if (0..ih).contains(&py) {
            let mut xx = 0;
            while xx < w {
                let px = x + xx;
                if (0..iw).contains(&px) {
                    let p = rgb.get_pixel(px as u32, py as u32);
                    sum += p[0] as u64 + p[1] as u64 + p[2] as u64;
                    n += 1;
                }
                xx += step;
            }
        }
        yy += step;
    }
    sum.checked_div(n).map_or(255, |average| average as u32)
}

/// Crop a [`Capture`] to `view`, downscale so the SHORT edge is at most
/// `max_short` (never upscale), overlay the Set-of-Mark label `grid`, then
/// JPEG-encode. Returns (jpeg, clamped view) — the clamped view is the exact
/// screen region the image represents (and the grid's coordinate frame).
pub(super) fn encode_view(
    cap: &Capture,
    view: View,
    max_short: u32,
    grid: Option<super::grid::Grid>,
    marker: Option<(i32, i32)>,
    marks: Option<&[(i32, i32, u32)]>,
) -> Result<(Vec<u8>, View)> {
    let clamped = view.clamped_to(cap.bounds()?);
    let sub = crop_capture(cap, clamped);
    let dynimg = super::vision_contract::resize_to_short_edge(
        image::DynamicImage::ImageRgb8(sub),
        max_short,
    );
    let mut rgb = dynimg.to_rgb8();
    if let Some(g) = grid {
        g.draw(&mut rgb);
    }
    if let Some(marks) = marks {
        for &(sx, sy, label) in marks {
            let fx = ((sx - clamped.x) as f64 / clamped.w.max(1) as f64 * rgb.width() as f64)
                .round() as i32;
            let fy = ((sy - clamped.y) as f64 / clamped.h.max(1) as f64 * rgb.height() as f64)
                .round() as i32;
            if fx >= 0 && fy >= 0 && (fx as u32) < rgb.width() && (fy as u32) < rgb.height() {
                super::grid::draw_anchor_marker(&mut rgb, fx, fy, label);
            }
        }
    }
    // Mark exactly where the last click landed, mapping screen px -> frame px via
    // the clamped view (the screen rect this image represents).
    if let Some((sx, sy)) = marker {
        let fx =
            ((sx - clamped.x) as f64 / clamped.w.max(1) as f64 * rgb.width() as f64).round() as i32;
        let fy = ((sy - clamped.y) as f64 / clamped.h.max(1) as f64 * rgb.height() as f64).round()
            as i32;
        if fx >= 0 && fy >= 0 && (fx as u32) < rgb.width() && (fy as u32) < rgb.height() {
            super::grid::draw_click_marker(&mut rgb, fx, fy);
        }
    }
    Ok((
        super::vision_contract::encode_jpeg(&image::DynamicImage::ImageRgb8(rgb))?,
        clamped,
    ))
}

/// A tiny 32x32 grayscale fingerprint of the CLEAN view region (no grid/marker
/// overlay), for detecting whether the on-screen content actually changed after
/// an action — the only "did it register?" signal for canvas content the UIA tree
/// can't see.
pub(super) fn view_fingerprint(cap: &Capture, view: View) -> Vec<u8> {
    let Some(view) = cap
        .bounds()
        .ok()
        .and_then(|bounds| view.intersection(bounds))
    else {
        return Vec::new();
    };
    let sub = crop_capture(cap, view);
    let small = image::imageops::resize(&sub, 32, 32, image::imageops::FilterType::Triangle);
    small
        .pixels()
        .map(|p| {
            let [r, g, b] = p.0;
            ((r as u16 * 30 + g as u16 * 59 + b as u16 * 11) / 100) as u8
        })
        .collect()
}

/// Fingerprint a small box (half-width `half` screen px) centred on a screen
/// pixel — for detecting whether a CLICK changed its own target cell, ignoring a
/// timer/animation elsewhere on screen.
pub(super) fn region_fingerprint(cap: &Capture, cx: i32, cy: i32, half: i32) -> Vec<u8> {
    let v = View {
        x: cx.saturating_sub(half),
        y: cy.saturating_sub(half),
        w: half.saturating_mul(2),
        h: half.saturating_mul(2),
    };
    view_fingerprint(cap, v)
}

/// RGB signature for a small target-local box. Unlike the grayscale action
/// postcondition fingerprint, this preserves state changes expressed only by
/// color and is used to fail closed before a grounded pointer effect.
pub(super) fn target_region_fingerprint(cap: &Capture, cx: i32, cy: i32, half: i32) -> Vec<u8> {
    let requested = View {
        x: cx.saturating_sub(half),
        y: cy.saturating_sub(half),
        w: half.saturating_mul(2),
        h: half.saturating_mul(2),
    };
    let Some(view) = cap
        .bounds()
        .ok()
        .and_then(|bounds| requested.intersection(bounds))
    else {
        return Vec::new();
    };
    let sub = crop_capture(cap, view);
    image::imageops::resize(&sub, 24, 24, image::imageops::FilterType::Triangle)
        .pixels()
        .flat_map(|pixel| pixel.0)
        .collect()
}

pub(super) fn target_fingerprint_matches(left: &[u8], right: &[u8]) -> bool {
    if left.is_empty() || left.len() != right.len() || !left.len().is_multiple_of(3) {
        return false;
    }
    let mut total_delta = 0_u64;
    let mut changed_samples = 0_usize;
    for (left_rgb, right_rgb) in left.as_chunks::<3>().0.iter().zip(right.as_chunks::<3>().0) {
        let delta = left_rgb
            .iter()
            .zip(right_rgb)
            .map(|(left, right)| u32::from(left.abs_diff(*right)))
            .sum::<u32>()
            / 3;
        total_delta += u64::from(delta);
        if delta >= 52 {
            changed_samples += 1;
        }
    }
    let samples = left.len() / 3;
    total_delta <= samples as u64 * 30 && changed_samples * 100 <= samples * 28
}

fn crop_capture(cap: &Capture, view: View) -> image::RgbImage {
    let x = (i64::from(view.x) - i64::from(cap.origin_x)) as u32;
    let y = (i64::from(view.y) - i64::from(cap.origin_y)) as u32;
    image::imageops::crop_imm(&cap.rgb, x, y, view.w as u32, view.h as u32).to_image()
}

/// Capture the screen now and fingerprint the box around (cx, cy) — the "before"
/// snapshot taken just prior to a click.
pub(super) fn capture_region_fp(cx: i32, cy: i32, half: i32) -> Option<Vec<u8>> {
    capture_virtual()
        .ok()
        .map(|cap| region_fingerprint(&cap, cx, cy, half))
}

/// Count of fingerprint cells that changed appreciably between two frames. Robust
/// to cursor blink / JPEG noise (small deltas ignored); a placed game piece or a
/// moved selection lights up enough cells to clear the caller's threshold.
pub(super) fn fingerprint_change(a: &[u8], b: &[u8]) -> u32 {
    if a.len() != b.len() {
        return u32::MAX;
    }
    a.iter()
        .zip(b)
        .filter(|(x, y)| x.abs_diff(**y) > 24)
        .count() as u32
}

#[cfg(test)]
mod view_tests {
    use super::{View, api_key_error_code, target_fingerprint_matches};

    #[test]
    fn classifies_only_explicit_startup_credential_failures() {
        let missing = anyhow::anyhow!("NO_API_KEY:gemini");
        assert_eq!(api_key_error_code(&missing), Some("NO_API_KEY:gemini"));

        let rejected = anyhow::anyhow!(
            "server closed during setup: API key not valid. Please pass a valid API key."
        );
        assert_eq!(
            api_key_error_code(&rejected),
            Some("INVALID_API_KEY:gemini")
        );

        for unrelated in [
            "timed out waiting for setupComplete",
            "HTTP error: 400 Bad Request",
            "quota exceeded",
            "connection reset",
        ] {
            assert_eq!(api_key_error_code(&anyhow::anyhow!(unrelated)), None);
        }
    }

    #[test]
    fn normalized_edges_stay_inside_the_view() {
        let view = View {
            x: -100,
            y: 20,
            w: 10,
            h: 8,
        };
        assert_eq!(view.to_screen_px(0.0, 0.0), (-100, 20));
        assert_eq!(view.to_screen_px(1000.0, 1000.0), (-91, 27));
        assert_eq!(view.to_screen_px(-50.0, 1200.0), (-100, 27));
    }

    #[test]
    fn target_signatures_preserve_color_and_tolerate_small_capture_noise() {
        let baseline = vec![80_u8; 24 * 24 * 3];
        let mut noisy = baseline.clone();
        noisy.iter_mut().for_each(|channel| *channel += 4);
        assert!(target_fingerprint_matches(&baseline, &noisy));

        let mut changed = baseline.clone();
        changed
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .take(200)
            .for_each(|pixel| {
                pixel.copy_from_slice(&[220, 20, 160]);
            });
        assert!(!target_fingerprint_matches(&baseline, &changed));
    }
}
