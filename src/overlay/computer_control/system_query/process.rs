use std::path::Path;

use serde_json::{Value, json};

#[cfg(windows)]
use windows::Win32::Foundation::CloseHandle;
#[cfg(windows)]
use windows::Win32::System::Diagnostics::ToolHelp::{
    CreateToolhelp32Snapshot, PROCESSENTRY32W, Process32FirstW, Process32NextW, TH32CS_SNAPPROCESS,
};
#[cfg(windows)]
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};

pub(super) fn list_basic(args: &Value, observed_at_ms: u128) -> Value {
    #[cfg(not(windows))]
    {
        let _ = args;
        super::failure(
            "process",
            "list_basic",
            "process.list_basic is only available on Windows",
            observed_at_ms,
        )
    }

    #[cfg(windows)]
    {
        let limit = args
            .get("limit")
            .and_then(Value::as_u64)
            .unwrap_or(100)
            .clamp(1, 2000) as usize;
        let name_filter = args
            .get("name_contains")
            .and_then(Value::as_str)
            .map(|value| value.to_ascii_lowercase());

        match enumerate_processes(limit, name_filter.as_deref()) {
            Ok((items, warnings)) => super::ok(
                "process",
                "list_basic",
                "windows_toolhelp",
                "high",
                items,
                warnings,
                observed_at_ms,
            ),
            Err(error) => super::failure(
                "process",
                "list_basic",
                &format!("failed to enumerate processes: {error}"),
                observed_at_ms,
            ),
        }
    }
}

pub(super) fn process_name_from_path(path: &str) -> Option<String> {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(ToOwned::to_owned)
}

#[cfg(windows)]
pub(super) fn exe_path(pid: u32) -> Option<String> {
    unsafe {
        let handle = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid).ok()?;
        let mut buffer = [0u16; 1024];
        let mut size = buffer.len() as u32;
        let result = QueryFullProcessImageNameW(
            handle,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buffer.as_mut_ptr()),
            &mut size,
        );
        let _ = CloseHandle(handle);

        if result.is_ok() && size > 0 {
            Some(String::from_utf16_lossy(&buffer[..size as usize]))
        } else {
            None
        }
    }
}

#[cfg(not(windows))]
pub(super) fn exe_path(_pid: u32) -> Option<String> {
    None
}

#[cfg(windows)]
fn enumerate_processes(
    limit: usize,
    name_filter: Option<&str>,
) -> windows::core::Result<(Vec<Value>, Vec<String>)> {
    unsafe {
        let snapshot = CreateToolhelp32Snapshot(TH32CS_SNAPPROCESS, 0)?;
        let mut entry = PROCESSENTRY32W {
            dwSize: std::mem::size_of::<PROCESSENTRY32W>() as u32,
            ..Default::default()
        };
        let mut items = Vec::new();
        let mut warnings = Vec::new();

        if Process32FirstW(snapshot, &mut entry).is_ok() {
            loop {
                let exe_file = wide_array_to_string(&entry.szExeFile);
                let matches_filter = name_filter
                    .map(|filter| exe_file.to_ascii_lowercase().contains(filter))
                    .unwrap_or(true);
                if matches_filter {
                    let pid = entry.th32ProcessID;
                    let exe_path = exe_path(pid);
                    items.push(json!({
                        "pid": pid,
                        "parent_pid": entry.th32ParentProcessID,
                        "process_name": exe_file,
                        "exe_path": exe_path,
                        "thread_count": entry.cntThreads,
                    }));
                    if items.len() >= limit {
                        warnings.push(format!("limited process.list_basic to {limit} items"));
                        break;
                    }
                }

                if Process32NextW(snapshot, &mut entry).is_err() {
                    break;
                }
            }
        }

        let _ = CloseHandle(snapshot);
        Ok((items, warnings))
    }
}

#[cfg(windows)]
fn wide_array_to_string(buffer: &[u16]) -> String {
    let len = buffer
        .iter()
        .position(|ch| *ch == 0)
        .unwrap_or(buffer.len());
    String::from_utf16_lossy(&buffer[..len])
}
