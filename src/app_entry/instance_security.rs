//! Same-user names and security for the desktop singleton at every integrity level.
//!
//! An elevated autostart instance and an ordinary launch run as the same user
//! at different integrity levels. Kernel objects created with an elevated
//! token's default security admit only Administrators and SYSTEM, so an
//! ordinary launch could neither detect nor reach the running instance. These
//! descriptors grant the token user only the rights the handoff needs and
//! carry an explicit Medium no-write-up label, so every same-user caller at
//! Medium or above is admitted and lower-integrity callers stay excluded.

use std::os::windows::io::{FromRawHandle, OwnedHandle};

use anyhow::{Context, Result, ensure};
use windows::Win32::Foundation::{
    ERROR_ALREADY_EXISTS, ERROR_INSUFFICIENT_BUFFER, GetLastError, HANDLE, HLOCAL, LocalFree,
};
use windows::Win32::Security::Authorization::{
    ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW, SDDL_REVISION_1,
};
use windows::Win32::Security::{
    GetTokenInformation, IsValidSid, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
    TOKEN_USER, TokenUser,
};
use windows::Win32::System::Threading::{CreateMutexExW, GetCurrentProcess, OpenProcessToken};
use windows::core::{HRESULT, PCWSTR, PWSTR};

/// `SYNCHRONIZE | MUTEX_MODIFY_STATE`: enough to create or open the singleton.
const MUTEX_ACCESS: u32 = 0x0010_0001;
const MEDIUM_NO_WRITE_UP: &str = "S:(ML;;NW;;;ME)";

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct InstanceNames {
    pub(super) mutex: String,
    pub(super) pipe: String,
}

/// Scope the executable-bound singleton name to one user, so a second user or
/// a second integrity level of the same user resolves the same objects only
/// when it is the same account.
pub(super) fn instance_names(base_mutex: &[u16], sid: &str) -> Result<InstanceNames> {
    ensure!(
        sid.len() <= 184
            && sid.strip_prefix("S-1-").is_some_and(|rest| !rest.is_empty()
                && rest
                    .bytes()
                    .all(|byte| byte.is_ascii_digit() || byte == b'-')),
        "Current user SID is not a valid account SID"
    );
    let base = base_mutex.strip_suffix(&[0]).unwrap_or(base_mutex);
    let base = String::from_utf16(base).context("Singleton name is not UTF-16")?;
    let object = base
        .strip_prefix("Global\\")
        .filter(|object| !object.is_empty() && !object.contains(['\\', '/']))
        .context("Singleton name is outside the global namespace")?;
    Ok(InstanceNames {
        mutex: format!("{base}-{sid}"),
        pipe: format!(r"\\.\pipe\{object}-{sid}-activation"),
    })
}

pub(super) fn current_user_sid() -> Result<String> {
    let mut token = HANDLE::default();
    unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut token) }
        .context("Open the current process token")?;
    let token = unsafe { OwnedHandle::from_raw_handle(token.0) };
    let handle = HANDLE(std::os::windows::io::AsRawHandle::as_raw_handle(&token));
    let mut length = 0;
    let size = unsafe { GetTokenInformation(handle, TokenUser, None, 0, &mut length) };
    ensure!(
        size.is_err_and(|error| error.code() == HRESULT::from_win32(ERROR_INSUFFICIENT_BUFFER.0))
            && (length as usize) >= size_of::<TOKEN_USER>()
            && length <= 65_536,
        "Cannot read the current user"
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
        let sid = (*(buffer.as_ptr().cast::<TOKEN_USER>())).User.Sid;
        ensure!(IsValidSid(sid).as_bool(), "Current user SID is invalid");
        let mut text = PWSTR::null();
        ConvertSidToStringSidW(sid, &mut text)?;
        let value = String::from_utf16(text.as_wide());
        LocalFree(Some(HLOCAL(text.0.cast())));
        value.context("Current user SID is not UTF-16")
    }
}

/// A self-relative security descriptor owned by this process.
pub(super) struct ObjectSecurity(PSECURITY_DESCRIPTOR);

impl ObjectSecurity {
    pub(super) fn for_mutex(sid: &str) -> Result<Self> {
        Self::from_sddl(&mutex_sddl(sid))
    }

    /// Read and write for the user; pipe instances are additionally limited
    /// to one, so this cannot be used to add a competing server instance.
    pub(super) fn for_pipe(sid: &str) -> Result<Self> {
        Self::from_sddl(&format!("D:P(A;;GRGW;;;{sid}){MEDIUM_NO_WRITE_UP}"))
    }

    fn from_sddl(sddl: &str) -> Result<Self> {
        let wide = sddl.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
        let mut descriptor = PSECURITY_DESCRIPTOR::default();
        unsafe {
            ConvertStringSecurityDescriptorToSecurityDescriptorW(
                PCWSTR(wide.as_ptr()),
                SDDL_REVISION_1,
                &mut descriptor,
                None,
            )
        }
        .context("Build singleton security descriptor")?;
        Ok(Self(descriptor))
    }

    /// The returned value borrows the descriptor; keep `self` alive across use.
    pub(super) fn attributes(&self) -> SECURITY_ATTRIBUTES {
        SECURITY_ATTRIBUTES {
            nLength: size_of::<SECURITY_ATTRIBUTES>() as u32,
            lpSecurityDescriptor: self.0.0,
            bInheritHandle: false.into(),
        }
    }
}

impl Drop for ObjectSecurity {
    fn drop(&mut self) {
        unsafe {
            LocalFree(Some(HLOCAL(self.0.0)));
        }
    }
}

fn mutex_sddl(sid: &str) -> String {
    format!("D:P(A;;0x{MUTEX_ACCESS:x};;;{sid}){MEDIUM_NO_WRITE_UP}")
}

/// Create the singleton, or open the existing one, requesting only the rights
/// the descriptor grants; returns whether another process already owns it.
pub(super) fn create_singleton_mutex(
    name: &str,
    security: &ObjectSecurity,
) -> Result<(OwnedHandle, bool)> {
    let wide = name.encode_utf16().chain(Some(0)).collect::<Vec<_>>();
    let attributes = security.attributes();
    let handle =
        unsafe { CreateMutexExW(Some(&attributes), PCWSTR(wide.as_ptr()), 0, MUTEX_ACCESS) }
            .context("Create the single-instance mutex")?;
    // Read before any other Win32 call can replace the creation status.
    let existed = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
    Ok((unsafe { OwnedHandle::from_raw_handle(handle.0) }, existed))
}

#[cfg(test)]
#[path = "instance_security_tests.rs"]
mod tests;
