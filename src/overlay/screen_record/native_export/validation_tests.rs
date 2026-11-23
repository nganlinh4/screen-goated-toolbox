use super::{single_export_extension, validate_background, validate_identifier};
use crate::overlay::screen_record::native_export::config::{BackgroundConfig, ExportConfig};

#[test]
fn identifiers_cannot_escape_owned_directories() {
    for unsafe_id in ["", ".", "..", "../outside", r"..\outside", "C:temp"] {
        assert!(validate_identifier(unsafe_id, "id").is_err());
    }
    assert!(validate_identifier("session_123-abc", "id").is_ok());
}

#[test]
fn legacy_bottom_crop_keeps_a_nonempty_source_rect() {
    let background = |crop_bottom| -> BackgroundConfig {
        serde_json::from_value(serde_json::json!({
            "scale": 100.0,
            "borderRadius": 0.0,
            "cropBottom": crop_bottom,
            "backgroundType": "solid",
            "shadow": 0.0,
            "cursorScale": 1.0
        }))
        .expect("background config parses")
    };
    assert!(validate_background(&background(0.0)).is_ok());
    assert!(validate_background(&background(99.9)).is_ok());
    assert!(validate_background(&background(-0.1)).is_err());
    assert!(validate_background(&background(100.0)).is_err());
}

fn single_export(format: &str) -> ExportConfig {
    serde_json::from_value(serde_json::json!({
        "width": 1280, "height": 720, "framerate": 30,
        "trimStart": 0, "duration": 2, "format": format,
        "sourceVideoPath": "C:/recordings/clip.mp4",
        "segment": {
            "crop": null, "cursorVisibilitySegments": null,
            "trimSegments": [{"startTime": 0.0, "endTime": 2.0}]
        },
        "backgroundConfig": {
            "scale": 90, "borderRadius": 0, "backgroundType": "solid",
            "shadow": 0, "cursorScale": 1
        }
    }))
    .expect("export config parses")
}

#[test]
fn single_export_names_its_file_after_a_real_container() {
    assert_eq!(single_export_extension("mp4"), Ok("mp4"));
    assert_eq!(single_export_extension("gif"), Ok("gif"));
    // "both" only fans out into an MP4 and a GIF through the composition
    // export; the single-clip pipeline would write one H.264 file named .both.
    assert!(single_export_extension("both").is_err());
    assert!(single_export("mp4").validate().is_ok());
    assert!(single_export("gif").validate().is_ok());
    let error = single_export("both").validate().unwrap_err();
    assert!(error.contains("both"), "{error}");
}
