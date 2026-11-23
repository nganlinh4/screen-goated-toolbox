use super::*;

#[derive(Debug)]
pub(super) struct InvalidImage;

impl std::fmt::Display for InvalidImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Failed to load image")
    }
}

impl std::error::Error for InvalidImage {}

pub(super) fn decode_image(bytes: &[u8]) -> anyhow::Result<image::RgbaImage> {
    use anyhow::Context as _;
    Ok(crate::image_decode::load_from_memory(bytes)
        .context(InvalidImage)?
        .to_rgba8())
}

/// Handle the execution result (success or error).
pub(super) fn handle_execution_result(
    res: anyhow::Result<String>,
    my_hwnd: Option<HWND>,
    window_shown: &Arc<Mutex<bool>>,
    config: &Config,
    model_full_name: &str,
) -> Option<String> {
    match res {
        Ok(txt) => {
            if let Some(h) = my_hwnd {
                crate::overlay::result::latency::mark_window(h, "provider_first_output");
                {
                    let mut s = WINDOW_STATES.lock().unwrap();
                    if let Some(st) = s.get_mut(&(h.0 as isize)) {
                        st.is_refining = false;
                        st.is_streaming_active = false;
                    }
                }
                update_window_text(h, &txt);
                if claim_result_reveal(window_shown) {
                    unsafe {
                        let _ = ShowWindow(h, SW_SHOW);
                    }
                }
            }
            Some(txt)
        }
        Err(e) => {
            crate::overlay::utils::show_api_key_error_notification(
                &e.to_string(),
                &config.ui_language,
            );
            let err = crate::overlay::utils::get_error_message(
                &e.to_string(),
                &config.ui_language,
                Some(model_full_name),
            );
            if let Some(h) = my_hwnd {
                crate::overlay::result::latency::mark_window(h, "provider_first_output");
                // Show window if hidden (image blocks)
                {
                    let mut shown = window_shown.lock().unwrap();
                    if !*shown {
                        *shown = true;
                        unsafe {
                            let _ = ShowWindow(h, SW_SHOW);
                        }
                    }
                }
                {
                    let mut s = WINDOW_STATES.lock().unwrap();
                    if let Some(st) = s.get_mut(&(h.0 as isize)) {
                        st.is_refining = false;
                        st.is_streaming_active = false;
                    }
                }
                update_window_text(h, &err);
            }
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_image_is_a_local_error_and_valid_image_decodes() {
        let err = decode_image(b"invalid image").unwrap_err();
        assert!(err.is::<InvalidImage>());
        let mut encoded = std::io::Cursor::new(Vec::new());
        image::DynamicImage::new_rgba8(1, 1)
            .write_to(&mut encoded, image::ImageFormat::Png)
            .unwrap();
        let decoded = decode_image(encoded.get_ref()).unwrap();
        assert_eq!(decoded.dimensions(), (1, 1));
    }

    #[test]
    fn failed_steps_produce_no_value_for_continuation() {
        let shown = Arc::new(Mutex::new(false));
        let config = Config::default();
        assert!(
            handle_execution_result(Err(InvalidImage.into()), None, &shown, &config, "model")
                .is_none()
        );
        assert_eq!(
            handle_execution_result(Ok("value".into()), None, &shown, &config, "model").as_deref(),
            Some("value")
        );
    }
}
