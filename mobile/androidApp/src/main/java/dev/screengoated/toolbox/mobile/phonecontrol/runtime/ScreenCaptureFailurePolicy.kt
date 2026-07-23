package dev.screengoated.toolbox.mobile.phonecontrol.runtime

/** Delays user-visible degradation while a retryable capture route can recover. */
internal class ScreenCaptureFailurePolicy(
    private val retryableGraceAttempts: Int = DEFAULT_RETRYABLE_GRACE_ATTEMPTS,
) {
    private var lastCode: String? = null
    private var consecutiveFailures = 0

    init {
        require(retryableGraceAttempts >= 0)
    }

    fun shouldPublish(code: String, retryable: Boolean): Boolean {
        consecutiveFailures = if (code == lastCode) consecutiveFailures + 1 else 1
        lastCode = code
        return !retryable || consecutiveFailures > retryableGraceAttempts
    }

    fun reset() {
        lastCode = null
        consecutiveFailures = 0
    }

    private companion object {
        const val DEFAULT_RETRYABLE_GRACE_ATTEMPTS = 2
    }
}
