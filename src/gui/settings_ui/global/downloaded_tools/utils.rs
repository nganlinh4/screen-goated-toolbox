use super::keyed_cache::{KeyedCache, OnDrop};
use crate::gui::theme::AppTheme;
use eframe::egui::{self, CornerRadius, Frame, Margin};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::time::Duration;

/// Themed Material container used for every downloaded-tools card, replacing the
/// default `ui.group(...)` so cards match the dialog surface (card fill, hairline
/// stroke, rounded corners, roomy padding).
pub(super) fn tool_card_frame(theme: &AppTheme) -> Frame {
    Frame::new()
        .fill(theme.card_bg())
        .stroke(theme.card_stroke())
        .corner_radius(CornerRadius::same(10))
        .inner_margin(Margin::same(crate::gui::theme::space::EDGE))
}

/// Render a tool card with the themed container, deriving the theme from `ui`.
pub(super) fn tool_card(ui: &mut egui::Ui, add_contents: impl FnOnce(&mut egui::Ui)) {
    let theme = AppTheme::from_ui(ui);
    tool_card_frame(&theme).show(ui, add_contents);
}

pub(super) fn render_install_progress(
    ui: &mut egui::Ui,
    text: &crate::gui::locale::LocaleText,
    progress: f32,
) {
    if progress < 0.0 {
        ui.label(text.auxiliary.managed_tools.tool_status_installing);
    } else {
        ui.label(format!("{progress:.0}%"));
    }
    ui.spinner();
    // Recheck cached install status even when an operation finishes quickly.
    ui.ctx().request_repaint_after(PROBE_CACHE_TTL);
}

pub(super) fn start_install(key: &'static str, install: impl FnOnce() + Send + 'static) {
    let Some(guard) = crate::install_progress::try_begin(key) else {
        return;
    };
    let result = std::thread::Builder::new()
        .name("sgt-tool-install".into())
        .spawn(move || {
            let _guard = guard;
            install();
        });
    if let Err(error) = result {
        crate::log_info!("[DownloadedTools] Could not start installation: {error}");
    }
}

const SIZE_CACHE_TTL: Duration = Duration::from_secs(30);
const PROBE_CACHE_TTL: Duration = Duration::from_secs(2);
const VALUE_CACHE_TTL: Duration = Duration::from_secs(30);

static SIZES: KeyedCache<PathBuf, u64> = KeyedCache::new(SIZE_CACHE_TTL, "sgt-tool-size");
static PROBES: KeyedCache<&'static str, bool> = KeyedCache::new(PROBE_CACHE_TTL, "sgt-tool-probe");
static VALUES: KeyedCache<&'static str, u64> = KeyedCache::new(VALUE_CACHE_TTL, "sgt-tool-value");
static REMOVING: LazyLock<Mutex<std::collections::HashSet<&'static str>>> =
    LazyLock::new(|| Mutex::new(std::collections::HashSet::new()));

pub(super) fn removal_in_progress(key: &'static str) -> bool {
    REMOVING
        .lock()
        .map(|removing| removing.contains(key))
        .unwrap_or(false)
}

pub(super) fn start_removal(
    key: &'static str,
    name: String,
    remove: impl FnOnce() -> anyhow::Result<()> + Send + 'static,
    after: impl FnOnce() + Send + 'static,
) {
    let error_name = name.clone();
    let spawned = spawn_removal(key, move || {
        let mut result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            show_removal_started(&name);
            remove()
        }))
        .unwrap_or_else(|_| Err(anyhow::anyhow!("component removal stopped unexpectedly")));
        if std::panic::catch_unwind(std::panic::AssertUnwindSafe(after)).is_err() {
            result = Err(anyhow::anyhow!(
                "component status refresh stopped unexpectedly"
            ));
        }
        show_removal_finished(&name, &result);
    });
    if let Err(error) = spawned {
        show_removal_finished(
            &error_name,
            &Err(anyhow::anyhow!(
                "could not start component removal: {error}"
            )),
        );
    }
}

