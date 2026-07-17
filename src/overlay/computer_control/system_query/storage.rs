//! Trusted volume capacity/free-space evidence via the Win32 volume API.

use serde_json::{Value, json};
use windows::Win32::Storage::FileSystem::{GetDiskFreeSpaceExW, GetLogicalDrives};
use windows::core::PCWSTR;

pub(super) fn volumes(observed_at_ms: u128) -> Value {
    let mut items = Vec::new();
    let drives = unsafe { GetLogicalDrives() };
    for index in 0..26u8 {
        if drives & (1 << index) == 0 {
            continue;
        }
        let root = format!("{}:\\", char::from(b'A' + index));
        let root_wide: Vec<u16> = root.encode_utf16().chain(std::iter::once(0)).collect();
        let mut available_bytes = 0u64;
        let mut total_bytes = 0u64;
        let mut free_bytes = 0u64;
        let readable = unsafe {
            GetDiskFreeSpaceExW(
                PCWSTR(root_wide.as_ptr()),
                Some(&mut available_bytes),
                Some(&mut total_bytes),
                Some(&mut free_bytes),
            )
        }
        .is_ok();
        // Unreadable roots (empty card readers, disconnected network drives)
        // carry no capacity evidence; skipping them is not an error.
        if !readable {
            continue;
        }
        items.push(json!({
            "root": root,
            "total_bytes": total_bytes,
            "free_bytes": free_bytes,
            "available_bytes": available_bytes,
        }));
    }
    super::ok(
        "storage",
        "volumes",
        "win32_volume_api",
        "high",
        items,
        Vec::new(),
        observed_at_ms,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volumes_reports_real_capacity_for_the_system_drive() {
        let result = volumes(7);
        assert_eq!(result["ok"], true);
        assert_eq!(result["source"], "win32_volume_api");
        let items = result["items"].as_array().unwrap();
        let system = items
            .iter()
            .find(|item| item["root"] == "C:\\")
            .expect("system drive present");
        assert!(system["total_bytes"].as_u64().unwrap() > 0);
        assert!(system["free_bytes"].as_u64().unwrap() <= system["total_bytes"].as_u64().unwrap());
    }
}
