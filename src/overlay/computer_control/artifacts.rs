//! Local large-content artifacts for Computer Control. These keep bulk text out of
//! model context: tools pass small ids/paths/stats while the bytes stay on disk.

use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::thread::sleep;
use std::time::{SystemTime, UNIX_EPOCH};

const PREVIEW_CHARS: usize = 900;

#[derive(Clone)]
pub(super) struct TextArtifact {
    id: String,
    kind: String,
    title: String,
    source_url: String,
    path: PathBuf,
    bytes: usize,
    chars: usize,
    words: usize,
    lines: usize,
    sha256: String,
    created_secs: u64,
}

fn store() -> &'static parking_lot::Mutex<HashMap<String, TextArtifact>> {
    static STORE: std::sync::OnceLock<parking_lot::Mutex<HashMap<String, TextArtifact>>> =
        std::sync::OnceLock::new();
    STORE.get_or_init(|| parking_lot::Mutex::new(HashMap::new()))
}

fn counter() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::SeqCst)
}

pub(super) fn create_text(
    kind: &str,
    title: Option<&str>,
    source_url: Option<&str>,
    text: &str,
) -> anyhow::Result<TextArtifact> {
    let created_secs = now_secs();
    let sha256 = sha256_hex(text.as_bytes());
    let id = format!("art_{created_secs}_{}_{}", counter(), &sha256[..8]);
    let safe = safe_name(title.unwrap_or(kind));
    let dir = crate::paths::app_temp_dir().join("cc-artifacts");
    std::fs::create_dir_all(&dir)?;
    let path = dir.join(format!("{id}_{safe}.txt"));
    std::fs::write(&path, text)?;
    let artifact = TextArtifact {
        id: id.clone(),
        kind: kind.to_string(),
        title: title.unwrap_or("").to_string(),
        source_url: source_url.unwrap_or("").to_string(),
        path,
        bytes: text.len(),
        chars: text.chars().count(),
        words: text.split_whitespace().count(),
        lines: text.lines().count(),
        sha256,
        created_secs,
    };
    store().lock().insert(id, artifact.clone());
    Ok(artifact)
}

pub(super) fn info_tool(id: &str) -> Value {
    match load_text(id) {
        Ok((artifact, text)) => artifact.response(&text),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

pub(super) fn dispatch_tool(
    name: &str,
    args: &Value,
    profile: &super::human_input::HumanProfile,
    cancel: &AtomicBool,
    dry: bool,
) -> Option<Value> {
    match name {
        "artifact_info" => Some(info_tool(
            args.get("id").and_then(Value::as_str).unwrap_or(""),
        )),
        "save_artifact" => Some(save_tool(
            args.get("id").and_then(Value::as_str).unwrap_or(""),
            args.get("path").and_then(Value::as_str),
            args.get("overwrite")
                .and_then(Value::as_bool)
                .unwrap_or(false),
        )),
        "paste_artifact" => Some(paste_tool(
            args.get("id").and_then(Value::as_str).unwrap_or(""),
            profile,
            cancel,
            dry,
        )),
        _ => None,
    }
}

pub(super) fn save_tool(id: &str, path: Option<&str>, overwrite: bool) -> Value {
    let Ok((artifact, text)) = load_text(id) else {
        return json!({"ok": false, "error": format!("artifact '{id}' not found")});
    };
    let Some(path) = path.filter(|p| !p.trim().is_empty()) else {
        return json!({"ok": true, "artifact": artifact.response(&text), "path": artifact.path_string()});
    };
    let target = PathBuf::from(path);
    if target.exists() && !overwrite {
        return json!({"ok": false, "error": "target file already exists", "path": path});
    }
    if let Some(parent) = target.parent()
        && !parent.as_os_str().is_empty()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        return json!({"ok": false, "error": format!("create parent failed: {e}"), "path": path});
    }
    match std::fs::write(&target, text) {
        Ok(()) => {
            json!({"ok": true, "path": target.to_string_lossy(), "artifact": artifact.response("")})
        }
        Err(e) => json!({"ok": false, "error": format!("write failed: {e}"), "path": path}),
    }
}

pub(super) fn load_text(id_or_path: &str) -> anyhow::Result<(TextArtifact, String)> {
    if let Some(artifact) = store().lock().get(id_or_path).cloned() {
        let text = std::fs::read_to_string(&artifact.path)?;
        return Ok((artifact, text));
    }
    let path = Path::new(id_or_path);
    if path.exists() {
        let text = std::fs::read_to_string(path)?;
        let artifact = create_text(
            "file",
            path.file_name().and_then(|n| n.to_str()),
            None,
            &text,
        )?;
        return Ok((artifact, text));
    }
    anyhow::bail!("artifact '{id_or_path}' not found")
}

fn paste_tool(
    id: &str,
    profile: &super::human_input::HumanProfile,
    cancel: &AtomicBool,
    dry: bool,
) -> Value {
    let Ok((artifact, text)) = load_text(id) else {
        return json!({"ok": false, "error": format!("artifact '{id}' not found")});
    };
    if dry {
        return json!({"ok": true, "dry": true, "artifact": artifact.response(&text)});
    }
    let clobbered_nontext = super::clipboard::has_nontext();
    super::clipboard::set_text(&text);
    sleep(std::time::Duration::from_millis(60));
    let paste = super::executor::execute_ex(
        "key_combination",
        &json!({"keys": "control+v"}),
        profile,
        cancel,
    );
    json!({
        "ok": paste.get("ok").and_then(Value::as_bool).unwrap_or(false),
        "method": "clipboard_paste",
        "artifact_id": artifact.id(),
        "pasted_chars": artifact.chars(),
        "source_sha256": artifact.sha256,
        "clipboard_left_as_artifact": true,
        "clobbered_nontext_clipboard": clobbered_nontext,
        "paste": paste,
        "verification": "Set clipboard from artifact and issued Ctrl+V to the focused app. Verify destination word/char count if the app exposes one.",
    })
}

impl TextArtifact {
    pub(super) fn response(&self, text: &str) -> Value {
        json!({
            "id": self.id,
            "kind": self.kind,
            "title": self.title,
            "source_url": self.source_url,
            "path": self.path_string(),
            "byte_count": self.bytes,
            "char_count": self.chars,
            "word_count": self.words,
            "line_count": self.lines,
            "sha256": self.sha256,
            "created_secs": self.created_secs,
            "preview": preview(text),
            "instruction": "For exact copy/export, pass this artifact id to paste_artifact or save_artifact; do not retype the preview or full text through type_text.",
        })
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn chars(&self) -> usize {
        self.chars
    }

    fn path_string(&self) -> String {
        self.path.to_string_lossy().to_string()
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|b| format!("{b:02x}")).collect()
}

fn safe_name(raw: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        let c = if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_') {
            ch
        } else if ch.is_whitespace() {
            '_'
        } else {
            continue;
        };
        out.push(c);
        if out.len() >= 48 {
            break;
        }
    }
    if out.is_empty() {
        "text".to_string()
    } else {
        out
    }
}

fn preview(text: &str) -> String {
    let mut out: String = text.chars().take(PREVIEW_CHARS).collect();
    if text.chars().count() > PREVIEW_CHARS {
        out.push_str("\n[preview truncated]");
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn creates_reloadable_text_artifact() {
        let artifact = create_text(
            "test",
            Some("Hello / World"),
            Some("https://example.test"),
            "one two\nthree",
        )
        .unwrap();
        let (loaded, text) = load_text(artifact.id()).unwrap();
        assert_eq!(text, "one two\nthree");
        assert_eq!(loaded.words, 3);
        assert!(loaded.path.exists());
    }
}
