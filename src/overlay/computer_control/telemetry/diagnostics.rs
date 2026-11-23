//! Human-readable diagnostics: static templates globally, rendered text locally.

use serde_json::{Value, json};

use super::{Privacy, event};

pub(in crate::overlay::computer_control) fn diagnostic(
    template: &'static str,
    arguments: std::fmt::Arguments<'_>,
) {
    emit("cc", template, &arguments.to_string());
}

pub(in crate::overlay::computer_control) fn human(component: &'static str, line: impl AsRef<str>) {
    emit(component, "diagnostic text", line.as_ref());
}

fn emit(component: &'static str, template: &'static str, line: &str) {
    event(
        "diagnostic",
        component,
        Privacy::Sensitive,
        json!({"template": template, "line": line}),
    );
    super::super::session_retention::persist(|| {
        let summary = global_summary(component, template, line);
        crate::log_info!("[cc-diagnostic] {summary}");
    });
}

fn global_summary(component: &'static str, template: &'static str, line: &str) -> Value {
    json!({
        "component": component,
        "template": template,
        "char_count": line.chars().count(),
        "byte_count": line.len(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn diagnostic_arguments_never_appear_in_the_global_summary() {
        let line = "clipboard: private text\nC:/private/file.txt https://private.invalid/path";
        let record = global_summary("cc", "result: {text}", line);
        assert_eq!(record["template"], "result: {text}");
        assert_eq!(record["char_count"], line.chars().count());
        assert!(!record.to_string().contains("private"));
        assert!(!record.to_string().contains("\n"));
    }
}
