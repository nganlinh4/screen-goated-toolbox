use std::collections::HashSet;
use std::path::Path;
use std::sync::{LazyLock, Mutex};

use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Dwm::{DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute};
use windows::Win32::Media::Audio::{
    AudioSessionStateActive, IAudioSessionControl2, IAudioSessionManager2, IMMDeviceEnumerator,
    MMDeviceEnumerator, eMultimedia, eRender,
};
use windows::Win32::System::Com::{
    CLSCTX_ALL, COINIT_APARTMENTTHREADED, CoCreateInstance, CoInitializeEx,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows_core::{BOOL, Interface};

use super::icons::get_app_icon_data_url;

static SELECTED_AUDIO_APP_CANDIDATE: LazyLock<Mutex<Option<AudioAppCandidate>>> =
    LazyLock::new(|| Mutex::new(None));

#[derive(Clone, Debug)]
pub struct AudioAppCandidate {
    pub pid: u32,
    pub capture_pid: u32,
    pub display_name: String,
    pub process_name: String,
    pub icon_data_url: Option<String>,
    pub window_hwnd: usize,
    pub width: u32,
    pub height: u32,
}

#[derive(Clone, Debug)]
struct AudioSessionCandidate {
    pid: u32,
    process_name: String,
    exe_path: Option<String>,
}

fn get_process_exe_path(pid: u32) -> Option<String> {
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

fn get_window_size(hwnd: HWND) -> (u32, u32) {
    unsafe {
        let mut rect = RECT::default();
        if DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            &mut rect as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<RECT>() as u32,
        )
        .is_err()
        {
            let _ = GetWindowRect(hwnd, &mut rect);
        }

        (
            (rect.right - rect.left).max(1) as u32,
            (rect.bottom - rect.top).max(1) as u32,
        )
    }
}

fn active_audio_session_candidates() -> Vec<AudioSessionCandidate> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let device_enumerator: IMMDeviceEnumerator =
            match CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) {
                Ok(enumerator) => enumerator,
                Err(error) => {
                    eprintln!("[AppSelection] failed to create audio device enumerator: {error}");
                    return Vec::new();
                }
            };
        let device = match device_enumerator.GetDefaultAudioEndpoint(eRender, eMultimedia) {
            Ok(device) => device,
            Err(error) => {
                eprintln!("[AppSelection] failed to get default render endpoint: {error}");
                return Vec::new();
            }
        };
        let session_manager: IAudioSessionManager2 = match device.Activate(CLSCTX_ALL, None) {
            Ok(manager) => manager,
            Err(error) => {
                eprintln!("[AppSelection] failed to activate audio session manager: {error}");
                return Vec::new();
            }
        };
        let session_enumerator = match session_manager.GetSessionEnumerator() {
            Ok(enumerator) => enumerator,
            Err(error) => {
                eprintln!("[AppSelection] failed to enumerate audio sessions: {error}");
                return Vec::new();
            }
        };
        let count = session_enumerator.GetCount().unwrap_or(0);
        let mut sessions = Vec::new();
        let mut seen = HashSet::new();

        for index in 0..count {
            let Ok(session_control) = session_enumerator.GetSession(index) else {
                continue;
            };
            if session_control.GetState().ok() != Some(AudioSessionStateActive) {
                continue;
            }
            let Ok(session_control2) = session_control.cast::<IAudioSessionControl2>() else {
                continue;
            };
            let Ok(pid) = session_control2.GetProcessId() else {
                continue;
            };
            if pid == 0 || pid == std::process::id() || !seen.insert(pid) {
                continue;
            }

            let exe_path = get_process_exe_path(pid);
            let process_name = exe_path
                .as_deref()
                .and_then(|path| Path::new(path).file_stem())
                .and_then(|stem| stem.to_str())
                .map(ToOwned::to_owned)
                .unwrap_or_else(|| format!("PID {pid}"));
            sessions.push(AudioSessionCandidate {
                pid,
                process_name,
                exe_path,
            });
        }
        sessions
    }
}

fn child_window_pids(hwnd: HWND) -> HashSet<u32> {
    let mut pids = HashSet::new();
    unsafe {
        let mut callback_data = &mut pids;

        extern "system" fn enum_child_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            unsafe {
                let pids = &mut *(lparam.0 as *mut &mut HashSet<u32>);
                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid > 0 && pid != std::process::id() {
                    pids.insert(pid);
                }
                BOOL(1)
            }
        }

        let _ = EnumChildWindows(
            Some(hwnd),
            Some(enum_child_callback),
            LPARAM(&mut callback_data as *mut _ as isize),
        );
    }
    pids
}

fn hosted_child_exe_paths(hwnd: HWND, window_pid: u32) -> Vec<String> {
    let mut paths = Vec::new();
    let mut seen = HashSet::new();
    for pid in child_window_pids(hwnd) {
        if pid == window_pid || !seen.insert(pid) {
            continue;
        }
        if let Some(path) = get_process_exe_path(pid) {
            paths.push(path);
        }
    }
    paths
}

fn resolve_capture_pid_for_window(
    window_pid: u32,
    window_hwnd: HWND,
    window_process_name: &str,
    window_exe_path: Option<&str>,
    active_sessions: &[AudioSessionCandidate],
) -> (u32, bool) {
    if active_sessions
        .iter()
        .any(|session| session.pid == window_pid)
    {
        return (window_pid, true);
    }

    let child_pids = child_window_pids(window_hwnd);
    for child_pid in child_pids {
        if active_sessions
            .iter()
            .any(|session| session.pid == child_pid)
        {
            return (child_pid, true);
        }
    }

    let normalized_window_name = normalize_process_match_key(window_process_name);
    for session in active_sessions {
        if normalize_process_match_key(&session.process_name) == normalized_window_name {
            return (session.pid, true);
        }
    }

    if let Some(window_exe_path) = window_exe_path
        && let Some(window_parent) = Path::new(window_exe_path).parent()
    {
        for session in active_sessions {
            if let Some(session_path) = session.exe_path.as_deref()
                && Path::new(session_path).parent() == Some(window_parent)
            {
                return (session.pid, true);
            }
        }
    }

    (window_pid, false)
}

