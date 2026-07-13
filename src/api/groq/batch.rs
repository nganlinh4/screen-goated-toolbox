//! Groq Batch API primitives for asynchronous, non-interactive workloads.

use anyhow::{Context, Result, anyhow};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::api::client::UREQ_AGENT;

const API_ROOT: &str = "https://api.groq.com/openai/v1";

#[derive(Clone, Debug, Serialize)]
pub struct RequestLine {
    pub custom_id: String,
    pub method: String,
    pub url: String,
    pub body: Value,
}

impl RequestLine {
    pub fn chat(custom_id: impl Into<String>, body: Value) -> Self {
        Self {
            custom_id: custom_id.into(),
            method: "POST".to_string(),
            url: "/v1/chat/completions".to_string(),
            body,
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct BatchJob {
    pub id: String,
    pub status: String,
    pub output_file_id: Option<String>,
    pub error_file_id: Option<String>,
}

pub fn encode_jsonl(lines: &[RequestLine]) -> Result<Vec<u8>> {
    if lines.is_empty() {
        return Err(anyhow!(
            "Groq batch input must contain at least one request"
        ));
    }
    let mut output = Vec::new();
    for line in lines {
        if line.custom_id.trim().is_empty() || line.custom_id.contains(['\r', '\n']) {
            return Err(anyhow!("Groq batch custom_id must be one non-empty line"));
        }
        serde_json::to_writer(&mut output, line).context("Serialize Groq batch request")?;
        output.push(b'\n');
    }
    Ok(output)
}

pub fn upload_input_file(api_key: &str, filename: &str, jsonl: &[u8]) -> Result<String> {
    let boundary = format!("----SGTGroqBatch{}", chrono::Utc::now().timestamp_millis());
    let mut body = Vec::new();
    multipart_field(&mut body, &boundary, "purpose", b"batch");
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!(
            "Content-Disposition: form-data; name=\"file\"; filename=\"{}\"\r\n",
            safe_filename(filename)
        )
        .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: application/jsonl\r\n\r\n");
    body.extend_from_slice(jsonl);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());

    let response = UREQ_AGENT
        .post(&format!("{API_ROOT}/files"))
        .header("Authorization", &format!("Bearer {api_key}"))
        .header(
            "Content-Type",
            &format!("multipart/form-data; boundary={boundary}"),
        )
        .send(&body)
        .context("Upload Groq batch input")?;
    let root: Value = response
        .into_body()
        .read_json()
        .context("Parse Groq file")?;
    root["id"]
        .as_str()
        .map(ToString::to_string)
        .ok_or_else(|| anyhow!("Groq file upload returned no id"))
}

pub fn create(api_key: &str, input_file_id: &str) -> Result<BatchJob> {
    post_job(
        api_key,
        "/batches",
        serde_json::json!({
            "input_file_id": input_file_id,
            "endpoint": "/v1/chat/completions",
            "completion_window": "24h"
        }),
    )
}

pub fn retrieve(api_key: &str, batch_id: &str) -> Result<BatchJob> {
    let response = UREQ_AGENT
        .get(&format!("{API_ROOT}/batches/{batch_id}"))
        .header("Authorization", &format!("Bearer {api_key}"))
        .call()
        .context("Retrieve Groq batch")?;
    response
        .into_body()
        .read_json()
        .context("Parse Groq batch status")
}

pub fn cancel(api_key: &str, batch_id: &str) -> Result<BatchJob> {
    post_job(api_key, &format!("/batches/{batch_id}/cancel"), Value::Null)
}

pub fn download_file(api_key: &str, file_id: &str) -> Result<Vec<u8>> {
    let response = UREQ_AGENT
        .get(&format!("{API_ROOT}/files/{file_id}/content"))
        .header("Authorization", &format!("Bearer {api_key}"))
        .call()
        .context("Download Groq batch result")?;
    response
        .into_body()
        .read_to_vec()
        .context("Read Groq batch result")
}

pub fn delete_file(api_key: &str, file_id: &str) -> Result<()> {
    UREQ_AGENT
        .delete(&format!("{API_ROOT}/files/{file_id}"))
        .header("Authorization", &format!("Bearer {api_key}"))
        .call()
        .context("Delete Groq batch file")?;
    Ok(())
}

fn post_job(api_key: &str, path: &str, payload: Value) -> Result<BatchJob> {
    let request = UREQ_AGENT
        .post(&format!("{API_ROOT}{path}"))
        .header("Authorization", &format!("Bearer {api_key}"));
    let response = if payload.is_null() {
        request.send_empty()
    } else {
        request.send_json(payload)
    }
    .with_context(|| format!("Groq batch request {path}"))?;
    response
        .into_body()
        .read_json()
        .with_context(|| format!("Parse Groq batch response {path}"))
}

fn safe_filename(filename: &str) -> String {
    filename
        .chars()
        .map(|character| match character {
            '\r' | '\n' | '"' | '\\' => '_',
            other => other,
        })
        .collect()
}

fn multipart_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &[u8]) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value);
    body.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_jsonl_is_one_request_per_line() {
        let bytes = encode_jsonl(&[
            RequestLine::chat("first", serde_json::json!({"model": "model-a"})),
            RequestLine::chat("second", serde_json::json!({"model": "model-b"})),
        ])
        .unwrap();
        let lines: Vec<&[u8]> = bytes.split(|byte| *byte == b'\n').collect();
        assert_eq!(lines.len(), 3);
        assert!(lines[2].is_empty());
        let first: Value = serde_json::from_slice(lines[0]).unwrap();
        assert_eq!(first["custom_id"], "first");
        assert_eq!(first["url"], "/v1/chat/completions");
    }

    #[test]
    fn batch_rejects_invalid_custom_ids() {
        let line = RequestLine::chat("bad\nid", serde_json::json!({}));
        assert!(encode_jsonl(&[line]).is_err());
    }
}
