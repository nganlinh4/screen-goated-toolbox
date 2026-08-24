//! Central host-side resolution for optional executable capabilities.

use std::sync::atomic::AtomicBool;

use anyhow::Result;

use super::external_tools::{self, ExternalTool, ExternalToolInstallEvent, ExternalToolUse};

const MISSING_CAPABILITY_PREFIX: &str = "MISSING_CAPABILITY:";

pub(crate) const RECORDER_REQUIRED_EXTERNAL_TOOLS: [ExternalTool; 0] = [];

pub(crate) fn acquire_external_tool(tool: ExternalTool) -> Result<ExternalToolUse> {
    external_tools::acquire_installed(tool)
}

pub(crate) fn resolve_external_tool(
    tool: ExternalTool,
    cancelled: &AtomicBool,
    on_event: impl Fn(ExternalToolInstallEvent),
) -> Result<ExternalToolUse> {
    if let Ok(component) = acquire_external_tool(tool) {
        return Ok(component);
    }
    external_tools::ensure(tool, cancelled, on_event)
}

pub(crate) fn resolve_external_tool_with_badge(
    tool: ExternalTool,
    cancelled: &AtomicBool,
    download_message: Option<&str>,
) -> Result<ExternalToolUse> {
    if let Ok(component) = acquire_external_tool(tool) {
        return Ok(component);
    }

    let name = external_tools::localized_tool_name(tool);
    let badge = download_message
        .filter(|message| !message.trim().is_empty())
        .map_or_else(
            || crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&name),
            |message| {
                crate::overlay::auto_copy_badge::DownloadProgressBadge::with_text(&name, message)
            },
        );
    let result = external_tools::ensure(tool, cancelled, |event| {
        external_tools::report_badge_event(&badge, &name, event);
    });
    badge.finish();
    notify_resolution(&name, &result);
    result
}

pub(crate) fn resolve_external_tool_with_badge_before_capture(
    tool: ExternalTool,
    cancelled: &AtomicBool,
) -> Result<ExternalToolUse> {
    if let Ok(component) = acquire_external_tool(tool) {
        return Ok(component);
    }

    let name = external_tools::localized_tool_name(tool);
    let badge = crate::overlay::auto_copy_badge::DownloadProgressBadge::new(&name);
    let result = external_tools::ensure(tool, cancelled, |event| {
        external_tools::report_badge_event(&badge, &name, event);
    });
    badge.finish();

    let result = match result {
        Ok(component)
            if crate::overlay::status_compositor::progress_remove_before_capture(
                std::time::Duration::from_secs(5),
            ) =>
        {
            Ok(component)
        }
        Ok(_) => Err(anyhow::anyhow!(
            "status notification did not clear before capture"
        )),
        Err(error) => Err(error),
    };
    if result.is_err() {
        notify_resolution(&name, &result);
    }
    result
}

pub(crate) fn requested_external_tool(error: &str) -> Option<ExternalTool> {
    error
        .match_indices(MISSING_CAPABILITY_PREFIX)
        .find_map(|(index, _)| {
            let tail = &error[index + MISSING_CAPABILITY_PREFIX.len()..];
            let id = tail
                .split(|character: char| character == ':' || character.is_whitespace())
                .next()
                .unwrap_or_default();
            ExternalTool::from_id(id)
        })
}

fn notify_resolution(name: &str, result: &Result<ExternalToolUse>) {
    let locale = crate::overlay::auto_copy_badge::locale_text();
    let (template, kind, detail) = match result {
        Ok(_) => (
            locale.component_installed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Success,
            String::new(),
        ),
        Err(error) => (
            locale.component_install_failed_fmt,
            crate::overlay::auto_copy_badge::NotificationType::Error,
            format!("{error:#}"),
        ),
    };
    let title = crate::overlay::auto_copy_badge::format_locale(template, &[("name", name)]);
    crate::overlay::auto_copy_badge::show_detailed_notification(&title, &detail, kind);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn recorder_dependencies_are_host_managed() {
        assert!(RECORDER_REQUIRED_EXTERNAL_TOOLS.is_empty());
    }

    #[test]
    fn missing_capability_parser_accepts_context_but_only_allowlisted_exact_ids() {
        assert_eq!(
            requested_external_tool(
                "export failed: MISSING_CAPABILITY:ffmpeg-x64: host must resolve it"
            ),
            Some(ExternalTool::Ffmpeg)
        );
        for rejected in [
            "MISSING_CAPABILITY:ffmpeg-x64-extra: no",
            "MISSING_CAPABILITY:unknown-x64: no",
            "ffmpeg-x64",
        ] {
            assert_eq!(requested_external_tool(rejected), None);
        }
    }
}
