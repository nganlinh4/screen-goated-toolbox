//! Conservative file ACL policy, independent of the inspecting process token.

use anyhow::{Context, Result, ensure};
use std::fs::File;
use std::os::windows::io::AsRawHandle as _;
use windows::Win32::Foundation::{HANDLE, HLOCAL, LocalFree};
use windows::Win32::Security::Authorization::{
    ConvertStringSecurityDescriptorToSecurityDescriptorW, GetSecurityInfo, SDDL_REVISION_1,
    SE_FILE_OBJECT,
};
use windows::Win32::Security::{
    DACL_SECURITY_INFORMATION, GENERIC_MAPPING, GetSecurityDescriptorControl,
    GetSecurityDescriptorDacl, GetSecurityDescriptorLength, GetSecurityDescriptorOwner,
    IsValidSecurityDescriptor, MapGenericMask, OWNER_SECURITY_INFORMATION, PSECURITY_DESCRIPTOR,
    PSID, SE_SELF_RELATIVE,
};
use windows::Win32::Storage::FileSystem::{
    DELETE, FILE_ADD_FILE, FILE_ADD_SUBDIRECTORY, FILE_ALL_ACCESS, FILE_APPEND_DATA,
    FILE_DELETE_CHILD, FILE_GENERIC_EXECUTE, FILE_GENERIC_READ, FILE_GENERIC_WRITE,
    FILE_WRITE_ATTRIBUTES, FILE_WRITE_DATA, FILE_WRITE_EA, WRITE_DAC, WRITE_OWNER,
};
use windows::core::{BOOL, PCWSTR};

#[derive(Clone, Copy, Debug)]
pub(super) enum Kind {
    File,
    Parent,
    Ancestor,
}

impl Kind {
    fn forbidden(self) -> u32 {
        let shared =
            DELETE.0 | WRITE_DAC.0 | WRITE_OWNER.0 | FILE_WRITE_EA.0 | FILE_WRITE_ATTRIBUTES.0;
        match self {
            Self::File => shared | FILE_WRITE_DATA.0 | FILE_APPEND_DATA.0,
            Self::Parent => {
                shared | FILE_ADD_FILE.0 | FILE_ADD_SUBDIRECTORY.0 | FILE_DELETE_CHILD.0
            }
            // Creating a sibling directory cannot replace an existing protected
            // component. Directory data writes and child deletion still can.
            Self::Ancestor => shared | FILE_ADD_FILE.0 | FILE_DELETE_CHILD.0,
        }
    }
}

struct Descriptor(PSECURITY_DESCRIPTOR);

impl Drop for Descriptor {
    fn drop(&mut self) {
        let _ = unsafe { LocalFree(Some(HLOCAL(self.0.0))) };
    }
}

pub(super) fn check(file: &File, kind: Kind) -> Result<()> {
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    let status = unsafe {
        GetSecurityInfo(
            HANDLE(file.as_raw_handle()),
            SE_FILE_OBJECT,
            OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION,
            None,
            None,
            None,
            None,
            Some(&mut descriptor),
        )
    };
    let descriptor = Descriptor(descriptor);
    ensure!(
        status.0 == 0,
        "Startup target security is unavailable: {status:?}"
    );
    unsafe { validate_descriptor(descriptor.0, kind) }
}

pub(super) fn check_task(sddl: &str) -> Result<()> {
    ensure!(
        !sddl.is_empty() && sddl.len() <= 131_072 && !sddl.contains('\0'),
        "Task security descriptor is invalid"
    );
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
    .context("Task security descriptor is unsupported")?;
    let descriptor = Descriptor(descriptor);
    unsafe { validate_descriptor(descriptor.0, Kind::File) }
}

