use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const RETAIN_SESSIONS: usize = 8;
const RETAIN_BYTES: u64 = 128 * 1024 * 1024;
const REPORT_BYTES: usize = 256 * 1024;

#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Identity {
    pub pid: u32,
    pub created: u64,
    pub role: String,
    pub version: String,
    pub executable: PathBuf,
    pub image_base: String,
    pub sha256: String,
    pub architecture: String,
    pub profile: String,
}

impl Identity {
    pub fn current() -> Result<Self> {
        let executable = std::env::current_exe().context("resolve diagnostic executable")?;
        let mut image = File::open(&executable).context("open diagnostic executable")?;
        let mut hash = Sha256::new();
        let mut buffer = [0; 64 * 1024];
        loop {
            let read = image.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            hash.update(&buffer[..read]);
        }
        Ok(Self {
            pid: std::process::id(),
            created: super::monitor::current_creation_time()?,
            role: role().to_owned(),
            version: env!("CARGO_PKG_VERSION").to_owned(),
            executable,
            image_base: format!(
                "0x{:X}",
                unsafe { windows::Win32::System::LibraryLoader::GetModuleHandleW(None)? }.0
                    as usize
            ),
            sha256: format!("{:x}", hash.finalize()),
            architecture: std::env::consts::ARCH.to_owned(),
            profile: if cfg!(debug_assertions) {
                "debug"
            } else {
                "release"
            }
            .to_owned(),
        })
    }

    pub fn session(&self) -> String {
        format!("sgt-{}-{}", self.pid, self.created)
    }
}

pub(super) fn role() -> &'static str {
    for argument in std::env::args_os().skip(1) {
        match argument.to_str() {
            Some("--internal-result-compositor") => return "result-compositor",
            Some("--internal-status-compositor") => return "status-compositor",
            Some("--internal-realtime-compositor") => return "realtime-compositor",
            _ => {}
        }
    }
    "host"
}

pub(super) fn root() -> PathBuf {
    crate::paths::app_sgt_dir().join("crashes")
}

pub(super) fn create_session(identity: &Identity) -> Result<PathBuf> {
    let root = root();
    std::fs::create_dir_all(&root).context("create diagnostic root")?;
    if let Err(error) = prune(&root) {
        crate::crash_report::record_event(&format!("[CrashRetention] deferred: {error:#}"));
    }
    let directory = root.join(identity.session());
    std::fs::create_dir(&directory).context("create diagnostic session")?;
    write_report(
        &directory.join("identity.json"),
        &serde_json::to_string_pretty(identity)?,
    )?;
    Ok(directory)
}

pub(super) fn read_identity(directory: &Path) -> Result<Identity> {
    let mut content = String::new();
    File::open(directory.join("identity.json"))?
        .take(REPORT_BYTES as u64)
        .read_to_string(&mut content)?;
    serde_json::from_str(&content).context("decode diagnostic identity")
}

pub(super) fn write_report(path: &Path, message: &str) -> Result<()> {
    let parent = path.parent().context("diagnostic path has no parent")?;
    std::fs::create_dir_all(parent)?;
    let mut end = message.len().min(REPORT_BYTES);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    let mut file = OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open(path)
        .with_context(|| format!("open diagnostic report {}", path.display()))?;
    file.write_all(&message.as_bytes()[..end])
        .context("write diagnostic report")?;
    file.sync_all().context("sync diagnostic report")
}

pub(super) fn append_event(directory: &Path, message: &str) -> Result<()> {
    static WRITER: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _guard = WRITER
        .try_lock()
        .map_err(|_| anyhow::anyhow!("diagnostic event writer busy"))?;
    const LIMIT: usize = 64 * 1024;
    let path = directory.join("events.log");
    let mut content = String::new();
    match File::open(&path) {
        Ok(file) => {
            file.take(LIMIT as u64).read_to_string(&mut content)?;
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error).context("read diagnostic event log"),
    }
    let mut end = message.len().min(8 * 1024);
    while !message.is_char_boundary(end) {
        end -= 1;
    }
    let line = format!(
        "{} {}\n",
        chrono::Local::now().to_rfc3339(),
        message[..end].replace(['\r', '\n'], " | ")
    );
    if content.len() + line.len() > LIMIT {
        let start = content.len() + line.len() - LIMIT;
        let boundary = content.as_bytes()[start..]
            .iter()
            .position(|byte| *byte == b'\n')
            .map_or(content.len(), |index| start + index + 1);
        content.drain(..boundary);
    }
    content.push_str(&line);
    write_report(&path, &content)
}

// Only completed, verified SGT session directories are disposable. Running or
// uncertain state, legacy dumps, and unfamiliar files are preserved.
pub(super) fn prune(root: &Path) -> Result<()> {
    let canonical_root = root.canonicalize()?;
    let mut sessions = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let path = entry.path();
        if !entry.file_type()?.is_dir() || is_reparse(&entry.metadata()?) {
            continue;
        }
        let Ok(identity) = read_identity(&path) else {
            continue;
        };
        if entry.file_name() != identity.session().as_str()
            || super::monitor::process_may_be_alive(identity.pid, identity.created)
        {
            continue;
        }
        let mut bytes = 0_u64;
        let mut newest = std::time::UNIX_EPOCH;
        let mut owned = true;
        let mut has_panic = false;
        for file in std::fs::read_dir(&path)? {
            let file = file?;
            let name = file.file_name();
            let name = name.to_string_lossy();
            let metadata = file.metadata()?;
            if !file.file_type()?.is_file() || is_reparse(&metadata) || !owned_file(&name) {
                owned = false;
                break;
            }
            bytes = bytes.saturating_add(metadata.len());
            newest = newest.max(metadata.modified()?);
            has_panic |= name.starts_with("panic-") || name == "native.dmp";
        }
        if owned {
            let exit = std::fs::read_to_string(path.join("exit.txt")).unwrap_or_default();
            let incident = has_panic
                || !exit.contains("exit_code=0x00000000\n")
                || !exit.contains("exit_intent_recorded=true\n");
            sessions.push((incident, newest, identity.created, path, bytes));
        }
    }
    // Routine launches must not evict the only evidence of an earlier crash.
    sessions.sort_by_key(|(incident, newest, created, _, _)| {
        std::cmp::Reverse((*incident, *newest, *created))
    });
    let mut total = 0_u64;
    let mut retained = [0_usize; 2];
    for (incident, _, _, path, bytes) in sessions {
        let count = &mut retained[usize::from(incident)];
        if *count < RETAIN_SESSIONS && total.saturating_add(bytes) <= RETAIN_BYTES {
            *count += 1;
            total = total.saturating_add(bytes);
            continue;
        }
        let resolved = path.canonicalize()?;
        if resolved.parent() != Some(canonical_root.as_path()) {
            bail!("diagnostic retention target escaped root");
        }
        std::fs::remove_dir_all(&resolved).context("prune old diagnostic session")?;
    }
    Ok(())
}

fn owned_file(name: &str) -> bool {
    matches!(
        name,
        "identity.json"
            | "events.log"
            | "exit-intent.txt"
            | "exit.txt"
            | "observer-error.txt"
            | "snapshot.request"
            | "panic.dmp"
            | "native.dmp"
            | "snapshot.txt"
    ) || (0..super::MAX_PANIC_REPORTS)
        .any(|n| name == format!("panic-{n}.txt") || name == format!("panic-{n}-stack.txt"))
}

fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}
