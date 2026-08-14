use super::config::CropRect;

#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct ExportCropGeometry {
    pub(super) video_width: u32,
    pub(super) video_height: u32,
    pub(super) camera_width: u32,
    pub(super) camera_height: u32,
    pub(super) x_offset: f64,
    pub(super) y_offset: f64,
}

pub(super) fn bottom_crop_factor(crop_bottom_percent: f64) -> f64 {
    (1.0 - crop_bottom_percent / 100.0).clamp(0.0, 1.0)
}

pub(super) fn resolve_export_crop_geometry(
    source_width: u32,
    source_height: u32,
    crop: Option<&CropRect>,
    crop_bottom_percent: f64,
) -> ExportCropGeometry {
    let (x, y, width, height) = crop
        .map(|crop| (crop.x, crop.y, crop.width, crop.height))
        .unwrap_or((0.0, 0.0, 1.0, 1.0));
    let source_width = f64::from(source_width);
    let source_height = f64::from(source_height);
    let camera_width = (source_width * width).max(1.0) as u32;
    let camera_height = (source_height * height).max(1.0) as u32;
    let video_height =
        (source_height * height * bottom_crop_factor(crop_bottom_percent)).max(1.0) as u32;
    ExportCropGeometry {
        video_width: camera_width,
        video_height,
        camera_width,
        camera_height,
        x_offset: source_width * x,
        y_offset: source_height * y,
    }
}

pub(super) fn logical_crop_size(
    capture_width: f64,
    capture_height: f64,
    crop: Option<&CropRect>,
    crop_bottom_percent: f64,
) -> (f64, f64) {
    let (crop_width, crop_height) = crop
        .map(|crop| (crop.width, crop.height))
        .unwrap_or((1.0, 1.0));
    (
        (capture_width * crop_width).max(1.0),
        (capture_height * crop_height * bottom_crop_factor(crop_bottom_percent)).max(1.0),
    )
}

#[cfg(test)]
mod tests {
    use super::{logical_crop_size, resolve_export_crop_geometry};
    use crate::overlay::screen_record::native_export::config::{BackgroundConfig, CropRect};

    #[test]
    fn legacy_bottom_crop_changes_render_source_but_not_camera_coordinate_space() {
        let crop = CropRect {
            x: 0.1,
            y: 0.2,
            width: 0.5,
            height: 0.5,
        };
        let geometry = resolve_export_crop_geometry(1920, 1080, Some(&crop), 25.0);
        assert_eq!(geometry.video_width, 960);
        assert_eq!(geometry.video_height, 405);
        assert_eq!(geometry.camera_height, 540);
        assert_eq!(geometry.x_offset, 192.0);
        assert_eq!(geometry.y_offset, 216.0);
        assert_eq!(
            logical_crop_size(1920.0, 1080.0, Some(&crop), 25.0),
            (960.0, 405.0)
        );
    }

    #[test]
    fn crop_bottom_survives_the_frontend_export_wire_contract() {
        let background: BackgroundConfig = serde_json::from_value(serde_json::json!({
            "scale": 100.0,
            "borderRadius": 0.0,
            "cropBottom": 12.5,
            "backgroundType": "solid",
            "shadow": 0.0,
            "cursorScale": 1.0
        }))
        .expect("background config parses");
        assert_eq!(background.crop_bottom, 12.5);
    }
}
