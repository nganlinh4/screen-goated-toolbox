//! Large-content artifacts keep bulk text outside model context. Tools pass ids
//! and stats; automatic storage follows the session's retention policy.

use crate::crypto::sha256_hex;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
#[cfg(test)]
use std::sync::atomic::AtomicU64;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread::sleep;
use std::time::{SystemTime, UNIX_EPOCH};

mod cache_file;
mod cache_sweep;
mod export;
pub(super) use export::PreparedSave;
#[path = "artifacts/extract.rs"]
mod extract;
mod session_storage;

const PREVIEW_CHARS: usize = 900;

#[derive(Clone)]
pub(super) struct TextArtifact {
    id: String,
    kind: String,
    title: String,
    source_url: String,
    path: PathBuf,
    owner: super::session_retention::SessionRetention,
    ephemeral: Option<std::sync::Arc<str>>,
    cache_file: Option<std::sync::Arc<cache_file::CacheFile>>,
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

#[cfg(test)]
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
    session_storage::check_size(kind, title.unwrap_or(""), source_url.unwrap_or(""), text)?;
    let owner = super::session_retention::current();
    let source_url = source_label(source_url);
    session_storage::check_size(kind, title.unwrap_or(""), &source_url, text)?;
    let created_secs = now_secs();
    let sha256 = sha256_hex(text.as_bytes());
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce)
        .map_err(|error| anyhow::anyhow!("create artifact identity: {error}"))?;
    let id = format!("art_{created_secs}_{:032x}", u128::from_le_bytes(nonce));
    let safe = safe_name(title.unwrap_or(kind));
    let dir = cache_sweep::cache_dir();
    let path = dir.join(format!("{id}_{safe}.txt"));
    let (ephemeral, cache_file) =
        match super::session_retention::persist(|| cache_file::CacheFile::create(&path, text)) {
            Some(result) => (None, Some(std::sync::Arc::new(result?))),
            None => (Some(std::sync::Arc::from(text)), None),
        };
    let artifact = TextArtifact {
        id: id.clone(),
        kind: kind.to_string(),
        title: title.unwrap_or("").to_string(),
        source_url,
        path,
        owner: owner.clone(),
        ephemeral,
        cache_file,
        bytes: text.len(),
        chars: text.chars().count(),
        words: text.split_whitespace().count(),
        lines: text.lines().count(),
        sha256,
        created_secs,
    };
    owner
        .while_open(|| session_storage::insert(&artifact))
        .ok_or_else(|| anyhow::anyhow!("artifact session has retired"))??;
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
    _profile: &super::human_input::HumanProfile,
    cancel: &AtomicBool,
    dry: bool,
) -> Option<Value> {
    match name {
        "artifact_info" => Some(info_tool(
            args.get("id").and_then(Value::as_str).unwrap_or(""),
        )),
        "extract_artifact" => Some(extract::tool(args, cancel, dry)),
        "save_artifact" if dry => Some(json!({"ok": true, "note": "dry"})),
        "save_artifact" => Some(save_tool_with_cancel(
            args.get("id").and_then(Value::as_str).unwrap_or(""),
            args.get("path").and_then(Value::as_str),
            args.get("overwrite")
                .and_then(Value::as_bool)
                .unwrap_or(false),
            cancel,
        )),
        _ => None,
    }
}

#[cfg(test)]
fn save_tool(id: &str, path: Option<&str>, overwrite: bool) -> Value {
    save_tool_with_cancel(id, path, overwrite, &AtomicBool::new(false))
}

