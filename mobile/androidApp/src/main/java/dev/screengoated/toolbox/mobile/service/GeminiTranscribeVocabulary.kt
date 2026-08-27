package dev.screengoated.toolbox.mobile.service

internal object GeminiTranscribeVocabulary {
    data class Snapshot(val version: Long, val entries: List<String>)

    private var version = 0L
    private var entries = emptyList<String>()

    @Synchronized
    fun snapshot(): Snapshot = Snapshot(version, entries)

    @Synchronized
    fun update(lines: String): Snapshot = replace(lines.lineSequence().toList())

    @Synchronized
    fun replace(values: List<String>): Snapshot {
        val normalized = values.asSequence()
            .map(String::trim)
            .filter(String::isNotEmpty)
            .distinct()
            .take(MAX_ENTRIES)
            .toList()
        if (entries != normalized) {
            entries = normalized
            version++
        }
        return Snapshot(version, entries)
    }

    private const val MAX_ENTRIES = 1_000
}
