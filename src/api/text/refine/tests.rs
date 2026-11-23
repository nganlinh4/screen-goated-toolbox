use super::{RefineCatalogState, RefineTextOnlyRequest, refine_text_only};
use std::io::{BufRead, Read, Write};
use std::net::TcpListener;
use std::time::{Duration, Instant};

fn local_server() -> (String, std::thread::JoinHandle<serde_json::Value>) {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let base_url = format!("http://{}", listener.local_addr().unwrap());
    let server = std::thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error)
                    if error.kind() == std::io::ErrorKind::WouldBlock
                        && Instant::now() < deadline =>
                {
                    std::thread::sleep(Duration::from_millis(5));
                }
                Err(error) => panic!("local request did not arrive: {error}"),
            }
        };
        stream.set_nonblocking(false).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut reader = std::io::BufReader::new(&mut stream);
        let mut line = String::new();
        reader.read_line(&mut line).unwrap();
        assert!(line.starts_with("POST /api/generate "));
        let mut length = 0;
        loop {
            line.clear();
            assert!(reader.read_line(&mut line).unwrap() > 0);
            if line == "\r\n" {
                break;
            }
            if let Some((name, value)) = line.split_once(':')
                && name.eq_ignore_ascii_case("content-length")
            {
                length = value.trim().parse::<usize>().unwrap();
            }
        }
        let mut body = vec![0; length];
        reader.read_exact(&mut body).unwrap();
        let request: serde_json::Value = serde_json::from_slice(&body).unwrap();
        let body = r#"{"response":"refined","done":true}"#;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        request
    });
    (base_url, server)
}

#[test]
fn local_refinement_uses_only_the_configured_ollama_endpoint() {
    let (base_url, server) = local_server();
    let mut on_chunk = |_: &str| {};
    let result = refine_text_only(RefineTextOnlyRequest {
        groq_api_key: "unused",
        gemini_api_key: "",
        openrouter_api_key: "",
        nvidia_api_key: "",
        ollama_base_url: &base_url,
        final_prompt: "private content",
        model: "local-model".into(),
        provider: "ollama".into(),
        streaming_enabled: false,
        ui_language: "en",
        cancel_token: &None,
        on_chunk: &mut on_chunk,
    });
    let request = server.join().unwrap();
    assert_eq!(request["model"], "local-model");
    assert_eq!(request["prompt"], "private content");
    assert_eq!(result.unwrap(), "refined");
}

#[test]
fn non_llm_image_refinement_uses_preferred_text_model_without_uploading_image() {
    let (base_url, server) = local_server();
    let catalog = RefineCatalogState {
        ollama_base_url: base_url,
        use_ollama: true,
        text_priority: vec!["test-local".into()],
        custom_models: vec![crate::config::types::CustomModelDefinition {
            id: "test-local".into(),
            provider: "ollama".into(),
            full_name: "local-model".into(),
            model_type: crate::config::types::CustomModelType::Text,
            ..Default::default()
        }],
        ..Default::default()
    };
    let mut selected = None;
    let result = super::refine_text_with_catalog(
        super::RefineTextRequest {
            groq_api_key: "unused",
            gemini_api_key: "",
            context: crate::overlay::result::RefineContext::Image(vec![1, 2, 3]),
            previous_text: "decoded content".into(),
            user_prompt: "summarize".into(),
            original_model_id: "qrserver-qr-scanner-vision",
            original_provider: "qrserver",
            streaming_enabled: false,
            ui_language: "en",
            cancel_token: None,
            on_model_selected: &mut |id, name, provider| {
                selected = Some((id.to_owned(), name.to_owned(), provider.to_owned()));
            },
        },
        &catalog,
        |_| {},
    );
    let request = server.join().unwrap();
    assert_eq!(result.unwrap(), "refined");
    assert_eq!(request["model"], "local-model");
    assert!(
        request["prompt"]
            .as_str()
            .unwrap()
            .contains("decoded content")
    );
    assert!(request.get("images").is_none());
    assert_eq!(
        selected,
        Some(("test-local".into(), "local-model".into(), "ollama".into()))
    );
}

#[test]
fn refinement_does_not_route_an_unresolved_model_to_another_provider() {
    let catalog = RefineCatalogState::default();
    let mut selected = false;
    let result = super::refine_text_with_catalog(
        super::RefineTextRequest {
            groq_api_key: "test-key",
            gemini_api_key: "test-key",
            context: crate::overlay::result::RefineContext::None,
            previous_text: "private content".into(),
            user_prompt: "summarize".into(),
            original_model_id: "unresolved-model",
            original_provider: "ollama",
            streaming_enabled: false,
            ui_language: "en",
            cancel_token: None,
            on_model_selected: &mut |_, _, _| {
                selected = true;
            },
        },
        &catalog,
        |_| panic!("unresolved model must not dispatch"),
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("MODEL_UNAVAILABLE")
    );
    assert!(!selected);
}

