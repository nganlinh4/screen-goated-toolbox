//! Computer Control conversation memory: each voice session is auto-saved and
//! auto-embedded (Gemini Embedding 2, multimodal) so the agent can SEARCH past
//! conversations semantically and open the relevant one — no preloading. Mirrors
//! the `history.rs` store (file-backed, atomic, auto-pruned) but with its OWN
//! directory and max so the busy media history can't evict it.

use std::fs;
use std::path::{Path, PathBuf};

use base64::{Engine as _, engine::general_purpose};
use chrono::Local;
use serde::{Deserialize, Serialize};

use crate::api::gemini_embed::{self, EmbedPart};

/// Index entry per saved conversation. The full transcript lives in a sidecar
/// file (`conv_<id>.json`); the index keeps only what search needs in memory.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
struct ConvMeta {
    id: i64,
    timestamp: String,
    title: String,
    snippet: String,
    images: Vec<String>, // jpeg filenames in the memory dir
    #[serde(default)]
    embedding: Vec<f32>, // empty if embedding failed (still keyword-findable)
}

#[derive(Serialize, Deserialize)]
struct Transcript {
    lines: Vec<String>,
}

/// Max chars of transcript text fed to the embedder (well under the 8k-token cap).
const EMBED_TEXT_BUDGET: usize = 12_000;
/// Embedding 2 accepts up to 6 images per call.
const MAX_EMBED_IMAGES: usize = 6;

fn mem_dir() -> PathBuf {
    let dir = crate::paths::app_config_dir().join("cc_memory");
    let _ = fs::create_dir_all(&dir);
    dir
}

fn index_path() -> PathBuf {
    mem_dir().join("index.json")
}

