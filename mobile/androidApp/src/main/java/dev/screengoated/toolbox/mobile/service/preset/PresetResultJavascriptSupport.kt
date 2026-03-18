package dev.screengoated.toolbox.mobile.service.preset

internal fun presetResultJavascript(): String {
    return listOf(
        presetResultJavascriptCore(),
        presetResultJavascriptTouchSupport(),
    ).joinToString("\n")
}

internal fun presetResultInteractionJavascript(): String {
    return presetResultJavascript()
}
