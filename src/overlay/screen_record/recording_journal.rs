//! Durable ownership before capture creates raw files and until project commit.
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::Ordering;

static IO: Mutex<()> = Mutex::new(());
const JOURNAL: &str = ".pending-recording.json";
const MEDIA: [(&str, &str); 4] = [
    ("videoFilePath", "videoUrl"),
    ("deviceAudioPath", "deviceAudioUrl"),
    ("micAudioPath", "micAudioUrl"),
    ("webcamVideoPath", "webcamVideoUrl"),
];

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Journal {
    version: u32,
    pub id: u64,
    pub paths: Vec<PathBuf>,
    pub result: Option<Value>,
    pub error: Option<String>,
}

struct Store(PathBuf);

impl Store {
    fn path(&self) -> PathBuf {
        self.0.join(JOURNAL)
    }

    fn normalize(&self, path: &Path) -> Result<PathBuf, String> {
        let root = self.0.canonicalize().map_err(|e| e.to_string())?;
        let parent = path
            .parent()
            .ok_or("Recording file has no parent")?
            .canonicalize()
            .map_err(|e| e.to_string())?;
        if parent != root {
            return Err("Recording ownership must remain inside recordings".into());
        }
        Ok(root.join(path.file_name().ok_or("Recording file has no name")?))
    }

    fn read(&self) -> Result<Option<Journal>, String> {
        let bytes = match std::fs::read(self.path()) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err(e.to_string()),
        };
        let journal: Journal = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
        if journal.version != 1 {
            return Err("Unknown recording journal version".into());
        }
        for path in &journal.paths {
            self.normalize(path)?;
        }
        if let Some(result) = &journal.result {
            let id = result["recordingId"]
                .as_str()
                .ok_or("Recording identity is missing")?;
            if id.len() != 32
                || !id
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
            {
                return Err("Recording identity is invalid".into());
            }
            for (path_key, _) in MEDIA {
                if let Some(path) = result[path_key].as_str().filter(|p| !p.is_empty()) {
                    let owned = self.normalize(Path::new(path))?;
                    if !journal.paths.contains(&owned) {
                        return Err("Recording result references an unowned file".into());
                    }
                }
            }
        }
        Ok(Some(journal))
    }

    fn write(&self, journal: &Journal) -> Result<(), String> {
        crate::atomic_json::write_json_atomic(&self.path(), journal).map_err(|e| e.to_string())
    }

    fn begin(&self, id: u64) -> Result<(), String> {
        std::fs::create_dir_all(&self.0).map_err(|e| e.to_string())?;
        if self.read()?.is_some() {
            return Err("A recording ownership journal still needs attention".into());
        }
        self.write(&Journal {
            version: 1,
            id,
            paths: Vec::new(),
            result: None,
            error: None,
        })
    }

    fn track(&self, path: &Path) -> Result<(), String> {
        let mut journal = self
            .read()?
            .ok_or("Recording ownership journal is unavailable")?;
        if journal.result.is_some() {
            return Err("Recording has already been finalized".into());
        }
        let path = self.normalize(path)?;
        if journal.paths.contains(&path) {
            return Ok(());
        }
        if path.try_exists().map_err(|e| e.to_string())? {
            return Err("Recording path already exists".into());
        }
        journal.paths.push(path);
        self.write(&journal)
    }

    fn ready(&self, result: &Value) -> Result<(), String> {
        let mut journal = self
            .read()?
            .ok_or("Recording ownership journal is unavailable")?;
        let mut result = result.clone();
        for (_, url) in MEDIA {
            result
                .as_object_mut()
                .ok_or("Invalid recording result")?
                .remove(url);
        }
        journal.result = Some(result);
        journal.error = None;
        self.write(&journal)
    }

    fn failed_start(&self, error: &str) -> Result<Option<Journal>, String> {
        let Some(mut journal) = self.read()? else {
            return Ok(None);
        };
        if !journal.paths.iter().any(|path| path.exists()) {
            self.clear()?;
            return Ok(None);
        }
        journal.error = Some(error.to_owned());
        self.write(&journal)?;
        Ok(Some(journal))
    }

    fn clear(&self) -> Result<(), String> {
        match std::fs::remove_file(self.path()) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }
}

fn with_store<T>(work: impl FnOnce(Store) -> Result<T, String>) -> Result<T, String> {
    let _lock = IO
        .lock()
        .map_err(|_| "Recording journal lock is unavailable")?;
    work(Store(
        crate::paths::app_runtime_local_data_dir().join("recordings"),
    ))
}

pub(super) fn read() -> Result<Option<Journal>, String> {
    with_store(|s| s.read())
}
pub(super) fn begin(id: u64) -> Result<(), String> {
    with_store(|s| s.begin(id))
}
pub(super) fn track(path: &Path) -> Result<(), String> {
    with_store(|s| s.track(path))
}
pub(super) fn ready(result: &Value) -> Result<(), String> {
    with_store(|s| s.ready(result))
}
pub(super) fn failed_start(error: &str) -> Result<Option<Journal>, String> {
    with_store(|s| s.failed_start(error))
}
pub(super) fn clear() -> Result<(), String> {
    with_store(|s| s.clear())
}

pub(super) fn refresh_media_urls(result: &Value) -> Result<Value, String> {
    let mut port = super::SERVER_PORT.load(Ordering::SeqCst);
    if port == 0 {
        port = super::ipc::start_global_media_server().map_err(|e| e.to_string())?;
    }
    let token = super::MEDIA_SERVER_TOKEN
        .get()
        .ok_or("Recording media server is unavailable")?;
    let mut value = result.clone();
    for (path, url) in MEDIA {
        value[url] = value[path]
            .as_str()
            .filter(|path| !path.is_empty())
            .map_or_else(
                || Value::String(String::new()),
                |path| {
                    Value::String(format!(
                        "http://localhost:{port}/?path={}&token={token}",
                        urlencoding::encode(path)
                    ))
                },
            );
    }
    Ok(value)
}

#[cfg(test)]
#[path = "recording_journal_tests.rs"]
mod tests;
