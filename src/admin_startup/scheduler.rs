//! Native Task Scheduler access. Ownership and target policy belong to the caller.

use anyhow::{Context, Result, ensure};
use std::ffi::OsString;
use std::marker::PhantomData;
use std::mem::size_of;
use std::os::windows::ffi::{OsStrExt, OsStringExt};
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use windows::Win32::Foundation::{
    ERROR_FILE_NOT_FOUND, ERROR_INSUFFICIENT_BUFFER, HANDLE, HLOCAL, LocalFree, RPC_E_CHANGED_MODE,
    VARIANT_FALSE, VARIANT_TRUE,
};
use windows::Win32::Security::Authorization::{ConvertSidToStringSidW, ConvertStringSidToSidW};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, GROUP_SECURITY_INFORMATION, GetTokenInformation, IsValidSid,
    LookupAccountNameW, OWNER_SECURITY_INFORMATION, PSID, SECURITY_MAX_SID_SIZE, SID_NAME_USE,
    TOKEN_QUERY, TOKEN_USER, TokenUser,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoUninitialize,
};
use windows::Win32::System::TaskScheduler::{
    IExecAction, ILogonTrigger, IRegisteredTask, ITaskDefinition, ITaskFolder, ITaskService,
    TASK_ACTION_EXEC, TASK_CREATE, TASK_DONT_ADD_PRINCIPAL_ACE, TASK_INSTANCES_IGNORE_NEW,
    TASK_LOGON_INTERACTIVE_TOKEN, TASK_RUNLEVEL_HIGHEST, TASK_TRIGGER_LOGON, TASK_UPDATE,
    TaskScheduler,
};
use windows::Win32::System::Threading::{GetCurrentProcess, OpenProcessToken};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{BSTR, HRESULT, Interface, PCWSTR, PWSTR};

const TASK_NAME: &str = "ScreenGoatedToolbox_AutoStart";
const PRINCIPAL_ID: &str = "SGTUser";
const MAX_TEXT_UNITS: usize = 32_768;
const MAX_TASK_PATH_UNITS: usize = 260;

#[derive(Clone, Debug, PartialEq)]
pub(super) struct TaskSnapshot {
    pub(super) target: PathBuf,
    pub(super) args: String,
    pub(super) working_directory: PathBuf,
    pub(super) principal_sid: String,
    pub(super) highest: bool,
    pub(super) interactive: bool,
    pub(super) one_exec: bool,
    pub(super) one_logon: bool,
    pub(super) source: Option<String>,
    pub(super) enabled: bool,
    pub(super) security_descriptor: String,
}

struct Apartment {
    acquired: bool,
    _thread: PhantomData<Rc<()>>,
}

impl Apartment {
    fn enter() -> Result<Self> {
        let status = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
        ensure!(
            status.is_ok() || status == RPC_E_CHANGED_MODE,
            "Initialize startup scheduler COM apartment: {status}"
        );
        Ok(Self {
            acquired: status.is_ok(),
            _thread: PhantomData,
        })
    }
}

impl Drop for Apartment {
    fn drop(&mut self) {
        if self.acquired {
            unsafe { CoUninitialize() };
        }
    }
}

struct Scheduler {
    folder: ITaskFolder,
    service: ITaskService,
    // Interfaces must be released before the apartment is uninitialized.
    _apartment: Apartment,
}

impl Scheduler {
    fn open() -> Result<Self> {
        let apartment = Apartment::enter()?;
        unsafe {
            let service: ITaskService =
                CoCreateInstance(&TaskScheduler, None, CLSCTX_INPROC_SERVER)
                    .context("Create startup scheduler service")?;
            let empty = VARIANT::default();
            service
                .Connect(&empty, &empty, &empty, &empty)
                .context("Connect to the local startup scheduler")?;
            let folder = service
                .GetFolder(&BSTR::from("\\"))
                .context("Open the startup task folder")?;
            Ok(Self {
                folder,
                service,
                _apartment: apartment,
            })
        }
    }

    fn task(&self) -> Result<Option<IRegisteredTask>> {
        match unsafe { self.folder.GetTask(&BSTR::from(TASK_NAME)) } {
            Ok(task) => Ok(Some(task)),
            Err(error) if task_missing(error.code()) => Ok(None),
            Err(error) => Err(error).context("Read the startup task"),
        }
    }
}

fn task_missing(code: HRESULT) -> bool {
    code == HRESULT::from_win32(ERROR_FILE_NOT_FOUND.0)
}

fn read_bstr(get: impl FnOnce(*mut BSTR) -> windows::core::Result<()>) -> Result<BSTR> {
    let mut value = BSTR::new();
    get(&mut value)?;
    checked_bstr(value)
}

