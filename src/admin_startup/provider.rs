//! Scoped current-user startup registration and reversible provider transitions.

use anyhow::{Context, Result, anyhow, ensure};
use std::ffi::OsString;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

mod native_registry;
use native_registry::NativeRegistry;

const VALUE_NAME: &str = "ScreenGoatedToolbox";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const APPROVED_KEY: &str =
    r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run";
const REG_SZ: u32 = 1;
const REG_BINARY: u32 = 3;
const MAX_VALUE_BYTES: usize = 65_536;
const MAX_RUN_COMMAND_UNITS: usize = 260;
static TRANSITION_GATE: Mutex<()> = Mutex::new(());

#[derive(Debug)]
pub(crate) struct UncertainElevation(anyhow::Error);

impl std::fmt::Display for UncertainElevation {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Windows could not confirm the elevated startup result: {:#}",
            self.0
        )
    }
}

impl std::error::Error for UncertainElevation {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(self.0.as_ref())
    }
}

pub(crate) fn uncertain(error: anyhow::Error) -> anyhow::Error {
    UncertainElevation(error).into()
}

#[derive(Clone, Debug, PartialEq)]
struct RawValue {
    kind: u32,
    bytes: Vec<u8>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Snapshot {
    run: Option<RawValue>,
    approved: Option<RawValue>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Slot {
    Run,
    Approved,
}
impl Slot {
    fn key(self) -> &'static str {
        match self {
            Self::Run => RUN_KEY,
            Self::Approved => APPROVED_KEY,
        }
    }
    fn value(self, snapshot: &Snapshot) -> &Option<RawValue> {
        match self {
            Self::Run => &snapshot.run,
            Self::Approved => &snapshot.approved,
        }
    }
    fn set(self, snapshot: &mut Snapshot, value: Option<RawValue>) {
        match self {
            Self::Run => snapshot.run = value,
            Self::Approved => snapshot.approved = value,
        }
    }
}

trait Registry: Send + Sync {
    fn read(&self) -> Result<Snapshot>;
    fn write(&self, slot: Slot, value: &Option<RawValue>) -> Result<()>;
}

pub(crate) trait Provider {
    type Snapshot: Clone + PartialEq;
    fn capture(&self) -> Result<Self::Snapshot>;
    fn enabled_state(&self, snapshot: &Self::Snapshot) -> Result<bool>;
    fn enable_from(&self, expected: &Self::Snapshot) -> Result<Self::Snapshot>;
    fn disable_from(&self, expected: &Self::Snapshot) -> Result<Self::Snapshot>;
    fn restore(&self, written: &Self::Snapshot, previous: &Self::Snapshot) -> Result<()>;
}

#[derive(Clone)]
pub(crate) struct CurrentUserStartup {
    path: PathBuf,
    authorized: PathBuf,
    registry: Arc<dyn Registry>,
}

impl CurrentUserStartup {
    pub(crate) fn new(path: &Path, authorized: &str) -> Self {
        Self {
            path: path.to_path_buf(),
            authorized: PathBuf::from(authorized),
            registry: Arc::new(NativeRegistry),
        }
    }

    pub(crate) fn is_enabled(&self) -> Result<bool> {
        let _gate = gate()?;
        self.enabled_state(&self.capture()?)
    }

    fn validate(&self, snapshot: &Snapshot) -> Result<()> {
        if let Some(run) = &snapshot.run {
            let target = run_target(run)?;
            ensure!(
                super::same_path(&target, &self.path)
                    || super::same_path(&target, &self.authorized),
                super::Issue::TaskConflict
            );
        }
        approved_enabled(snapshot.approved.as_ref())?;
        Ok(())
    }

