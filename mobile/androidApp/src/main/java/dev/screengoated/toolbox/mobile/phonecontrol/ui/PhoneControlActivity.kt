package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import android.os.SystemClock
import android.provider.Settings
import android.widget.Toast
import androidx.activity.ComponentActivity
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlService
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlSetupNotification
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
            Mode.RESUME_CAPTURE -> {
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
                val message = getString(R.string.phone_control_activation_projection_needed)
                Toast.makeText(this, message, Toast.LENGTH_SHORT).show()
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
        PhoneControlLog.i(TAG, "coordinator_open mode=${mode.wireName}")
        when (mode) {
            Mode.ACTIVATE -> advanceActivation()
            Mode.SGT_ADB -> startSgtAdbSetup()
            Mode.SHIZUKU -> startShizukuSetup(savedInstanceState)
            Mode.ROOT -> requestRootAuthorization()
            Mode.SGT_ADB_FORGET -> forgetSgtAdbPairing()
            Mode.RESUME_CAPTURE -> requestCaptureResume()
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
        setIntent(intent)
        mode = intent.mode()
        awaitingStep = null
        projectionGrant = null
        when (mode) {
            Mode.ACTIVATE -> advanceActivation()
            Mode.SGT_ADB -> startSgtAdbSetup()
            Mode.SHIZUKU -> startShizukuSetup(savedState = null)
            Mode.ROOT -> requestRootAuthorization()
            Mode.SGT_ADB_FORGET -> forgetSgtAdbPairing()
            Mode.RESUME_CAPTURE -> requestCaptureResume()
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
                launchPlatformStep(userSteps.projection) {
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
                    Toast.makeText(
                        this,
                        R.string.phone_control_activation_start_failed,
                        Toast.LENGTH_SHORT,
                    ).show()
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
        launchPlatformStep(userSteps.settings) {
            settingsLauncher.launch(intent)
            if (step == PhoneControlActivationStep.ACCESSIBILITY ||
                step == PhoneControlActivationStep.OVERLAY
            ) {
                startSettingsNavigation(step, intent)
            }
        }
    }

    private fun startSettingsNavigation(
        step: PhoneControlActivationStep,
        intent: Intent,
    ) {
        val settingsPackage = intent.resolveActivity(packageManager)?.packageName
        val appLabel = applicationInfo.loadLabel(packageManager).toString()
        if (settingsPackage.isNullOrBlank() || appLabel.isBlank()) {
            PhoneControlLog.w(TAG, "settings_navigation unavailable=true")
            return
        }
        settingsNavigationJob?.cancel()
        settingsNavigationJob = lifecycleScope.launch {
            val result = PhoneControlPlatformSettingsNavigator.openAppRow(
                settingsPackage = settingsPackage,
                appLabel = appLabel,
                permissionReady = {
                    when (step) {
                        PhoneControlActivationStep.ACCESSIBILITY ->
                            isAccessibilityReady(this@PhoneControlActivity)
                        PhoneControlActivationStep.OVERLAY ->
                            Settings.canDrawOverlays(this@PhoneControlActivity)
                        else -> false
                    }
                },
            )
            PhoneControlLog.i(TAG, "settings_navigation result=${result.wireName}")
        }
    }

    private fun openApiKeySettings() {
        PhoneControlLog.i(
            TAG,
            "activation_user_step_opened step=gemini_api surface=app_settings",
        )
        Toast.makeText(
            this,
            R.string.phone_control_activation_api_key_needed,
            Toast.LENGTH_LONG,
        ).show()
        PhoneControlLog.w(TAG, "activation_stopped unresolved=gemini_api")
        startActivity(MainActivity.settingsIntent(this))
        finish()
    }

    private fun prepareAccessibilityStep(step: PhoneControlActivationStep) {
        if (probePhoneControlAccessibilityState(this) !=
            PhoneControlAccessibilityState.RECONNECTING
        ) {
            launchActivationSettings(step, accessibilitySettingsIntent(this))
            return
        }
        awaitingStep = step
        PhoneControlLog.i(TAG, "activation_accessibility_reconnect_wait started=true")
        activationResumeJob?.cancel()
        activationResumeJob = lifecycleScope.launch {
            val next = awaitActivationProgress(step)
            awaitingStep = null
            if (next != step) {
                PhoneControlLog.i(TAG, "activation_accessibility_reconnect_wait recovered=true")
                advanceActivation()
            } else {
                PhoneControlLog.w(TAG, "activation_accessibility_reconnect_wait recovered=false")
                Toast.makeText(
                    this@PhoneControlActivity,
                    R.string.phone_control_setup_accessibility_reconnecting,
                    Toast.LENGTH_LONG,
                ).show()
                launchActivationSettings(step, accessibilitySettingsIntent(this@PhoneControlActivity))
            }
        }
    }

    private fun completeActivationStep() {
        val completed = awaitingStep ?: return
        awaitingStep = null
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
        Toast.makeText(
            this,
            when (step) {
                PhoneControlActivationStep.GEMINI_API ->
                    R.string.phone_control_activation_api_key_needed
                PhoneControlActivationStep.RUNTIME_PERMISSIONS ->
                    R.string.phone_control_activation_microphone_needed
                PhoneControlActivationStep.ACCESSIBILITY ->
                    R.string.phone_control_activation_accessibility_needed
                PhoneControlActivationStep.OVERLAY ->
                    R.string.phone_control_activation_overlay_needed
                PhoneControlActivationStep.MEDIA_PROJECTION ->
                    R.string.phone_control_activation_projection_needed
                PhoneControlActivationStep.START ->
                    R.string.phone_control_activation_start_failed
            },
            Toast.LENGTH_SHORT,
        ).show()
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
        PhoneControlLog.i(
            TAG,
            "authority_setup_deferred reason=protected_checkpoint",
        )
        startActivity(resumeCaptureIntent(this))
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
            Toast.makeText(
                this@PhoneControlActivity,
                if (forgotten) {
                    R.string.phone_control_sgt_adb_forgotten
                } else {
                    R.string.phone_control_sgt_adb_forget_failed
                },
                Toast.LENGTH_SHORT,
            ).show()
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
        if (consentIntent == null || !userSteps.projection.begin()) {
            PhoneControlLog.w(TAG, "capture_resume_skipped reason=consent_unavailable")
            finish()
            return
        }
        awaitingStep = PhoneControlActivationStep.MEDIA_PROJECTION
        PhoneControlLog.i(
            TAG,
            "capture_resume_user_step_opened surface=system_capture_dialog",
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
        launch: () -> Unit,
    ) {
        if (!slot.begin()) return
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
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)

        internal fun sgtAdbForgetIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(
            EXTRA_MODE,
            Mode.SGT_ADB_FORGET.wireName,
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)

        internal fun cancelSetupIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(
            EXTRA_MODE,
            Mode.CANCEL_SETUP.wireName,
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)

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
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
    }
}
