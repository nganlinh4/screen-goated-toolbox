package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.content.Intent
import android.os.SystemClock
import android.provider.Settings
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlExternalGoalRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.showPhoneControlToast
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

private const val TAG = "SGTPhoneControlActivation"
private const val ACTIVATION_PROPAGATION_ATTEMPTS = 30
private const val ACTIVATION_PROPAGATION_POLL_MS = 100L

internal fun PhoneControlActivity.launchActivationSettings(
    step: PhoneControlActivationStep,
    intent: Intent,
) {
    awaitingStep = step
    PhoneControlLog.i(
        TAG,
        "activation_user_step_opened step=${step.wireName} surface=android_settings",
    )
    launchPlatformStep(userSteps.settings, intent) {
        settingsLauncher.launch(intent)
        when (step) {
            // Only the accessibility intent lands on a list that needs driving.
            PhoneControlActivationStep.ACCESSIBILITY ->
                startAccessibilitySettingsNavigation(intent)
            // The overlay intent deep-links straight to this app's page, so only watch for
            // the grant and return. Driving rows here would click the app row on whichever
            // Settings screen is still in front, stacking that page over the overlay one.
            PhoneControlActivationStep.OVERLAY -> startOverlayGrantWatch(intent)
            else -> Unit
        }
    }
}

internal fun PhoneControlActivity.startAccessibilitySettingsNavigation(intent: Intent) {
    val appLabel = applicationInfo.loadLabel(packageManager).toString()
    if (appLabel.isBlank()) {
        PhoneControlLog.w(TAG, "settings_navigation unavailable=true")
        return
    }
    startSettingsWatch(intent) { settingsPackage ->
        PhoneControlPlatformSettingsNavigator.openAppRow(
            settingsPackage = settingsPackage,
            appLabel = appLabel,
            permissionReady = { isAccessibilityReady(this@startAccessibilitySettingsNavigation) },
        )
    }
}

internal fun PhoneControlActivity.startOverlayGrantWatch(intent: Intent) {
    startSettingsWatch(intent) { settingsPackage ->
        PhoneControlPlatformSettingsNavigator.awaitGrantAndReturn(
            settingsPackage = settingsPackage,
            permissionReady = { Settings.canDrawOverlays(this@startOverlayGrantWatch) },
        )
    }
}

internal fun PhoneControlActivity.startSettingsWatch(
    intent: Intent,
    watch: suspend (settingsPackage: String) -> PlatformSettingsNavigationResult,
) {
    val settingsPackage = intent.resolveActivity(packageManager)?.packageName
    if (settingsPackage.isNullOrBlank()) {
        PhoneControlLog.w(TAG, "settings_navigation unavailable=true")
        return
    }
    settingsNavigationJob?.cancel()
    settingsNavigationJob = lifecycleScope.launch {
        val result = watch(settingsPackage)
        PhoneControlLog.i(TAG, "settings_navigation result=${result.wireName}")
    }
}

internal fun PhoneControlActivity.openApiKeySettings() {
    PhoneControlLog.i(
        TAG,
        "activation_user_step_opened step=gemini_api surface=app_settings",
    )
    showPhoneControlToast(R.string.phone_control_activation_api_key_toast)
    PhoneControlLog.w(TAG, "activation_stopped unresolved=gemini_api")
    startActivity(MainActivity.settingsIntent(this))
    finishAndReplayProjectionReentry()
}

internal fun PhoneControlActivity.prepareAccessibilityStep(step: PhoneControlActivationStep) {
    when (phoneControlAccessibilityResolution(
        probePhoneControlAccessibilityState(this),
        reconnectWaitExhausted = false,
    )) {
        PhoneControlAccessibilityResolution.CONTINUE -> advanceActivation()
        PhoneControlAccessibilityResolution.OPEN_SETTINGS ->
            launchActivationSettings(step, accessibilitySettingsIntent(this))
        PhoneControlAccessibilityResolution.STOP -> stopAccessibilityReconnect()
        PhoneControlAccessibilityResolution.WAIT -> waitForAccessibilityReconnect(step)
    }
}

