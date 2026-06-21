//! Shared low-level session primitives for the Computer Control runtime and
//! probe: API-key loading, WebSocket connect (endpoint version overridable via
//! `CC_WS_BASE`), JSON send, and screenshot capture.

use std::io::Cursor;
use std::time::Duration;

use anyhow::{Context, Result};
use base64::{Engine as _, engine::general_purpose};
use tungstenite::Message;

pub(super) type Sock = tungstenite::WebSocket<native_tls::TlsStream<std::net::TcpStream>>;

/// Pixel dimensions of the (downscaled) frame sent to the model. Kept for
/// logging/debug only — coordinate mapping is 0-1000 normalized and does not
/// depend on these.
#[derive(Debug, Clone, Copy)]
pub(super) struct FrameGeometry {
    pub frame_w: u32,
    pub frame_h: u32,
}

/// Longest screenshot edge sent to the model (keeps JPEG + token cost sane).
/// Overridable via `CC_MAX_DIM` for fidelity experiments.
fn max_frame_dim() -> u32 {
    std::env::var("CC_MAX_DIM")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&d| d >= 256)
        .unwrap_or(1536)
}

/// Prefer the `GEMINI_API_KEY` env var (repo `.env`); fall back to app config.
pub(super) fn load_key() -> Result<String> {
    let key = std::env::var("GEMINI_API_KEY")
        .ok()
        .map(|k| k.trim().to_string())
        .filter(|k| !k.is_empty())
        .unwrap_or_else(|| crate::load_config().gemini_api_key.trim().to_string());
    if key.is_empty() {
        anyhow::bail!("no Gemini key (set GEMINI_API_KEY or configure it in app settings)");
    }
    Ok(key)
}

/// Connect like `realtime_audio::websocket::connect_websocket`, but with the WS
/// base URL overridable via `CC_WS_BASE` (for testing v1alpha vs v1beta).
pub(super) fn connect_ws(api_key: &str) -> Result<Sock> {
    use std::net::ToSocketAddrs;
    let base = std::env::var("CC_WS_BASE").unwrap_or_else(|_| {
        crate::api::realtime_audio::websocket::GEMINI_LIVE_WS_BASE_URL.to_string()
    });
    let ws_url = format!("{base}?key={api_key}");
    let url = url::Url::parse(&ws_url)?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("no host in WS url"))?
        .to_string();
    let addr = format!("{host}:443")
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| anyhow::anyhow!("failed to resolve {host}"))?;
    let tcp = std::net::TcpStream::connect_timeout(&addr, Duration::from_secs(10))?;
    tcp.set_read_timeout(Some(Duration::from_secs(30)))?;
    tcp.set_write_timeout(Some(Duration::from_secs(30)))?;
    tcp.set_nodelay(true)?;
    let connector = native_tls::TlsConnector::new()?;
    let tls = connector.connect(&host, tcp)?;
    let (socket, _resp) = tungstenite::client::client(&ws_url, tls)?;
    Ok(socket)
}

pub(super) fn send(socket: &mut Sock, value: serde_json::Value) -> Result<()> {
    socket.write(Message::Text(value.to_string().into()))?;
    socket.flush()?;
    Ok(())
}

/// Capture the whole virtual screen, downscale to [`MAX_FRAME_DIM`], and return
/// (base64 JPEG, geometry).
pub(super) fn capture_frame() -> Result<(String, FrameGeometry)> {
    let (jpeg, geom) = capture_frame_jpeg()?;
    Ok((general_purpose::STANDARD.encode(jpeg), geom))
}

/// Like [`capture_frame`] but returns the raw JPEG bytes (so callers can also
/// save the exact frame the model sees to disk for visual debugging).
pub(super) fn capture_frame_jpeg() -> Result<(Vec<u8>, FrameGeometry)> {
    let cap = capture_virtual()?;
    let mut dynimg = image::DynamicImage::ImageRgb8(cap.rgb);
    let max_dim = max_frame_dim();
    let longest = dynimg.width().max(dynimg.height());
    if longest > max_dim {
        let scale = max_dim as f32 / longest as f32;
        let nw = (dynimg.width() as f32 * scale).round().max(1.0) as u32;
        let nh = (dynimg.height() as f32 * scale).round().max(1.0) as u32;
        dynimg = dynimg.resize(nw, nh, image::imageops::FilterType::Triangle);
    }
    let geom = FrameGeometry {
        frame_w: dynimg.width(),
        frame_h: dynimg.height(),
    };
    Ok((encode_jpeg(&dynimg)?, geom))
}

/// The whole virtual screen as RGB, with its top-left origin in screen pixels.
pub(super) struct Capture {
    pub rgb: image::RgbImage,
    pub origin_x: i32,
    pub origin_y: i32,
}

/// A screen-pixel rectangle: the region the model is currently shown. All of the
/// model's `click_at` / `zoom` coordinates (0-1000) are relative to THIS region.
#[derive(Clone, Copy, Debug)]
pub(super) struct View {
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
}

