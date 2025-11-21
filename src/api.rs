use anyhow::Result;
use serde::{Deserialize, Serialize};
use image::{ImageBuffer, Rgba, ImageFormat};
use base64::{Engine as _, engine::general_purpose};
use std::io::{Cursor, BufRead, BufReader};

#[derive(Serialize, Deserialize)]
struct GroqResponse {
    translation: String,
}

#[derive(Serialize, Deserialize)]
struct StreamChunk {
    choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize)]
struct Choice {
    delta: Delta,
}

#[derive(Serialize, Deserialize)]
struct Delta {
    content: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct ChatCompletionResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Serialize, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    content: String,
}

pub fn translate_image_streaming<F>(
    api_key: &str,
    target_lang: String,
    model: String,
    image: ImageBuffer<Rgba<u8>, Vec<u8>>,
    streaming_enabled: bool,
    mut on_chunk: F,
) -> Result<String>
where
    F: FnMut(&str),
{
    let mut png_data = Vec::new();
    image.write_to(&mut Cursor::new(&mut png_data), ImageFormat::Png)?;
    let b64_image = general_purpose::STANDARD.encode(&png_data);

    let prompt = format!(
        "Extract text from this image and translate it to {}. \
        Output ONLY the translation text directly. Do not use JSON. Do not include any other text.",
        target_lang
    );

    let payload = serde_json::json!({
        "model": model,
        "messages": [
            {
                "role": "user",
                "content": [
                    { "type": "text", "text": prompt },
                    { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{}", b64_image) } }
                ]
            }
        ],
        "stream": streaming_enabled
    });

    // Check if API key is empty
    if api_key.trim().is_empty() {
        return Err(anyhow::anyhow!("NO_API_KEY"));
    }

    let resp = ureq::post("https://api.groq.com/openai/v1/chat/completions")
        .set("Authorization", &format!("Bearer {}", api_key))
        .send_json(payload)
        .map_err(|e| {
            let err_str = e.to_string();
            if err_str.contains("401") {
                anyhow::anyhow!("INVALID_API_KEY")
            } else {
                anyhow::anyhow!("{}", err_str)
            }
        })?;

    let mut full_content = String::new();

    if streaming_enabled {
        let reader = BufReader::new(resp.into_reader());
        for line in reader.lines() {
            let line = line?;
            
            if line.starts_with("data: ") {
                let data = &line[6..]; // Remove "data: " prefix
                
                if data == "[DONE]" {
                    break;
                }
                
                match serde_json::from_str::<StreamChunk>(data) {
                    Ok(chunk) => {
                        if let Some(content) = chunk.choices.get(0)
                            .and_then(|c| c.delta.content.as_ref()) {
                            full_content.push_str(content);
                            // Call callback with the chunk as we receive it
                            on_chunk(content);
                        }
                    }
                    Err(_) => continue, // Skip malformed chunks
                }
            }
        }
    } else {
        let chat_resp: ChatCompletionResponse = resp.into_json()
            .map_err(|e| anyhow::anyhow!("Failed to parse non-streaming response: {}", e))?;
            
        if let Some(choice) = chat_resp.choices.first() {
            full_content = choice.message.content.clone();
            on_chunk(&full_content);
        }
    }

    if full_content.is_empty() {
        return Err(anyhow::anyhow!("No content received from API"));
    }

    Ok(full_content)
}
