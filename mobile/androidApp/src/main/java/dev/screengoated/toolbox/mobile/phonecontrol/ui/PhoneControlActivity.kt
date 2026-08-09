package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.provider.Settings
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlService
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlSetupNotification
import dev.screengoated.toolbox.mobile.phonecontrol.phoneControlString
import dev.screengoated.toolbox.mobile.phonecontrol.showPhoneControlToast
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSlot
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionGrant
import dev.screengoated.toolbox.mobile.phonecontrol.projection.createPhoneControlProjectionConsentIntent
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.RootCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.SgtAdbCommandBridge
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay

/**
 * Transparent coordinator for Android-owned Phone Control setup steps.
 * Product UI stays on the Apps card and orb; this activity never renders a setup page.
 */
class PhoneControlActivity : ComponentActivity() {
    private val userSteps by viewModels<PhoneControlUserStepState>()
    private var mode = Mode.ACTIVATE
    private var awaitingStep: PhoneControlActivationStep? = null
    private var requestedNotification = false
    private var projectionGrant: PhoneControlProjectionGrant? = null
    private var sgtAdbSetup: PhoneControlSgtAdbSetupCoordinator? = null
    private var shizukuSetup: PhoneControlShizukuSetupCoordinator? = null
    private var activationResumeJob: Job? = null
    private var settingsNavigationJob: Job? = null
    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        PhoneControlLog.i(
            TAG,
            "activation_user_step_returned step=runtime_permissions surface=runtime_dialog",
        )
        userSteps.permission.finish()
        if (requestedNotification) {
            markPhoneControlNotificationPrompted(this)
            requestedNotification = false
        }
        completeActivationStep()
    }
    private val settingsLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        val returnedStep = awaitingStep
        settingsNavigationJob?.cancel()
        settingsNavigationJob = null
        when (phoneControlExternalResultDisposition(
            PhoneControlCoordinatorReentryLauncher.hasPendingReceipt(),
            userSteps.settings.active,
        )) {
            PhoneControlExternalResultDisposition.RETIRE_FOR_REENTRY -> {
                userSteps.settings.finish()
                PhoneControlLog.i(TAG, "external_step_result_ignored owner=reentry_pending")
                return@registerForActivityResult
            }
            PhoneControlExternalResultDisposition.IGNORE_RETIRED -> {
                PhoneControlLog.i(TAG, "external_step_result_ignored owner=retired")
                return@registerForActivityResult
            }
            PhoneControlExternalResultDisposition.HANDLE -> Unit
        }
        PhoneControlLog.i(
            TAG,
            "activation_user_step_returned step=${returnedStep?.wireName ?: "optional"} " +
                "surface=android_settings",
        )
        when (mode) {
            Mode.ACTIVATE -> {
                userSteps.settings.finish()
                completeActivationStep()
            }
            Mode.SGT_ADB -> sgtAdbSetup?.onExternalReturn()
            Mode.SHIZUKU -> shizukuSetup?.onExternalReturn()
            Mode.ROOT -> {
                userSteps.settings.finish()
                finish()
            }
            Mode.SGT_ADB_FORGET,
            Mode.RESUME_CAPTURE,
            Mode.RETURN_TO_APP -> {
                userSteps.settings.finish()
                finish()
            }
            Mode.CANCEL_SETUP -> cancelAuthoritySetup()
        }
    }
    private val projectionLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) { result ->
        val step = awaitingStep
        userSteps.projection.finish()
        val grant = PhoneControlProjectionGrant.fromActivityResult(
            result.resultCode,
            result.data,
        )
        PhoneControlLog.i(
            TAG,
            "activation_user_step_returned step=media_projection " +
                "surface=system_capture_dialog accepted=${grant != null}",
        )
        if (mode == Mode.RESUME_CAPTURE) {
            awaitingStep = null
            val accepted = grant != null && PhoneControlService.attachProjection(this, grant)
            PhoneControlLog.i(
                TAG,
                "capture_resume_result accepted=$accepted runtime_reused=true",
            )
            if (!accepted) {
                val message = phoneControlString(
                    R.string.phone_control_activation_projection_needed,
                )
                showPhoneControlToast(R.string.phone_control_activation_projection_toast)
                PhoneControlSetupNotification.show(
                    this,
                    message,
                    resumeCaptureIntent(this),
                )
            }
            finish()
            return@registerForActivityResult
        }
        if (step != PhoneControlActivationStep.MEDIA_PROJECTION || grant == null) {
            awaitingStep = null
            projectionGrant = null
            abortActivation(PhoneControlActivationStep.MEDIA_PROJECTION)
        } else {
            projectionGrant = grant
            completeActivationStep()
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        mode = intent.mode()
        PhoneControlCoordinatorReentryLauncher.acknowledge(intent, mode.wireName)
        PhoneControlLog.i(TAG, intent.phoneControlCoordinatorEvent("coordinator_open", mode.wireName))
        when (mode) {
            Mode.ACTIVATE -> advanceActivation()
            Mode.SGT_ADB -> startSgtAdbSetup()
            Mode.SHIZUKU -> startShizukuSetup(savedInstanceState)
            Mode.ROOT -> requestRootAuthorization()
            Mode.SGT_ADB_FORGET -> forgetSgtAdbPairing()
            Mode.RESUME_CAPTURE -> requestCaptureResume()
            Mode.RETURN_TO_APP -> finish()
            Mode.CANCEL_SETUP -> cancelAuthoritySetup()
        }
    }
    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        activationResumeJob?.cancel()
        settingsNavigationJob?.cancel()
        sgtAdbSetup?.close()
        sgtAdbSetup = null
        shizukuSetup?.close()
        shizukuSetup = null
        userSteps.settings.finish()
        setIntent(intent)
        mode = intent.mode()
        PhoneControlCoordinatorReentryLauncher.acknowledge(intent, mode.wireName)
        PhoneControlLog.i(TAG, intent.phoneControlCoordinatorEvent("coordinator_reentry", mode.wireName))
        awaitingStep = null
        projectionGrant = null
        when (mode) {
            Mode.ACTIVATE -> advanceActivation()
            Mode.SGT_ADB -> startSgtAdbSetup()
            Mode.SHIZUKU -> startShizukuSetup(savedState = null)
            Mode.ROOT -> requestRootAuthorization()
            Mode.SGT_ADB_FORGET -> forgetSgtAdbPairing()
            Mode.RESUME_CAPTURE -> requestCaptureResume()
            Mode.RETURN_TO_APP -> finish()
            Mode.CANCEL_SETUP -> cancelAuthoritySetup()
        }
    }

    override fun onDestroy() {
        activationResumeJob?.cancel()
        settingsNavigationJob?.cancel()
        sgtAdbSetup?.close()
        shizukuSetup?.close()
        super.onDestroy()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        shizukuSetup?.save(outState)
    }

    private fun advanceActivation() {
        if (isFinishing || awaitingStep != null) return
        val snapshot = probePhoneControlActivation(
            this,
            mediaProjectionReady = projectionGrant != null,
        )
        val step = nextPhoneControlActivationStep(snapshot)
        PhoneControlLog.i(TAG, "activation_step_selected step=${step.wireName}")
        when (step) {
            PhoneControlActivationStep.GEMINI_API -> openApiKeySettings()
            PhoneControlActivationStep.RUNTIME_PERMISSIONS -> {
                val permissions = buildList {
                    if (!snapshot.microphoneReady) add(Manifest.permission.RECORD_AUDIO)
                    if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU &&
                        !snapshot.notificationsReady && !snapshot.notificationPrompted
                    ) {
                        requestedNotification = true
                        add(Manifest.permission.POST_NOTIFICATIONS)
                    }
                }
                if (permissions.isEmpty()) {
                    abortActivation(step)
                } else {
                    awaitingStep = step
                    PhoneControlLog.i(
                        TAG,
                        "activation_user_step_opened step=${step.wireName} surface=runtime_dialog",
                    )
                    launchPlatformStep(userSteps.permission) {
                        permissionLauncher.launch(permissions.toTypedArray())
                    }
                }
            }
            PhoneControlActivationStep.ACCESSIBILITY -> prepareAccessibilityStep(step)
            PhoneControlActivationStep.OVERLAY -> launchActivationSettings(
                step,
                overlaySettingsIntent(this),
            )
            PhoneControlActivationStep.MEDIA_PROJECTION -> {
                val consentIntent = createPhoneControlProjectionConsentIntent(this)
                if (consentIntent == null) {
                    abortActivation(step)
                    return
                }
                awaitingStep = step
                PhoneControlLog.i(
                    TAG,
                    "activation_user_step_opened step=${step.wireName} " +
                        "surface=system_capture_dialog",
                )
                launchPlatformStep(userSteps.projection, consentIntent) {
                    projectionLauncher.launch(consentIntent)
                }
            }
            PhoneControlActivationStep.START -> {
                val grant = projectionGrant
                if (grant == null) {
                    abortActivation(PhoneControlActivationStep.MEDIA_PROJECTION)
                    return
                }
                val accepted = PhoneControlService.start(this, grant)
                projectionGrant = null
                PhoneControlLog.i(TAG, "activation_service_start accepted=$accepted")
                if (!accepted) {
                    showPhoneControlToast(R.string.phone_control_activation_start_failed_toast)
                }
                finish()
            }
        }
    }

    private fun launchActivationSettings(
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

    private fun startAccessibilitySettingsNavigation(intent: Intent) {
        val appLabel = applicationInfo.loadLabel(packageManager).toString()
        if (appLabel.isBlank()) {
            PhoneControlLog.w(TAG, "settings_navigation unavailable=true")
            return
        }
        startSettingsWatch(intent) { settingsPackage ->
            PhoneControlPlatformSettingsNavigator.openAppRow(
                settingsPackage = settingsPackage,
                appLabel = appLabel,
                permissionReady = { isAccessibilityReady(this@PhoneControlActivity) },
            )
        }
    }

    private fun startOverlayGrantWatch(intent: Intent) {
        startSettingsWatch(intent) { settingsPackage ->
            PhoneControlPlatformSettingsNavigator.awaitGrantAndReturn(
                settingsPackage = settingsPackage,
                permissionReady = { Settings.canDrawOverlays(this@PhoneControlActivity) },
            )
        }
    }

    private fun startSettingsWatch(
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

    private fun openApiKeySettings() {
        PhoneControlLog.i(
            TAG,
            "activation_user_step_opened step=gemini_api surface=app_settings",
        )
        showPhoneControlToast(R.string.phone_control_activation_api_key_toast)
        PhoneControlLog.w(TAG, "activation_stopped unresolved=gemini_api")
        startActivity(MainActivity.settingsIntent(this))
        finish()
    }

    private fun prepareAccessibilityStep(step: PhoneControlActivationStep) {
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

    private fun waitForAccessibilityReconnect(step: PhoneControlActivationStep) {
        awaitingStep = step
        PhoneControlLog.i(TAG, "activation_accessibility_reconnect_wait started=true")
        activationResumeJob?.cancel()
        activationResumeJob = lifecycleScope.launch {
            awaitActivationProgress(step)
            awaitingStep = null
            when (phoneControlAccessibilityResolution(
                probePhoneControlAccessibilityState(this@PhoneControlActivity),
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
                        accessibilitySettingsIntent(this@PhoneControlActivity),
                    )
                }
                PhoneControlAccessibilityResolution.STOP -> stopAccessibilityReconnect()
                PhoneControlAccessibilityResolution.WAIT -> error(
                    "exhausted accessibility reconnect must resolve to a terminal action",
                )
            }
        }
    }

    private fun stopAccessibilityReconnect() {
        PhoneControlLog.w(
            TAG,
            "activation_accessibility_reconnect_wait outcome=still_reconnecting",
        )
        showPhoneControlToast(R.string.phone_control_accessibility_reconnecting_toast)
        finish()
    }

    private fun completeActivationStep() {
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

    private suspend fun awaitActivationProgress(
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

    private fun abortActivation(step: PhoneControlActivationStep) {
        PhoneControlLog.w(TAG, "activation_stopped unresolved=${step.wireName}")
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
        finish()
    }

    private fun startShizukuSetup(savedState: Bundle?) {
        if (resumeCaptureBeforeAuthoritySetup()) return
        shizukuSetup = PhoneControlShizukuSetupCoordinator(
            activity = this,
            externalStep = userSteps.settings,
            permissionStep = userSteps.shizuku,
            launchExternal = settingsLauncher::launch,
            finishActivity = ::finish,
        ).also { coordinator -> coordinator.start(savedState) }
    }

    private fun startSgtAdbSetup() {
        if (resumeCaptureBeforeAuthoritySetup()) return
        sgtAdbSetup = PhoneControlSgtAdbSetupCoordinator(
            activity = this,
            externalStep = userSteps.settings,
            launchExternal = settingsLauncher::launch,
            finishActivity = ::finish,
        ).also(PhoneControlSgtAdbSetupCoordinator::start)
    }

    private fun requestRootAuthorization() {
        if (resumeCaptureBeforeAuthoritySetup()) return
        lifecycleScope.launch {
            if (!userSteps.root.begin()) return@launch
            val state = try {
                RootCommandBridge.requestAuthorization().state
            } finally {
                userSteps.root.finish()
            }
            PhoneControlLog.i(TAG, "optional_setup_result provider=root state=${state.wireName}")
            finish()
        }
    }

    private fun resumeCaptureBeforeAuthoritySetup(): Boolean {
        if (!PhoneControlService.captureSuspended) return false
        PhoneControlLog.i(TAG, "authority_setup_deferred reason=protected_checkpoint")
        if (PhoneControlProtectedCheckpointRegistry.freshProjectionRequired()) {
            startActivity(resumeCaptureIntent(this))
        }
        finish()
        return true
    }

    private fun forgetSgtAdbPairing() {
        lifecycleScope.launch {
            val forgotten = SgtAdbCommandBridge.forget(this@PhoneControlActivity)
            if (forgotten) {
                PhoneControlPowerPreferences.save(
                    this@PhoneControlActivity,
                    PhoneControlPowerChoice.STANDARD,
                )
                PhoneControlService.clearAuthoritySetup(
                    this@PhoneControlActivity,
                    PhoneControlPowerChoice.SGT_ADB.elevatedProviderId,
                )
            }
            showPhoneControlToast(
                if (forgotten) {
                    R.string.phone_control_sgt_adb_forgotten_toast
                } else {
                    R.string.phone_control_sgt_adb_forget_failed_toast
                },
            )
            PhoneControlLog.i(TAG, "sgt_adb_forget completed=$forgotten")
            finish()
        }
    }

    private fun requestCaptureResume() {
        if (!PhoneControlService.captureSuspended) {
            PhoneControlLog.w(TAG, "capture_resume_skipped reason=no_suspended_runtime")
            finish()
            return
        }
        val consentIntent = createPhoneControlProjectionConsentIntent(this)
        if (consentIntent == null ||
            !userSteps.projection.begin(consentIntent.resolveActivity(packageManager)?.packageName)
        ) {
            PhoneControlLog.w(TAG, "capture_resume_skipped reason=consent_unavailable")
            finish()
            return
        }
        awaitingStep = PhoneControlActivationStep.MEDIA_PROJECTION
        PhoneControlLog.i(
            TAG,
            "capture_resume_launcher_dispatched surface=system_capture_dialog",
        )
        projectionLauncher.launch(consentIntent)
    }

    private fun cancelAuthoritySetup() {
        PhoneControlPowerPreferences.save(this, PhoneControlPowerChoice.STANDARD)
        PhoneControlService.clearAuthoritySetup(this)
        PhoneControlLog.i(TAG, "authority_setup_result pending=false reason=user_cancelled")
        finish()
    }

    private inline fun launchPlatformStep(
        slot: PlatformUserStepSlot,
        intent: Intent? = null,
        launch: () -> Unit,
    ) {
        if (!slot.begin(intent?.resolveActivity(packageManager)?.packageName)) return
        try {
            launch()
        } catch (error: RuntimeException) {
            slot.finish()
            throw error
        }
    }

    private fun Intent.mode(): Mode = when (getStringExtra(EXTRA_MODE)) {
        Mode.SGT_ADB.wireName -> Mode.SGT_ADB
        Mode.SHIZUKU.wireName -> Mode.SHIZUKU
        Mode.ROOT.wireName -> Mode.ROOT
        Mode.SGT_ADB_FORGET.wireName -> Mode.SGT_ADB_FORGET
        Mode.RESUME_CAPTURE.wireName -> Mode.RESUME_CAPTURE
        Mode.CANCEL_SETUP.wireName -> Mode.CANCEL_SETUP
        else -> Mode.ACTIVATE
    }

    private enum class Mode(val wireName: String) {
        ACTIVATE("activate"),
        SGT_ADB("sgt_adb"),
        SHIZUKU("shizuku"),
        ROOT("root"),
        SGT_ADB_FORGET("sgt_adb_forget"),
        RESUME_CAPTURE("resume_capture"),
        RETURN_TO_APP("return_to_app"),
        CANCEL_SETUP("cancel_setup"),
    }

    companion object {
        private const val TAG = "SGTPhoneControlActivation"
        private const val EXTRA_MODE = "dev.screengoated.toolbox.mobile.phonecontrol.MODE"
        private const val ACTIVATION_PROPAGATION_ATTEMPTS = 30
        private const val ACTIVATION_PROPAGATION_POLL_MS = 100L
        internal fun activationIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(EXTRA_MODE, Mode.ACTIVATE.wireName)
        internal fun resumeCaptureIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(
            EXTRA_MODE,
            Mode.RESUME_CAPTURE.wireName,
        ).addFlags(COORDINATOR_REENTRY_FLAGS)
        /**
         * Brings the coordinator forward purely to pull the user out of a system Settings
         * screen. Nothing is suspended on this route, so it must not ask for a capture resume.
         */
        internal fun returnToAppIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(
            EXTRA_MODE,
            Mode.RETURN_TO_APP.wireName,
        ).addFlags(COORDINATOR_REENTRY_FLAGS)
        internal fun sgtAdbForgetIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(
            EXTRA_MODE,
            Mode.SGT_ADB_FORGET.wireName,
        ).addFlags(COORDINATOR_REENTRY_FLAGS)
        internal fun cancelSetupIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(
            EXTRA_MODE,
            Mode.CANCEL_SETUP.wireName,
        ).addFlags(COORDINATOR_REENTRY_FLAGS)
        internal fun optionalPowerIntent(
            context: Context,
            choice: PhoneControlPowerChoice,
        ): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(
            EXTRA_MODE,
            when (choice) {
                PhoneControlPowerChoice.STANDARD -> Mode.ACTIVATE.wireName
                PhoneControlPowerChoice.SGT_ADB -> Mode.SGT_ADB.wireName
                PhoneControlPowerChoice.SHIZUKU -> Mode.SHIZUKU.wireName
                PhoneControlPowerChoice.ROOT -> Mode.ROOT.wireName
            },
        ).addFlags(COORDINATOR_REENTRY_FLAGS)
        internal const val COORDINATOR_REENTRY_FLAGS = Intent.FLAG_ACTIVITY_NEW_TASK or
            Intent.FLAG_ACTIVITY_CLEAR_TOP or Intent.FLAG_ACTIVITY_SINGLE_TOP
    }
}
