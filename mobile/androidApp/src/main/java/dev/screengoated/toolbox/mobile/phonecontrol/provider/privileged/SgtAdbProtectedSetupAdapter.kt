package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupResult
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState

internal object SgtAdbProtectedSetupAdapter : PhoneControlProtectedSetupAdapter {
    override suspend fun complete(
        context: Context,
        token: PhoneControlProtectedCheckpointToken,
    ): PhoneControlProtectedSetupResult {
        val pairingCode = ProtectedPairingCodeReader.await(
            context = context,
            token = token,
            timeoutMs = PAIRING_CODE_TIMEOUT_MS,
        ) ?: return PhoneControlProtectedSetupResult.NeedsUserStep(
            "pairing_code_unavailable",
        )
        return try {
            if (!PhoneControlProtectedCheckpointRegistry.owns(token)) {
                return PhoneControlProtectedSetupResult.Failed("checkpoint_owner_lost")
            }
            val result = SgtAdbCommandBridge.pairProtected(context, pairingCode)
            if (!PhoneControlProtectedCheckpointRegistry.owns(token)) {
                PhoneControlProtectedSetupResult.Failed("checkpoint_owner_lost")
            } else if (sgtAdbPairingRelayCompleted(result)) {
                PhoneControlProtectedSetupResult.Completed
            } else {
                PhoneControlProtectedSetupResult.NeedsUserStep(
                    result.condition.name.lowercase(),
                )
            }
        } finally {
            pairingCode.fill('\u0000')
        }
    }

    private const val PAIRING_CODE_TIMEOUT_MS = 10_000L
}

internal fun sgtAdbPairingRelayCompleted(result: SgtAdbBridgeProbe): Boolean =
    (result.pairingEstablished || result.state == CapabilityState.READY) &&
        result.deviceIdentity != null