pub fn refresh_audio_capture_pid(candidate: &AudioAppCandidate) -> u32 {
    let hwnd = HWND(candidate.window_hwnd as *mut std::ffi::c_void);
    let exe_path = get_process_exe_path(candidate.pid);
    let active_sessions = active_audio_session_candidates();
    let (capture_pid, resolved_audio) = resolve_capture_pid_for_window(
        candidate.pid,
        hwnd,
        &candidate.process_name,
        exe_path.as_deref(),
        &active_sessions,
    );
    if resolved_audio {
        if capture_pid != candidate.capture_pid {
            crate::log_info!(
                "[AppSelection] refreshed window audio pid window_pid={} old_capture_pid={} capture_pid={} name={}",
                candidate.pid,
                candidate.capture_pid,
                capture_pid,
                candidate.display_name
            );
        }
        return capture_pid;
    }

    crate::log_info!(
        "[AppSelection] audio pid refresh kept window pid={} capture_pid={} name={}",
        candidate.pid,
        candidate.capture_pid,
        candidate.display_name
    );
    candidate.capture_pid
}

pub fn store_selected_audio_app_candidate(candidate: AudioAppCandidate) {
    if let Ok(mut selected) = SELECTED_AUDIO_APP_CANDIDATE.lock() {
        *selected = Some(candidate);
    }
}

pub fn clear_selected_audio_app_candidate() {
    if let Ok(mut selected) = SELECTED_AUDIO_APP_CANDIDATE.lock() {
        *selected = None;
    }
}

pub fn refresh_selected_audio_capture_pid() -> Option<u32> {
    let candidate = SELECTED_AUDIO_APP_CANDIDATE.lock().ok()?.clone()?;
    Some(refresh_audio_capture_pid(&candidate))
}

fn normalize_process_match_key(name: &str) -> String {
    name.chars()
        .filter(|ch| ch.is_ascii_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn enumerate_audio_app_candidates() -> Vec<AudioAppCandidate> {
    let mut apps: Vec<AudioAppCandidate> = Vec::new();
    let mut seen_pids: HashSet<u32> = HashSet::new();
    let active_sessions = active_audio_session_candidates();

    unsafe {
        let mut callback_data = (&mut apps, &mut seen_pids, &active_sessions);

        extern "system" fn enum_callback(hwnd: HWND, lparam: LPARAM) -> BOOL {
            unsafe {
                if !IsWindowVisible(hwnd).as_bool() {
                    return BOOL(1);
                }

                let mut title_buf = [0u16; 256];
                let len = GetWindowTextW(hwnd, &mut title_buf);
                if len == 0 {
                    return BOOL(1);
                }

                let title = String::from_utf16_lossy(&title_buf[..len as usize]);
                if title.is_empty() || title == "Program Manager" || title == "Settings" {
                    return BOOL(1);
                }

                let mut pid = 0u32;
                GetWindowThreadProcessId(hwnd, Some(&mut pid));
                if pid == 0 || pid == std::process::id() {
                    return BOOL(1);
                }

                let data = &mut *(lparam.0
                    as *mut (
                        &mut Vec<AudioAppCandidate>,
                        &mut HashSet<u32>,
                        &Vec<AudioSessionCandidate>,
                    ));
                let (apps, seen_pids, active_sessions) = data;
                if seen_pids.contains(&pid) {
                    return BOOL(1);
                }
                seen_pids.insert(pid);

                let exe_path = get_process_exe_path(pid);
                let process_name = exe_path
                    .as_deref()
                    .and_then(|path| Path::new(path).file_stem())
                    .and_then(|stem| stem.to_str())
                    .map(ToOwned::to_owned)
                    .unwrap_or_else(|| format!("PID {pid}"));
                let (width, height) = get_window_size(hwnd);
                let (capture_pid, resolved_audio) = resolve_capture_pid_for_window(
                    pid,
                    hwnd,
                    &process_name,
                    exe_path.as_deref(),
                    active_sessions,
                );
                if resolved_audio && capture_pid != pid {
                    crate::log_info!(
                        "[AppSelection] resolved window audio pid window_pid={} capture_pid={} name={}",
                        pid,
                        capture_pid,
                        title
                    );
                }
                let capture_exe_path = if capture_pid != pid {
                    get_process_exe_path(capture_pid)
                } else {
                    None
                };
                let hosted_exe_paths = hosted_child_exe_paths(hwnd, pid);
                let icon_data_url = get_app_icon_data_url(
                    pid,
                    hwnd,
                    exe_path.as_deref(),
                    capture_exe_path.as_deref(),
                    &hosted_exe_paths,
                );

                apps.push(AudioAppCandidate {
                    pid,
                    capture_pid,
                    display_name: title,
                    process_name,
                    icon_data_url,
                    window_hwnd: hwnd.0 as usize,
                    width,
                    height,
                });

                BOOL(1)
            }
        }

        let _ = EnumWindows(
            Some(enum_callback),
            LPARAM(&mut callback_data as *mut _ as isize),
        );
    }

    apps.sort_by(|left, right| {
        left.display_name
            .to_lowercase()
            .cmp(&right.display_name.to_lowercase())
    });
    apps
}