    fn apply(&self, written: &mut Snapshot, desired: &Snapshot) -> Result<()> {
        for slot in [Slot::Run, Slot::Approved] {
            ensure!(
                self.registry.read()? == *written,
                "The startup registry values changed"
            );
            if slot.value(written) == slot.value(desired) {
                continue;
            }
            let mut next = written.clone();
            slot.set(&mut next, slot.value(desired).clone());
            if let Err(error) = self.registry.write(slot, slot.value(desired)) {
                // A failing backend can have completed the individual value write.
                let observed = self.registry.read()?;
                if observed == next {
                    *written = next;
                } else {
                    ensure!(
                        observed == *written,
                        "The startup registry values changed during a failed write"
                    );
                }
                return Err(error);
            }
            *written = next;
        }
        ensure!(
            self.registry.read()? == *desired,
            "Windows did not retain the startup registry values"
        );
        Ok(())
    }

    fn replace(&self, expected: &Snapshot, desired: &Snapshot) -> Result<Snapshot> {
        let mut written = expected.clone();
        if let Err(error) = self.apply(&mut written, desired) {
            if written != *expected {
                let rollback = self.apply(&mut written, expected);
                return Err(rollback_error(error, rollback));
            }
            return Err(error);
        }
        Ok(written)
    }
}

impl Provider for CurrentUserStartup {
    type Snapshot = Snapshot;

    fn capture(&self) -> Result<Snapshot> {
        let snapshot = self.registry.read()?;
        self.validate(&snapshot)?;
        Ok(snapshot)
    }

    fn enabled_state(&self, snapshot: &Snapshot) -> Result<bool> {
        self.validate(snapshot)?;
        Ok(snapshot
            .run
            .as_ref()
            .is_some_and(|value| value.bytes.len() / 2 <= MAX_RUN_COMMAND_UNITS + 1)
            && approved_enabled(snapshot.approved.as_ref())?)
    }

    fn enable_from(&self, expected: &Snapshot) -> Result<Snapshot> {
        self.validate(expected)?;
        let desired = Snapshot {
            run: Some(run_value(&self.path)?),
            approved: Some(enabled_approval()),
        };
        self.replace(expected, &desired)
    }

    fn disable_from(&self, expected: &Snapshot) -> Result<Snapshot> {
        self.validate(expected)?;
        let mut desired = expected.clone();
        desired.run = None;
        self.replace(expected, &desired)
    }

    fn restore(&self, written: &Snapshot, previous: &Snapshot) -> Result<()> {
        self.validate(previous)?;
        self.replace(written, previous).map(|_| ())
    }
}

fn gate() -> Result<std::sync::MutexGuard<'static, ()>> {
    TRANSITION_GATE
        .lock()
        .map_err(|_| anyhow!("Startup provider transition is unavailable"))
}

fn rollback_error(original: anyhow::Error, rollback: Result<()>) -> anyhow::Error {
    match rollback {
        Ok(()) => original,
        Err(error) => anyhow!(
            "{original:#}; restoring the previous startup registration also failed: {error:#}"
        ),
    }
}

fn coordinate<P: Provider>(
    provider: &P,
    normal_enabled: bool,
    apply: impl FnOnce() -> Result<()>,
) -> Result<()> {
    let _gate = gate()?;
    let previous = provider.capture()?;
    let written = if normal_enabled {
        provider.enable_from(&previous)?
    } else {
        provider.disable_from(&previous)?
    };
    let mut apply_completed = false;
    let result = (|| {
        ensure!(
            provider.capture()? == written,
            "The startup provider changed before the transition"
        );
        ensure!(
            provider.enabled_state(&written)? == normal_enabled,
            "Windows did not apply the ordinary startup setting"
        );
        apply()?;
        apply_completed = true;
        ensure!(
            provider.capture()? == written,
            "The ordinary startup registration changed during the task operation"
        );
        Ok(())
    })();
    match result {
        Ok(()) => Ok(()),
        Err(error)
            if !normal_enabled
                && (apply_completed || error.downcast_ref::<UncertainElevation>().is_some()) =>
        {
            let error = if error.downcast_ref::<UncertainElevation>().is_some() {
                error
            } else {
                uncertain(error)
            };
            // A task may have committed before its service reply failed. Re-enabling
            // the ordinary provider would then introduce a second active provider.
            match provider.capture() {
                Ok(observed) if observed == written => Err(error),
                Ok(_) => {
                    Err(error
                        .context("Ordinary startup changed during the uncertain task operation"))
                }
                Err(inspection) => Err(error.context(format!(
                    "Ordinary startup could not be rechecked: {inspection:#}"
                ))),
            }
        }
        Err(error) => Err(rollback_error(error, provider.restore(&written, &previous))),
    }
}

