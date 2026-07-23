package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import android.content.Intent
import androidx.annotation.StringRes
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import java.io.Closeable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.launch

internal class PhoneControlProtectedSetupCoordinator(
    private val context: Context,
    private val adapters: Map<String, PhoneControlProtectedSetupAdapter> = mapOf(
        SGT_ADB_PROVIDER_ID to SgtAdbProtectedSetupAdapter,
        SHIZUKU_PROVIDER_ID to ShizukuProtectedSetupAdapter,
    ),
) : Closeable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val continuation = PhoneControlProtectedSetupContinuation()
    private var job: Job? = null

    fun start(providerId: String, token: PhoneControlProtectedCheckpointToken) {
        cancel()
        continuation.begin()
        val adapter = adapters[providerId]
        if (adapter == null) {
            publishPending(providerId, token, "adapter_unavailable")
            return
        }
        job = scope.launch {
            val result = try {
                adapter.complete(context, token)
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (_: Throwable) {
                PhoneControlProtectedSetupResult.Failed("adapter_failed")
            }
            if (!PhoneControlProtectedCheckpointRegistry.owns(token)) return@launch
            when (result) {
                PhoneControlProtectedSetupResult.Completed -> {
                    continuation.relayCompleted()
                    PhoneControlLog.i(
                        TAG,
                        "protected_setup_result provider=$providerId result=relay_complete",
                    )
                    continueProviderSetup(providerId, R.string.phone_control_private_setup_continue)
                }
                is PhoneControlProtectedSetupResult.NeedsUserStep ->
                    publishPending(providerId, token, result.code)
                is PhoneControlProtectedSetupResult.Failed ->
                    publishPending(providerId, token, result.code)
            }
        }
    }

    fun cancel(resumeSelectedSetupAfterCapture: Boolean = false) {
        job?.cancel()
        job = null
        continuation.authorityChanged(resumeSelectedSetupAfterCapture)
    }

    fun onProjectionAttached(
        authoritySetup: PhoneControlAuthoritySetupController,
        resumeSelectedSetup: () -> Unit,
    ) {
        val shouldResume = continuation.consumeResumeSelectedSetup()
        PhoneControlLog.i(
            TAG,
            "protected_setup_projection_resume accepted=$shouldResume " +
                "reason=${if (shouldResume) "provider_progress" else "awaiting_external_progress"}",
        )
        if (!shouldResume) return
        authoritySetup.clear(reason = "projection_restored")
        resumeSelectedSetup()
    }

    override fun close() {
        scope.cancel()
    }

    private fun publishPending(
        providerId: String,
        token: PhoneControlProtectedCheckpointToken,
        code: String,
    ) {
        if (!PhoneControlProtectedCheckpointRegistry.owns(token)) return
        continuation.relayNeedsUserStep()
        PhoneControlLog.i(
            TAG,
            "protected_setup_result provider=$providerId result=user_step code=$code",
        )
        continueProviderSetup(providerId, R.string.phone_control_private_setup_pending)
    }

    private fun continueProviderSetup(providerId: String, @StringRes message: Int) {
        if (PhoneControlPowerPreferences.current(context)?.elevatedProviderId != providerId) return
        val intent = PhoneControlActivity.resumeCaptureIntent(context)
            .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
        PhoneControlSetupNotification.show(context, context.getString(message), intent)
        runCatching { context.startActivity(intent) }
            .onSuccess {
                PhoneControlLog.i(
                    TAG,
                    "protected_setup_continue provider=$providerId " +
                        "capture_resume_requested=true",
                )
            }
            .onFailure {
                PhoneControlLog.i(
                    TAG,
                    "protected_setup_continue provider=$providerId " +
                        "capture_resume_requested=false",
                )
            }
    }

    private companion object {
        const val TAG = "SGTPhoneControlSetup"
        const val SGT_ADB_PROVIDER_ID = "sgt_adb_bridge"
        const val SHIZUKU_PROVIDER_ID = "shizuku_shell"
    }
}
