package dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility

internal object AccessibilityWindowAttribution {
    private data class Entry(
        val generation: Long,
        val packageName: String,
    )

    private val lock = Any()
    private val entries = linkedMapOf<Int, Entry>()

    fun record(
        windowId: Int,
        packageName: String?,
        generation: Long,
    ) {
        val packageIdentity = packageName?.trim()?.takeIf(String::isNotEmpty) ?: return
        if (windowId < 0 || generation <= 0) return
        synchronized(lock) {
            entries.entries.removeAll { (_, entry) -> entry.generation != generation }
            entries[windowId] = Entry(generation, packageIdentity)
        }
    }

    fun resolve(
        windowId: Int,
        generation: Long,
    ): String? = synchronized(lock) {
        entries[windowId]
            ?.takeIf { entry -> entry.generation == generation }
            ?.packageName
    }

    fun clear() = synchronized(lock) {
        entries.clear()
    }
}
