/// Launch the replacement directly; it waits for this process before startup.
pub(super) fn perform_restart(prepared: ExitPreparation) -> anyhow::Result<()> {
    let mut command =
        crate::runtime_support::restart_command(&std::env::current_exe()?, ["--restarted"])?;
    super::exit_requests::reserve_explicit_exit(|| command.spawn())?;
    exit_prepared(prepared)
}

/// Cleanly exit after the recorder has durably saved any active recording.
///
/// Flushes the window geometry first. `persist_geometry_when_settled` has
/// normally written it already; this catches a quit within the settle delay of
/// a resize. It cannot live in `App::on_exit` — `process::exit` never unwinds,
/// so eframe never sees the shutdown.
static EXIT_PREPARATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub(crate) struct ExitPreparation {
    _lock: std::sync::MutexGuard<'static, ()>,
    _settings: Option<super::settings_exit_gate::Exit<'static>>,
    _html_save: Option<crate::overlay::result::markdown_view::file_ops::HtmlSaveExitGuard>,
    thaw: fn(),
    _drain: Option<crate::overlay::tts_playground::ExitDrainGuard>,
}

impl Drop for ExitPreparation {
    fn drop(&mut self) {
        (self.thaw)();
    }
}

enum ExitPreparationError {
    Recorder(anyhow::Error),
    Draft(std::io::Error),
}

fn prepare_exit_steps<T>(
    recorder: impl FnOnce() -> anyhow::Result<()>,
    draft: impl FnOnce() -> std::io::Result<T>,
) -> std::result::Result<T, ExitPreparationError> {
    recorder().map_err(ExitPreparationError::Recorder)?;
    draft().map_err(ExitPreparationError::Draft)
}

/// Complete all fallible preparation before launching a restart/update helper.
/// Dropping the returned guard aborts exit and allows editing the draft again.
pub(crate) fn prepare_to_exit() -> Result<ExitPreparation, RestartPrepareDenied> {
    if super::settings_exit_gate::automatic_restart_blocked() {
        return Err(RestartPrepareDenied::Blocked);
    }
    let prepared = prepare_to_exit_detailed().map_err(|denied| match denied {
        DetailedDenied::Busy => RestartPrepareDenied::Busy,
        DetailedDenied::Conflict | DetailedDenied::Failed => RestartPrepareDenied::Blocked,
    })?;
    reject_startup_activation(
        prepared,
        super::settings_exit_gate::automatic_restart_blocked(),
    )
}

fn reject_startup_activation<T>(prepared: T, blocked: bool) -> Result<T, RestartPrepareDenied> {
    if blocked {
        Err(RestartPrepareDenied::Blocked)
    } else {
        Ok(prepared)
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum RestartPrepareDenied {
    Busy,
    Blocked,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum PrepareDenied {
    Retry,
    Failed,
}

pub(super) fn try_prepare_to_exit() -> Result<ExitPreparation, PrepareDenied> {
    prepare_to_exit_detailed().map_err(|denied| match denied {
        DetailedDenied::Busy | DetailedDenied::Conflict => PrepareDenied::Retry,
        DetailedDenied::Failed => PrepareDenied::Failed,
    })
}

enum DetailedDenied {
    Busy,
    Conflict,
    Failed,
}

fn prepare_to_exit_detailed() -> Result<ExitPreparation, DetailedDenied> {
    let lock = match EXIT_PREPARATION_LOCK.try_lock() {
        Ok(lock) => lock,
        Err(std::sync::TryLockError::Poisoned(error)) => error.into_inner(),
        Err(std::sync::TryLockError::WouldBlock) => return Err(DetailedDenied::Busy),
    };
    let settings = match super::settings_exit_gate::begin_exit() {
        Ok(settings) => settings,
        Err(super::settings_exit_gate::Denied::Conflict) => {
            let language = crate::APP
                .lock()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_default();
            let text = crate::gui::locale::LocaleText::get(&language);
            crate::overlay::auto_copy_badge::show_error_notification(
                text.shell.settings_save_failed_title,
            );
            super::signal_restore_window();
            return Err(DetailedDenied::Conflict);
        }
        Err(super::settings_exit_gate::Denied::Busy) => return Err(DetailedDenied::Busy),
    };
    let Some(html_save) = crate::overlay::result::markdown_view::file_ops::prepare_html_save_exit()
    else {
        crate::overlay::result::markdown_view::file_ops::notify_pending_save_exit();
        return Err(DetailedDenied::Failed);
    };
    let drain = match prepare_exit_steps(crate::overlay::screen_record::cleanup_on_app_exit, || {
        let drain = crate::overlay::tts_playground::drain_draft_for_exit()?;
        crate::overlay::tts_playground::freeze_draft_for_exit()?;
        Ok(drain)
    }) {
        Ok(drain) => drain,
        Err(error) => {
            let language = crate::APP
                .lock()
                .map(|app| app.config.ui_language.clone())
                .unwrap_or_default();
            let text = crate::gui::locale::LocaleText::get(&language);
            match error {
                ExitPreparationError::Recorder(error) => {
                    crate::log_info!("[Shutdown] recorder preparation failed: {error:#}");
                    crate::overlay::auto_copy_badge::show_error_notification(
                        text.shell.recorder_shutdown_failed,
                    );
                }
                ExitPreparationError::Draft(error) => {
                    crate::log_info!("[Shutdown] Could not save TTS draft: {error}");
                    crate::overlay::auto_copy_badge::show_notification(
                        text.shell.settings_save_failed_title,
                    );
                }
            }
            return Err(DetailedDenied::Failed);
        }
    };
    Ok(ExitPreparation {
        _lock: lock,
        _settings: Some(settings),
        _html_save: Some(html_save),
        thaw: crate::overlay::tts_playground::thaw_draft_after_aborted_exit,
        _drain: Some(drain),
    })
}

pub(crate) fn exit_prepared(_prepared: ExitPreparation) -> ! {
    crate::gui::window_state::save_main_window();
    release_owned_resources();
    crate::crash_report::exit(0, "quit")
}

thread_local! {
    /// The tray icon, registered by the thread that owns it.
    static TRAY_ICON: std::cell::RefCell<Option<tray_icon::TrayIcon>> =
        const { std::cell::RefCell::new(None) };
}

pub(crate) fn register_tray_icon(icon: &tray_icon::TrayIcon) {
    TRAY_ICON.with_borrow_mut(|slot| *slot = Some(icon.clone()));
}

/// What every exit owes the system. `process::exit` runs no destructors and
/// eframe never sees the shutdown, so nothing else releases these: a tray
/// icon left registered stays in the notification area until hovered, and
/// compositors killed with their job never record a clean exit.
pub(crate) fn release_owned_resources() {
    TRAY_ICON.with_borrow_mut(|slot| {
        if let Some(icon) = slot.take() {
            let _ = icon.set_visible(false);
        }
    });
    crate::overlay::screen_translate::subtitles::stop();
    let _ = crate::overlay::three_d_generator::shutdown();
    let _ = crate::overlay::image_to_svg::shutdown();
    crate::overlay::creation_runtime::shutdown();
    crate::overlay::result::scene_compositor::shutdown_for_exit();
    crate::overlay::status_compositor::shutdown_for_exit();
    crate::overlay::realtime_webview::shutdown_for_exit();
}

#[cfg(test)]
#[path = "exit_preparation_tests.rs"]
mod exit_preparation_tests;