/// Runs `job` on a removal thread while rows keyed by `key` show it as
/// removing. A second request for the same key while one runs is ignored.
fn spawn_removal(key: &'static str, job: impl FnOnce() + Send + 'static) -> std::io::Result<()> {
    let started = REMOVING
        .lock()
        .map(|mut removing| removing.insert(key))
        .unwrap_or(false);
    if !started {
        return Ok(());
    }
    let guard = OnDrop(Some(move || {
        REMOVING
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .remove(key);
        request_gui_repaint();
    }));
    std::thread::Builder::new()
        .name("sgt-tool-removal".into())
        .spawn(move || {
            let _guard = guard;
            job();
        })
        .map(|_| ())
}

fn show_removal_started(name: &str) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let title = crate::overlay::auto_copy_badge::format_locale(
        locale.removing_component_fmt,
        &[("name", name)],
    );
    crate::overlay::auto_copy_badge::show_detailed_notification(
        &title,
        "",
        crate::overlay::auto_copy_badge::NotificationType::Info,
    );
}

fn show_removal_finished(name: &str, result: &anyhow::Result<()>) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let (template, kind, detail) = match result {
        Ok(()) => (
            locale.component_removed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Success,
            String::new(),
        ),
        Err(error) => (
            locale.component_remove_failed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Error,
            format!("{error:#}"),
        ),
    };
    let title = crate::overlay::auto_copy_badge::format_locale(template, &[("name", name)]);
    crate::overlay::auto_copy_badge::show_detailed_notification(&title, &detail, kind);
}

pub(super) fn request_gui_repaint() {
    let context = crate::gui::GUI_CONTEXT
        .lock()
        .ok()
        .and_then(|context| context.clone());
    if let Some(context) = context {
        context.request_repaint();
    }
}

pub(super) fn cached_probe(
    key: &'static str,
    compute: impl FnOnce() -> bool + Send + 'static,
) -> bool {
    PROBES.get(key, compute)
}

pub(super) fn invalidate_probe_cache(key: &'static str) {
    PROBES.invalidate(&key);
}

pub(super) fn cached_u64(key: &'static str, compute: impl FnOnce() -> u64 + Send + 'static) -> u64 {
    VALUES.get(key, compute)
}

pub(super) fn invalidate_u64_cache(key: &'static str) {
    VALUES.invalidate(&key);
}

pub(super) fn clear_downloaded_tools_caches() {
    super::ai_runtime::invalidate_statuses();
    super::backgrounds::pack_status::invalidate_backgrounds();
    super::backgrounds::pack_status::invalidate_pointers();
    SIZES.clear();
    PROBES.clear();
    VALUES.clear();
    request_gui_repaint();
}

pub(super) fn get_dir_size(path: &Path) -> u64 {
    cached_dir_size(path)
}

pub(super) fn invalidate_size_cache(path: &Path) {
    SIZES.invalidate(&path.to_path_buf());
}

fn cached_dir_size(path: &Path) -> u64 {
    let compute_path = path.to_path_buf();
    SIZES.get(path.to_path_buf(), move || compute_dir_size(&compute_path))
}

fn compute_dir_size(path: &Path) -> u64 {
    let mut total_size = 0;
    if let Ok(entries) = fs::read_dir(path) {
        for entry in entries.flatten() {
            if let Ok(metadata) = entry.metadata() {
                if metadata.is_dir() {
                    total_size += compute_dir_size(&entry.path());
                } else {
                    total_size += metadata.len();
                }
            }
        }
    }
    total_size
}

pub(super) fn format_size(bytes: u64) -> String {
    let mb = bytes as f64 / 1024.0 / 1024.0;
    format!("{:.1} MB", mb)
}

