use serde_json::{Value, json};
use std::path::Path;

pub(super) fn inline_interpreter_rejection(program: &str, args: &[String]) -> Option<Value> {
    invokes_inline_interpreter(program, args).then(|| {
        json!({
            "ok": false,
            "code": "INLINE_INTERPRETER_NOT_EXACT_PROCESS",
            "status": "exact_process_rejected",
            "retryable": true,
            "effect_verified": false,
            "effect_may_have_occurred": false,
            "error": "inline interpreter code cannot use exact process mode",
            "repair": "invoke the target executable directly with program,args,cwd; otherwise use command, whose output is withheld from completion proof",
        })
    })
}

pub(super) fn invokes_inline_interpreter(program: &str, args: &[String]) -> bool {
    let name = Path::new(program)
        .file_stem()
        .and_then(|name| name.to_str())
        .unwrap_or(program)
        .to_ascii_lowercase();
    if versioned_name(&name, &["python", "pythonw", "pypy"]) || name == "py" {
        return options(args, |arg| matches!(arg, "-W" | "-X"))
            .any(|arg| arg.eq_ignore_ascii_case("-c") || flag_group_reaches(arg, "c", "WXm"));
    }
    if versioned_name(&name, &["perl"]) {
        return options(args, |_| false).any(|arg| flag_group_reaches(arg, "eE", "CdDFiIMmx"));
    }
    if versioned_name(&name, &["ruby"]) {
        return options(args, |_| false)
            .any(|arg| arg == "-E" || flag_group_reaches(arg, "e", "CEFIKrTWx"));
    }
    if versioned_name(&name, &["lua", "luajit"]) {
        return options(args, |_| false).any(|arg| arg.eq_ignore_ascii_case("-e"));
    }
    if versioned_name(&name, &["php"]) {
        return options(args, |_| false).any(|arg| arg == "-r");
    }
    if versioned_name(&name, &["r", "rscript"]) {
        return options(args, |_| false)
            .any(|arg| arg == "-e" || arg.eq_ignore_ascii_case("--expr"));
    }
    match name.as_str() {
        "cmd" => cmd_runs_inline_code(args),
        // Windows PowerShell binds a bare positional to -Command; pwsh binds
        // it to -File.
        "powershell" => powershell_runs_inline_code(args, true),
        "pwsh" => powershell_runs_inline_code(args, false),
        "sh" | "bash" | "dash" | "zsh" | "fish" | "nu" => options(args, |arg| {
            matches!(arg, "-o" | "+o" | "-O" | "+O" | "--rcfile" | "--init-file")
        })
        .any(|arg| {
            arg.eq_ignore_ascii_case("--command")
                || arg
                    .strip_prefix('-')
                    .is_some_and(|flags| !flags.starts_with('-') && flags.contains('c'))
        }),
        "node" | "nodejs" | "bun" => options(args, |arg| {
            matches!(
                arg,
                "-r" | "--require"
                    | "--import"
                    | "--loader"
                    | "--experimental-loader"
                    | "-C"
                    | "--conditions"
            )
        })
        .any(|arg| {
            matches!(
                arg.to_ascii_lowercase().as_str(),
                "-e" | "--eval" | "-p" | "--print"
            ) || arg.to_ascii_lowercase().starts_with("--eval=")
                || arg.to_ascii_lowercase().starts_with("--print=")
        }),
        _ => false,
    }
}

/// Leading interpreter options. An option that takes a separate value keeps
/// the scan going past that value, so it cannot hide a later inline-code flag.
fn options(args: &[String], takes_value: impl Fn(&str) -> bool) -> impl Iterator<Item = &str> {
    let mut args = args.iter().map(String::as_str);
    std::iter::from_fn(move || {
        let arg = args.next()?;
        if arg == "--" || !(arg.starts_with('-') || arg.starts_with('/')) {
            return None;
        }
        if takes_value(arg) {
            args.next();
        }
        Some(arg)
    })
}

/// A short-flag group such as `-c`, `-Bc` or `-ne` that reaches one of
/// `code_flags`. A letter in `value_flags` consumes the rest of the group.
fn flag_group_reaches(arg: &str, code_flags: &str, value_flags: &str) -> bool {
    let Some(group) = arg
        .strip_prefix('-')
        .filter(|group| !group.starts_with('-'))
    else {
        return false;
    };
    for flag in group.chars() {
        if code_flags.contains(flag) {
            return true;
        }
        if !flag.is_ascii_alphabetic() || value_flags.contains(flag) {
            return false;
        }
    }
    false
}

/// cmd scans its whole command line for `/C`, `/K` and `/R`, including a
/// switch chained onto another (`/d/c`) or glued to its command (`/cecho`).
fn cmd_runs_inline_code(args: &[String]) -> bool {
    args.iter().any(|arg| {
        arg.split('/').skip(1).any(|switch| {
            matches!(
                switch.bytes().next(),
                Some(b'c' | b'C' | b'k' | b'K' | b'r' | b'R')
            )
        })
    })
}

/// PowerShell parameters accept `-` or `/`, any unambiguous prefix and an
/// attached `:value`. `-File` and a bare positional end the host's options;
/// the positional itself is code when the host binds it to -Command.
fn powershell_runs_inline_code(args: &[String], positional_is_command: bool) -> bool {
    const VALUE_PARAMETERS: [&str; 11] = [
        "configurationfile",
        "configurationname",
        "custompipename",
        "executionpolicy",
        "inputformat",
        "outputformat",
        "psconsolefile",
        "settingsfile",
        "version",
        "windowstyle",
        "workingdirectory",
    ];
    const VALUE_ALIASES: [&str; 11] = [
        "config", "ep", "ex", "if", "inp", "o", "of", "settings", "w", "wd", "wo",
    ];
    let mut args = args.iter().map(String::as_str);
    while let Some(arg) = args.next() {
        let Some(parameter) = arg.strip_prefix(['-', '/']) else {
            return positional_is_command;
        };
        let (name, attached) = match parameter.split_once(':') {
            Some((name, _)) => (name.to_ascii_lowercase(), true),
            None => (parameter.to_ascii_lowercase(), false),
        };
        let abbreviates =
            |full: &str, minimum: usize| name.len() >= minimum && full.starts_with(&name);
        if matches!(name.as_str(), "c" | "e" | "ec" | "cwa")
            || abbreviates("command", 3)
            || abbreviates("commandwithargs", 8)
            || abbreviates("encodedcommand", 3)
        {
            return true;
        }
        if name == "f" || abbreviates("file", 2) {
            return false;
        }
        if !attached
            && (VALUE_ALIASES.contains(&name.as_str())
                || VALUE_PARAMETERS.iter().any(|full| abbreviates(full, 3)))
        {
            args.next();
        }
    }
    false
}

fn versioned_name(name: &str, stems: &[&str]) -> bool {
    stems.iter().any(|stem| {
        name.strip_prefix(stem)
            .is_some_and(|suffix| suffix.chars().all(|ch| ch.is_ascii_digit() || ch == '.'))
    })
}
