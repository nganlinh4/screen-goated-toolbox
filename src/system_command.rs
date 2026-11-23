#[cfg(any(test, not(feature = "recorder-worker")))]
use std::ffi::OsStr;
use std::io;
use std::path::PathBuf;
#[cfg(any(test, not(feature = "recorder-worker")))]
use std::process::Command;

/// Prepare a noninteractive helper without allocating a Windows console.
#[cfg(any(test, not(feature = "recorder-worker")))]
pub(crate) fn background_command(executable: impl AsRef<OsStr>) -> Command {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        use windows::Win32::System::Threading::CREATE_NO_WINDOW;
        let mut command = Command::new(executable);
        command.creation_flags(CREATE_NO_WINDOW.0);
        command
    }
    #[cfg(not(windows))]
    {
        Command::new(executable)
    }
}

/// Resolve Windows-owned tools without searching the application directory or PATH.
#[cfg(any(test, not(feature = "recorder-worker")))]
pub(crate) fn system_executable(name: &str) -> io::Result<PathBuf> {
    system_relative_executable(&[name])
}

#[cfg(any(test, not(feature = "recorder-worker")))]
pub(crate) fn system_relative_executable(components: &[&str]) -> io::Result<PathBuf> {
    if components.is_empty()
        || components.iter().any(|name| {
            name.is_empty()
                || name.ends_with(['.', ' '])
                || name
                    .chars()
                    .any(|c| c.is_control() || "<>:\"/\\|?*".contains(c))
        })
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "invalid system tool name",
        ));
    }
    let mut path = windows_directory(false)?;
    for component in components {
        path.push(component);
    }
    Ok(path)
}

/// The Windows shell is installed in the Windows directory, not System32.
#[cfg(any(test, feature = "recorder-worker"))]
pub(crate) fn explorer_executable() -> io::Result<PathBuf> {
    Ok(windows_directory(true)?.join("explorer.exe"))
}

fn windows_directory(shell: bool) -> io::Result<PathBuf> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStringExt;
        use windows::Win32::System::SystemInformation::{
            GetSystemDirectoryW, GetSystemWindowsDirectoryW,
        };
        let mut buffer = vec![0u16; 260];
        loop {
            let length = unsafe {
                if shell {
                    GetSystemWindowsDirectoryW(Some(&mut buffer))
                } else {
                    GetSystemDirectoryW(Some(&mut buffer))
                }
            } as usize;
            if length == 0 {
                return Err(io::Error::last_os_error());
            }
            if length < buffer.len() {
                buffer.truncate(length);
                return Ok(PathBuf::from(std::ffi::OsString::from_wide(&buffer)));
            }
            if length > 32_767 {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    "system directory too long",
                ));
            }
            buffer.resize(length + 1, 0);
        }
    }
    #[cfg(not(windows))]
    {
        let _ = shell;
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "Windows system tools are unavailable",
        ))
    }
}

#[cfg(test)]
#[path = "system_command/background_tests.rs"]
mod background_tests;

#[cfg(test)]
mod tests {
    use super::{explorer_executable, system_executable};

    #[test]
    fn system_tools_never_accept_paths_or_search_the_working_directory() {
        for name in [
            "",
            ".",
            "..",
            ".. ",
            "../cmd.exe",
            "dir\\cmd.exe",
            "C:cmd.exe",
            "cmd.exe\0",
        ] {
            assert!(system_executable(name).is_err());
        }
        #[cfg(windows)]
        {
            let path = system_executable("schtasks.exe").unwrap();
            assert!(path.is_absolute());
            assert_eq!(path.file_name().unwrap(), "schtasks.exe");
            assert!(path.is_file());
            let shell = explorer_executable().unwrap();
            assert!(shell.is_absolute());
            assert_eq!(shell.file_name().unwrap(), "explorer.exe");
            assert!(shell.is_file());
        }
    }
}
