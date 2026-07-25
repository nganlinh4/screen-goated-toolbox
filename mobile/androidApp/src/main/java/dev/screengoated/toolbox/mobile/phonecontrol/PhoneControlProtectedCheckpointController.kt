package dev.screengoated.toolbox.mobile.phonecontrol

import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCapturePolicy
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.provider.accessibility.PhoneControlAccessibilityProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.detector.UiDetectorGroundingFrameStore
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime

internal class PhoneControlProtectedCheckpointController {
    private var token: PhoneControlProtectedCheckpointToken? = null
    private var providerId: String? = null
    private var capturePolicy: PhoneControlProtectedCapturePolicy? = null

    val active: Boolean
        get() = token != null

    val activeToken: PhoneControlProtectedCheckpointToken?
        get() = token

    val freshProjectionRequired: Boolean
        get() = active &&
            capturePolicy == PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION

    fun begin(
        providerId: String,
        policy: PhoneControlProtectedCapturePolicy,
        runtime: PhoneControlRuntime,
        releaseProjection: () -> Unit,
    ): PhoneControlProtectedCheckpointToken? {
        if (active) return null
        val candidate = runCatching {
            PhoneControlProtectedCheckpointRegistry.begin(policy)
        }
            .getOrNull()
            ?: return null
        token = candidate
        this.providerId = providerId
        capturePolicy = policy
        runtime.suspendVisualEvidence()
        PhoneControlAccessibilityProvider.invalidate("protected_checkpoint")
        UiDetectorGroundingFrameStore.clear()
        if (policy == PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION) {
            releaseProjection()
        }
        PhoneControlLog.i(
            TAG,
            "protected_checkpoint_enter accepted=true provider=$providerId " +
                "capture_policy=${policy.wireName} runtime_alive=true visual_evidence=false",
        )
        return candidate
    }

    fun restoreRetained(
        expectedToken: PhoneControlProtectedCheckpointToken,
        runtime: PhoneControlRuntime,
    ): Boolean {
        if (capturePolicy != PhoneControlProtectedCapturePolicy.RETAIN_PROJECTION) return false
        return restore(expectedToken, runtime, "retained_projection")
    }

    fun attachFresh(
        expectedToken: PhoneControlProtectedCheckpointToken,
        runtime: PhoneControlRuntime,
    ): Boolean {
        if (capturePolicy != PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION) return false
        return restore(expectedToken, runtime, "fresh_projection")
    }

    fun cancelRetained(runtime: PhoneControlRuntime): Boolean {
        val expectedToken = token ?: return false
        return restoreRetained(expectedToken, runtime)
    }

    fun close() {
        token?.let(PhoneControlProtectedCheckpointRegistry::end)
        clear()
    }

    private fun restore(
        expectedToken: PhoneControlProtectedCheckpointToken,
        runtime: PhoneControlRuntime,
        source: String,
    ): Boolean {
        if (token != expectedToken ||
            !PhoneControlProtectedCheckpointRegistry.end(expectedToken)
        ) {
            return false
        }
        val restoredProvider = providerId.orEmpty()
        clear()
        runtime.resumeVisualEvidence()
        PhoneControlLog.i(
            TAG,
            "protected_checkpoint_exit accepted=true provider=$restoredProvider " +
                "source=$source visual_evidence=true",
        )
        return true
    }

    private fun clear() {
        token = null
        providerId = null
        capturePolicy = null
    }

    private val PhoneControlProtectedCapturePolicy.wireName: String
        get() = when (this) {
            PhoneControlProtectedCapturePolicy.RETAIN_PROJECTION -> "retain_projection"
            PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION -> "release_projection"
        }

    private companion object {
        const val TAG = "SGTPhoneControlService"
    }
}
