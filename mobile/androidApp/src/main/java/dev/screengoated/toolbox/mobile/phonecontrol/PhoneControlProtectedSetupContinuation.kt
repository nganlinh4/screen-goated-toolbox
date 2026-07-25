package dev.screengoated.toolbox.mobile.phonecontrol

/**
 * One-shot continuation decision after a protected checkpoint restores visual evidence.
 *
 * A relay that made real provider progress may continue automatically. An unresolved relay
 * restores visual evidence but waits for fresh external evidence or an explicit user retry.
 */
internal class PhoneControlProtectedSetupContinuation {
    private var resumeSelectedSetup = false

    fun begin() {
        resumeSelectedSetup = false
    }

    fun relayCompleted() {
        resumeSelectedSetup = true
    }

    fun relayNeedsUserStep() {
        resumeSelectedSetup = false
    }

    fun authorityChanged(nextProviderNeedsSetup: Boolean) {
        resumeSelectedSetup = nextProviderNeedsSetup
    }

    fun consumeResumeSelectedSetup(): Boolean =
        resumeSelectedSetup.also { resumeSelectedSetup = false }
}
