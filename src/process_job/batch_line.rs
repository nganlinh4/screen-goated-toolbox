//! `cmd.exe` command line for a batch script, matching Rust std's rules.
//!
//! std's `Command` runs a `.bat`/`.cmd` file as
//! `cmd.exe /e:ON /v:OFF /d /c ""<script>" <args>"`. Arguments containing
//! anything but known-safe ASCII are quoted, `"` is doubled, backslashes
//! before a quote are doubled, and `%` becomes `%%cd:~,%` so cmd.exe cannot
//! expand variables. CR and LF are rejected because cmd.exe would end the
//! command there. Like std, a verbatim (`\\?\`) script path is converted to its
//! ordinary form, which cmd.exe requires. Building the same line ourselves lets
//! a batch script be created inside its job in one native call.

use anyhow::{Result, ensure};
use std::ffi::{OsStr, OsString};
use std::os::windows::ffi::OsStrExt;

const UNQUOTED: &str = r"#$*+-./:?@\_";

pub(super) fn command_line(script: &OsStr, arguments: &[OsString]) -> Result<Vec<u16>> {
    let script = user_path(script.encode_wide().collect());
    ensure!(
        !script.contains(&0) && !script.contains(&u16::from(b'"')),
        "batch script path contains an invalid character"
    );
    ensure!(
        script.last() != Some(&u16::from(b'\\')),
        "batch script path must not end with a backslash"
    );
    let mut line: Vec<u16> = "cmd.exe /e:ON /v:OFF /d /c \"\"".encode_utf16().collect();
    line.extend_from_slice(&script);
    line.push(u16::from(b'"'));
    for argument in arguments {
        line.push(u16::from(b' '));
        append_argument(argument, &mut line)?;
    }
    line.push(u16::from(b'"'));
    Ok(line)
}

/// Strip a verbatim prefix from a drive (`\\?\C:\`) or UNC (`\\?\UNC\`) path
/// when no `.` or `..` component would make the ordinary form name a
/// different file. Anything else is kept as given.
fn user_path(path: Vec<u16>) -> Vec<u16> {
    let text = String::from_utf16_lossy(&path);
    let (prefix_len, replacement) = if text.starts_with(r"\\?\UNC\") {
        (8, r"\\")
    } else if text.starts_with(r"\\?\")
        && text.as_bytes().get(4).is_some_and(u8::is_ascii_alphabetic)
        && text.as_bytes().get(5..7) == Some(br":\".as_slice())
    {
        (4, "")
    } else {
        return path;
    };
    let rest = &path[prefix_len..];
    let dot = u16::from(b'.');
    let has_relative_component = rest
        .split(|unit| *unit == u16::from(b'\\'))
        .any(|component| component == [dot] || component == [dot, dot]);
    if has_relative_component {
        return path;
    }
    replacement
        .encode_utf16()
        .chain(rest.iter().copied())
        .collect()
}

fn append_argument(argument: &OsStr, line: &mut Vec<u16>) -> Result<()> {
    let units: Vec<u16> = argument.encode_wide().collect();
    ensure!(
        !units.iter().any(|unit| matches!(*unit, 0 | 0x0A | 0x0D)),
        "batch file arguments are invalid"
    );
    let quote = units.is_empty()
        || units.last() == Some(&u16::from(b'\\'))
        || char::decode_utf16(units.iter().copied()).any(|decoded| {
            decoded.is_ok_and(|character| {
                (character.is_ascii()
                    && !(character.is_ascii_alphanumeric() || UNQUOTED.contains(character)))
                    || character.is_control()
            })
        });
    if quote {
        line.push(u16::from(b'"'));
    }
    let mut backslashes = 0_usize;
    for unit in units {
        if unit == u16::from(b'\\') {
            backslashes += 1;
        } else {
            if unit == u16::from(b'"') {
                line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
                line.push(u16::from(b'"'));
            } else if unit == u16::from(b'%') {
                line.extend("%%cd:~,".encode_utf16());
            }
            backslashes = 0;
        }
        line.push(unit);
    }
    if quote {
        line.extend(std::iter::repeat_n(u16::from(b'\\'), backslashes));
        line.push(u16::from(b'"'));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::command_line;
    use std::ffi::{OsStr, OsString};

    fn render(arguments: &[&str]) -> String {
        let arguments: Vec<OsString> = arguments.iter().map(OsString::from).collect();
        String::from_utf16(&command_line(OsStr::new(r"C:\s p\run.cmd"), &arguments).unwrap())
            .unwrap()
    }

    #[test]
    fn safe_arguments_stay_bare_and_the_script_is_double_quoted() {
        assert_eq!(
            render(&["plain", r"C:\dir\file.txt", "-x", "가"]),
            r#"cmd.exe /e:ON /v:OFF /d /c ""C:\s p\run.cmd" plain C:\dir\file.txt -x 가""#
        );
    }

    #[test]
    fn metacharacters_quotes_percent_and_trailing_backslashes_are_escaped() {
        assert_eq!(
            render(&["a&b", "", r#"q"t"#, r#"s\"t"#, r"dir\", "100%"]),
            r#"cmd.exe /e:ON /v:OFF /d /c ""C:\s p\run.cmd" "a&b" "" "q""t" "s\\""t" "dir\\" "100%%cd:~,%"""#
        );
    }

    #[test]
    fn verbatim_script_paths_use_their_ordinary_form() {
        let line = |script: &str| {
            String::from_utf16(&command_line(OsStr::new(script), &[]).unwrap()).unwrap()
        };
        assert_eq!(
            line(r"\\?\C:\dir\run.cmd"),
            r#"cmd.exe /e:ON /v:OFF /d /c ""C:\dir\run.cmd"""#
        );
        assert_eq!(
            line(r"\\?\UNC\host\share\run.cmd"),
            r#"cmd.exe /e:ON /v:OFF /d /c ""\\host\share\run.cmd"""#
        );
        assert_eq!(
            line(r"\\?\C:\dir\..\run.cmd"),
            r#"cmd.exe /e:ON /v:OFF /d /c ""\\?\C:\dir\..\run.cmd"""#
        );
    }

    #[test]
    fn line_breaks_and_invalid_script_paths_are_rejected() {
        for argument in ["a\nb", "a\rb", "a\0b"] {
            assert!(command_line(OsStr::new(r"C:\run.cmd"), &[OsString::from(argument)]).is_err());
        }
        for script in [r#"C:\a"b.cmd"#, r"C:\dir\"] {
            assert!(command_line(OsStr::new(script), &[]).is_err());
        }
    }
}
