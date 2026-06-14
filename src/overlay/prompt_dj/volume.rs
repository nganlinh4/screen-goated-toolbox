use windows::Win32::Media::Audio::{
    IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator, ISimpleAudioVolume,
    MMDeviceEnumerator, eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::Threading::GetCurrentProcessId;
use windows::core::*;

lazy_static::lazy_static! {
    static ref CHILD_PIDS: std::sync::Mutex<Vec<u32>> = std::sync::Mutex::new(Vec::new());
}

pub(super) fn update_child_pids() {
    let current_pid = unsafe { GetCurrentProcessId() };

    // Use wmic to get all processes (PID, PPID) - fast and standard
    #[cfg(windows)]
    use std::os::windows::process::CommandExt;

    let mut cmd = std::process::Command::new("wmic");
    cmd.args(["process", "get", "ProcessId,ParentProcessId", "/format:csv"]);

    // CREATE_NO_WINDOW = 0x08000000 - prevents console window flash
    #[cfg(windows)]
    cmd.creation_flags(0x08000000);

    let output = cmd.output();

    if let Ok(o) = output
        && let Ok(s) = String::from_utf8(o.stdout)
    {
        let mut tree = std::collections::HashMap::new();

        // Parse CSV output
        for line in s.lines() {
            if line.trim().is_empty() {
                continue;
            }
            let parts: Vec<&str> = line.split(',').collect();
            // Format is: Node, ParentProcessId, ProcessId (usually)
            // But wmic csv header is: Node,ParentProcessId,ProcessId
            if parts.len() >= 3
                && let (Ok(ppid), Ok(pid)) = (
                    parts[1].trim().parse::<u32>(),
                    parts[2].trim().parse::<u32>(),
                )
            {
                tree.entry(ppid).or_insert_with(Vec::new).push(pid);
            }
        }

        // Find all descendants recursively
        let mut descendants = Vec::new();
        let mut queue = vec![current_pid];
        let mut visited = std::collections::HashSet::new();
        visited.insert(current_pid);

        while let Some(pid) = queue.pop() {
            if let Some(children) = tree.get(&pid) {
                for &child in children {
                    if visited.insert(child) {
                        descendants.push(child);
                        queue.push(child);
                    }
                }
            }
        }

        if let Ok(mut lock) = CHILD_PIDS.lock() {
            *lock = descendants;
        }
    }
}

pub(super) unsafe fn set_app_volume(volume: f32) -> Result<()> {
    unsafe {
        // Access cache
        let current_pid = GetCurrentProcessId();
        let child_pids = CHILD_PIDS.lock().unwrap_or_else(|e| e.into_inner()).clone();

        // We try to initialize COM, but ignore error if already initialized
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        let device_enumerator: IMMDeviceEnumerator =
            CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)?;

        let device = device_enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia)?;
        let session_manager: IAudioSessionManager2 = device.Activate(CLSCTX_ALL, None)?;
        let session_enumerator = session_manager.GetSessionEnumerator()?;
        let count = session_enumerator.GetCount()?;

        for i in 0..count {
            if let Ok(session_control) = session_enumerator.GetSession(i)
                && let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>()
                && let Ok(pid) = session_control2.GetProcessId()
            {
                // Match Main Process OR known Children
                if (pid == current_pid || child_pids.contains(&pid))
                    && let Ok(simple_volume) = session_control.cast::<ISimpleAudioVolume>()
                {
                    let _ = simple_volume.SetMasterVolume(volume, std::ptr::null());
                }
            }
        }
        Ok(())
    }
}
