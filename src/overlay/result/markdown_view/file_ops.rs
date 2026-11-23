//! File operations for markdown view (save HTML)

use super::conversion::markdown_to_html_for_export;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

const DEFAULT_NAME: &str = "result.html";
const NAME_WAIT: Duration = Duration::from_secs(2);
static SAVE_GATE: SaveGate = SaveGate::new();
static NAME_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

#[derive(Default)]
struct SaveState {
    saving: bool,
    exiting: bool,
}

struct SaveGate(Mutex<SaveState>);

impl SaveGate {
    const fn new() -> Self {
        Self(Mutex::new(SaveState {
            saving: false,
            exiting: false,
        }))
    }

    fn start(&'static self) -> Result<SaveLease, StartError> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.exiting {
            return Err(StartError::Exiting);
        }
        if state.saving {
            return Err(StartError::Busy);
        }
        state.saving = true;
        Ok(SaveLease(self))
    }

    fn prepare_exit(&'static self) -> Option<HtmlSaveExitGuard> {
        let mut state = self.0.lock().unwrap_or_else(|error| error.into_inner());
        if state.saving || state.exiting {
            return None;
        }
        state.exiting = true;
        Some(HtmlSaveExitGuard(self))
    }
}

struct SaveLease(&'static SaveGate);

impl Drop for SaveLease {
    fn drop(&mut self) {
        self.0
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .saving = false;
    }
}

pub(crate) struct HtmlSaveExitGuard(&'static SaveGate);

impl Drop for HtmlSaveExitGuard {
    fn drop(&mut self) {
        self.0
            .0
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .exiting = false;
    }
}

/// Reject exit while a save is pending and reject new saves until exit is
/// either committed or abandoned. No dialog or disk write is waited on here.
pub(crate) fn prepare_html_save_exit() -> Option<HtmlSaveExitGuard> {
    SAVE_GATE.prepare_exit()
}

pub(crate) fn notify_pending_save_exit() {
    let language = crate::APP
        .try_lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_default();
    crate::overlay::auto_copy_badge::show_error_notification(
        crate::gui::locale::LocaleText::get(&language)
            .overlay
            .overlay_download_finish_before_exit,
    );
}

struct Slot(&'static AtomicBool);

impl Slot {
    fn acquire(flag: &'static AtomicBool) -> Option<Self> {
        flag.compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
            .ok()
            .map(|_| Self(flag))
    }
}

impl Drop for Slot {
    fn drop(&mut self) {
        self.0.store(false, Ordering::Release);
    }
}

/// A save owns the content snapshot independently of the result window.
/// A single dialog is admitted at a time to bound native threads and memory.
pub fn save_html_file(markdown_text: String) {
    let result = start_save_job(&SAVE_GATE, markdown_text, |markdown_text| {
        let filename = bounded_filename(&markdown_text);
        match save_html_dialog(&markdown_text, &filename) {
            Ok(()) => {}
            Err(error) => {
                eprintln!("Failed to save result HTML: {error:#}");
                notify_save_error(false);
            }
        }
    });
    match result {
        Ok(_) => {}
        Err(StartError::Busy) => notify_save_error(true),
        Err(StartError::Exiting) => notify_save_exit_in_progress(),
        Err(StartError::Spawn) => notify_save_error(false),
    }
}

#[derive(Debug, PartialEq, Eq)]
enum StartError {
    Busy,
    Exiting,
    Spawn,
}

fn start_save_job(
    gate: &'static SaveGate,
    text: String,
    run: impl FnOnce(String) + Send + 'static,
) -> Result<std::thread::JoinHandle<()>, StartError> {
    let slot = gate.start()?;
    std::thread::Builder::new()
        .name("result-save-html".to_string())
        .spawn(move || {
            let _slot = slot;
            run(text);
        })
        .map_err(|_| StartError::Spawn)
}

fn notify_save_error(busy: bool) {
    let language = crate::APP
        .try_lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_else(|_| "en".to_string());
    let locale = crate::gui::locale::LocaleText::get(&language);
    let message = if busy {
        locale.overlay.overlay_download_busy
    } else {
        locale.overlay.overlay_download_failed
    };
    crate::overlay::auto_copy_badge::show_error_notification(message);
}

fn notify_save_exit_in_progress() {
    let language = crate::APP
        .try_lock()
        .map(|app| app.config.ui_language.clone())
        .unwrap_or_default();
    crate::overlay::auto_copy_badge::show_error_notification(
        crate::gui::locale::LocaleText::get(&language)
            .overlay
            .overlay_download_exit_in_progress,
    );
}

fn bounded_filename(content: &str) -> String {
    bounded_filename_with(content, &NAME_IN_PROGRESS, NAME_WAIT, generate_filename)
}

fn bounded_filename_with(
    content: &str,
    flag: &'static AtomicBool,
    wait: Duration,
    generate: impl FnOnce(&str, Arc<AtomicBool>) -> String + Send + 'static,
) -> String {
    let Some(slot) = Slot::acquire(flag) else {
        return DEFAULT_NAME.to_string();
    };
    let input: String = content.chars().take(4000).collect();
    let cancel = Arc::new(AtomicBool::new(false));
    let worker_cancel = cancel.clone();
    let (tx, rx) = mpsc::sync_channel(1);
    if std::thread::Builder::new()
        .name("result-filename".to_string())
        .spawn(move || {
            let _slot = slot;
            let filename = generate(&input, worker_cancel);
            let _ = tx.send(filename);
        })
        .is_err()
    {
        return DEFAULT_NAME.to_string();
    }
    match rx.recv_timeout(wait) {
        Ok(filename) => filename,
        Err(_) => {
            cancel.store(true, Ordering::Release);
            DEFAULT_NAME.to_string()
        }
    }
}

/// Generate a filename using the centralized default text model.
///
/// Routes through the shared text path so the filename request follows whichever
/// provider the default text model belongs to instead of one hardcoded endpoint.
fn generate_filename(content: &str, cancel: Arc<AtomicBool>) -> String {
    let default_name = DEFAULT_NAME.to_string();

    let Some(default_model) =
        crate::model_config::get_model_by_id(crate::model_config::DEFAULT_TEXT_MODEL_ID)
    else {
        return default_name;
    };
    if crate::model_config::model_is_non_llm(&default_model.id) {
        return default_name;
    }

    let Ok(app) = crate::APP.lock() else {
        return default_name;
    };
    let groq_api_key =
        crate::api::provider_credentials::resolve("GROQ_API_KEY", &app.config.api_key);
    let gemini_api_key =
        crate::api::provider_credentials::resolve("GEMINI_API_KEY", &app.config.gemini_api_key);
    let ui_language = app.config.ui_language.clone();
    drop(app);

    // Truncate to avoid token limits (first 4000 chars is enough for context).
    // Slice by chars, not bytes — a byte cut would panic mid-UTF-8 on ko/vi content.
    let prompt_content: String = content.chars().take(4000).collect();

    let instruction = "Generate a short, kebab-case filename (without extension) for the following content.         Do NOT include 'html' in the name.         The filename must be descriptive but concise (max 5 words).         Output ONLY the filename, nothing else. No markdown, no quotes, no explanations."
        .to_string();

    let response = crate::api::text::translate_text_streaming(
        crate::api::text::TranslateTextRequest {
            max_output_tokens: None,
            search_enabled: false,
            search_capability: None,
            search_tool_enabled_by_default: None,
            groq_api_key: &groq_api_key,
            gemini_api_key: &gemini_api_key,
            text: prompt_content,
            instruction,
            model: default_model.full_name.clone(),
            provider: default_model.provider.clone(),
            streaming_enabled: false,
            use_json_format: false,
            response_schema: None,
            search_label: None,
            ui_language: &ui_language,
            cancel_token: Some(cancel),
            request_timeout: Some(crate::api::client::RequestTimeouts::uniform(NAME_WAIT)),
            target_language: None,
        },
        |_| {},
    );

    match response {
        Ok(raw) => sanitize_generated_filename(&raw).unwrap_or(default_name),
        Err(_) => default_name,
    }
}

/// Turns a model's raw filename suggestion into a safe `.html` filename.
fn sanitize_generated_filename(raw: &str) -> Option<String> {
    // Clean up quotes/markdown
    let mut name = raw.trim().replace(['"', '\'', '`'], "");

    // Remove potential .html extension if the model disobeyed
    if name.to_lowercase().ends_with(".html") {
        name.truncate(name.len() - 5);
    }

    // Remove trailing -html or _html if present to avoid redundancy
    let lower_name = name.to_lowercase();
    if lower_name.ends_with("-html") || lower_name.ends_with("_html") {
        name.truncate(name.len() - 5);
    }

    // Basic validation: remove invalid characters for Windows filenames
    const INVALID_CHARS: [char; 9] = ['<', '>', ':', '"', '/', '\\', '|', '?', '*'];
    name = name
        .chars()
        .filter(|c| !INVALID_CHARS.contains(c) && !c.is_control())
        .take(80)
        .collect();

    let name = name.trim().trim_end_matches('.').trim().to_string();
    if name.is_empty() {
        return None;
    }
    let stem = name
        .split('.')
        .next()
        .unwrap_or("")
        .trim_end()
        .to_ascii_uppercase();
    let device_number = stem
        .strip_prefix("COM")
        .or_else(|| stem.strip_prefix("LPT"));
    if matches!(
        stem.as_str(),
        "CON" | "PRN" | "AUX" | "NUL" | "CONIN$" | "CONOUT$"
    ) || device_number.is_some_and(|number| {
        matches!(
            number,
            "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
        )
    }) {
        return None;
    }
    Some(format!("{name}.html"))
}

/// Save the current content as HTML using the native filesystem dialog.
fn save_html_dialog(markdown_text: &str, filename: &str) -> anyhow::Result<()> {
    use crate::native_file_dialog::{Filter, Kind, Options, select};
    use windows::Win32::UI::Shell::FOLDERID_Downloads;
    let Some(path) = select(
        None,
        &Options {
            kind: Kind::Save,
            initial_name: Some(filename),
            default_extension: Some("html"),
            filters: &[Filter {
                label: "HTML Files (*.html)",
                pattern: "*.html",
            }],
            default_folder: &FOLDERID_Downloads,
            initial_folder: None,
            maximum_selections: 1,
        },
    )?
    .pop() else {
        return Ok(());
    };
    let html = markdown_to_html_for_export(markdown_text);
    crate::atomic_json::write_bytes_atomic(&path, html.as_bytes())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filename_suggestions_are_safe_windows_names() {
        assert_eq!(
            sanitize_generated_filename("`Report-html`"),
            Some("Report.html".into())
        );
        for value in ["CON", "lpt1.txt", "COM¹", "CONIN$", "...", "\0\n"] {
            assert_eq!(sanitize_generated_filename(value), None, "{value:?}");
        }
        assert_eq!(
            sanitize_generated_filename("a/b:c*"),
            Some("abc.html".into())
        );
        assert_eq!(
            sanitize_generated_filename(&"a".repeat(200)).unwrap().len(),
            85
        );
    }

    #[test]
    fn save_job_is_bounded_and_retains_its_content_snapshot() {
        static TEST_SAVE: SaveGate = SaveGate::new();
        let (started_tx, started_rx) = mpsc::channel();
        let (release_tx, release_rx) = mpsc::channel();
        let handle = start_save_job(&TEST_SAVE, "original".to_string(), move |text| {
            started_tx.send(text).unwrap();
            release_rx.recv().unwrap();
        })
        .unwrap();
        assert_eq!(
            started_rx.recv_timeout(Duration::from_secs(2)).unwrap(),
            "original"
        );
        assert!(matches!(
            start_save_job(&TEST_SAVE, "another".to_string(), |_| {}),
            Err(StartError::Busy)
        ));
        release_tx.send(()).unwrap();
        handle.join().unwrap();
        start_save_job(&TEST_SAVE, "next".to_string(), |_| {})
            .unwrap()
            .join()
            .unwrap();
    }

    #[test]
    fn exit_and_save_admission_are_mutually_exclusive() {
        static TEST_EXIT: SaveGate = SaveGate::new();
        let save = TEST_EXIT.start().unwrap();
        assert!(TEST_EXIT.prepare_exit().is_none());
        drop(save);
        let exit = TEST_EXIT.prepare_exit().unwrap();
        assert!(matches!(TEST_EXIT.start(), Err(StartError::Exiting)));
        assert!(TEST_EXIT.prepare_exit().is_none());
        drop(exit);
        assert!(TEST_EXIT.start().is_ok());
    }

    #[test]
    fn racing_exit_and_save_admit_only_one_owner() {
        static RACE_GATE: SaveGate = SaveGate::new();
        let barrier = std::sync::Barrier::new(3);
        let finish = std::sync::Barrier::new(3);
        let (sender, receiver) = mpsc::channel();
        std::thread::scope(|scope| {
            let sender_save = sender.clone();
            let start_save = &barrier;
            let finish_save = &finish;
            scope.spawn(move || {
                start_save.wait();
                let lease = RACE_GATE.start();
                sender_save.send(lease.is_ok()).unwrap();
                finish_save.wait();
            });
            scope.spawn(|| {
                barrier.wait();
                let lease = RACE_GATE.prepare_exit();
                sender.send(lease.is_some()).unwrap();
                finish.wait();
            });
            barrier.wait();
            let admitted = [receiver.recv().unwrap(), receiver.recv().unwrap()];
            assert_eq!(admitted.into_iter().filter(|value| *value).count(), 1);
            finish.wait();
        });
        assert!(RACE_GATE.start().is_ok());
    }

    #[test]
    fn slow_naming_falls_back_and_cannot_create_unbounded_workers() {
        static TEST_NAME: AtomicBool = AtomicBool::new(false);
        let (release_tx, release_rx) = mpsc::channel();
        let (cancel_tx, cancel_rx) = mpsc::channel();
        let result = bounded_filename_with(
            "some content",
            &TEST_NAME,
            Duration::from_millis(30),
            move |_, cancel| {
                release_rx.recv().unwrap();
                cancel_tx.send(cancel.load(Ordering::Acquire)).unwrap();
                "late.html".to_string()
            },
        );
        assert_eq!(result, DEFAULT_NAME);
        assert_eq!(
            bounded_filename_with("other", &TEST_NAME, Duration::ZERO, |_, _| {
                panic!("a second naming worker must not start")
            }),
            DEFAULT_NAME
        );
        release_tx.send(()).unwrap();
        assert!(cancel_rx.recv_timeout(Duration::from_secs(2)).unwrap());
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while TEST_NAME.load(Ordering::Acquire) && std::time::Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert!(!TEST_NAME.load(Ordering::Acquire));
    }
}
