use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize)]
pub struct StreamChunk {
    pub choices: Vec<Choice>,
}

#[derive(Serialize, Deserialize)]
pub struct Choice {
    pub delta: Delta,
}

#[derive(Serialize, Deserialize)]
pub struct Delta {
    pub content: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct ChatCompletionResponse {
    pub choices: Vec<ChatChoice>,
}

#[derive(Serialize, Deserialize)]
pub struct ChatChoice {
    pub message: ChatMessage,
}

#[derive(Serialize, Deserialize)]
pub struct ChatMessage {
    pub content: String,
}
