//! Only the two reserved current-user values are read or changed.

use super::{MAX_VALUE_BYTES, RawValue, Registry, Slot, Snapshot, VALUE_NAME};
use anyhow::{Context, Result, ensure};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, ERROR_SUCCESS};
use windows::Win32::System::Registry::{
    HKEY, HKEY_CURRENT_USER, KEY_QUERY_VALUE, KEY_SET_VALUE, REG_OPTION_NON_VOLATILE,
    REG_VALUE_TYPE, RegCloseKey, RegCreateKeyExW, RegDeleteValueW, RegOpenKeyExW, RegQueryValueExW,
    RegSetValueExW,
};
use windows::core::PCWSTR;

pub(super) struct NativeRegistry;

struct Key(HKEY);
impl Drop for Key {
    fn drop(&mut self) {
        let _ = unsafe { RegCloseKey(self.0) };
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

impl NativeRegistry {
    fn read_value(&self, slot: Slot) -> Result<Option<RawValue>> {
        let path = wide(slot.key());
        let name = wide(VALUE_NAME);
        let mut key = HKEY::default();
        let status = unsafe {
            RegOpenKeyExW(
                HKEY_CURRENT_USER,
                PCWSTR(path.as_ptr()),
                None,
                KEY_QUERY_VALUE,
                &mut key,
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status
            .ok()
            .context("Open the current-user startup registry key")?;
        let key = Key(key);
        let mut kind = REG_VALUE_TYPE::default();
        let mut length = 0;
        let status = unsafe {
            RegQueryValueExW(
                key.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                None,
                Some(&mut length),
            )
        };
        if status == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        status
            .ok()
            .context("Read the startup registry value size")?;
        ensure!(
            length as usize <= MAX_VALUE_BYTES,
            "Startup registry value is too large"
        );
        let mut bytes = vec![0; length as usize];
        unsafe {
            RegQueryValueExW(
                key.0,
                PCWSTR(name.as_ptr()),
                None,
                Some(&mut kind),
                Some(bytes.as_mut_ptr()),
                Some(&mut length),
            )
        }
        .ok()
        .context("Read the current-user startup registry value")?;
        ensure!(
            length as usize <= bytes.len(),
            "Startup registry value changed size"
        );
        bytes.truncate(length as usize);
        Ok(Some(RawValue {
            kind: kind.0,
            bytes,
        }))
    }
}

impl Registry for NativeRegistry {
    fn read(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            run: self.read_value(Slot::Run)?,
            approved: self.read_value(Slot::Approved)?,
        })
    }

    fn write(&self, slot: Slot, value: &Option<RawValue>) -> Result<()> {
        let path = wide(slot.key());
        let name = wide(VALUE_NAME);
        let mut raw = HKEY::default();
        let status = if value.is_some() {
            unsafe {
                RegCreateKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    None,
                    None,
                    REG_OPTION_NON_VOLATILE,
                    KEY_SET_VALUE,
                    None,
                    &mut raw,
                    None,
                )
            }
        } else {
            unsafe {
                RegOpenKeyExW(
                    HKEY_CURRENT_USER,
                    PCWSTR(path.as_ptr()),
                    None,
                    KEY_SET_VALUE,
                    &mut raw,
                )
            }
        };
        if status == ERROR_FILE_NOT_FOUND && value.is_none() {
            return Ok(());
        }
        status
            .ok()
            .context("Open the current-user startup registry value for writing")?;
        let key = Key(raw);
        let status = match value {
            Some(value) => unsafe {
                RegSetValueExW(
                    key.0,
                    PCWSTR(name.as_ptr()),
                    None,
                    REG_VALUE_TYPE(value.kind),
                    Some(&value.bytes),
                )
            },
            None => unsafe { RegDeleteValueW(key.0, PCWSTR(name.as_ptr())) },
        };
        if status == ERROR_SUCCESS || (status == ERROR_FILE_NOT_FOUND && value.is_none()) {
            Ok(())
        } else {
            status
                .ok()
                .context("Write the current-user startup registry value")
        }
    }
}
