package dev.screengoated.toolbox.mobile.phonecontrol.authority

import android.content.Context

internal sealed interface PhoneControlProtectedSetupResult {
    data object Completed : PhoneControlProtectedSetupResult

    data class NeedsUserStep(val code: String) : PhoneControlProtectedSetupResult

    data class Failed(val code: String) : PhoneControlProtectedSetupResult
}

/**
 * Local-only adapter for a platform-owned setup checkpoint.
 *
 * Implementations may handle an ephemeral setup secret only while [token]
 * owns the process checkpoint. They must not persist, log, caption, trace, or
 * return that secret.
 */
internal fun interface PhoneControlProtectedSetupAdapter {
    suspend fun complete(
        context: Context,
        token: PhoneControlProtectedCheckpointToken,
    ): PhoneControlProtectedSetupResult
}