#[cfg(test)]
pub(super) static CACHE_TEST_LOCK: Mutex<()> = Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::Ordering;
    use std::sync::mpsc;
    use std::time::Instant;

    fn wait_until_settled(key: &'static str) {
        let deadline = Instant::now() + Duration::from_secs(2);
        while PROBES.is_calculating(&key) {
            assert!(Instant::now() < deadline, "probe reservation stayed busy");
            std::thread::yield_now();
        }
    }

    #[test]
    fn completed_background_probe_wakes_ui_with_new_value() {
        let _lock = CACHE_TEST_LOCK.lock().unwrap();
        const KEY: &str = "downloaded-tools:test-background-probe-repaint";

        PROBES.forget(&KEY);
        let context = egui::Context::default();
        let (sender, receiver) = mpsc::channel();
        context.set_request_repaint_callback(move |_| {
            let _ = sender.send(());
        });
        *crate::gui::GUI_CONTEXT.lock().unwrap() = Some(context);

        let initial = cached_probe(KEY, || true);
        let repainted = receiver.recv_timeout(Duration::from_secs(1)).is_ok();
        let updated = cached_probe(KEY, || false);

        *crate::gui::GUI_CONTEXT.lock().unwrap() = None;
        PROBES.forget(&KEY);

        assert!(!initial);
        assert!(repainted);
        assert!(updated);
    }

    #[test]
    fn panicking_probe_releases_its_reservation() {
        let _lock = CACHE_TEST_LOCK.lock().unwrap();
        const KEY: &str = "downloaded-tools:test-panicking-probe";
        PROBES.forget(&KEY);
        let (sender, receiver) = mpsc::channel();
        cached_probe(KEY, move || {
            sender.send(()).unwrap();
            panic!("injected probe panic");
        });
        receiver.recv_timeout(Duration::from_secs(2)).unwrap();
        wait_until_settled(KEY);
        PROBES.forget(&KEY);
    }

    #[test]
    fn invalidating_one_tool_keeps_every_card_on_its_last_value() {
        let _lock = CACHE_TEST_LOCK.lock().unwrap();
        const REMOVED: &str = "downloaded-tools:test-invalidated-tool";
        const OTHER: &str = "downloaded-tools:test-untouched-tool";
        PROBES.forget(&REMOVED);
        PROBES.forget(&OTHER);
        cached_probe(REMOVED, || true);
        cached_probe(OTHER, || true);
        wait_until_settled(REMOVED);
        wait_until_settled(OTHER);

        invalidate_probe_cache(REMOVED);
        let (release, gate) = mpsc::channel::<()>();
        let while_recomputing = cached_probe(REMOVED, move || {
            let _ = gate.recv_timeout(Duration::from_secs(2));
            false
        });
        let other = cached_probe(OTHER, || false);
        release.send(()).unwrap();
        wait_until_settled(REMOVED);
        let recomputed = cached_probe(REMOVED, || true);
        PROBES.forget(&REMOVED);
        PROBES.forget(&OTHER);

        assert!(while_recomputing, "the card must keep its last value");
        assert!(other, "another tool's card must not be reset");
        assert!(
            !recomputed,
            "the invalidated key must pick up the new value"
        );
    }

    #[test]
    fn reservation_guard_runs_during_unwind() {
        let released = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let signal = released.clone();
        let result = std::panic::catch_unwind(move || {
            let _guard = OnDrop(Some(move || signal.store(true, Ordering::Release)));
            panic!("injected worker failure");
        });
        assert!(result.is_err());
        assert!(released.load(Ordering::Acquire));
    }

    #[test]
    fn committed_guard_cannot_release_a_later_reservation() {
        let busy = std::sync::Arc::new(Mutex::new(true));
        let owned = busy.clone();
        let mut guard = OnDrop(Some(move || *owned.lock().unwrap() = false));
        {
            let mut state = busy.lock().unwrap();
            *state = false;
            guard.0.take();
        }
        *busy.lock().unwrap() = true;
        drop(guard);
        assert!(*busy.lock().unwrap());
    }
}