fn checked_bstr(value: BSTR) -> Result<BSTR> {
    ensure!(
        value.len() <= MAX_TEXT_UNITS,
        "Startup task field is too long"
    );
    ensure!(
        !value.contains(&0),
        "Startup task field contains a null character"
    );
    Ok(value)
}

fn text(get: impl FnOnce(*mut BSTR) -> windows::core::Result<()>) -> Result<String> {
    String::try_from(read_bstr(get)?).context("Startup task field is not valid UTF-16")
}

fn path(get: impl FnOnce(*mut BSTR) -> windows::core::Result<()>) -> Result<PathBuf> {
    Ok(PathBuf::from(OsString::from_wide(&read_bstr(get)?)))
}

fn native_path(value: &Path) -> Result<BSTR> {
    let wide: Vec<u16> = value.as_os_str().encode_wide().collect();
    ensure!(
        !wide.is_empty() && wide.len() <= MAX_TASK_PATH_UNITS,
        super::Issue::PathTooLong
    );
    ensure!(
        !wide.contains(&0),
        "Startup executable path contains a null character"
    );
    Ok(BSTR::from_wide(&wide))
}

pub(super) fn query() -> Result<Option<TaskSnapshot>> {
    let scheduler = Scheduler::open()?;
    let Some(task) = scheduler.task()? else {
        return Ok(None);
    };
    unsafe {
        let definition = task
            .Definition()
            .context("Read the startup task definition")?;
        let principal = definition.Principal()?;
        let principal_user = text(|out| principal.UserId(out))?;
        let principal_sid = resolve_principal_sid(&principal_user)?;
        let group = text(|out| principal.GroupId(out))?;
        let mut runlevel = Default::default();
        principal.RunLevel(&mut runlevel)?;
        let mut logon_type = Default::default();
        principal.LogonType(&mut logon_type)?;
        let actions = definition.Actions()?;
        let mut action_count = 0;
        actions.Count(&mut action_count)?;
        let (one_exec, target, args, working_directory) = if action_count == 1 {
            let action = actions.get_Item(1)?;
            let mut kind = Default::default();
            action.Type(&mut kind)?;
            if kind == TASK_ACTION_EXEC {
                let exec: IExecAction = action.cast()?;
                (
                    true,
                    path(|out| exec.Path(out))?,
                    text(|out| exec.Arguments(out))?,
                    path(|out| exec.WorkingDirectory(out))?,
                )
            } else {
                (false, PathBuf::new(), String::new(), PathBuf::new())
            }
        } else {
            (false, PathBuf::new(), String::new(), PathBuf::new())
        };
        let one_logon = one_logon(&definition, &principal_sid)?;
        let registration = definition.RegistrationInfo()?;
        let source = text(|out| registration.Source(out))?;
        let information = OWNER_SECURITY_INFORMATION.0
            | GROUP_SECURITY_INFORMATION.0
            | DACL_SECURITY_INFORMATION.0;
        let descriptor = checked_bstr(task.GetSecurityDescriptor(information as i32)?)?;
        let security_descriptor = String::try_from(descriptor)
            .context("Startup task security descriptor is not valid UTF-16")?;
        Ok(Some(TaskSnapshot {
            target,
            args,
            working_directory,
            principal_sid,
            highest: runlevel == TASK_RUNLEVEL_HIGHEST,
            interactive: logon_type == TASK_LOGON_INTERACTIVE_TOKEN && group.is_empty(),
            one_exec,
            one_logon,
            source: (!source.is_empty()).then_some(source),
            enabled: task.Enabled()?.as_bool(),
            security_descriptor,
        }))
    }
}

fn one_logon(definition: &ITaskDefinition, principal_sid: &str) -> Result<bool> {
    unsafe {
        let triggers = definition.Triggers()?;
        let mut count = 0;
        triggers.Count(&mut count)?;
        if count != 1 {
            return Ok(false);
        }
        let trigger = triggers.get_Item(1)?;
        let mut kind = Default::default();
        trigger.Type(&mut kind)?;
        let mut enabled = VARIANT_FALSE;
        trigger.Enabled(&mut enabled)?;
        if kind != TASK_TRIGGER_LOGON || !enabled.as_bool() {
            return Ok(false);
        }
        let logon: ILogonTrigger = trigger.cast()?;
        let user = text(|out| logon.UserId(out))?;
        if !user.is_empty() && resolve_principal_sid(&user)? != principal_sid {
            return Ok(false);
        }
        let delay = text(|out| logon.Delay(out))?;
        let repetition = trigger.Repetition()?;
        let interval = text(|out| repetition.Interval(out))?;
        let duration = text(|out| repetition.Duration(out))?;
        Ok(zero_duration(&delay) && zero_duration(&interval) && zero_duration(&duration))
    }
}

