package dev.screengoated.toolbox.mobile.phonecontrol.authority

import android.content.Context

internal sealed interface PhoneControlProtectedSetupResult {
    data object Completed : PhoneControlProtectedSetupResult

    data class NeedsUserStep(val code: String) : PhoneControlProtectedSetupResult

    data class Failed(val code: String) : PhoneControlProtectedSetupResult
}

internal enum class PhoneControlProtectedCapturePolicy {
    RETAIN_PROJECTION,
    RELEASE_PROJECTION,
}

internal sealed interface PhoneControlProtectedCheckpointReadiness {
    data object Ready : PhoneControlProtectedCheckpointReadiness

    data class NotReady(val code: String) : PhoneControlProtectedCheckpointReadiness
}

internal data class PhoneControlProtectedSetupNavigationContract(
    val platformCapability: String,
    val destinationState: String,
)

/**
 * Local-only adapter for a platform-owned setup checkpoint.
 *
 * Implementations may handle an ephemeral setup secret only while [token]
 * owns the process checkpoint. They must not persist, log, caption, trace, or
 * return that secret.
 */
internal interface PhoneControlProtectedSetupAdapter {
    val capturePolicy: PhoneControlProtectedCapturePolicy
    val navigationContract: PhoneControlProtectedSetupNavigationContract

    fun checkpointReadiness(context: Context): PhoneControlProtectedCheckpointReadiness

    suspend fun complete(
        context: Context,
        token: PhoneControlProtectedCheckpointToken,
    ): PhoneControlProtectedSetupResult
}
