use std::sync::atomic::AtomicBool;

use crate::component_registry::capabilities;
use crate::component_registry::external_tools::{ExternalTool, ExternalToolUse};

pub(crate) fn acquire_ffmpeg_with_badge() -> Result<ExternalToolUse, String> {
    acquire_ffmpeg_with_badge_message("")
}

pub(crate) fn acquire_ffmpeg_with_badge_message(
    download_message: &str,
) -> Result<ExternalToolUse, String> {
    let cancelled = AtomicBool::new(false);
    capabilities::resolve_external_tool_with_badge(
        ExternalTool::Ffmpeg,
        &cancelled,
        Some(download_message),
    )
    .map_err(|error| error.to_string())
}