fn zero_duration(value: &str) -> bool {
    if value.is_empty() {
        return true;
    }
    // Task Scheduler uses xs:duration: ordered date/time components, with
    // fractions allowed only for seconds. Compare digits without rounding.
    let value = value.trim_matches([' ', '\t', '\r', '\n']);
    let value = value.strip_prefix('-').unwrap_or(value);
    let Some(mut remaining) = value.strip_prefix('P') else {
        return false;
    };
    let mut time = false;
    let mut time_component = false;
    let mut last_rank = 0;
    while !remaining.is_empty() {
        if let Some(rest) = remaining.strip_prefix('T') {
            if time {
                return false;
            }
            time = true;
            remaining = rest;
            continue;
        }
        let bytes = remaining.as_bytes();
        let whole = bytes
            .iter()
            .take_while(|byte| byte.is_ascii_digit())
            .count();
        if whole == 0 || bytes[..whole].iter().any(|byte| *byte != b'0') {
            return false;
        }
        let mut end = whole;
        let fractional = bytes.get(end) == Some(&b'.');
        if fractional {
            end += 1;
            let start = end;
            while bytes.get(end).is_some_and(u8::is_ascii_digit) {
                end += 1;
            }
            if end == start || bytes[start..end].iter().any(|byte| *byte != b'0') {
                return false;
            }
        }
        let rank = match (time, bytes.get(end)) {
            (false, Some(b'Y')) => 1,
            (false, Some(b'M')) => 2,
            (false, Some(b'D')) => 3,
            (true, Some(b'H')) => 4,
            (true, Some(b'M')) => 5,
            (true, Some(b'S')) => 6,
            _ => return false,
        };
        if rank <= last_rank || (fractional && rank != 6) {
            return false;
        }
        last_rank = rank;
        time_component |= time;
        remaining = &remaining[end + 1..];
    }
    last_rank != 0 && (!time || time_component)
}

pub(super) fn register(
    target: &Path,
    user_sid: &str,
    source: &str,
    replace_existing: bool,
) -> Result<()> {
    ensure!(
        target.is_absolute(),
        "Startup executable must have an absolute path"
    );
    ensure!(
        target.file_name().is_some(),
        "Startup executable path has no file name"
    );
    let parent = target
        .parent()
        .context("Startup executable has no parent directory")?;
    let target = native_path(target)?;
    let parent = native_path(parent)?;
    ensure!(
        !source.is_empty() && !source.contains('\0'),
        "Startup task source is invalid"
    );
    let user_sid = parse_sid(user_sid)?;
    let descriptor = task_descriptor(&user_sid);
    let scheduler = Scheduler::open()?;
    unsafe {
        let definition = scheduler.service.NewTask(0)?;
        let principal = definition.Principal()?;
        principal.SetId(&BSTR::from(PRINCIPAL_ID))?;
        principal.SetUserId(&BSTR::from(&user_sid))?;
        principal.SetLogonType(TASK_LOGON_INTERACTIVE_TOKEN)?;
        principal.SetRunLevel(TASK_RUNLEVEL_HIGHEST)?;
        definition
            .RegistrationInfo()?
            .SetSource(&BSTR::from(source))?;
        let settings = definition.Settings()?;
        settings.SetEnabled(VARIANT_TRUE)?;
        settings.SetMultipleInstances(TASK_INSTANCES_IGNORE_NEW)?;
        settings.SetDisallowStartIfOnBatteries(VARIANT_FALSE)?;
        settings.SetStopIfGoingOnBatteries(VARIANT_FALSE)?;
        settings.SetExecutionTimeLimit(&BSTR::from("PT0S"))?;
        let trigger = definition.Triggers()?.Create(TASK_TRIGGER_LOGON)?;
        trigger.SetEnabled(VARIANT_TRUE)?;
        let logon: ILogonTrigger = trigger.cast()?;
        logon.SetUserId(&BSTR::from(&user_sid))?;
        let actions = definition.Actions()?;
        actions.SetContext(&BSTR::from(PRINCIPAL_ID))?;
        let exec: IExecAction = actions.Create(TASK_ACTION_EXEC)?.cast()?;
        exec.SetPath(&target)?;
        exec.SetArguments(&BSTR::from(crate::startup_launch::AUTOSTART_ARG))?;
        exec.SetWorkingDirectory(&parent)?;
        scheduler
            .folder
            .RegisterTaskDefinition(
                &BSTR::from(TASK_NAME),
                &definition,
                registration_flags(replace_existing),
                &VARIANT::from(user_sid.as_str()),
                &VARIANT::default(),
                TASK_LOGON_INTERACTIVE_TOKEN,
                &VARIANT::from(descriptor.as_str()),
            )
            .map_err(|error| {
                super::provider::uncertain(
                    anyhow::Error::from(error).context("Register the protected startup task"),
                )
            })?;
    }
    Ok(())
}

