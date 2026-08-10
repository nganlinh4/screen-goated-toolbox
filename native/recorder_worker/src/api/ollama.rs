use anyhow::Result;
use serde::Deserialize;
use std::io::{BufRead, BufReader};

#[derive(Deserialize)]
struct OllamaStreamChunk {
    #[serde(default)]
    response: String,
    #[serde(default)]
    thinking: Option<String>,
    #[serde(default)]
    done: bool,
}

#[derive(Deserialize)]
struct OllamaGenerateResponse {
    #[serde(default)]
    response: String,
}

#[derive(Deserialize)]
struct OllamaModel {
    name: String,
}

#[derive(Deserialize)]
struct OllamaTagsResponse {
    #[serde(default)]
    models: Vec<OllamaModel>,
}

#[derive(Deserialize, Default)]
struct OllamaModelDetails {
    #[serde(default)]
    families: Vec<String>,
}

#[derive(Deserialize)]
struct OllamaShowResponse {
    #[serde(default)]
    modelfile: String,
    #[serde(default)]
    details: OllamaModelDetails,
}

pub(crate) struct OllamaModelWithCaps {
    pub(crate) name: String,
    pub(crate) has_vision: bool,
}

pub(crate) fn fetch_ollama_models_with_caps(base_url: &str) -> Result<Vec<OllamaModelWithCaps>> {
    let tags: OllamaTagsResponse = super::client::UREQ_AGENT
        .get(&format!("{}/api/tags", base_url.trim_end_matches('/')))
        .call()?
        .into_body()
        .read_json()?;
    Ok(tags
        .models
        .into_iter()
        .map(|model| OllamaModelWithCaps {
            has_vision: ollama_model_has_vision(base_url, &model.name),
            name: model.name,
        })
        .collect())
}

fn ollama_model_has_vision(base_url: &str, model: &str) -> bool {
    let response = super::client::UREQ_AGENT
        .post(&format!("{}/api/show", base_url.trim_end_matches('/')))
        .send_json(serde_json::json!({ "name": model }))
        .ok()
        .and_then(|response| response.into_body().read_json::<OllamaShowResponse>().ok());
    let families = response
        .as_ref()
        .map(|value| value.details.families.join(" ").to_ascii_lowercase())
        .unwrap_or_default();
    let modelfile = response
        .as_ref()
        .map(|value| value.modelfile.to_ascii_lowercase())
        .unwrap_or_default();
    let name = model.to_ascii_lowercase();
    families.contains("clip")
        || families.contains("vision")
        || modelfile.contains("projector")
        || modelfile.contains("vision")
        || name.contains("vision")
        || name.contains("llava")
        || name.contains("bakllava")
        || name.contains("moondream")
        || name.contains("minicpm-v")
        || name.split([':', '-', '.', '/']).any(|token| token == "vl")
        || name.contains("qwen2vl")
        || name.contains("qwen2.5vl")
}

pub(crate) fn ollama_generate_text<F>(
    base_url: &str,
    model: &str,
    prompt: &str,
    streaming: bool,
    ui_language: &str,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let response = super::client::UREQ_AGENT
        .post(&format!("{}/api/generate", base_url.trim_end_matches('/')))
        .send_json(serde_json::json!({
            "model": model,
            "prompt": prompt,
            "stream": streaming
        }))?;
    if !streaming {
        let response: OllamaGenerateResponse = response.into_body().read_json()?;
        on_chunk(&response.response);
        return Ok(response.response);
    }

    let locale = crate::gui::locale::LocaleText::get(ui_language);
    let mut output = String::new();
    let mut thinking_shown = false;
    for line in BufReader::new(response.into_body().into_reader()).lines() {
        let Ok(chunk) = serde_json::from_str::<OllamaStreamChunk>(&line?) else {
            continue;
        };
        if chunk
            .thinking
            .as_ref()
            .is_some_and(|value| !value.is_empty())
            && !thinking_shown
            && output.is_empty()
        {
            on_chunk(locale.global_settings.model_thinking);
            thinking_shown = true;
        }
        if !chunk.response.is_empty() {
            output.push_str(&chunk.response);
            if thinking_shown && output.len() == chunk.response.len() {
                on_chunk(&format!("{}{}", super::WIPE_SIGNAL, output));
            } else {
                on_chunk(&chunk.response);
            }
        }
        if chunk.done {
            break;
        }
    }
    Ok(output)
}
