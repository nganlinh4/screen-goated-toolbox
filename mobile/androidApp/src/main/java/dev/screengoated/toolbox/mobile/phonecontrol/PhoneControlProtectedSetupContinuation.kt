package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken

internal fun canDeliverFreshProjectionConsent(
    owner: String?,
    generation: String,
    resumeMode: Boolean,
): Boolean = resumeMode &&
    PhoneControlProtectedCheckpointRegistry.ownsFreshProjection(owner) &&
    PhoneControlProjectionDispatches.ownsConsent(generation)

internal fun <T> startOwnedFreshProtectedProjection(
    token: PhoneControlProtectedCheckpointToken,
    owner: String?,
    start: () -> T,
): T? {
    if (!PhoneControlProtectedCheckpointRegistry.ownsFreshProjection(token, owner)) return null
    return start()
}

internal fun acceptFreshProtectedProjection(
    token: PhoneControlProtectedCheckpointToken,
    attach: (PhoneControlProtectedCheckpointToken) -> Boolean,
    release: () -> Unit,
    accepted: () -> Unit,
): Boolean {
    if (!PhoneControlProtectedCheckpointRegistry.owns(token) || !attach(token)) {
        release()
        return false
    }
    accepted()
    return true
}

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

    fun projectionAttached(clearNotification: () -> Unit): Boolean {
        clearNotification()
        return consumeResumeSelectedSetup()
    }
}
