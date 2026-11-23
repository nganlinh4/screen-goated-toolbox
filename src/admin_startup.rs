//! Elevated logon registration requires a protected executable and exact task identity.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU8, Ordering};

use anyhow::{Result, ensure};

mod identity;
mod protection;
pub(crate) mod provider;
mod scheduler;

use identity::{absolute_startup_path, local_absolute, same_executable, same_path};
use scheduler::TaskSnapshot;

const SOURCE: &str = "ScreenGoatedToolbox/startup/v1";
static LAST_ISSUE: AtomicU8 = AtomicU8::new(0);

#[derive(Clone, Copy, Debug, PartialEq)]
#[repr(u8)]
pub(crate) enum Issue {
    AdministratorRequired = 1,
    ProtectedLocationRequired,
    TaskConflict,
    OperationFailed,
    UncertainState,
    PathTooLong,
}

impl std::fmt::Display for Issue {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::AdministratorRequired => {
                "Administrator access is required to change elevated startup."
            }
            Self::ProtectedLocationRequired => {
                "Elevated startup requires an administrator-protected executable and directory."
            }
            Self::TaskConflict => {
                "The startup task does not match this application's registration."
            }
            Self::OperationFailed => "Windows could not verify the startup operation.",
            Self::UncertainState => {
                "Windows could not determine whether the startup operation committed."
            }
            Self::PathTooLong => "The startup command exceeds the supported Windows path limit.",
        })
    }
}
impl std::error::Error for Issue {}

pub(crate) fn last_issue() -> Option<Issue> {
    match LAST_ISSUE.load(Ordering::Acquire) {
        0 => None,
        1 => Some(Issue::AdministratorRequired),
        2 => Some(Issue::ProtectedLocationRequired),
        3 => Some(Issue::TaskConflict),
        5 => Some(Issue::UncertainState),
        6 => Some(Issue::PathTooLong),
        _ => Some(Issue::OperationFailed),
    }
}

pub(crate) fn previous_copy_is_missing(path: &str) -> bool {
    matches!(protection::reference_exists(Path::new(path)), Ok(false))
}

fn remember<T>(result: &Result<T>) {
    let issue = result.as_ref().err().map(|error| {
        if error
            .downcast_ref::<provider::UncertainElevation>()
            .is_some()
        {
            return Issue::UncertainState;
        }
        error.downcast_ref::<Issue>().copied().unwrap_or_else(|| {
            if error
                .downcast_ref::<windows::core::Error>()
                .is_some_and(|error| error.code() == windows::core::HRESULT::from_win32(5))
            {
                Issue::AdministratorRequired
            } else {
                Issue::OperationFailed
            }
        })
    });
    LAST_ISSUE.store(issue.map_or(0, |issue| issue as u8), Ordering::Release);
    if let Some(issue) = issue {
        crate::log_info!("[Startup] {issue}");
    }
}

trait Scheduler {
    fn query(&mut self) -> Result<Option<TaskSnapshot>>;
    fn register(&mut self, target: &Path, user_sid: &str, update: bool) -> Result<()>;
    fn disable(&mut self) -> Result<()>;
    fn delete(&mut self) -> Result<()>;
}

struct Native;

#[derive(Debug)]
struct NotSubmitted;
impl std::fmt::Display for NotSubmitted {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Task registration was not submitted")
    }
}

impl Scheduler for Native {
    fn query(&mut self) -> Result<Option<TaskSnapshot>> {
        scheduler::query()
    }
    fn register(&mut self, target: &Path, user_sid: &str, update: bool) -> Result<()> {
        let result = (|| {
            ensure!(
                crate::gui::utils::is_running_as_admin(),
                Issue::AdministratorRequired
            );
            scheduler::register(target, user_sid, SOURCE, update)
        })();
        result.map_err(|error| {
            if error
                .downcast_ref::<provider::UncertainElevation>()
                .is_some()
            {
                error
            } else {
                error.context(NotSubmitted)
            }
        })
    }
    fn disable(&mut self) -> Result<()> {
        scheduler::disable()
    }
    fn delete(&mut self) -> Result<()> {
        scheduler::delete()
    }
}

trait Protected {
    fn path(&self) -> &Path;
}
impl Protected for protection::ProtectedTarget {
    fn path(&self) -> &Path {
        self.path()
    }
}

fn current() -> Result<(PathBuf, String)> {
    ensure!(
        crate::paths::runtime_state_root().is_none(),
        Issue::OperationFailed
    );
    Ok((std::env::current_exe()?, scheduler::current_user_sid()?))
}

