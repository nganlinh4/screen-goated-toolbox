package dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged

import android.accessibilityservice.AccessibilityService
import android.content.Context
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.text.SpannableStringBuilder
import android.view.accessibility.AccessibilityNodeInfo
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCapturePolicy
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupAdapter
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedSetupResult
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import rikka.shizuku.Shizuku

internal object ShizukuProtectedSetupAdapter : PhoneControlProtectedSetupAdapter {
    override val capturePolicy = PhoneControlProtectedCapturePolicy.RELEASE_PROJECTION

    override suspend fun complete(
        context: Context,
        token: PhoneControlProtectedCheckpointToken,
    ): PhoneControlProtectedSetupResult = withContext(Dispatchers.Main.immediate) {
        val service = SgtAccessibilityService.instance
            ?: return@withContext needsUserStep("accessibility_unavailable")
        val oneTimeCode = ProtectedPairingCodeReader.await(context, token)
            ?: return@withContext needsUserStep("pairing_code_unavailable")
        try {
            if (!owns(token)) return@withContext failed("checkpoint_owner_lost")
            if (!service.performGlobalAction(AccessibilityService.GLOBAL_ACTION_NOTIFICATIONS)) {
                return@withContext needsUserStep("notification_shade_unavailable")
            }
            val providerLabel = providerLabel(context)
                ?: return@withContext needsUserStep("provider_label_unavailable")
            val action = awaitProviderAction(service, providerLabel, token)
                ?: return@withContext needsUserStep("pairing_action_unavailable")
            if (!action.performAction(AccessibilityNodeInfo.ACTION_CLICK)) {
                return@withContext needsUserStep("pairing_action_rejected")
            }
            val input = awaitRemoteInput(service, token)
                ?: return@withContext needsUserStep("pairing_input_unavailable")
            if (!setSecret(input, oneTimeCode)) {
                return@withContext needsUserStep("pairing_input_rejected")
            }
            if (!submitRemoteInput(service, input, token)) {
                return@withContext needsUserStep("pairing_submit_unavailable")
            }
            if (!awaitBinder(token)) {
                return@withContext needsUserStep("provider_start_pending")
            }
            PhoneControlProtectedSetupResult.Completed
        } finally {
            oneTimeCode.fill('\u0000')
        }
    }

    private suspend fun awaitProviderAction(
        service: SgtAccessibilityService,
        providerLabel: CharSequence,
        token: PhoneControlProtectedCheckpointToken,
    ): AccessibilityNodeInfo? = poll(NOTIFICATION_TIMEOUT_MS, token) {
        val labelNodes = accessibilityRoots(service)
            .flatMap(::accessibilityNodes)
            .filter { node ->
                node.text?.toString()?.trim()?.equals(
                    providerLabel.toString().trim(),
                    ignoreCase = true,
                ) == true
            }
        uniqueProviderAction(labelNodes)
    }

    private suspend fun awaitRemoteInput(
        service: SgtAccessibilityService,
        token: PhoneControlProtectedCheckpointToken,
    ): AccessibilityNodeInfo? = poll(NOTIFICATION_TIMEOUT_MS, token) {
        val nodes = accessibilityRoots(service).flatMap(::accessibilityNodes)
        nodes.singleOrNull { node ->
            node.isEditable &&
                node.viewIdResourceName?.endsWith(REMOTE_INPUT_TEXT_ID) == true
        } ?: nodes.filter(AccessibilityNodeInfo::isEditable).singleOrNull()
    }

    private fun setSecret(node: AccessibilityNodeInfo, code: CharArray): Boolean {
        val secret = SpannableStringBuilder(code.concatToString())
        return try {
            node.performAction(
                AccessibilityNodeInfo.ACTION_SET_TEXT,
                Bundle().apply {
                    putCharSequence(
                        AccessibilityNodeInfo.ACTION_ARGUMENT_SET_TEXT_CHARSEQUENCE,
                        secret,
                    )
                },
            )
        } finally {
            secret.clear()
        }
    }

    private fun submitRemoteInput(
        service: SgtAccessibilityService,
        input: AccessibilityNodeInfo,
        token: PhoneControlProtectedCheckpointToken,
    ): Boolean {
        if (!owns(token)) return false
        val sendButtons = accessibilityRoots(service)
            .flatMap(::accessibilityNodes)
            .filter { node ->
                node.isClickable &&
                    node.viewIdResourceName?.endsWith(REMOTE_INPUT_SEND_ID) == true
        }
        val send = sendButtons.singleOrNull()
        if (send != null) return send.performAction(AccessibilityNodeInfo.ACTION_CLICK)
        if (Build.VERSION.SDK_INT < Build.VERSION_CODES.R) return false
        return input.performAction(
            AccessibilityNodeInfo.AccessibilityAction.ACTION_IME_ENTER.id,
        )
    }

    private suspend fun awaitBinder(
        token: PhoneControlProtectedCheckpointToken,
    ): Boolean = poll(BINDER_TIMEOUT_MS, token) {
        runCatching(Shizuku::pingBinder).getOrDefault(false).takeIf { it }
    } == true

    private suspend fun <T> poll(
        timeoutMs: Long,
        token: PhoneControlProtectedCheckpointToken,
        block: () -> T?,
    ): T? {
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        do {
            if (!owns(token)) return null
            block()?.let { return it }
            delay(POLL_INTERVAL_MS)
        } while (SystemClock.elapsedRealtime() < deadline)
        return null
    }

    private fun uniqueProviderAction(
        labelNodes: List<AccessibilityNodeInfo>,
    ): AccessibilityNodeInfo? {
        val candidates = linkedSetOf<AccessibilityNodeInfo>()
        labelNodes.forEach { label ->
            var ancestor = label.parent
            repeat(MAX_NOTIFICATION_ANCESTORS) {
                val current = ancestor ?: return@repeat
                val buttons = accessibilityNodes(current).filter { node ->
                    node.isClickable &&
                        node.className?.toString()?.endsWith("Button") == true &&
                        node !== label
                }
                if (buttons.size == 1) candidates += buttons.single()
                ancestor = current.parent
            }
        }
        return candidates.singleOrNull()
    }

    @Suppress("DEPRECATION")
    private fun providerLabel(context: Context): CharSequence? = runCatching {
        val info = context.packageManager.getApplicationInfo(SHIZUKU_PACKAGE, 0)
        context.packageManager.getApplicationLabel(info)
    }.getOrNull()

    private fun owns(token: PhoneControlProtectedCheckpointToken): Boolean =
        PhoneControlProtectedCheckpointRegistry.owns(token)

    private fun needsUserStep(code: String) =
        PhoneControlProtectedSetupResult.NeedsUserStep(code)

    private fun failed(code: String) = PhoneControlProtectedSetupResult.Failed(code)

    private const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"
    private const val REMOTE_INPUT_TEXT_ID = "/id/remote_input_text"
    private const val REMOTE_INPUT_SEND_ID = "/id/remote_input_send"
    private const val MAX_NOTIFICATION_ANCESTORS = 8
    private const val POLL_INTERVAL_MS = 200L
    private const val NOTIFICATION_TIMEOUT_MS = 5_000L
    private const val BINDER_TIMEOUT_MS = 15_000L
}
