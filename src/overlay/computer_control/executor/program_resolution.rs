use anyhow::{Result, anyhow};
use std::path::{Component, Path, PathBuf};

pub(super) fn resolve_program(program: &str, cwd: &Path) -> Result<String> {
    let directories = std::env::var_os("PATH")
        .map(|path| std::env::split_paths(&path).collect::<Vec<_>>())
        .unwrap_or_default();
    resolve_program_with(program, cwd, &directories, &windows_program_extensions())
}

fn resolve_program_with(
    program: &str,
    cwd: &Path,
    directories: &[PathBuf],
    extensions: &[String],
) -> Result<String> {
    let requested = Path::new(program);
    if !requested.is_absolute()
        && matches!(requested.components().next(), Some(Component::Prefix(_)))
    {
        return Err(anyhow!(
            "program must use an absolute or unambiguous relative path"
        ));
    }
    let explicit_path = requested.is_absolute() || requested.components().count() > 1;
    let explicit_base = explicit_path.then(|| cwd.join(requested));
    let mut bases = Vec::new();
    if let Some(base) = &explicit_base {
        bases.push(std::path::absolute(base)?);
    } else {
        bases.extend(
            directories
                .iter()
                .filter(|path| !path.as_os_str().is_empty())
                .map(|path| path.join(requested)),
        );
    }
    for base in bases {
        let Ok(base) = std::path::absolute(base) else {
            continue;
        };
        if requested.extension().is_some() {
            if base.is_file() {
                return Ok(base.to_string_lossy().into_owned());
            }
            continue;
        }
        for extension in extensions {
            let candidate = base.with_extension(extension);
            if candidate.is_file() {
                return Ok(candidate.to_string_lossy().into_owned());
            }
        }
        if explicit_path && base.is_file() {
            return Ok(base.to_string_lossy().into_owned());
        }
    }
    match explicit_base {
        Some(base) => Ok(std::path::absolute(base)?.to_string_lossy().into_owned()),
        None => Ok(program.to_string()),
    }
}

/// Resolve a bare name that PATH lookup did not find the way Rust std's
/// `Command` continues its search: the host executable's directory, then the
/// system directory, then the Windows directory, adding `.exe` to a name
/// without an extension.
pub(super) fn native_fallback(program: &str) -> Option<PathBuf> {
    native_fallback_in(program, &native_fallback_directories())
}

fn native_fallback_in(program: &str, directories: &[PathBuf]) -> Option<PathBuf> {
    let requested = Path::new(program);
    if requested.is_absolute() || requested.components().count() != 1 {
        return None;
    }
    let name = if requested.extension().is_some() {
        program.to_owned()
    } else {
        format!("{program}.exe")
    };
    directories
        .iter()
        .map(|directory| directory.join(&name))
        .find(|candidate| candidate.is_file())
}

fn native_fallback_directories() -> Vec<PathBuf> {
    let mut directories = Vec::new();
    if let Ok(executable) = std::env::current_exe()
        && let Some(parent) = executable.parent()
    {
        directories.push(parent.to_owned());
    }
    if let Ok(probe) = crate::system_command::system_executable("cmd.exe")
        && let Some(system) = probe.parent()
    {
        directories.push(system.to_owned());
        if let Some(windows) = system.parent() {
            directories.push(windows.to_owned());
        }
    }
    directories
}

fn windows_program_extensions() -> Vec<String> {
    let configured = std::env::var("PATHEXT").unwrap_or_default();
    let extensions = configured
        .split(';')
        .map(|extension| {
            extension
                .trim()
                .trim_start_matches('.')
                .to_ascii_lowercase()
        })
        .filter(|extension| matches!(extension.as_str(), "com" | "exe" | "bat" | "cmd"))
        .collect::<Vec<_>>();
    if extensions.is_empty() {
        vec!["com".into(), "exe".into(), "bat".into(), "cmd".into()]
    } else {
        extensions
    }
}

#[cfg(test)]
#[path = "program_resolution_tests.rs"]
mod tests;
