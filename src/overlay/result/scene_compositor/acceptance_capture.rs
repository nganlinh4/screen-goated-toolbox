use image::{GenericImageView, ImageFormat};
use webview2_com::CapturePreviewCompletedHandler;
use webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG;
use windows061::Win32::Foundation::HGLOBAL;
use windows061::Win32::System::Com::StructuredStorage::CreateStreamOnHGlobal;
use windows061::Win32::System::Com::{IStream, STATFLAG_NONAME, STREAM_SEEK_SET};
use wry::WebViewExtWindows;

const AUTHORED_PIXEL: [u8; 3] = [0x0d, 0xb1, 0x5b];
const MIN_AUTHORED_PIXELS: usize = 4_096;

pub(in crate::overlay::result) fn capture_for_trace(webview: &wry::WebView, trace_id: String) {
    if !super::acceptance_offscreen() {
        return;
    }
    let result = start_capture_with(webview, "result-navigation", move |result| match result {
        Ok(pixel_count) => {
            crate::overlay::result::latency::mark(&trace_id, "interactive_pixels_visible");
            crate::debug_log::log_debug(&format!(
                "[OverlaySmoke] phase=interactive_pixels_visible pixels={pixel_count}"
            ));
        }
        Err(error) => crate::debug_log::log_debug(&format!(
            "[OverlaySmoke] phase=interactive_pixels_rejected error={error}"
        )),
    });
    if let Err(error) = result {
        crate::debug_log::log_debug(&format!(
            "[OverlaySmoke] phase=interactive_pixels_rejected error={error}"
        ));
    }
}

pub(super) fn capture_for_card(webview: &wry::WebView, id: isize) {
    if !super::acceptance_offscreen() {
        return;
    }
    let complete = move |result| match result {
        Ok(pixel_count) => {
            crate::debug_log::log_debug(&format!(
                "[OverlaySmoke] phase=interactive_pixels_visible pixels={pixel_count}"
            ));
            emit_card_capture(id, "interactive_pixels_visible", None);
        }
        Err(error) => {
            crate::debug_log::log_debug(&format!(
                "[OverlaySmoke] phase=interactive_pixels_rejected error={error}"
            ));
            emit_card_capture(id, "interactive_pixels_rejected", Some(error));
        }
    };
    if let Err(error) = start_capture_with(webview, "result-compositor", complete) {
        let error = error.to_string();
        crate::debug_log::log_debug(&format!(
            "[OverlaySmoke] phase=interactive_pixels_rejected error={error}"
        ));
        emit_card_capture(id, "interactive_pixels_rejected", Some(error));
    }
}

fn emit_card_capture(id: isize, phase: &str, error: Option<String>) {
    super::child::emit_event(super::protocol::ChildEvent::CardDiagnostic {
        id,
        phase: phase.to_string(),
        revision: 0,
        visible: true,
        ready: true,
        payload_len: 0,
        text_len: 0,
        opacity: String::new(),
        error,
    });
}

fn start_capture_with(
    webview: &wry::WebView,
    evidence_name: &'static str,
    complete: impl FnOnce(Result<usize, String>) + 'static,
) -> windows061::core::Result<()> {
    unsafe {
        let stream = CreateStreamOnHGlobal(HGLOBAL::default(), true)?;
        let completion_stream = stream.clone();
        let complete = std::sync::Arc::new(std::sync::Mutex::new(Some(complete)));
        let handler = CapturePreviewCompletedHandler::create(Box::new(move |code| {
            let result = if code.is_err() {
                Err(format!("CapturePreview failed: {code:?}"))
            } else {
                verify_pixels(&completion_stream, evidence_name)
            };
            if let Ok(mut complete) = complete.lock()
                && let Some(complete) = complete.take()
            {
                complete(result);
            }
            Ok(())
        }));
        webview.controller().CoreWebView2()?.CapturePreview(
            COREWEBVIEW2_CAPTURE_PREVIEW_IMAGE_FORMAT_PNG,
            &stream,
            &handler,
        )
    }
}

fn verify_pixels(stream: &IStream, evidence_name: &str) -> Result<usize, String> {
    let bytes = read_stream(stream)?;
    let evidence_dir = crate::paths::app_sgt_dir().join("acceptance");
    let _ = std::fs::create_dir_all(&evidence_dir);
    let _ = std::fs::write(evidence_dir.join(format!("{evidence_name}.png")), &bytes);
    let image = image::load_from_memory_with_format(&bytes, ImageFormat::Png)
        .map_err(|error| format!("invalid preview PNG: {error}"))?;
    let dimensions = image.dimensions();
    let mut nontransparent = 0_usize;
    let mut opaque = 0_usize;
    let authored_pixels = image
        .pixels()
        .filter(|(_, _, pixel)| {
            if pixel[3] > 0 {
                nontransparent += 1;
            }
            if pixel[3] == u8::MAX {
                opaque += 1;
            }
            pixel[0].abs_diff(AUTHORED_PIXEL[0]) <= 2
                && pixel[1].abs_diff(AUTHORED_PIXEL[1]) <= 2
                && pixel[2].abs_diff(AUTHORED_PIXEL[2]) <= 2
        })
        .count();
    if authored_pixels < MIN_AUTHORED_PIXELS {
        return Err(format!(
            "preview={}x{} nontransparent={nontransparent} opaque={opaque} authored={authored_pixels}, expected authored>={MIN_AUTHORED_PIXELS}",
            dimensions.0, dimensions.1
        ));
    }
    Ok(authored_pixels)
}

fn read_stream(stream: &IStream) -> Result<Vec<u8>, String> {
    unsafe {
        let mut stat = std::mem::zeroed();
        stream
            .Stat(&raw mut stat, STATFLAG_NONAME)
            .map_err(|error| format!("preview stream stat failed: {error}"))?;
        let size =
            usize::try_from(stat.cbSize).map_err(|_| "preview stream is too large".to_string())?;
        if size == 0 || size > 64 * 1024 * 1024 {
            return Err(format!("preview stream size is invalid: {size}"));
        }
        stream
            .Seek(0, STREAM_SEEK_SET, None)
            .map_err(|error| format!("preview stream seek failed: {error}"))?;
        let mut bytes = vec![0_u8; size];
        let mut read = 0_u32;
        let read_status = stream.Read(
            bytes.as_mut_ptr().cast(),
            u32::try_from(size).map_err(|_| "preview stream exceeds u32".to_string())?,
            Some(&raw mut read),
        );
        if read_status.is_err() {
            return Err(format!("preview stream read failed: {read_status:?}"));
        }
        bytes.truncate(read as usize);
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::{AUTHORED_PIXEL, MIN_AUTHORED_PIXELS};

    #[test]
    fn authored_pixel_contract_is_distinct_and_substantial() {
        assert_eq!(AUTHORED_PIXEL, [13, 177, 91]);
        assert!(std::hint::black_box(MIN_AUTHORED_PIXELS) >= 4_096);
    }
}