impl View {
    /// Map a 0-1000 coordinate over this view to an absolute screen pixel.
    pub fn to_screen_px(self, mx: f64, my: f64) -> (i32, i32) {
        (
            self.x + (mx / 1000.0 * self.w as f64).round() as i32,
            self.y + (my / 1000.0 * self.h as f64).round() as i32,
        )
    }
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
    for px in bgra.chunks_exact(4) {
        rgb.push(px[2]); // R
        rgb.push(px[1]); // G
        rgb.push(px[0]); // B
    }
    let img = image::RgbImage::from_raw(w as u32, h as u32, rgb)
        .ok_or_else(|| anyhow::anyhow!("rgb buffer size mismatch"))?;
    let (ox, oy) = unsafe {
        (
            GetSystemMetrics(SM_XVIRTUALSCREEN),
            GetSystemMetrics(SM_YVIRTUALSCREEN),
        )
    };
    Ok(Capture {
        rgb: img,
        origin_x: ox,
        origin_y: oy,
    })
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
) -> Result<(Vec<u8>, View)> {
    let iw = cap.rgb.width() as i32;
    let ih = cap.rgb.height() as i32;
    // View -> image coords (subtract virtual origin), clamped to the image.
    let mut x = (view.x - cap.origin_x).clamp(0, iw.saturating_sub(1).max(0));
    let mut y = (view.y - cap.origin_y).clamp(0, ih.saturating_sub(1).max(0));
    let mut w = view.w.clamp(1, iw - x);
    let mut h = view.h.clamp(1, ih - y);
    if w < 8 || h < 8 {
        // Degenerate view -> fall back to the whole screen.
        x = 0;
        y = 0;
        w = iw;
        h = ih;
    }
    let clamped = View {
        x: cap.origin_x + x,
        y: cap.origin_y + y,
        w,
        h,
    };
    let sub = image::imageops::crop_imm(&cap.rgb, x as u32, y as u32, w as u32, h as u32).to_image();
    let mut dynimg = image::DynamicImage::ImageRgb8(sub);
    let short = dynimg.width().min(dynimg.height());
    if short > max_short {
        let scale = max_short as f32 / short as f32;
        let nw = (dynimg.width() as f32 * scale).round().max(1.0) as u32;
        let nh = (dynimg.height() as f32 * scale).round().max(1.0) as u32;
        dynimg = dynimg.resize(nw, nh, image::imageops::FilterType::Triangle);
    }
    let mut rgb = dynimg.to_rgb8();
    if let Some(g) = grid {
        g.draw(&mut rgb);
    }
    // Mark exactly where the last click landed, mapping screen px -> frame px via
    // the clamped view (the screen rect this image represents).
    if let Some((sx, sy)) = marker {
        let fx = ((sx - clamped.x) as f64 / clamped.w.max(1) as f64 * rgb.width() as f64).round() as i32;
        let fy = ((sy - clamped.y) as f64 / clamped.h.max(1) as f64 * rgb.height() as f64).round() as i32;
        if fx >= 0 && fy >= 0 && (fx as u32) < rgb.width() && (fy as u32) < rgb.height() {
            super::grid::draw_click_marker(&mut rgb, fx, fy);
        }
    }
    Ok((encode_jpeg(&image::DynamicImage::ImageRgb8(rgb))?, clamped))
}

/// A tiny 32x32 grayscale fingerprint of the CLEAN view region (no grid/marker
/// overlay), for detecting whether the on-screen content actually changed after
/// an action — the only "did it register?" signal for canvas content the UIA tree
/// can't see.
pub(super) fn view_fingerprint(cap: &Capture, view: View) -> Vec<u8> {
    let iw = cap.rgb.width() as i32;
    let ih = cap.rgb.height() as i32;
    let x = (view.x - cap.origin_x).clamp(0, iw.saturating_sub(1).max(0));
    let y = (view.y - cap.origin_y).clamp(0, ih.saturating_sub(1).max(0));
    let w = view.w.clamp(1, iw - x);
    let h = view.h.clamp(1, ih - y);
    let sub = image::imageops::crop_imm(&cap.rgb, x as u32, y as u32, w as u32, h as u32).to_image();
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
    let v = View { x: cx - half, y: cy - half, w: half * 2, h: half * 2 };
    view_fingerprint(cap, v)
}

/// Capture the screen now and fingerprint the box around (cx, cy) — the "before"
/// snapshot taken just prior to a click.
pub(super) fn capture_region_fp(cx: i32, cy: i32, half: i32) -> Option<Vec<u8>> {
    capture_virtual().ok().map(|cap| region_fingerprint(&cap, cx, cy, half))
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

fn encode_jpeg(img: &image::DynamicImage) -> Result<Vec<u8>> {
    let mut buf = Cursor::new(Vec::new());
    img.write_to(&mut buf, image::ImageFormat::Jpeg)
        .context("jpeg encode")?;
    Ok(buf.into_inner())
}
