//! Computer Control conversation memory: each voice session is auto-saved and
//! auto-embedded (Gemini Embedding 2, multimodal) so the agent can SEARCH past
//! conversations semantically and open the relevant one — no preloading. Mirrors
//! the `history.rs` store (file-backed, atomic, auto-pruned) but with its OWN
//! directory and max so the busy media history can't evict it.

use std::path::PathBuf;

use base64::{Engine as _, engine::general_purpose};
use serde::{Deserialize, Serialize};

use crate::api::gemini_embed::{self, EmbedPart};

mod store;

/// Index entry per saved conversation. The full transcript lives in a sidecar
/// file (`conv_<id>.json`); the index keeps only what search needs in memory.
#[derive(Clone, Debug, Serialize, Deserialize, Default, PartialEq)]
struct ConvMeta {
    id: i64,
    timestamp: String,
    title: String,
    snippet: String,
    images: Vec<String>, // jpeg filenames in the memory dir
    #[serde(default)]
    embedding: Vec<f32>, // empty if embedding failed (still keyword-findable)
}

#[derive(Clone, Serialize, Deserialize)]
struct Transcript {
    lines: Vec<String>,
    #[serde(default)]
    frames: Vec<SavedFrame>,
    #[serde(default)]
    version: u32,
    #[serde(default)]
    metadata: Option<ConvMeta>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct MemoryFrame {
    pub b64: String,
    pub source: super::uia_task::FrameSource,
    pub turn_id: u64,
}

#[derive(Clone, Serialize, Deserialize)]
struct SavedFrame {
    image: String,
    turn_id: u64,
    source: serde_json::Value,
}

/// Max chars of transcript text fed to the embedder (well under the 8k-token cap).
const EMBED_TEXT_BUDGET: usize = 12_000;
/// Embedding 2 accepts up to 6 images per call.
const MAX_EMBED_IMAGES: usize = 6;

fn mem_dir() -> PathBuf {
    crate::paths::app_runtime_config_dir().join("cc_memory")
}

fn max_items() -> usize {
    super::app_config::read(|config| config.cc_max_memory_items.max(1))
}

// ── Save ────────────────────────────────────────────────────────────────────

/// Auto-save + auto-embed a finished session on a DETACHED thread (embedding is
/// a slow network call — never block session teardown). `transcript` is the
/// rolling User/Assistant/Observed log; `frames` are representative JPEG frames.
pub(super) fn save(transcript: Vec<String>, frames: Vec<MemoryFrame>) {
    let generation = store::clear_generation();
    schedule_save(transcript, frames, |transcript, frames| {
        std::thread::spawn(super::session_retention::inherit(move || {
            if let Err(e) = save_inner(transcript, frames, generation) {
                cc_diagnostic!("[cc-mem] save failed: {e}");
            }
        }));
    });
}

fn schedule_save(
    transcript: Vec<String>,
    frames: Vec<MemoryFrame>,
    schedule: impl FnOnce(Vec<String>, Vec<MemoryFrame>),
) {
    if !super::session_retention::current().allows() {
        return;
    }
    // Nothing worth remembering if the user never actually spoke a command.
    if !transcript.iter().any(|l| l.starts_with("User:")) {
        return;
    }
    // These immutable finished-session inputs cannot acquire later private turns.
    // An admitted voice-session save may finish after its runtime has retired.
    schedule(transcript, frames);
}

fn save_inner(
    transcript: Vec<String>,
    frames: Vec<MemoryFrame>,
    generation: u64,
) -> anyhow::Result<()> {
    let frames = frames
        .into_iter()
        .filter_map(|frame| {
            let jpeg = general_purpose::STANDARD.decode(&frame.b64).ok()?;
            Some(store::Image {
                jpeg,
                turn_id: frame.turn_id,
                source: serde_json::to_value(frame.source).ok()?,
            })
        })
        .collect::<Vec<_>>();
    let store = store::Store::new(mem_dir(), max_items()).admitted_at(generation);
    let id = store.create(&transcript, &frames)?;
    // The complete record is already keyword-searchable; no network call holds
    // the store lock or delays durable publication.
    let embedding = embed_conversation(&transcript, &frames).unwrap_or_default();
    if embedding.is_empty() {
        cc_diagnostic!(
            "[cc-mem] saved conversation {id} without embedding; keyword search remains available"
        );
    } else {
        store.set_embedding(id, embedding)?;
    }
    Ok(())
}

fn embed_conversation(transcript: &[String], frames: &[store::Image]) -> anyhow::Result<Vec<f32>> {
    let key = super::session::load_key()?;
    let mut text = transcript.join("\n");
    if text.len() > EMBED_TEXT_BUDGET {
        text = text.chars().take(EMBED_TEXT_BUDGET).collect();
    }
    let mut parts = vec![EmbedPart::Text(text)];
    for frame in frames.iter().take(MAX_EMBED_IMAGES) {
        parts.push(EmbedPart::Image(
            "image/jpeg".to_string(),
            general_purpose::STANDARD.encode(&frame.jpeg),
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
pub(super) fn search(query: &str, top_k: usize) -> anyhow::Result<Vec<Hit>> {
    let index = store::Store::new(mem_dir(), max_items()).snapshot()?;
    if index.is_empty() {
        return Ok(Vec::new());
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
    Ok(scored)
}

/// The full transcript of one saved conversation, formatted for the agent.
pub(super) fn open(id: i64) -> anyhow::Result<Option<String>> {
    store::Store::new(mem_dir(), max_items()).open(id)
}

/// Delete every saved conversation and its screen frames ("Clear all
/// history"). Saves admitted earlier are refused and late embeddings find no
/// record. Content already sent to the embedding provider is outside this.
pub(super) fn clear_all() -> anyhow::Result<()> {
    let dir = mem_dir();
    if !dir.try_exists()? {
        return Ok(());
    }
    store::Store::new(dir, max_items()).clear()
}

#[cfg(test)]
mod retention_tests {
    use super::*;

    #[test]
    fn retention_limit_uses_unsaved_in_memory_settings() {
        let previous = {
            let mut app = crate::APP.lock().unwrap();
            std::mem::replace(&mut app.config.cc_max_memory_items, 7_777)
        };
        let observed = max_items();
        let snapshot = super::super::app_config::snapshot().cc_max_memory_items;
        crate::APP.lock().unwrap().config.cc_max_memory_items = previous;
        assert_eq!(observed, 7_777);
        assert_eq!(snapshot, 7_777);
    }

    fn frame() -> MemoryFrame {
        MemoryFrame {
            b64: general_purpose::STANDARD.encode([1, 2, 3]),
            source: super::super::uia_task::FrameSource {
                frame_id: 1,
                view: super::super::session::View {
                    x: 0,
                    y: 0,
                    w: 100,
                    h: 100,
                },
                surface: super::super::controller::world::SurfaceIdentity::Native {
                    hwnd: 2,
                    pid: 3,
                    generation: 4,
                },
            },
            turn_id: 5,
        }
    }

    #[test]
    fn private_finished_history_and_frames_never_reach_memory_or_embedding_work() {
        let _session = super::super::session_retention::start_session();
        super::super::session_retention::current().forbid();
        let mut scheduled = false;
        schedule_save(
            vec![
                "User: private request".into(),
                "Assistant: private answer".into(),
            ],
            vec![frame()],
            |_, _| scheduled = true,
        );
        assert!(!scheduled);
    }

    #[test]
    fn ordinary_finished_history_remains_eligible_for_memory() {
        let _session = super::super::session_retention::start_session();
        let mut captured = None;
        schedule_save(
            vec!["User: ordinary request".into()],
            vec![frame()],
            |history, frames| captured = Some((history, frames)),
        );
        assert_eq!(
            captured,
            Some((vec!["User: ordinary request".into()], vec![frame()]))
        );
    }

    #[test]
    fn transcript_sidecar_preserves_image_identity_and_reads_older_records() {
        let old: Transcript = serde_json::from_value(serde_json::json!({
            "lines": ["User: earlier request"]
        }))
        .unwrap();
        assert!(old.frames.is_empty());
        let older_frame: Transcript = serde_json::from_value(serde_json::json!({
            "lines": ["User: earlier request"],
            "frames": [{"image":"img_1_0.jpg", "turn_id":5, "source":{
                "frame_id":1, "surface":{"kind":"native","hwnd":2,"pid":3,"generation":4}
            }}]
        }))
        .unwrap();
        assert_eq!(older_frame.frames[0].turn_id, 5);
        assert!(older_frame.frames[0].source.get("view").is_none());
        let image = frame();
        let transcript = Transcript {
            lines: old.lines,
            version: 0,
            metadata: None,
            frames: vec![SavedFrame {
                image: "img_1_0.jpg".into(),
                turn_id: image.turn_id,
                source: serde_json::to_value(&image.source).unwrap(),
            }],
        };
        let restored: Transcript =
            serde_json::from_slice(&serde_json::to_vec(&transcript).unwrap()).unwrap();
        assert_eq!(restored.frames[0].turn_id, 5);
        assert_eq!(restored.frames[0].image, "img_1_0.jpg");
        assert_eq!(
            restored.frames[0].source,
            serde_json::json!({
                "frame_id": 1,
                "view": {"x":0,"y":0,"w":100,"h":100},
                "surface": {"kind": "native", "hwnd": 2, "pid": 3, "generation": 4}
            })
        );
    }
}
