package dev.screengoated.toolbox.mobile.ui

/**
 * Suppresses settled-page selection sync while a tab-initiated pager scroll runs.
 * A scroll cancelled by a user swipe or a newer tab request always releases the
 * flag, but only the newest request may clear it.
 */
internal class ShellTabNavigation {
    private var generation = 0L

    var navigating: Boolean = false
        private set

    suspend fun navigate(scroll: suspend () -> Unit) {
        val token = ++generation
        navigating = true
        try {
            scroll()
        } finally {
            if (generation == token) navigating = false
        }
    }
}
