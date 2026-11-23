//! Checked native Unicode environment snapshots and case-insensitive overrides.
use anyhow::{Result, bail, ensure};
use std::cmp::Ordering;
use std::ffi::OsString;
use std::os::windows::ffi::OsStrExt;
use windows::Win32::Globalization::{
    CSTR_EQUAL, CSTR_GREATER_THAN, CSTR_LESS_THAN, CompareStringOrdinal,
};
use windows::Win32::System::Environment::{FreeEnvironmentStringsW, GetEnvironmentStringsW};
use windows::core::{PCWSTR, PWSTR};

type Variables = Vec<(Vec<u16>, Vec<u16>)>;
pub(super) type Changes = Vec<(OsString, Option<OsString>)>;

struct Snapshot(PWSTR);
impl Drop for Snapshot {
    fn drop(&mut self) {
        let _ = unsafe { FreeEnvironmentStringsW(PCWSTR(self.0.0)) };
    }
}

fn snapshot() -> Result<Variables> {
    let snapshot = Snapshot(unsafe { GetEnvironmentStringsW() });
    ensure!(!snapshot.0.is_null(), "Read process environment failed");
    let mut cursor = snapshot.0;
    let mut variables = Vec::new();
    while unsafe { *cursor.0 } != 0 {
        let entry = unsafe { cursor.as_wide() };
        // Windows also preserves hidden drive-directory names such as =C:.
        let separator = entry
            .iter()
            .enumerate()
            .skip(1)
            .find(|(_, unit)| **unit == b'=' as u16)
            .map(|(index, _)| index)
            .ok_or_else(|| anyhow::anyhow!("Invalid inherited environment entry"))?;
        variables.push((entry[..separator].to_vec(), entry[separator + 1..].to_vec()));
        cursor = PWSTR(unsafe { cursor.0.add(entry.len() + 1) });
    }
    Ok(variables)
}

fn compare(left: &[u16], right: &[u16]) -> Result<Ordering> {
    ensure!(
        left.len() <= i32::MAX as usize && right.len() <= i32::MAX as usize,
        "Environment name is too large"
    );
    match unsafe { CompareStringOrdinal(left, right, true) } {
        CSTR_LESS_THAN => Ok(Ordering::Less),
        CSTR_EQUAL => Ok(Ordering::Equal),
        CSTR_GREATER_THAN => Ok(Ordering::Greater),
        _ => bail!("Compare environment names failed"),
    }
}

fn set(variables: &mut Variables, name: Vec<u16>, value: Option<Vec<u16>>) -> Result<()> {
    let (mut low, mut high) = (0, variables.len());
    while low < high {
        let middle = low + (high - low) / 2;
        match compare(&variables[middle].0, &name)? {
            Ordering::Less => low = middle + 1,
            Ordering::Greater => high = middle,
            Ordering::Equal => {
                variables.remove(middle);
                low = middle;
                break;
            }
        }
    }
    if let Some(value) = value {
        variables.insert(low, (name, value));
    }
    Ok(())
}

fn encode(inherited: Variables, changes: &Changes) -> Result<Vec<u16>> {
    let mut variables = Vec::new();
    for (name, value) in inherited {
        set(&mut variables, name, Some(value))?;
    }
    for (name, value) in changes {
        let name: Vec<_> = name.encode_wide().collect();
        ensure!(
            !name.is_empty() && !name.contains(&0) && !name.contains(&(b'=' as u16)),
            "Invalid helper environment name"
        );
        let value = value
            .as_ref()
            .map(|value| value.encode_wide().collect::<Vec<_>>());
        ensure!(
            !value.as_ref().is_some_and(|value| value.contains(&0)),
            "Invalid helper environment value"
        );
        set(&mut variables, name, value)?;
    }
    let mut block = Vec::new();
    for (name, value) in variables {
        block.extend(name);
        block.push(b'=' as u16);
        block.extend(value);
        block.push(0);
    }
    if block.is_empty() {
        block.push(0);
    }
    block.push(0);
    Ok(block)
}

pub(super) fn prepare(changes: &Changes, inherit: bool) -> Result<Option<Vec<u16>>> {
    if changes.is_empty() && inherit {
        return Ok(None);
    }
    encode(if inherit { snapshot()? } else { Vec::new() }, changes).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().collect()
    }

    #[test]
    fn overrides_remove_case_variants_preserve_drive_entries_and_sort_unicode() {
        let inherited = vec![
            (wide("z"), wide("last")),
            (wide("=C:"), wide("C:\\work")),
            (wide("Ä"), wide("old")),
            (wide("Path"), wide("old")),
            (wide("remove"), wide("old")),
        ];
        let changes = vec![
            ("ä".into(), Some("new=value".into())),
            ("PATH".into(), Some("new".into())),
            ("REMOVE".into(), None),
        ];
        let block = encode(inherited, &changes).unwrap();
        let entries: Vec<_> = block
            .split(|unit| *unit == 0)
            .filter(|entry| !entry.is_empty())
            .map(|entry| String::from_utf16(entry).unwrap())
            .collect();
        assert_eq!(
            entries,
            ["=C:=C:\\work", "PATH=new", "z=last", "ä=new=value"]
        );
        assert!(block.ends_with(&[0, 0]));
    }

    #[test]
    fn empty_block_and_invalid_overrides_are_checked_before_launch() {
        assert_eq!(encode(Vec::new(), &Vec::new()).unwrap(), [0, 0]);
        for (name, value) in [
            ("", Some("v")),
            ("a=b", Some("v")),
            ("a\0b", None),
            ("a", Some("v\0x")),
        ] {
            assert!(encode(Vec::new(), &vec![(name.into(), value.map(Into::into))]).is_err());
        }
    }
}
