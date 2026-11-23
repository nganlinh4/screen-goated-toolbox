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
    pub architecture: String,
    pub profile: String,
}

impl Identity {
    pub fn current() -> Result<Self> {
        // The image hash is left to the observer process: hashing a large
        // executable here would put that work on every launch's critical path.
        let executable = std::env::current_exe().context("resolve diagnostic executable")?;
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

/// SHA-256 of the executable image. A running image cannot be replaced on
/// disk, so the observer's hash identifies the binary the process runs.
pub(super) fn image_sha256(executable: &Path) -> Result<String> {
    let mut image = File::open(executable).context("open diagnostic executable")?;
    let mut hash = Sha256::new();
    let mut buffer = vec![0; 64 * 1024];
    loop {
        let read = image.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hash.update(&buffer[..read]);
    }
    Ok(format!("{:x}", hash.finalize()))
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

/// Earlier builds wrote full-memory crash dumps under
/// `<local app data>/crash-dumps`. Nothing reads them and they can hold API
/// keys or screen text, so they are removed once found.
pub(super) fn remove_legacy_dump_directory(local_app_data: &Path) -> std::io::Result<bool> {
    let directory = local_app_data.join("crash-dumps");
    match std::fs::symlink_metadata(&directory) {
        Ok(metadata) if metadata.is_dir() && !is_reparse(&metadata) => {
            std::fs::remove_dir_all(&directory)?;
            Ok(true)
        }
        Ok(_) => Ok(false),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

pub(super) fn create_session(identity: &Identity) -> Result<PathBuf> {
    let root = root();
    std::fs::create_dir_all(&root).context("create diagnostic root")?;
    let directory = root.join(identity.session());
    std::fs::create_dir(&directory).context("create diagnostic session")?;
    write_report(
        &directory.join("identity.json"),
        &serde_json::to_string_pretty(identity)?,
    )?;
    // Retention problems belong to this session's own log, never to a new
    // unowned directory that nothing would ever prune.
    if let Err(error) = prune(&root) {
        let _ = append_event(&directory, &format!("[CrashRetention] deferred: {error:#}"));
    }
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
    thread_local! {
        static WRITING: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }
    struct Writing;
    impl Drop for Writing {
        fn drop(&mut self) {
            WRITING.with(|writing| writing.set(false));
        }
    }
    // Concurrent writers (several renderers exiting together) wait in turn. A
    // panic or exception that interrupts a writer on its own thread re-enters
    // here and is refused instead of waiting on a lock it already holds.
    if WRITING.with(|writing| writing.replace(true)) {
        bail!("diagnostic event writer re-entered");
    }
    let _writing = Writing;
    let _guard = WRITER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
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
    let mut first_error = None;
    for entry in std::fs::read_dir(root)? {
        // Another process may be pruning the same root: an entry that vanishes
        // or cannot be read is skipped instead of abandoning the whole pass.
        match scan_session(entry, &canonical_root) {
            Ok(Some(session)) => sessions.push(session),
            Ok(None) => {}
            Err(error) => {
                first_error.get_or_insert(error);
            }
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
        if let Err(error) = remove_session(&path, &canonical_root) {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Local-data reset: removes every finished session and other processes'
/// bootstrap logs. Sessions of running processes (this one included) and
/// anything this module did not write are preserved.
pub(super) fn purge_finished(root: &Path) -> Result<()> {
    let canonical_root = match root.canonicalize() {
        Ok(path) => path,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    let own_bootstrap = format!("bootstrap-{}", std::process::id());
    let mut first_error = None;
    for entry in std::fs::read_dir(root)? {
        let removed = entry.map_err(anyhow::Error::from).and_then(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if !name.starts_with("bootstrap-") {
                return match scan_session(Ok(entry), &canonical_root)? {
                    Some((_, _, _, path, _)) => remove_session(&path, &canonical_root),
                    None => Ok(()),
                };
            }
            let metadata = entry.metadata()?;
            let path = entry.path();
            if name != own_bootstrap
                && metadata.is_dir()
                && !is_reparse(&metadata)
                && owned_files(&path)?.is_some()
            {
                remove_session(&path, &canonical_root)?;
            }
            Ok(())
        });
        if let Err(error) = removed {
            first_error.get_or_insert(error);
        }
    }
    first_error.map_or(Ok(()), Err)
}

type ScannedSession = (bool, std::time::SystemTime, u64, PathBuf, u64);

/// Bootstrap logs are written before a session exists; once stale they hold
/// nothing a later launch needs.
const BOOTSTRAP_MAX_AGE: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 60 * 60);

fn scan_session(
    entry: std::io::Result<std::fs::DirEntry>,
    canonical_root: &Path,
) -> Result<Option<ScannedSession>> {
    let entry = match entry {
        Ok(entry) => entry,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    let path = entry.path();
    let metadata = match entry.metadata() {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.into()),
    };
    if !metadata.is_dir() || is_reparse(&metadata) {
        return Ok(None);
    }
    if entry
        .file_name()
        .to_string_lossy()
        .starts_with("bootstrap-")
    {
        let stale = metadata
            .modified()
            .ok()
            .and_then(|modified| modified.elapsed().ok())
            .is_some_and(|age| age >= BOOTSTRAP_MAX_AGE);
        if stale && owned_files(&path)?.is_some() {
            remove_session(&path, canonical_root)?;
        }
        return Ok(None);
    }
    let Ok(identity) = read_identity(&path) else {
        return Ok(None);
    };
    if entry.file_name() != identity.session().as_str()
        || super::monitor::process_may_be_alive(identity.pid, identity.created)
    {
        return Ok(None);
    }
    let Some((bytes, newest, has_panic)) = owned_files(&path)? else {
        return Ok(None);
    };
    let exit = std::fs::read_to_string(path.join("exit.txt")).unwrap_or_default();
    let incident = is_incident(&identity.role, has_panic, &exit);
    Ok(Some((incident, newest, identity.created, path, bytes)))
}

/// Sessions retention protects first. A crash artifact always counts. An
/// unexplained exit counts only for the host: renderers are torn down with
/// their job whenever the host retires or quits, and the host already logs
/// each renderer's exit status, so those teardowns must not evict the
/// evidence of a real crash.
fn is_incident(role: &str, has_panic: bool, exit: &str) -> bool {
    has_panic
        || (role == "host"
            && (!exit.contains("exit_code=0x00000000\n")
                || !exit.contains("exit_intent_recorded=true\n")))
}

/// Total bytes, newest modification and whether a crash artifact exists, or
/// `None` when the directory holds anything this module did not write.
fn owned_files(path: &Path) -> Result<Option<(u64, std::time::SystemTime, bool)>> {
    let mut bytes = 0_u64;
    let mut newest = std::time::UNIX_EPOCH;
    let mut has_panic = false;
    for file in std::fs::read_dir(path)? {
        let file = file?;
        let name = file.file_name();
        let name = name.to_string_lossy();
        let metadata = file.metadata()?;
        if !file.file_type()?.is_file() || is_reparse(&metadata) || !owned_file(&name) {
            return Ok(None);
        }
        bytes = bytes.saturating_add(metadata.len());
        newest = newest.max(metadata.modified()?);
        has_panic |= name.starts_with("panic-") || name == "native.dmp";
    }
    Ok(Some((bytes, newest, has_panic)))
}

fn remove_session(path: &Path, canonical_root: &Path) -> Result<()> {
    let resolved = match path.canonicalize() {
        Ok(resolved) => resolved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if resolved.parent() != Some(canonical_root) {
        bail!("diagnostic retention target escaped root");
    }
    // The files that identify and classify a session go last: one interrupted
    // part way (a file held open, or the process killed) stays recognisable,
    // keeps its routine/incident class and is retried by a later pass instead
    // of becoming an orphan or a false incident.
    const LAST: [&str; 2] = ["exit.txt", "identity.json"];
    let files = match std::fs::read_dir(&resolved) {
        Ok(files) => files,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error).context("list old diagnostic session"),
    };
    for file in files {
        let file = file?;
        if !LAST.iter().any(|name| file.file_name() == *name) {
            remove_if_present(&file.path(), |path| std::fs::remove_file(path))?;
        }
    }
    for name in LAST {
        remove_if_present(&resolved.join(name), |path| std::fs::remove_file(path))?;
    }
    remove_if_present(&resolved, |path| std::fs::remove_dir(path))
}

fn remove_if_present(path: &Path, remove: impl FnOnce(&Path) -> std::io::Result<()>) -> Result<()> {
    match remove(path) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => {
            Err(error).with_context(|| format!("prune old diagnostic file {}", path.display()))
        }
    }
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
            | super::monitor::PANIC_DUMP
            | super::monitor::LATEST_PANIC_DUMP
            | "native.dmp"
            | "snapshot.txt"
    ) || (0..super::MAX_PANIC_REPORTS)
        .any(|n| name == format!("panic-{n}.txt") || name == format!("panic-{n}-stack.txt"))
}

fn is_reparse(metadata: &std::fs::Metadata) -> bool {
    use std::os::windows::fs::MetadataExt;
    metadata.file_attributes() & 0x400 != 0
}

#[cfg(test)]
mod reset_tests {
    use super::*;

    #[test]
    fn reset_removes_finished_reports_but_keeps_live_and_unfamiliar_data() {
        let root = std::env::temp_dir().join(format!(
            "sgt-crash-reset-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let reports = root.join("SGT").join("crashes");
        std::fs::create_dir_all(&reports).unwrap();
        let write_session = |identity: &Identity, extra: &str| {
            let directory = reports.join(identity.session());
            std::fs::create_dir(&directory).unwrap();
            write_report(
                &directory.join("identity.json"),
                &serde_json::to_string(identity).unwrap(),
            )
            .unwrap();
            std::fs::write(directory.join(extra), "x").unwrap();
            directory
        };
        let mut identity = Identity::current().unwrap();
        let live = write_session(&identity, "events.log");
        identity.pid = u32::MAX;
        identity.created = 1;
        let finished = write_session(&identity, "panic-0.txt");
        identity.created = 2;
        let unfamiliar_session = write_session(&identity, "user-notes.txt");
        let other_bootstrap = reports.join("bootstrap-4294967295");
        std::fs::create_dir(&other_bootstrap).unwrap();
        std::fs::write(other_bootstrap.join("events.log"), "x").unwrap();
        let own_bootstrap = reports.join(format!("bootstrap-{}", std::process::id()));
        std::fs::create_dir(&own_bootstrap).unwrap();
        std::fs::write(own_bootstrap.join("events.log"), "x").unwrap();
        let legacy = root.join("crash-dumps");
        std::fs::create_dir_all(&legacy).unwrap();
        std::fs::write(legacy.join("old.dmp"), "dump").unwrap();

        let result = crate::crash_report::clear_for_reset(&reports, &root);
        let outcome = (
            live.join("identity.json").exists(),
            finished.exists(),
            unfamiliar_session.join("user-notes.txt").exists(),
            other_bootstrap.exists(),
            own_bootstrap.exists(),
            legacy.exists(),
        );
        std::fs::remove_dir_all(&root).unwrap();
        result.unwrap();
        assert_eq!(outcome, (true, false, true, false, true, false));
    }

    #[test]
    fn reset_without_any_reports_is_a_no_op() {
        let missing =
            std::env::temp_dir().join(format!("sgt-crash-reset-missing-{}", std::process::id()));
        assert!(!missing.exists());
        crate::crash_report::clear_for_reset(&missing.join("crashes"), &missing).unwrap();
    }
}
