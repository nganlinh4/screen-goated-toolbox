use std::path::Path;

pub(super) const MAX_INPUT_BYTES: u64 = 4_490_000;

pub(super) fn error(path: &Path) -> Option<&'static str> {
    if std::fs::metadata(path).is_ok_and(|metadata| metadata.len() > MAX_INPUT_BYTES) {
        return Some("image_too_large");
    }
    crate::overlay::creation_source::inspect_image(path)
        .err()
        .map(|_| "image_invalid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shared_parity_svg_input_limits_are_checked_on_actual_files() {
        let fixture: serde_json::Value = serde_json::from_str(include_str!(
            "../../../parity-fixtures/image-to-svg/input-contract.json"
        ))
        .unwrap();
        assert_eq!(
            fixture["windows"]["maximumBytes"].as_u64().unwrap(),
            MAX_INPUT_BYTES
        );
        struct Scratch {
            root: std::path::PathBuf,
            files: Vec<std::path::PathBuf>,
        }
        impl Drop for Scratch {
            fn drop(&mut self) {
                for path in &self.files {
                    std::fs::remove_file(path).unwrap();
                }
                std::fs::remove_dir(&self.root).unwrap();
            }
        }
        let root = std::env::temp_dir().join(format!(
            "sgt-svg-parity-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&root).unwrap();
        let mut scratch = Scratch {
            root,
            files: Vec::new(),
        };
        for (index, case) in fixture["cases"].as_array().unwrap().iter().enumerate() {
            let path = scratch.root.join(format!("input-{index}.png"));
            let mut file = std::fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .unwrap();
            scratch.files.push(path.clone());
            let width = case["width"].as_u64().unwrap() as u32;
            let height = case["height"].as_u64().unwrap() as u32;
            if width > 0 && height > 0 {
                let mut encoded = std::io::Cursor::new(Vec::new());
                image::DynamicImage::ImageRgba8(image::RgbaImage::new(width, height))
                    .write_to(&mut encoded, image::ImageFormat::Png)
                    .unwrap();
                std::io::Write::write_all(&mut file, encoded.get_ref()).unwrap();
            }
            file.set_len(case["bytes"].as_u64().unwrap()).unwrap();
            drop(file);
            assert_eq!(error(&path), case["error"].as_str(), "{case}");
        }
    }
}
