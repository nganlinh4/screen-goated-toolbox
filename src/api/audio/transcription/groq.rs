use crate::api::client::{UREQ_RESPONSE_AGENT, record_usage_simple};

pub(super) fn upload_audio_to_whisper(
    api_key: &str,
    model: &str,
    audio_data: Vec<u8>,
) -> anyhow::Result<String> {
    let boundary = format!(
        "----SGTBoundary{}",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis()
    );

    let mut body = Vec::new();
    add_field(&mut body, &boundary, "model", model.as_bytes());
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        b"Content-Disposition: form-data; name=\"file\"; filename=\"audio.wav\"\r\n",
    );
    body.extend_from_slice(b"Content-Type: audio/wav\r\n\r\n");
    body.extend_from_slice(&audio_data);
    body.extend_from_slice(b"\r\n");
    body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

    let response = UREQ_RESPONSE_AGENT
        .post("https://api.groq.com/openai/v1/audio/transcriptions")
        .header("Authorization", &format!("Bearer {api_key}"))
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body)
        .map_err(|error| anyhow::anyhow!("Groq transcription transport error: {error}"))?;

    record_usage_simple(response.headers(), model);
    if !response.status().is_success() {
        let status = response.status().as_u16();
        if matches!(status, 401 | 403) {
            return Err(anyhow::anyhow!("INVALID_API_KEY"));
        }
        let body = response.into_body().read_to_string().unwrap_or_default();
        return Err(anyhow::anyhow!("Groq transcription HTTP {status}: {body}"));
    }

    let json: serde_json::Value = response
        .into_body()
        .read_json()
        .map_err(|error| anyhow::anyhow!("Failed to parse transcription: {error}"))?;
    json.get("text")
        .and_then(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| anyhow::anyhow!("No text in transcription response"))
}

fn add_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value);
    body.extend_from_slice(b"\r\n");
}
