package dev.screengoated.toolbox.mobile.phonecontrol

import android.content.Context
import androidx.annotation.StringRes
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCapturePolicy
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupResult
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlCoordinatorReentryLauncher
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
    private val replaceGuidance: (String, String) -> Boolean = { _, _ -> false },
    private val restoreRetainedProjection: (
        String,
        PhoneControlProtectedCheckpointToken,
        Boolean,
    ) -> Unit = { _, _, _ -> },
    private val adapters: Map<String, PhoneControlProtectedSetupAdapter> = mapOf(
        SGT_ADB_PROVIDER_ID to SgtAdbProtectedSetupAdapter,
        SHIZUKU_PROVIDER_ID to ShizukuProtectedSetupAdapter,
    ),
) : Closeable {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main.immediate)
    private val continuation = PhoneControlProtectedSetupContinuation()
    private var job: Job? = null

    fun capturePolicy(providerId: String): PhoneControlProtectedCapturePolicy? =
        adapters[providerId]?.capturePolicy

    fun start(providerId: String, token: PhoneControlProtectedCheckpointToken) {
        cancel()
        continuation.begin()
        replaceGuidance(
            providerId,
            context.phoneControlString(R.string.phone_control_private_setup_working),
        )
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
                    continueAfterProtectedStep(providerId, token, adapter.capturePolicy)
                }
                is PhoneControlProtectedSetupResult.NeedsUserStep -> publishPending(
                    providerId,
                    token,
                    result.code,
                    adapter.capturePolicy,
                )
                is PhoneControlProtectedSetupResult.Failed -> publishPending(
                    providerId,
                    token,
                    result.code,
                    adapter.capturePolicy,
                )
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
        authoritySetup.resumeSelectedAuthoritySetup(announceReady = true) {
            resumeSelectedSetup()
        }
    }

    override fun close() {
        scope.cancel()
    }

    private fun publishPending(
        providerId: String,
        token: PhoneControlProtectedCheckpointToken,
        code: String,
        capturePolicy: PhoneControlProtectedCapturePolicy =
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION,
    ) {
        if (!PhoneControlProtectedCheckpointRegistry.owns(token)) return
        continuation.relayNeedsUserStep()
        PhoneControlLog.i(
            TAG,
            "protected_setup_result provider=$providerId result=user_step code=$code",
        )
        if (capturePolicy == PhoneControlProtectedCapturePolicy.RETAIN_PROJECTION) {
            publishRetainedPending(providerId)
            restoreRetainedProjection(
                providerId,
                token,
                continuation.consumeResumeSelectedSetup(),
            )
        } else {
            continueProviderSetup(
                providerId,
                R.string.phone_control_private_setup_pending,
                R.string.phone_control_private_setup_pending_toast,
            )
        }
    }

    private fun continueAfterProtectedStep(
        providerId: String,
        token: PhoneControlProtectedCheckpointToken,
        capturePolicy: PhoneControlProtectedCapturePolicy,
    ) {
        if (capturePolicy == PhoneControlProtectedCapturePolicy.RETAIN_PROJECTION) {
            restoreRetainedProjection(
                providerId,
                token,
                continuation.consumeResumeSelectedSetup(),
            )
        } else {
            continueProviderSetup(
                providerId,
                R.string.phone_control_private_setup_continue,
                R.string.phone_control_private_setup_complete_toast,
            )
        }
    }

    private fun publishRetainedPending(providerId: String) {
        if (PhoneControlPowerPreferences.current(context)?.elevatedProviderId != providerId) return
        val intent = PhoneControlActivity.optionalPowerIntent(
            context,
            PhoneControlPowerPreferences.current(context) ?: return,
        )
        val guidance = context.phoneControlString(
            R.string.phone_control_private_setup_needs_user_step,
        )
        replaceGuidance(providerId, guidance)
        PhoneControlSetupNotification.show(context, guidance, intent)
        context.showPhoneControlToast(
            R.string.phone_control_private_setup_needs_user_step_toast,
        )
    }

    private fun continueProviderSetup(
        providerId: String,
        @StringRes message: Int,
        @StringRes toast: Int,
    ) {
        if (PhoneControlPowerPreferences.current(context)?.elevatedProviderId != providerId) return
        val intent = PhoneControlActivity.resumeCaptureIntent(context)
        val guidance = context.phoneControlString(message)
        replaceGuidance(providerId, guidance)
        PhoneControlSetupNotification.show(context, guidance, intent)
        context.showPhoneControlToast(toast)
        val dispatch = PhoneControlCoordinatorReentryLauncher.dispatch(context, intent)
        PhoneControlLog.i(
            TAG,
            "protected_setup_continue provider=$providerId " +
                "reentry_sequence=${dispatch.token} " +
                "capture_resume_dispatched=${dispatch.dispatched}",
        )
    }

    private companion object {
        const val TAG = "SGTPhoneControlSetup"
        const val SGT_ADB_PROVIDER_ID = "sgt_adb_bridge"
        const val SHIZUKU_PROVIDER_ID = "shizuku_shell"
    }
}