fn request(enable: bool, authorized_path: &str) -> Result<()> {
    let (path, sid) = current()?;
    if enable {
        enable_using(
            &mut Native,
            &path,
            Path::new(authorized_path),
            &sid,
            protection::open,
        )
    } else {
        delete_using(&mut Native, &path, Path::new(authorized_path), &sid)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Mode {
    Elevated,
    Normal,
    Off,
}

pub(crate) fn set_mode(
    launcher: &provider::CurrentUserStartup,
    mode: Mode,
    authorized_path: &str,
) -> bool {
    if crate::paths::runtime_state_root().is_some() {
        remember::<()>(&Err(Issue::OperationFailed.into()));
        return false;
    }
    let result = match mode {
        Mode::Elevated => provider::elevated(launcher, || request(true, authorized_path)),
        Mode::Normal => provider::normal(launcher, || request(false, authorized_path)),
        Mode::Off => provider::elevated(launcher, || request(false, authorized_path)),
    };
    remember(&result);
    result.is_ok()
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Reconciled {
    Missing,
    Enabled,
    OtherCopy,
    Disabled,
    Retired,
    Unknown,
}

pub(crate) fn reconcile(authorized_path: &str) -> Reconciled {
    let result = (|| {
        let (path, sid) = current()?;
        reconcile_using(
            &mut Native,
            &path,
            Path::new(authorized_path),
            &sid,
            protection::open,
        )
    })();
    remember(&result);
    match result {
        Ok(Reconciled::Retired) => {
            LAST_ISSUE.store(Issue::ProtectedLocationRequired as u8, Ordering::Release);
            Reconciled::Retired
        }
        Ok(state) => state,
        Err(_) => Reconciled::Unknown,
    }
}

pub(crate) fn enabled_for(path: &Path) -> bool {
    if crate::paths::runtime_state_root().is_some() || !local_absolute(path) {
        return false;
    }
    let result: Result<bool> = (|| {
        let sid = scheduler::current_user_sid()?;
        let Some(task) = Native.query()? else {
            return Ok(false);
        };
        Ok(task.enabled
            && owned(&task, path, Path::new(""), &sid)
            && same_executable(&task.target, path)
            && protection::validate_task_descriptor(&task.security_descriptor).is_ok()
            && protection::open(&task.target).is_ok())
    })();
    result.unwrap_or(false)
}

fn shape(task: &TaskSnapshot, sid: &str) -> bool {
    task.one_exec
        && task.one_logon
        && task.highest
        && task.interactive
        && task.principal_sid == sid
        && task.args == crate::startup_launch::AUTOSTART_ARG
        && absolute_startup_path(&task.target)
        && (task.working_directory.as_os_str().is_empty()
            || task
                .target
                .parent()
                .is_some_and(|parent| same_path(parent, &task.working_directory)))
}

fn owned(task: &TaskSnapshot, current: &Path, authorized: &Path, sid: &str) -> bool {
    if !shape(task, sid) {
        return false;
    }
    match task.source.as_deref() {
        Some(SOURCE) => true,
        None | Some("") => {
            same_path(&task.target, current)
                || same_path(&task.target, authorized)
                || same_executable(&task.target, current)
        }
        Some(_) => false,
    }
}

fn unchanged(backend: &mut impl Scheduler, expected: &Option<TaskSnapshot>) -> Result<()> {
    ensure!(&backend.query()? == expected, Issue::TaskConflict);
    Ok(())
}

fn enable_using<G: Protected>(
    backend: &mut impl Scheduler,
    current: &Path,
    authorized: &Path,
    sid: &str,
    mut protect: impl FnMut(&Path) -> Result<G>,
) -> Result<()> {
    ensure!(local_absolute(current), Issue::ProtectedLocationRequired);
    let guarded = protect(current).map_err(|_| Issue::ProtectedLocationRequired)?;
    let before = backend.query()?;
    ensure!(
        before
            .as_ref()
            .is_none_or(|task| owned(task, current, authorized, sid)),
        Issue::TaskConflict
    );
    let previous_guard = before
        .as_ref()
        .filter(|task| task.enabled && local_absolute(&task.target))
        .and_then(|task| protect(&task.target).ok());
    let previous_safe = previous_guard.is_some()
        && before.as_ref().is_some_and(|task| {
            protection::validate_task_descriptor(&task.security_descriptor).is_ok()
        });
    unchanged(backend, &before)?;
    if before.as_ref().is_some_and(|task| {
        task.enabled
            && task.source.as_deref() == Some(SOURCE)
            && same_path(&task.target, guarded.path())
            && protection::validate_task_descriptor(&task.security_descriptor).is_ok()
    }) {
        return Ok(());
    }
    if let Err(error) = backend.register(guarded.path(), sid, before.is_some()) {
        return Err(resolve_registration_failure(
            backend,
            &before,
            previous_safe,
            guarded.path(),
            sid,
            error,
        ));
    }
    let after = match backend.query() {
        Ok(Some(after)) => after,
        result => {
            let error = result
                .err()
                .unwrap_or_else(|| Issue::OperationFailed.into());
            return Err(resolve_registration_failure(
                backend,
                &before,
                previous_safe,
                guarded.path(),
                sid,
                error,
            ));
        }
    };
    let identity_matches = shape(&after, sid)
        && after.source.as_deref() == Some(SOURCE)
        && same_path(&after.target, guarded.path());
    if !identity_matches
        || !after.enabled
        || protection::validate_task_descriptor(&after.security_descriptor).is_err()
    {
        // Contain a positively identified registration whose postcondition failed.
        if identity_matches && after.enabled {
            unchanged(backend, &Some(after.clone())).map_err(provider::uncertain)?;
            backend.disable().map_err(provider::uncertain)?;
            let mut disabled = after;
            disabled.enabled = false;
            let observed = backend.query().map_err(provider::uncertain)?;
            if observed.as_ref() != Some(&disabled) {
                return Err(provider::uncertain(Issue::OperationFailed.into()));
            }
        } else if !identity_matches {
            return Err(provider::uncertain(Issue::TaskConflict.into()));
        }
        return Err(Issue::OperationFailed.into());
    }
    Ok(())
}

fn resolve_registration_failure(
    backend: &mut impl Scheduler,
    previous: &Option<TaskSnapshot>,
    previous_safe: bool,
    target: &Path,
    sid: &str,
    original: anyhow::Error,
) -> anyhow::Error {
    if original.downcast_ref::<NotSubmitted>().is_some() && (previous.is_none() || previous_safe) {
        return original;
    }
    let resolve = (|| -> Result<()> {
        let observed = backend.query()?;
        let Some(task) = &observed else {
            return Ok(());
        };
        if !task.enabled || (previous_safe && &observed == previous) {
            return Ok(());
        }
        let known_previous = &observed == previous;
        let known_new = shape(task, sid)
            && task.source.as_deref() == Some(SOURCE)
            && same_path(&task.target, target);
        ensure!(known_previous || known_new, Issue::TaskConflict);
        unchanged(backend, &observed)?;
        backend.disable()?;
        let mut disabled = task.clone();
        disabled.enabled = false;
        ensure!(
            backend.query()?.as_ref() == Some(&disabled),
            Issue::OperationFailed
        );
        Ok(())
    })();
    match resolve {
        Ok(()) => original,
        Err(error) => provider::uncertain(
            original.context(format!("The task outcome remains unverified: {error:#}")),
        ),
    }
}

fn delete_using(
    backend: &mut impl Scheduler,
    current: &Path,
    authorized: &Path,
    sid: &str,
) -> Result<()> {
    let before = backend.query()?;
    let Some(task) = &before else { return Ok(()) };
    ensure!(owned(task, current, authorized, sid), Issue::TaskConflict);
    if !task.enabled && task.source.as_deref() == Some(SOURCE) {
        return Ok(());
    }
    unchanged(backend, &before)?;
    backend.delete()?;
    ensure!(backend.query()?.is_none(), Issue::OperationFailed);
    Ok(())
}

fn reconcile_using<G: Protected>(
    backend: &mut impl Scheduler,
    current: &Path,
    authorized: &Path,
    sid: &str,
    protect: impl FnOnce(&Path) -> Result<G>,
) -> Result<Reconciled> {
    let before = backend.query()?;
    let Some(task) = &before else {
        return Ok(Reconciled::Missing);
    };
    ensure!(owned(task, current, authorized, sid), Issue::TaskConflict);
    if !task.enabled {
        return Ok(Reconciled::Disabled);
    }
    if local_absolute(&task.target)
        && let Ok(_guard) = protect(&task.target)
        && protection::validate_task_descriptor(&task.security_descriptor).is_ok()
    {
        return Ok(
            if same_path(&task.target, current) || same_executable(&task.target, current) {
                Reconciled::Enabled
            } else {
                Reconciled::OtherCopy
            },
        );
    }
    // A legacy stored path is retirement authority only. It never authorizes execution.
    unchanged(backend, &before)?;
    backend.disable()?;
    let after = backend.query()?.ok_or(Issue::OperationFailed)?;
    let mut expected = task.clone();
    expected.enabled = false;
    ensure!(after == expected, Issue::TaskConflict);
    Ok(Reconciled::Retired)
}

#[cfg(test)]
mod tests;