fn registration_flags(replace_existing: bool) -> i32 {
    let mode = if replace_existing {
        TASK_UPDATE
    } else {
        TASK_CREATE
    };
    mode.0 | TASK_DONT_ADD_PRINCIPAL_ACE.0
}

fn task_descriptor(user_sid: &str) -> String {
    format!("O:BAG:SYD:P(A;;FA;;;SY)(A;;FA;;;BA)(A;;FRFX;;;{user_sid})")
}

pub(super) fn disable() -> Result<()> {
    let scheduler = Scheduler::open()?;
    let task = scheduler
        .task()?
        .context("The startup task no longer exists")?;
    unsafe { task.SetEnabled(VARIANT_FALSE) }.context("Disable the startup task")
}

pub(super) fn delete() -> Result<()> {
    let scheduler = Scheduler::open()?;
    unsafe { scheduler.folder.DeleteTask(&BSTR::from(TASK_NAME), 0) }
        .context("Delete the startup task")
}

struct LocalAllocation(*mut std::ffi::c_void);

impl Drop for LocalAllocation {
    fn drop(&mut self) {
        unsafe { LocalFree(Some(HLOCAL(self.0))) };
    }
}

fn sid_text(sid: PSID) -> Result<String> {
    ensure!(
        unsafe { IsValidSid(sid) }.as_bool(),
        "Startup principal SID is invalid"
    );
    let mut value = PWSTR::null();
    unsafe { ConvertSidToStringSidW(sid, &mut value) }?;
    let _allocation = LocalAllocation(value.0.cast());
    String::from_utf16(unsafe { value.as_wide() }).context("Startup principal SID is not UTF-16")
}

fn wide_text(value: &str) -> Result<Vec<u16>> {
    ensure!(
        !value.is_empty() && !value.contains('\0'),
        "Startup principal is invalid"
    );
    let mut wide: Vec<u16> = value.encode_utf16().collect();
    ensure!(
        wide.len() <= MAX_TEXT_UNITS,
        "Startup principal is too long"
    );
    wide.push(0);
    Ok(wide)
}

fn parse_sid(value: &str) -> Result<String> {
    let wide = wide_text(value)?;
    let mut sid = PSID::default();
    unsafe { ConvertStringSidToSidW(PCWSTR(wide.as_ptr()), &mut sid) }?;
    let _allocation = LocalAllocation(sid.0);
    sid_text(sid)
}

fn resolve_principal_sid(value: &str) -> Result<String> {
    if value.starts_with("S-") {
        return parse_sid(value);
    }
    let wide = wide_text(value)?;
    let mut sid_bytes = 0;
    let mut domain_units = 0;
    let mut usage = SID_NAME_USE::default();
    let size_result = unsafe {
        LookupAccountNameW(
            None,
            PCWSTR(wide.as_ptr()),
            None,
            &mut sid_bytes,
            None,
            &mut domain_units,
            &mut usage,
        )
    };
    ensure!(
        size_result
            .is_err_and(|error| error.code() == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0)),
        "Cannot resolve the startup task principal"
    );
    ensure!(
        sid_bytes > 0
            && sid_bytes <= SECURITY_MAX_SID_SIZE
            && domain_units as usize <= MAX_TEXT_UNITS,
        "Startup principal lookup returned invalid buffer sizes"
    );
    let mut sid_buffer = vec![0_u32; (sid_bytes as usize).div_ceil(size_of::<u32>())];
    let sid = PSID(sid_buffer.as_mut_ptr().cast());
    let mut domain = vec![0_u16; (domain_units as usize).max(1)];
    unsafe {
        LookupAccountNameW(
            None,
            PCWSTR(wide.as_ptr()),
            Some(sid),
            &mut sid_bytes,
            Some(PWSTR(domain.as_mut_ptr())),
            &mut domain_units,
            &mut usage,
        )
    }
    .context("Resolve the startup task principal")?;
    sid_text(sid)
}

pub(super) fn current_user_sid() -> Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }?;
    let token = unsafe { OwnedHandle::from_raw_handle(token.0) };
    let handle = HANDLE(token.as_raw_handle());
    let mut length = 0;
    let size_result = unsafe { GetTokenInformation(handle, TokenUser, None, 0, &mut length) };
    ensure!(
        size_result
            .is_err_and(|error| error.code() == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0))
            && (length as usize) >= size_of::<TOKEN_USER>()
            && length <= 65_536,
        "Cannot read the current startup principal"
    );
    let mut buffer = vec![0_u64; (length as usize).div_ceil(size_of::<u64>())];
    unsafe {
        GetTokenInformation(
            handle,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            length,
            &mut length,
        )?;
        sid_text((*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid)
    }
}

#[cfg(test)]
#[path = "scheduler_tests.rs"]
mod tests;