/// Suspend the ordinary provider before changing elevated startup.
pub(crate) fn elevated<P: Provider>(
    provider: &P,
    apply: impl FnOnce() -> Result<()>,
) -> Result<()> {
    coordinate(provider, false, apply)
}

/// Establish ordinary startup before retiring an owned elevated registration.
pub(crate) fn normal<P: Provider>(provider: &P, retire: impl FnOnce() -> Result<()>) -> Result<()> {
    coordinate(provider, true, retire)
}

fn run_target(value: &RawValue) -> Result<PathBuf> {
    ensure!(
        value.kind == REG_SZ,
        "The startup Run entry has an unsupported registry type"
    );
    ensure!(
        value.bytes.len() <= MAX_VALUE_BYTES && value.bytes.len().is_multiple_of(2),
        "The startup Run entry has an invalid size"
    );
    let mut units = value
        .bytes
        .as_chunks::<2>()
        .0
        .iter()
        .map(|bytes| u16::from_le_bytes([bytes[0], bytes[1]]))
        .collect::<Vec<_>>();
    ensure!(
        units.pop() == Some(0) && !units.contains(&0),
        "The startup Run entry is not a terminated command"
    );
    let suffix = format!(" {}", crate::startup_launch::AUTOSTART_ARG)
        .encode_utf16()
        .collect::<Vec<_>>();
    let target = units
        .strip_suffix(suffix.as_slice())
        .context("The startup Run arguments do not match")?;
    let target = if target.first() == Some(&34) {
        target
            .strip_prefix(&[34])
            .and_then(|value| value.strip_suffix(&[34]))
            .context("The startup executable quoting is invalid")?
    } else {
        target
    };
    ensure!(
        !target.contains(&34),
        "The startup executable quoting is ambiguous"
    );
    let target = PathBuf::from(OsString::from_wide(target));
    ensure!(
        super::absolute_startup_path(&target),
        "The startup executable is not an absolute Windows path"
    );
    Ok(target)
}

fn run_value(path: &Path) -> Result<RawValue> {
    ensure!(
        super::absolute_startup_path(path) && path.file_name().is_some(),
        "An absolute Windows startup executable is required"
    );
    let target = path.as_os_str().encode_wide().collect::<Vec<_>>();
    ensure!(
        !target.contains(&0) && !target.contains(&34),
        "The startup executable path is invalid"
    );
    let mut units = vec![34];
    units.extend(target);
    units.push(34);
    units.extend(format!(" {}", crate::startup_launch::AUTOSTART_ARG).encode_utf16());
    ensure!(
        units.len() <= MAX_RUN_COMMAND_UNITS,
        super::Issue::PathTooLong
    );
    units.push(0);
    let bytes = units
        .into_iter()
        .flat_map(u16::to_le_bytes)
        .collect::<Vec<_>>();
    ensure!(
        bytes.len() <= MAX_VALUE_BYTES,
        "The startup executable command is too long"
    );
    Ok(RawValue {
        kind: REG_SZ,
        bytes,
    })
}

fn approved_enabled(value: Option<&RawValue>) -> Result<bool> {
    let Some(value) = value else { return Ok(true) };
    ensure!(
        value.kind == REG_BINARY && value.bytes.len() == 12,
        "The startup approval entry has an unsupported format"
    );
    match value.bytes[..4] {
        [2, 0, 0, 0] if value.bytes[4..].iter().all(|byte| *byte == 0) => Ok(true),
        [3, 0, 0, 0] => Ok(false),
        _ => anyhow::bail!("The startup approval state is unsupported"),
    }
}

fn enabled_approval() -> RawValue {
    let mut bytes = vec![0; 12];
    bytes[0] = 2;
    RawValue {
        kind: REG_BINARY,
        bytes,
    }
}

#[cfg(test)]
mod tests;
