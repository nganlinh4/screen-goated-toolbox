package dev.screengoated.toolbox.mobile.service.preset

internal fun plainPresetResult(text: String): String =
    "<div class=\"preset-plain-text\" style=\"white-space:pre-wrap;overflow-wrap:anywhere\">${escapeHtml(text)}</div>"
