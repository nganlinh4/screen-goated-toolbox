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
    if versioned_name(&name, &["python", "pythonw", "pypy"]) {
        return option_prefix(args).any(|arg| arg.eq_ignore_ascii_case("-c"));
    }
    if versioned_name(&name, &["ruby", "perl", "lua", "luajit"]) {
        return option_prefix(args).any(|arg| arg.eq_ignore_ascii_case("-e"));
    }
    if versioned_name(&name, &["php"]) {
        return option_prefix(args).any(|arg| arg == "-r");
    }
    if versioned_name(&name, &["r", "rscript"]) {
        return option_prefix(args).any(|arg| arg == "-e" || arg.eq_ignore_ascii_case("--expr"));
    }
    match name.as_str() {
        "cmd" => {
            option_prefix(args).any(|arg| matches!(arg.to_ascii_lowercase().as_str(), "/c" | "/k"))
        }
        "powershell" | "pwsh" => option_prefix(args).any(|arg| {
            matches!(
                arg.trim_start_matches(['-', '/'])
                    .to_ascii_lowercase()
                    .as_str(),
                "c" | "command" | "commandwithargs" | "e" | "ec" | "enc" | "encodedcommand"
            )
        }),
        "sh" | "bash" | "dash" | "zsh" | "fish" | "nu" => option_prefix(args).any(|arg| {
            arg.eq_ignore_ascii_case("--command")
                || arg
                    .strip_prefix('-')
                    .is_some_and(|flags| !flags.starts_with('-') && flags.contains('c'))
        }),
        "node" | "nodejs" | "bun" => option_prefix(args).any(|arg| {
            matches!(
                arg.to_ascii_lowercase().as_str(),
                "-e" | "--eval" | "-p" | "--print"
            ) || arg.to_ascii_lowercase().starts_with("--eval=")
                || arg.to_ascii_lowercase().starts_with("--print=")
        }),
        "py" => option_prefix(args).any(|arg| arg.eq_ignore_ascii_case("-c")),
        _ => false,
    }
}

fn option_prefix(args: &[String]) -> impl Iterator<Item = &str> {
    args.iter()
        .map(String::as_str)
        .take_while(|arg| *arg != "--" && (arg.starts_with('-') || arg.starts_with('/')))
}

fn versioned_name(name: &str, stems: &[&str]) -> bool {
    stems.iter().any(|stem| {
        name.strip_prefix(stem)
            .is_some_and(|suffix| suffix.chars().all(|ch| ch.is_ascii_digit() || ch == '.'))
    })
}
