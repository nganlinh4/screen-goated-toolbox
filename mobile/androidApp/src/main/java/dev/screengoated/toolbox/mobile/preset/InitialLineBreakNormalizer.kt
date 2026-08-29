package dev.screengoated.toolbox.mobile.preset

internal class InitialLineBreakNormalizer {
    private var started = false

    fun observe(chunk: String): String? {
        if (chunk.startsWith(TextApiClient.WIPE_SIGNAL)) {
            started = false
            val replacement = chunk.removePrefix(TextApiClient.WIPE_SIGNAL)
            return TextApiClient.WIPE_SIGNAL + normalizeStart(replacement)
        }
        return normalizeStart(chunk).takeIf(String::isNotEmpty)
    }

    fun finish(output: String): String = output.trimStart('\r', '\n')

    private fun normalizeStart(text: String): String {
        if (started) return text
        val normalized = text.trimStart('\r', '\n')
        if (normalized.isNotEmpty()) started = true
        return normalized
    }
}
