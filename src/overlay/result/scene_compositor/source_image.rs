//! Copies the original capture with source-relative, settled translation pixels.
use super::protocol::{HostCommand, SceneRect};
use anyhow::{Context, Result, ensure};
use base64::Engine as _;
use serde::{Deserialize, Serialize};
use std::sync::{
    Arc, LazyLock, Mutex,
    atomic::{AtomicU64, Ordering},
};
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct Cell {
    pub id: isize,
    pub rect: SceneRect,
    pub segments: Vec<String>,
}

pub(super) struct Snapshot {
    pub image: Arc<image::RgbaImage>,
    pub cells: Vec<Cell>,
    pub opacity: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RenderRequest {
    pub id: isize,
    pub token: u64,
    pub source: String,
    pub width: u32,
    pub height: u32,
    pub cells: Vec<Cell>,
    pub opacity: u8,
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
pub struct RenderResult {
    pub id: isize,
    pub token: u64,
    pub png: Option<String>,
    pub error: Option<String>,
}

struct Pending {
    id: isize,
    token: u64,
    dimensions: (u32, u32),
    started: Instant,
}
static PENDING: LazyLock<Mutex<Option<Pending>>> = LazyLock::new(|| Mutex::new(None));
static NEXT_TOKEN: AtomicU64 = AtomicU64::new(1);
const MAX_PNG_BYTES: usize = 64 * 1024 * 1024;
const TIMEOUT: Duration = Duration::from_secs(30);

pub(super) fn cancel(id: isize) {
    let mut pending = PENDING.lock().unwrap();
    if pending.as_ref().is_some_and(|value| value.id == id) {
        pending.take();
    }
}

fn current(id: isize, token: u64) -> bool {
    PENDING
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|value| value.id == id && value.token == token)
}

pub(super) fn request(id: isize) {
    let Some(snapshot) = super::scene_groups::image_snapshot(id) else {
        status(id, false);
        return;
    };
    let token = NEXT_TOKEN.fetch_add(1, Ordering::Relaxed);
    let previous = PENDING.lock().unwrap().replace(Pending {
        id,
        token,
        dimensions: snapshot.image.dimensions(),
        started: Instant::now(),
    });
    if let Some(previous) = previous {
        status(previous.id, false);
    }
    std::thread::spawn(move || {
        match render_request(id, token, snapshot) {
            Ok(request) if current(id, token) => {
                super::delivery::send_command(HostCommand::SourceImage { request })
            }
            Ok(_) => return,
            Err(error) => {
                failed(id, token, &error.to_string());
                return;
            }
        }
        while current(id, token) {
            let expired = PENDING
                .lock()
                .unwrap()
                .as_ref()
                .is_some_and(|value| value.started.elapsed() >= TIMEOUT);
            if expired {
                failed(id, token, "image export timed out");
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}

fn render_request(id: isize, token: u64, snapshot: Snapshot) -> Result<RenderRequest> {
    use image::ImageEncoder;
    ensure!(
        !snapshot.cells.is_empty(),
        "no translated cells are available"
    );
    let (width, height) = snapshot.image.dimensions();
    let mut png = Vec::new();
    image::codecs::png::PngEncoder::new_with_quality(
        &mut png,
        image::codecs::png::CompressionType::Fast,
        image::codecs::png::FilterType::Sub,
    )
    .write_image(
        snapshot.image.as_raw(),
        width,
        height,
        image::ExtendedColorType::Rgba8,
    )?;
    ensure!(
        png.len() <= MAX_PNG_BYTES,
        "source image exceeds clipboard export limit"
    );
    Ok(RenderRequest {
        id,
        token,
        source: format!(
            "data:image/png;base64,{}",
            base64::engine::general_purpose::STANDARD.encode(png)
        ),
        width,
        height,
        cells: snapshot.cells,
        opacity: snapshot.opacity,
    })
}

pub(super) fn complete(result: RenderResult) {
    std::thread::spawn(move || {
        if !current(result.id, result.token) {
            return;
        }
        let decoded = (|| -> Result<Vec<u8>> {
            ensure!(
                result.error.is_none(),
                "{}",
                result.error.as_deref().unwrap_or_default()
            );
            let encoded = result
                .png
                .as_deref()
                .context("renderer returned no image")?;
            ensure!(
                encoded.len() <= MAX_PNG_BYTES.div_ceil(3) * 4,
                "export image is too large"
            );
            let bytes = base64::engine::general_purpose::STANDARD.decode(encoded)?;
            let dimensions = image::ImageReader::with_format(
                std::io::Cursor::new(&bytes),
                image::ImageFormat::Png,
            )
            .into_dimensions()?;
            let pending = PENDING.lock().unwrap();
            ensure!(
                pending.as_ref().is_some_and(
                    |value| value.token == result.token && value.dimensions == dimensions
                ),
                "image export identity or dimensions changed"
            );
            Ok(bytes)
        })();
        let bytes = match decoded {
            Ok(bytes) => bytes,
            Err(error) => {
                failed(result.id, result.token, &error.to_string());
                return;
            }
        };
        // The last requested copy owns the clipboard; late results never replace it.
        let mut pending = PENDING.lock().unwrap();
        if !pending
            .as_ref()
            .is_some_and(|value| value.id == result.id && value.token == result.token)
        {
            return;
        }
        if !super::scene_groups::has_source_image(result.id) {
            pending.take();
            return;
        }
        let copied = crate::overlay::utils::try_copy_image_to_clipboard(
            &bytes,
            windows::Win32::Foundation::HWND(result.id as _),
        );
        pending.take();
        drop(pending);
        status(result.id, copied.is_ok());
        match copied {
            Ok(()) => crate::overlay::auto_copy_badge::show_auto_copy_badge_image(),
            Err(error) => report_error(&error.to_string()),
        }
    });
}

fn status(id: isize, success: bool) {
    super::delivery::send_command(HostCommand::SourceImageStatus { id, success });
}

fn failed(id: isize, token: u64, error: &str) {
    let mut pending = PENDING.lock().unwrap();
    if !pending
        .as_ref()
        .is_some_and(|value| value.id == id && value.token == token)
    {
        return;
    }
    pending.take();
    drop(pending);
    status(id, false);
    report_error(error);
}

fn report_error(error: &str) {
    crate::log_info!("[SourceImage] copy failed: {error}");
    let language = crate::APP.lock().unwrap().config.ui_language.clone();
    crate::overlay::auto_copy_badge::show_error_notification(
        crate::gui::locale::LocaleText::get(&language)
            .overlay
            .overlay_copy_image_failed,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn export_encodes_owned_source_and_original_cell_coordinates() {
        let image = Arc::new(image::RgbaImage::from_pixel(
            32,
            24,
            image::Rgba([12, 34, 56, 255]),
        ));
        let cell = Cell {
            id: -1,
            rect: SceneRect {
                x: 3,
                y: 5,
                width: 20,
                height: 10,
            },
            segments: vec!["Translated text".into()],
        };
        let request = render_request(
            42,
            7,
            Snapshot {
                image: image.clone(),
                cells: vec![cell.clone()],
                opacity: 65,
            },
        )
        .unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(
                request
                    .source
                    .strip_prefix("data:image/png;base64,")
                    .unwrap(),
            )
            .unwrap();
        assert_eq!(image::load_from_memory(&bytes).unwrap().to_rgba8(), *image);
        assert_eq!(request.cells, vec![cell]);
        assert_eq!(
            (
                request.id,
                request.token,
                request.width,
                request.height,
                request.opacity
            ),
            (42, 7, 32, 24, 65)
        );
    }
}
