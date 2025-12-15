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
    // Compound model fields (optional)
    #[serde(default)]
    pub reasoning: Option<String>,
    #[serde(default)]
    pub executed_tools: Option<Vec<ExecutedTool>>,
}

// --- COMPOUND MODEL TYPES ---

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct ExecutedTool {
    #[serde(default)]
    pub index: i32,
    #[serde(rename = "type", default)]
    pub tool_type: String,
    #[serde(default)]
    pub arguments: Option<String>,
    #[serde(default)]
    pub output: Option<String>,
    #[serde(default)]
    pub search_results: Option<SearchResults>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResults {
    #[serde(default)]
    pub results: Vec<SearchResult>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct SearchResult {
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub score: f64,
}
