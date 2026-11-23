package dev.screengoated.toolbox.mobile.updater

internal fun tryStartPlayUpdate(start: () -> Boolean): Boolean = runCatching(start).getOrDefault(false)
