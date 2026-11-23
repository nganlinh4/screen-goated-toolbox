//! Keep observed job members alive as handles through asynchronous termination.
use anyhow::{Context, Result, anyhow, ensure};
use std::collections::BTreeMap;
use std::ffi::c_void;
use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
use windows::Win32::Foundation::{
    ERROR_INVALID_PARAMETER, ERROR_MORE_DATA, FILETIME, HANDLE, WAIT_OBJECT_0, WAIT_TIMEOUT,
};
use windows::Win32::System::JobObjects::{
    IsProcessInJob, JOBOBJECT_BASIC_PROCESS_ID_LIST, JobObjectBasicProcessIdList,
    QueryInformationJobObject,
};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION, PROCESS_SYNCHRONIZE,
    WaitForSingleObject,
};
use windows::core::{BOOL, HRESULT};

#[derive(Default)]
pub(super) struct Members {
    processes: BTreeMap<(u32, u64), OwnedHandle>,
    failure: Option<String>,
}

impl Members {
    pub(super) fn observe(&mut self, job: HANDLE) -> Result<()> {
        let result = self.capture(job);
        if let Err(error) = &result {
            self.failure.get_or_insert_with(|| error.to_string());
        }
        result
    }

    pub(super) fn pending(&mut self) -> Result<u32> {
        if let Some(error) = &self.failure {
            return Err(anyhow!("Process completion could not be verified: {error}"));
        }
        let mut retired = Vec::new();
        for (identity, process) in &self.processes {
            match unsafe { WaitForSingleObject(HANDLE(process.as_raw_handle()), 0) } {
                WAIT_OBJECT_0 => retired.push(*identity),
                WAIT_TIMEOUT => {}
                _ => {
                    return Err(std::io::Error::last_os_error())
                        .context("query owned process completion");
                }
            }
        }
        for identity in retired {
            self.processes.remove(&identity);
        }
        Ok(self.processes.len().try_into()?)
    }

    fn capture(&mut self, job: HANDLE) -> Result<()> {
        self.pending()?;
        for pid in members(job)? {
            let raw = match unsafe {
                OpenProcess(
                    PROCESS_QUERY_LIMITED_INFORMATION | PROCESS_SYNCHRONIZE,
                    false,
                    pid,
                )
            } {
                Ok(raw) => raw,
                Err(error) if error.code() == HRESULT::from_win32(ERROR_INVALID_PARAMETER.0) => {
                    continue;
                }
                Err(error) => return Err(error).context("open owned process completion handle"),
            };
            let process = unsafe { OwnedHandle::from_raw_handle(raw.0) };
            let mut member = BOOL::default();
            unsafe {
                IsProcessInJob(raw, Some(job), &mut member)?;
            }
            if !member.as_bool() {
                continue;
            }
            let mut created = FILETIME::default();
            let mut exited = FILETIME::default();
            let mut kernel = FILETIME::default();
            let mut user = FILETIME::default();
            unsafe {
                GetProcessTimes(raw, &mut created, &mut exited, &mut kernel, &mut user)?;
            }
            let created =
                (u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime);
            self.processes.entry((pid, created)).or_insert(process);
        }
        Ok(())
    }
}

pub(super) fn members(job: HANDLE) -> Result<Vec<u32>> {
    let mut capacity = 16;
    for _ in 0..16 {
        let header = std::mem::offset_of!(JOBOBJECT_BASIC_PROCESS_ID_LIST, ProcessIdList)
            .div_ceil(std::mem::size_of::<usize>());
        let mut buffer = vec![0_usize; capacity + header];
        let bytes = std::mem::size_of_val(buffer.as_slice()).try_into()?;
        let list = buffer
            .as_mut_ptr()
            .cast::<JOBOBJECT_BASIC_PROCESS_ID_LIST>();
        let queried = unsafe {
            QueryInformationJobObject(
                Some(job),
                JobObjectBasicProcessIdList,
                list.cast::<c_void>(),
                bytes,
                None,
            )
        };
        let incomplete = match queried {
            Ok(()) => false,
            Err(error) if error.code() == HRESULT::from_win32(ERROR_MORE_DATA.0) => true,
            Err(error) => return Err(error).context("enumerate owned process handles"),
        };
        let list = unsafe { &*list };
        if incomplete || list.NumberOfAssignedProcesses as usize > capacity {
            capacity = capacity
                .checked_mul(2)
                .context("process list capacity overflow")?;
            continue;
        }
        ensure!(
            list.NumberOfProcessIdsInList as usize <= capacity,
            "Invalid process list length"
        );
        let ids = unsafe {
            std::slice::from_raw_parts(
                list.ProcessIdList.as_ptr(),
                list.NumberOfProcessIdsInList as usize,
            )
        };
        return ids
            .iter()
            .map(|pid| u32::try_from(*pid).context("invalid process identifier"))
            .collect();
    }
    Err(anyhow!(
        "Owned process list did not settle within its capture bound"
    ))
}