internal fun PhoneControlActivity.waitForAccessibilityReconnect(step: PhoneControlActivationStep) {
    awaitingStep = step
    PhoneControlLog.i(TAG, "activation_accessibility_reconnect_wait started=true")
    activationResumeJob?.cancel()
    activationResumeJob = lifecycleScope.launch {
        awaitActivationProgress(step)
        awaitingStep = null
        when (phoneControlAccessibilityResolution(
            probePhoneControlAccessibilityState(this@waitForAccessibilityReconnect),
            reconnectWaitExhausted = true,
        )) {
            PhoneControlAccessibilityResolution.CONTINUE -> {
                PhoneControlLog.i(
                    TAG,
                    "activation_accessibility_reconnect_wait outcome=recovered",
                )
                advanceActivation()
            }
            PhoneControlAccessibilityResolution.OPEN_SETTINGS -> {
                PhoneControlLog.i(
                    TAG,
                    "activation_accessibility_reconnect_wait outcome=disabled",
                )
                launchActivationSettings(
                    step,
                    accessibilitySettingsIntent(this@waitForAccessibilityReconnect),
                )
            }
            PhoneControlAccessibilityResolution.STOP -> stopAccessibilityReconnect()
            PhoneControlAccessibilityResolution.WAIT -> error(
                "exhausted accessibility reconnect must resolve to a terminal action",
            )
        }
    }
}

internal fun PhoneControlActivity.stopAccessibilityReconnect() {
    PhoneControlLog.w(
        TAG,
        "activation_accessibility_reconnect_wait outcome=still_reconnecting",
    )
    showPhoneControlToast(R.string.phone_control_accessibility_reconnecting_toast)
    finishAndReplayProjectionReentry()
}

internal fun PhoneControlActivity.completeActivationStep() {
    val completed = awaitingStep ?: return
    awaitingStep = null
    // The step's Settings navigator has served its purpose once we are resumed; leaving it
    // running lets its clicks and back presses land on the next step's Settings screen.
    settingsNavigationJob?.cancel()
    activationResumeJob?.cancel()
    activationResumeJob = lifecycleScope.launch {
        val startedAtMs = SystemClock.elapsedRealtime()
        val next = awaitActivationProgress(completed)
        val propagationMs = SystemClock.elapsedRealtime() - startedAtMs
        if (next == completed) {
            abortActivation(completed)
        } else {
            PhoneControlLog.i(
                TAG,
                "activation_step_complete step=${completed.wireName} " +
                    "next=${next.wireName} propagation_ms=$propagationMs",
            )
            advanceActivation()
        }
    }
}

internal suspend fun PhoneControlActivity.awaitActivationProgress(
    completed: PhoneControlActivationStep,
): PhoneControlActivationStep {
    repeat(ACTIVATION_PROPAGATION_ATTEMPTS) { attempt ->
        val next = nextPhoneControlActivationStep(
            probePhoneControlActivation(
                this,
                mediaProjectionReady = projectionGrant != null,
            ),
        )
        if (next != completed) return next
        if (attempt + 1 < ACTIVATION_PROPAGATION_ATTEMPTS) {
            delay(ACTIVATION_PROPAGATION_POLL_MS)
        }
    }
    return completed
}

internal fun PhoneControlActivity.abortActivation(step: PhoneControlActivationStep) {
    PhoneControlLog.w(TAG, "activation_stopped unresolved=${step.wireName}")
    // An aborted activation must not leave its goal for a later manual start.
    PhoneControlExternalGoalRegistry.clear()
    showPhoneControlToast(
        when (step) {
            PhoneControlActivationStep.GEMINI_API ->
                R.string.phone_control_activation_api_key_toast
            PhoneControlActivationStep.RUNTIME_PERMISSIONS ->
                R.string.phone_control_activation_microphone_toast
            PhoneControlActivationStep.ACCESSIBILITY ->
                R.string.phone_control_activation_accessibility_toast
            PhoneControlActivationStep.OVERLAY ->
                R.string.phone_control_activation_overlay_toast
            PhoneControlActivationStep.MEDIA_PROJECTION ->
                R.string.phone_control_activation_projection_toast
            PhoneControlActivationStep.START ->
                R.string.phone_control_activation_start_failed_toast
        },
    )
    finishAndReplayProjectionReentry()
}

