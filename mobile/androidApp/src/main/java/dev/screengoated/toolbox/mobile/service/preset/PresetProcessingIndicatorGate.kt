package dev.screengoated.toolbox.mobile.service.preset

/**
 * The processing indicator covers a session only until its first result window
 * appears, as Windows closes it on the first result. A session whose windows
 * are later retired (skipped downstream blocks) never brings it back.
 */
internal class PresetProcessingIndicatorGate {
    private var resultShownSessionId: String? = null

    fun shouldShow(sessionId: String?, executing: Boolean, hasResultWindow: Boolean): Boolean {
        if (hasResultWindow) {
            resultShownSessionId = sessionId
            return false
        }
        return executing && (sessionId == null || sessionId != resultShownSessionId)
    }
}