fn load_index() -> Vec<ConvMeta> {
    match fs::File::open(index_path()) {
        Ok(f) => serde_json::from_reader(f).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

fn save_index(items: &[ConvMeta]) {
    let _ = crate::atomic_json::write_json_atomic(&index_path(), &items);
}

fn max_items() -> usize {
    crate::load_config().cc_max_memory_items.max(1)
}

// ── Save ────────────────────────────────────────────────────────────────────

/// Auto-save + auto-embed a finished session on a DETACHED thread (embedding is
/// a slow network call — never block session teardown). `transcript` is the
/// rolling User/Assistant/Observed log; `frames` are representative JPEG frames.
pub(super) fn save(transcript: Vec<String>, frames: Vec<Vec<u8>>) {
    // Nothing worth remembering if the user never actually spoke a command.
    if !transcript.iter().any(|l| l.starts_with("User:")) {
        return;
    }
    std::thread::spawn(move || {
        if let Err(e) = save_inner(transcript, frames) {
            eprintln!("[cc-mem] save failed: {e}");
        }
    });
}

fn save_inner(transcript: Vec<String>, frames: Vec<Vec<u8>>) -> anyhow::Result<()> {
    let now = Local::now();
    let id = now.timestamp_nanos_opt().unwrap_or(0);
    let timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
    let dir = mem_dir();

    // Persist representative frames (already JPEG).
    let mut images = Vec::new();
    for (i, jpeg) in frames.iter().enumerate() {
        let name = format!("img_{id}_{i}.jpg");
        if fs::write(dir.join(&name), jpeg).is_ok() {
            images.push(name);
        }
    }

    // Write the full transcript sidecar FIRST, so even if embedding fails the
    // conversation is fully recoverable and openable.
    let conv_path = dir.join(format!("conv_{id}.json"));
    crate::atomic_json::write_json_atomic(
        &conv_path,
        &Transcript {
            lines: transcript.clone(),
        },
    )?;

    // Embed the transcript text + representative frames, interleaved, into one
    // cross-modal vector. Empty on failure (re-embedded on a later launch).
    let embedding = embed_conversation(&transcript, &frames).unwrap_or_default();
    if embedding.is_empty() {
        eprintln!(
            "[cc-mem] saved conversation {id} WITHOUT embedding (keyword-only until re-embedded)"
        );
    }

    let mut index = load_index();
    index.insert(
        0,
        ConvMeta {
            id,
            timestamp,
            title: derive_title(&transcript),
            snippet: derive_snippet(&transcript),
            images,
            embedding,
        },
    );
    prune(&mut index, &dir);
    save_index(&index);
    Ok(())
}

fn embed_conversation(transcript: &[String], frames: &[Vec<u8>]) -> anyhow::Result<Vec<f32>> {
    let key = super::session::load_key()?;
    let mut text = transcript.join("\n");
    if text.len() > EMBED_TEXT_BUDGET {
        text = text.chars().take(EMBED_TEXT_BUDGET).collect();
    }
    let mut parts = vec![EmbedPart::Text(text)];
    for jpeg in frames.iter().take(MAX_EMBED_IMAGES) {
        parts.push(EmbedPart::Image(
            "image/jpeg".to_string(),
            general_purpose::STANDARD.encode(jpeg),
        ));
    }
    gemini_embed::embed(&parts, &key)
}

fn derive_title(transcript: &[String]) -> String {
    let first_user = transcript
        .iter()
        .find_map(|l| l.strip_prefix("User:"))
        .unwrap_or("")
        .trim();
    if first_user.is_empty() {
        "Conversation".to_string()
    } else {
        first_user.chars().take(80).collect()
    }
}

fn derive_snippet(transcript: &[String]) -> String {
    transcript.join(" • ").chars().take(240).collect()
}

fn prune(index: &mut Vec<ConvMeta>, dir: &Path) {
    let max = max_items();
    while index.len() > max {
        if let Some(old) = index.pop() {
            let _ = fs::remove_file(dir.join(format!("conv_{}.json", old.id)));
            for img in &old.images {
                let _ = fs::remove_file(dir.join(img));
            }
        }
    }
}

// ── Search / open (the agent's tools) ────────────────────────────────────────

/// A search hit handed back to the agent.
pub(super) struct Hit {
    pub id: i64,
    pub timestamp: String,
    pub title: String,
    pub snippet: String,
    pub score: f32,
}

/// Semantic search over saved conversations. Embeds the query and cosine-ranks;
/// falls back to keyword matching when the query can't be embedded (offline /
/// quota) or for a conversation whose own embedding failed.
pub(super) fn search(query: &str, top_k: usize) -> Vec<Hit> {
    let index = load_index();
    if index.is_empty() {
        return Vec::new();
    }
    let q_vec = super::session::load_key()
        .ok()
        .and_then(|k| gemini_embed::embed(&[EmbedPart::Text(query.to_string())], &k).ok());
    let ql = query.to_lowercase();
    // The index is newest-first, so `rank` 0 is the most recent conversation.
    // Add a small recency boost so "the last / most recent conversation" surfaces
    // for a vague query, without overriding a clearly stronger semantic match.
    let n = index.len().max(1) as f32;

    let mut scored: Vec<Hit> = index
        .iter()
        .enumerate()
        .map(|(rank, m)| {
            let semantic = q_vec
                .as_ref()
                .map(|qv| gemini_embed::cosine(qv, &m.embedding))
                .unwrap_or(0.0);
            // Keyword fallback/boost on title+snippet.
            let hay = format!("{} {}", m.title, m.snippet).to_lowercase();
            let keyword = if !ql.is_empty() && hay.contains(&ql) {
                0.5
            } else {
                0.0
            };
            let recency = 1.0 - (rank as f32 / n); // 1.0 newest .. ~0 oldest
            Hit {
                id: m.id,
                timestamp: m.timestamp.clone(),
                title: m.title.clone(),
                snippet: m.snippet.clone(),
                score: semantic.max(keyword) + 0.08 * recency,
            }
        })
        .collect();
    scored.sort_by(|a, b| {
        b.score
            .partial_cmp(&a.score)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    scored.retain(|h| h.score > 0.0);
    scored.truncate(top_k.max(1));
    scored
}

/// The full transcript of one saved conversation, formatted for the agent.
pub(super) fn open(id: i64) -> Option<String> {
    let f = fs::File::open(mem_dir().join(format!("conv_{id}.json"))).ok()?;
    let t: Transcript = serde_json::from_reader(f).ok()?;
    Some(t.lines.join("\n"))
}