fn save_tool_with_cancel(
    id: &str,
    path: Option<&str>,
    overwrite: bool,
    cancel: &AtomicBool,
) -> Value {
    if let Some(path) = path.filter(|p| !p.trim().is_empty()) {
        return match PreparedSave::prepare(id, path, overwrite) {
            Ok(prepared) => prepared.commit(cancel),
            Err(error) => error,
        };
    }
    match load_text(id) {
        Ok((artifact, text)) => json!({
            "ok": true, "saved": false, "artifact": artifact.response(&text),
            "path": artifact.path_string(), "effect_may_have_occurred": false, "executed": false,
        }),
        Err(error) => json!({
            "ok": false, "error": error.to_string(),
            "effect_may_have_occurred": false, "executed": false,
        }),
    }
}

pub(super) fn load_text(id_or_path: &str) -> anyhow::Result<(TextArtifact, String)> {
    let artifact = {
        let cache = store().lock();
        cache
            .get(id_or_path)
            .or_else(|| {
                cache
                    .values()
                    .find(|entry| entry.cache_file.is_some() && entry.path == Path::new(id_or_path))
            })
            .cloned()
    };
    if let Some(artifact) = artifact {
        if !artifact
            .owner
            .same_session(&super::session_retention::current())
        {
            anyhow::bail!("artifact is no longer available in this session");
        }
        let text = if let Some(text) = &artifact.ephemeral {
            text.to_string()
        } else {
            artifact
                .cache_file
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("artifact content is unavailable"))?
                .read(session_storage::MAX_BYTES as u64)?
        };
        if text.len() != artifact.bytes || sha256_hex(text.as_bytes()) != artifact.sha256 {
            anyhow::bail!("artifact content no longer matches its observation");
        }
        return Ok((artifact, text));
    }
    let path = Path::new(id_or_path);
    if !path.is_absolute() {
        anyhow::bail!(
            "artifact '{id_or_path}' was not found; a fallback file path must be absolute"
        );
    }
    if path.exists() {
        let text = cache_file::read_path(path, session_storage::MAX_BYTES as u64)?;
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

pub(super) fn paste_tool(
    args: &Value,
    profile: &super::human_input::HumanProfile,
    cancel: &AtomicBool,
    dry: bool,
) -> Value {
    let id = args.get("id").and_then(Value::as_str).unwrap_or("");
    if !dry && args.get("expected_input_target").is_none() {
        return json!({"ok": false, "error": "model-visible input target is unavailable", "executed": false});
    }
    let Ok((artifact, text)) = load_text(id) else {
        return json!({"ok": false, "error": format!("artifact '{id}' not found")});
    };
    if dry {
        return json!({"ok": true, "dry": true, "artifact": artifact.response(&text)});
    }
    if cancel.load(Ordering::SeqCst) {
        return json!({"ok": false, "error": "cancelled", "executed": false});
    }
    if let Err(error) = super::executor::verify_expected_keyboard_target(args) {
        return json!({"ok": false, "error": error.to_string(), "executed": false});
    }
    let require_document = args
        .pointer("/expected_input_target/kind")
        .and_then(Value::as_str)
        == Some("browser");
    let focused = match super::uia::focused_editable_guard(require_document) {
        Ok(focused) => focused,
        Err(error) => return json!({"ok": false, "error": error.to_string(), "executed": false}),
    };
    let mut paste_args = args.clone();
    paste_args["expected_paste_element"] = focused;
    if let Err(error) = super::executor::verify_expected_keyboard_target(&paste_args) {
        return json!({"ok": false, "error": error.to_string(), "executed": false});
    }
    if cancel.load(Ordering::SeqCst) {
        return json!({"ok": false, "error": "cancelled", "executed": false});
    }
    if let Some(refusal) = irreplaceable_clipboard_refusal() {
        return refusal;
    }
    let Some(staged_sequence) = super::clipboard::stage_text(&text) else {
        return json!({"ok": false, "error": "could not set clipboard", "executed": false});
    };
    sleep(std::time::Duration::from_millis(60));
    paste_args["keys"] = json!("control+v");
    paste_args["expected_clipboard_sequence"] = json!(staged_sequence);
    let paste = super::executor::execute_ex("key_combination", &paste_args, profile, cancel);
    paste_receipt(
        &artifact,
        paste,
        super::clipboard::sequence() == staged_sequence,
    )
}

/// Restoring after the paste is unsafe (the target may consume Ctrl+V late),
/// so refuse before staging rather than silently destroy a copied image or
/// file list.
fn irreplaceable_clipboard_refusal() -> Option<Value> {
    super::clipboard::holds_irreplaceable_content().then(|| {
        json!({
            "ok": false,
            "code": "ERR_CLIPBOARD_HOLDS_NONTEXT",
            "error": "the clipboard holds an image or copied files that pasting would destroy; ask the user to paste or clear them first, or use save_artifact",
            "executed": false,
        })
    })
}

/// The target may process Ctrl+V after dispatch returns, so a clipboard
/// replacement observed afterwards leaves the pasted content unproven.
fn paste_receipt(artifact: &TextArtifact, paste: Value, clipboard_still_staged: bool) -> Value {
    let dispatched = paste.get("ok").and_then(Value::as_bool).unwrap_or(false);
    let content_uncertain = dispatched && !clipboard_still_staged;
    json!({
        "ok": dispatched,
        "method": "clipboard_paste",
        "artifact_id": artifact.id(),
        "pasted_chars": artifact.chars(),
        "source_sha256": artifact.sha256,
        "clipboard_left_as_artifact": clipboard_still_staged,
        "pasted_content_uncertain": content_uncertain,
        "paste": paste,
        "verification": if content_uncertain {
            "Another app replaced the clipboard after Ctrl+V was sent; a target that processed the paste late may have inserted that content instead. Inspect the destination before continuing."
        } else {
            "Set clipboard from artifact and issued Ctrl+V to the focused app. Verify destination word/char count if the app exposes one."
        },
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
            "instruction": "For a whole exact copy/export, pass this id to paste_artifact or save_artifact. If the requested output is only part of this artifact, call extract_artifact first with exact boundary text; never paste the whole artifact for a subset request.",
        })
    }

    pub(super) fn id(&self) -> &str {
        &self.id
    }

    pub(super) fn chars(&self) -> usize {
        self.chars
    }

    fn path_string(&self) -> Option<String> {
        self.ephemeral
            .is_none()
            .then(|| self.path.to_string_lossy().to_string())
    }
}