#[test]
fn unsupported_providers_do_not_fall_through_to_groq() {
    let mut on_chunk = |_: &str| {};
    let result = refine_text_only(RefineTextOnlyRequest {
        groq_api_key: "unused",
        gemini_api_key: "",
        openrouter_api_key: "",
        nvidia_api_key: "",
        ollama_base_url: "http://127.0.0.1:1",
        final_prompt: "private content",
        model: "model".into(),
        provider: "future-provider".into(),
        streaming_enabled: false,
        ui_language: "en",
        cancel_token: &None,
        on_chunk: &mut on_chunk,
    });
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unsupported refinement provider")
    );
    assert!(!super::original_model_can_refine_text(
        "nonempty",
        "google-gtx"
    ));
    assert!(super::original_model_is_non_llm("nonempty", "qrserver"));
    assert!(super::original_model_can_refine_text(
        "local-model",
        "ollama"
    ));
}

#[test]
fn unsupported_vision_provider_does_not_fall_through_to_groq() {
    let result = crate::api::vision::translate_image_streaming(
        crate::api::TranslateImageRequest {
            groq_api_key: "unused",
            gemini_api_key: "",
            prompt: "private content".into(),
            model: "model".into(),
            provider: "future-provider".into(),
            image: image::RgbaImage::new(1, 1),
            original_bytes: None,
            streaming_enabled: false,
            response_schema: None,
            cancel_token: None,
            request_timeout: None,
        },
        |_| {},
    );
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("Unsupported vision provider")
    );
}

#[test]
fn refinement_uses_the_encoded_image_format_for_its_mime_type() {
    assert_eq!(
        super::original_image_mime_type(b"\x89PNG\r\n\x1a\n").unwrap(),
        "image/png"
    );
    assert_eq!(
        super::original_image_mime_type(b"\xff\xd8\xff\xe0").unwrap(),
        "image/jpeg"
    );
    assert!(super::original_image_mime_type(b"not an image").is_err());
}

#[test]
fn refinement_reports_resolved_identity_before_provider_dispatch() {
    let id = crate::model_config::DEFAULT_IMAGE_MODEL_ID;
    let model = crate::model_config::get_model_by_id_with_custom(id, &[]).unwrap();
    let mut selected = None;
    let result = super::refine_text_streaming(
        super::RefineTextRequest {
            groq_api_key: "",
            gemini_api_key: "",
            context: crate::overlay::result::RefineContext::Image(Vec::new()),
            previous_text: "previous output".to_string(),
            user_prompt: "shorten".to_string(),
            original_model_id: id,
            original_provider: "stale-provider",
            streaming_enabled: false,
            ui_language: "en",
            cancel_token: None,
            on_model_selected: &mut |id, name, provider| {
                selected = Some((id.to_string(), name.to_string(), provider.to_string()));
            },
        },
        |_| panic!("invalid image and missing credentials must not produce output"),
    );
    assert!(result.is_err());
    assert_eq!(selected, Some((model.id, model.full_name, model.provider)));
}

#[test]
fn fallback_selection_follows_priority_and_provider_availability() {
    let mut state = RefineCatalogState::default();
    let mut local = crate::model_config::get_model_by_id_with_custom(
        crate::model_config::DEFAULT_TEXT_MODEL_ID,
        &[],
    )
    .unwrap();
    local.provider = "ollama".to_string();
    state.use_ollama = true;
    assert!(state.provider_is_available(&local, "", "", "", ""));
    state.use_ollama = false;
    assert!(!state.provider_is_available(&local, "", "", "", ""));
    assert!(!state.provider_enabled("ollama"));
    assert_eq!(
        state
            .preferred_text_model("groq", "gemini", "openrouter", "")
            .map(|model| model.0),
        Some(crate::model_config::DEFAULT_TEXT_MODEL_ID.to_string())
    );

    state.use_groq = false;
    assert_eq!(
        state
            .preferred_text_model("", "gemini", "openrouter", "")
            .map(|model| model.0),
        Some("google-gemini-3-5-flash-lite-text".to_string())
    );

    state.use_gemini = false;
    assert_eq!(
        state
            .preferred_text_model("", "", "openrouter", "")
            .map(|model| model.0),
        Some("openrouter-particle-1-text".to_string())
    );
}
