//! Durable process diagnostics, independent of the background session logger.
mod monitor;
mod storage;

#[cfg(test)]
mod tests;

use anyhow::Result;
use std::backtrace::Backtrace;
use std::io::Write;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicU32, Ordering};
use windows::Win32::System::Diagnostics::Debug::{EXCEPTION_POINTERS, SetUnhandledExceptionFilter};
use windows::Win32::System::Threading::GetCurrentThreadId;

pub(crate) use monitor::run_if_requested as run_monitor_if_requested;

const MAX_PANIC_REPORTS: u32 = 4;
static REPORTER: OnceLock<Reporter> = OnceLock::new();
static PANICS: AtomicU32 = AtomicU32::new(0);

struct Reporter {
    directory: PathBuf,
    identity: String,
    monitor: Option<monitor::Client>,
}

pub(crate) fn install() {
    // Install before hashing the image or starting the observer: bootstrap
    // failures must also have a synchronous reporting path.
    std::panic::set_hook(Box::new(move |info| {
        let number = PANICS.fetch_add(1, Ordering::Relaxed);
        let location = info.location().map_or_else(
            || "unknown".to_owned(),
            |location| {
                format!(
                    "{}:{}",
                    crate::initialization::sanitize_panic_source_path(location.file()),
                    location.line()
                )
            },
        );
        let payload = info
            .payload()
            .downcast_ref::<&str>()
            .copied()
            .or_else(|| info.payload().downcast_ref::<String>().map(String::as_str))
            .unwrap_or("unknown panic payload");
        let summary = format!("Rust panic: {payload}\nLocation: {location}");
        {
            // Preserve the first origin and rotate the remaining slots, so
            // earlier caught panics cannot suppress a later fatal report.
            let slot = if number == 0 {
                0
            } else {
                1 + (number - 1) % (MAX_PANIC_REPORTS - 1)
            };
            // Write the original failure first, before symbol resolution or any
            // later diagnostic work can itself fail.
            persist(&format!("panic-{slot}.txt"), &summary);
            let thread = std::thread::current();
            let stack = format!(
                "{summary}\nThread: {:?} name={:?} native_id={}\nStack:\n{}",
                thread.id(),
                thread.name(),
                unsafe { GetCurrentThreadId() },
                Backtrace::force_capture()
            );
            persist(&format!("panic-{slot}-stack.txt"), &stack);
            request_dump(0);
        }
        let _ = writeln!(std::io::stderr().lock(), "{summary}");
    }));

    if !cfg!(test) {
        match prepare() {
            Ok(reporter) => {
                let _ = REPORTER.set(reporter);
                std::thread::spawn(|| {
                    match storage::remove_legacy_dump_directory(&crate::paths::app_local_data_dir())
                    {
                        Ok(true) => record_event("[ProcessDiagnostic] removed legacy crash dumps"),
                        Ok(false) => {}
                        Err(error) => record_event(&format!(
                            "[ProcessDiagnostic] legacy crash dump removal deferred: {error}"
                        )),
                    }
                });
            }
            Err(error) => persist(
                "bootstrap-error.txt",
                &format!("diagnostic setup failed: {error:#}"),
            ),
        }
        unsafe {
            SetUnhandledExceptionFilter(Some(native_exception));
        }
    }
}

fn prepare() -> Result<Reporter> {
    let identity = storage::Identity::current()?;
    let directory = storage::create_session(&identity)?;
    let header = serde_json::to_string(&identity)?;
    let monitor = match monitor::start(&identity, &directory) {
        Ok(client) => Some(client),
        Err(error) => {
            storage::write_report(
                &directory.join("observer-error.txt"),
                &format!("{header}\n{error:#}"),
            )?;
            None
        }
    };
    Ok(Reporter {
        directory,
        identity: header,
        monitor,
    })
}

fn report_directory() -> PathBuf {
    REPORTER.get().map_or_else(
        || {
            crate::paths::app_sgt_dir()
                .join("crashes")
                .join(format!("bootstrap-{}", std::process::id()))
        },
        |reporter| reporter.directory.clone(),
    )
}

fn persist(name: &str, message: &str) {
    if cfg!(test) && REPORTER.get().is_none() {
        return;
    }
    let identity = REPORTER.get().map_or_else(
        || {
            format!(
                "pid={} version={} role={}",
                std::process::id(),
                env!("CARGO_PKG_VERSION"),
                storage::role()
            )
        },
        |reporter| reporter.identity.clone(),
    );
    let report = format!(
        "{}\n{identity}\n{message}\n",
        chrono::Local::now().to_rfc3339()
    );
    let path = report_directory().join(name);
    if let Err(error) = storage::write_report(&path, &report) {
        let _ = writeln!(
            std::io::stderr().lock(),
            "crash report write failed: {error:#}"
        );
    }
    crate::crash_report::record_event(&format!(
        "[ProcessDiagnostic] {identity} report={} {}",
        path.display(),
        message.lines().next().unwrap_or("")
    ));
}

/// Local-data reset: removes finished crash reports under `crash_root` and
/// legacy full-memory dumps under `local_app_data`. Reports of running
/// processes and files this module did not write are kept.
pub(crate) fn clear_for_reset(
    crash_root: &std::path::Path,
    local_app_data: &std::path::Path,
) -> Result<()> {
    let legacy = storage::remove_legacy_dump_directory(local_app_data);
    storage::purge_finished(crash_root)?;
    legacy.map(drop).map_err(Into::into)
}

pub(crate) fn record_event(message: &str) {
    if cfg!(test) && REPORTER.get().is_none() {
        return;
    }
    // Reports use their own synchronous, bounded file. Keeping this separate
    // also avoids relying on (or re-entering) the shared background log writer.
    let _ = storage::append_event(&report_directory(), message);
}

fn request_dump(exception: usize) {
    if let Some(client) = REPORTER
        .get()
        .and_then(|reporter| reporter.monitor.as_ref())
    {
        client.request_dump(exception);
    }
}

unsafe extern "system" fn native_exception(exception: *const EXCEPTION_POINTERS) -> i32 {
    // Exception pointers remain in the faulting process. The observer uses
    // ReadProcessMemory through MiniDumpWriteDump; it never dereferences them.
    request_dump(exception as usize);
    // Preserve normal Windows exception handling and any configured WER policy.
    0
}

pub(crate) fn record_exit_intent(code: i32, reason: &str) {
    persist(
        "exit-intent.txt",
        &format!("requested_exit_code=0x{:08X}\nreason={reason}", code as u32),
    );
}

pub(crate) fn exit(code: i32, reason: &str) -> ! {
    record_exit_intent(code, reason);
    std::process::exit(code)
}

pub(crate) fn record_main_result(result: &eframe::Result<std::process::ExitCode>) {
    if run_is_monitor() {
        return;
    }
    match result {
        Ok(code) => record_exit_intent(
            if *code == std::process::ExitCode::SUCCESS {
                0
            } else {
                1
            },
            "main-return",
        ),
        Err(error) => record_exit_intent(1, &format!("main-error: {error}")),
    }
}

fn run_is_monitor() -> bool {
    std::env::args_os()
        .nth(1)
        .is_some_and(|arg| arg == monitor::FLAG)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(Some(0)).collect()
}