// The pointer must refer to a native, allocated security descriptor. Every
// variable-sized field is bounded by its self-relative allocation before use.
unsafe fn validate_descriptor(descriptor: PSECURITY_DESCRIPTOR, kind: Kind) -> Result<()> {
    ensure!(
        !descriptor.0.is_null() && unsafe { IsValidSecurityDescriptor(descriptor) }.as_bool(),
        "Startup target security descriptor is invalid"
    );
    let mut control = 0_u16;
    let mut revision = 0;
    unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) }?;
    ensure!(
        revision == 1 && control & SE_SELF_RELATIVE.0 != 0,
        "Startup target security descriptor is unsupported"
    );
    let length = unsafe { GetSecurityDescriptorLength(descriptor) } as usize;
    ensure!(
        (20..=1024 * 1024).contains(&length),
        "Invalid security descriptor size"
    );
    let bytes = unsafe { std::slice::from_raw_parts(descriptor.0.cast::<u8>(), length) };
    let mut owner = PSID::default();
    let mut defaulted = BOOL::default();
    unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut defaulted) }?;
    let owner = sid_at(bytes, owner.0.cast())?;
    ensure!(
        trusted(owner, false),
        "Startup target has an untrusted owner"
    );

    let mut present = BOOL::default();
    let mut dacl = std::ptr::null_mut();
    unsafe { GetSecurityDescriptorDacl(descriptor, &mut present, &mut dacl, &mut defaulted) }?;
    ensure!(
        present.as_bool() && !dacl.is_null(),
        "Startup target has no protective DACL"
    );
    let header = region(bytes, dacl.cast(), 8)?;
    ensure!(matches!(header[0], 2 | 4), "Unsupported ACL revision");
    ensure!(
        header[1] == 0 && header[6..8] == [0, 0],
        "Invalid ACL header"
    );
    let size = u16::from_le_bytes([header[2], header[3]]) as usize;
    ensure!(size >= 8, "Invalid ACL size");
    let acl = region(bytes, dacl.cast(), size)?;
    let count = u16::from_le_bytes([header[4], header[5]]);
    let mut offset = 8_usize;
    for _ in 0..count {
        let header = acl
            .get(offset..offset + 4)
            .context("Truncated ACE header")?;
        let size = u16::from_le_bytes([header[2], header[3]]) as usize;
        ensure!(size >= 16 && size.is_multiple_of(4), "Invalid ACE size");
        let ace = acl
            .get(offset..offset.checked_add(size).context("ACE size overflow")?)
            .context("Truncated ACE")?;
        validate_ace(ace, kind)?;
        offset += size;
    }
    Ok(())
}

fn region(bytes: &[u8], pointer: *const u8, length: usize) -> Result<&[u8]> {
    let offset = (pointer as usize)
        .checked_sub(bytes.as_ptr() as usize)
        .context("Security field is outside its descriptor")?;
    bytes
        .get(
            offset
                ..offset
                    .checked_add(length)
                    .context("Security field size overflow")?,
        )
        .context("Security field exceeds its descriptor")
}

fn sid_at(bytes: &[u8], pointer: *const u8) -> Result<&[u8]> {
    let header = region(bytes, pointer, 8)?;
    ensure!(
        header[0] == 1 && header[1] <= 15,
        "Invalid security identifier"
    );
    region(bytes, pointer, 8 + usize::from(header[1]) * 4)
}

fn validate_ace(ace: &[u8], kind: Kind) -> Result<()> {
    ensure!(
        matches!(ace[0], 0 | 1),
        "Unsupported startup target ACE type"
    );
    ensure!(ace[1] & !0x1f == 0, "Unsupported startup target ACE flags");
    let sid = sid_at(ace, ace[8..].as_ptr())?;
    ensure!(sid.len() + 8 == ace.len(), "Unsupported ACE payload");
    let mut mask = u32::from_le_bytes(ace[4..8].try_into()?);
    let mapping = GENERIC_MAPPING {
        GenericRead: FILE_GENERIC_READ.0,
        GenericWrite: FILE_GENERIC_WRITE.0,
        GenericExecute: FILE_GENERIC_EXECUTE.0,
        GenericAll: FILE_ALL_ACCESS.0,
    };
    unsafe { MapGenericMask(&mut mask, &mapping) };
    ensure!(
        mask & !FILE_ALL_ACCESS.0 == 0,
        "Unsupported startup target access mask"
    );
    if ace[1] & 8 == 0 && ace[0] == 0 && mask & kind.forbidden() != 0 {
        ensure!(
            trusted(sid, true),
            "Startup target can be changed without administrator access"
        );
    }
    Ok(())
}

fn trusted(sid: &[u8], owner_rights: bool) -> bool {
    sid_is(sid, 5, &[18])
        || sid_is(sid, 5, &[32, 544])
        || sid_is(
            sid,
            5,
            &[
                80, 956008885, 3418522649, 1831038044, 1853292631, 2271478464,
            ],
        )
        || (owner_rights && sid_is(sid, 3, &[4]))
}

fn sid_is(sid: &[u8], authority: u8, subauthorities: &[u32]) -> bool {
    sid.len() == 8 + subauthorities.len() * 4
        && sid[0] == 1
        && usize::from(sid[1]) == subauthorities.len()
        && sid[2..8] == [0, 0, 0, 0, 0, authority]
        && sid[8..]
            .as_chunks::<4>()
            .0
            .iter()
            .zip(subauthorities)
            .all(|(bytes, expected)| u32::from_le_bytes(*bytes) == *expected)
}

#[cfg(test)]
mod tests;