/// Remove orphaned cache files left by older builds or crashes, once per process.
pub(super) fn sweep_orphaned_cache() {
    cache_sweep::sweep_once_in_background();
}

pub(super) fn retire_session(owner: &super::session_retention::SessionRetention) {
    store()
        .lock()
        .retain(|_, artifact| !artifact.owner.same_session(owner));
}

fn source_label(raw: Option<&str>) -> String {
    let Some(mut url) = raw.and_then(|value| url::Url::parse(value).ok()) else {
        return String::new();
    };
    if url.cannot_be_a_base() {
        return format!("{}:", url.scheme());
    }
    url.set_query(None);
    url.set_fragment(None);
    let _ = url.set_username("");
    let _ = url.set_password(None);
    url.to_string()
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
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
#[path = "artifacts/retention_tests.rs"]
mod retention_tests;

#[cfg(test)]
#[path = "artifacts/cache_tests.rs"]
mod cache_tests;

#[cfg(test)]
#[path = "artifacts/live_clipboard_tests.rs"]
mod live_clipboard_tests;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paste_rejects_missing_observed_target_before_loading_artifact() {
        let result = paste_tool(
            &json!({"id": "missing"}),
            &super::super::human_input::HumanProfile::instant(),
            &AtomicBool::new(false),
            false,
        );
        assert_eq!(result["ok"], false);
        assert_eq!(result["executed"], false);
        assert_eq!(result["error"], "model-visible input target is unavailable");
    }

    #[test]
    fn paste_receipt_marks_content_uncertain_when_clipboard_is_replaced_after_dispatch() {
        let artifact = create_text("test", Some("paste"), None, "staged words").unwrap();
        let dispatched = json!({"ok": true, "keys": "control+v"});

        let settled = paste_receipt(&artifact, dispatched.clone(), true);
        assert_eq!(settled["ok"], true);
        assert_eq!(settled["clipboard_left_as_artifact"], true);
        assert_eq!(settled["pasted_content_uncertain"], false);

        let replaced = paste_receipt(&artifact, dispatched, false);
        assert_eq!(replaced["ok"], true);
        assert_eq!(replaced["clipboard_left_as_artifact"], false);
        assert_eq!(replaced["pasted_content_uncertain"], true);
        assert!(
            replaced["verification"]
                .as_str()
                .unwrap()
                .contains("Inspect the destination")
        );

        let rejected = json!({"ok": false, "error": "clipboard changed"});
        let not_sent = paste_receipt(&artifact, rejected, false);
        assert_eq!(not_sent["ok"], false);
        assert_eq!(not_sent["pasted_content_uncertain"], false);
    }

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

    #[test]
    fn save_rejects_relative_destination_without_creating_it() {
        let artifact = create_text("test", Some("scope"), None, "bounded").unwrap();
        let relative = format!("sgt-artifact-relative-{}.txt", counter());
        let target = std::env::current_dir().unwrap().join(&relative);
        assert!(!target.exists());

        let result = save_tool(artifact.id(), Some(&relative), false);

        assert_eq!(result["code"], "ERR_ARTIFACT_PATH_NOT_ABSOLUTE");
        assert_eq!(result["effect_may_have_occurred"], false);
        assert!(!target.exists());
    }

    #[test]
    fn save_requires_a_file_name_before_creating_directories() {
        let artifact = create_text("test", Some("root"), None, "bounded").unwrap();
        let root = Path::new(r"C:\");

        let result = save_tool(artifact.id(), root.to_str(), false);

        assert_eq!(result["code"], "ERR_ARTIFACT_PATH_NOT_FILE");
        assert_eq!(result["effect_may_have_occurred"], false);
    }

    #[test]
    fn save_accepts_absolute_destination_and_preserves_existing_by_default() {
        let artifact = create_text("test", Some("absolute"), None, "new bytes").unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = crate::paths::app_temp_dir()
            .join(format!("cc-artifact-save-{}-{nonce}", std::process::id()));
        let target = dir.join("saved.txt");

        let saved = save_tool(artifact.id(), target.to_str(), false);
        assert_eq!(saved["ok"], true);
        assert_eq!(saved["effect_verified"], true);
        assert_eq!(saved["sha256"], artifact.sha256);
        assert_eq!(saved["source_artifact_id"], artifact.id());
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new bytes");

        std::fs::write(&target, "existing bytes").unwrap();
        let rejected = save_tool(artifact.id(), target.to_str(), false);
        assert_eq!(rejected["code"], "ERR_ARTIFACT_TARGET_EXISTS");
        assert_eq!(rejected["effect_may_have_occurred"], false);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "existing bytes");

        let overwritten = save_tool(artifact.id(), target.to_str(), true);
        assert_eq!(overwritten["effect_verified"], true);
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "new bytes");
        std::fs::remove_file(&target).unwrap();
        std::fs::remove_dir(&dir).unwrap();
    }

    #[test]
    fn artifact_file_fallback_rejects_relative_working_directory_paths() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let relative = format!("sgt-artifact-source-{}-{nonce}.txt", std::process::id());
        let target = std::env::current_dir().unwrap().join(&relative);
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&target)
            .unwrap();

        let result = load_text(&relative);

        std::fs::remove_file(&target).unwrap();
        let error = match result {
            Err(error) => error,
            Ok(_) => panic!("relative fallback unexpectedly loaded"),
        };
        assert!(error.to_string().contains("file path must be absolute"));
    }
}
