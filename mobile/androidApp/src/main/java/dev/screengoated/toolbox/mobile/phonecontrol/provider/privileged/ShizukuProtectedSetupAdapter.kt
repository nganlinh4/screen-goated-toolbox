package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.content.Context
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCapturePolicy
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointReadiness
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupResult
import kotlinx.coroutines.delay
import rikka.shizuku.Shizuku

internal object ShizukuProtectedSetupAdapter : PhoneControlProtectedSetupAdapter {
    override val capturePolicy = PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION
    override val navigationContract = WirelessDebuggingSetupContract
    override val userStepGuidance = R.string.phone_control_private_setup_shizuku_manual

    override fun checkpointReadiness(context: Context): PhoneControlProtectedCheckpointReadiness =
        ProtectedPairingCodeReader.surfaceReadiness(context)

    override suspend fun complete(
        context: Context,
        token: PhoneControlProtectedCheckpointToken,
    ): PhoneControlProtectedSetupResult = awaitManualProtectedSetup(
        owns = { PhoneControlProtectedCheckpointRegistry.owns(token) },
        providerReady = { runCatching(Shizuku::pingBinder).getOrDefault(false) },
        wait = { delay(POLL_INTERVAL_MS) },
    )

    private const val POLL_INTERVAL_MS = 500L
}

/** No accessibility notification identity proves which app receives a remote reply. */
internal suspend fun awaitManualProtectedSetup(
    owns: () -> Boolean,
    providerReady: () -> Boolean,
    wait: suspend () -> Unit,
): PhoneControlProtectedSetupResult {
    while (owns()) {
        if (providerReady()) {
            return if (owns()) {
                PhoneControlProtectedSetupResult.Completed
            } else {
                PhoneControlProtectedSetupResult.Failed("checkpoint_owner_lost")
            }
        }
        wait()
    }
    return PhoneControlProtectedSetupResult.Failed("checkpoint_owner_lost")
}
