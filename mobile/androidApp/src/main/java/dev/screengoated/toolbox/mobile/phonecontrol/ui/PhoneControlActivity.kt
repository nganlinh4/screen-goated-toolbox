package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.net.Uri
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
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PlatformUserStepSlot
import dev.screengoated.toolbox.mobile.phonecontrol.capability.CapabilityState
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.RootCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuCommandBridge
import kotlinx.coroutines.launch
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import rikka.shizuku.Shizuku

/**
 * Transparent coordinator for Android-owned Phone Control setup steps.
 * Product UI stays on the Apps card and orb; this activity never renders a setup page.
 */
class PhoneControlActivity : ComponentActivity() {
    private val userSteps by viewModels<PhoneControlUserStepState>()
    private var mode = Mode.ACTIVATE
    private var awaitingStep: PhoneControlActivationStep? = null
    private var requestedNotification = false
    private var shizukuReturnCount = 0
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
        userSteps.settings.finish()
        PhoneControlLog.i(
            TAG,
            "activation_user_step_returned step=${returnedStep?.wireName ?: "optional"} " +
                "surface=android_settings",
        )
        when (mode) {
            Mode.ACTIVATE -> completeActivationStep()
            Mode.SHIZUKU -> continueShizukuSetup()
            Mode.ROOT -> finish()
        }
    }

    private val shizukuPermissionListener =
        Shizuku.OnRequestPermissionResultListener { requestCode, _ ->
            if (requestCode == SHIZUKU_PERMISSION_REQUEST) {
                userSteps.shizuku.finish()
                val ready = ShizukuCommandBridge.probe(this).state == CapabilityState.READY
                PhoneControlLog.i(TAG, "optional_setup_result provider=shizuku ready=$ready")
                finish()
            }
        }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Shizuku.addRequestPermissionResultListener(shizukuPermissionListener)
        mode = intent.mode()
        PhoneControlLog.i(TAG, "coordinator_open mode=${mode.wireName}")
        when (mode) {
            Mode.ACTIVATE -> advanceActivation()
            Mode.SHIZUKU -> continueShizukuSetup()
            Mode.ROOT -> requestRootAuthorization()
        }
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        activationResumeJob?.cancel()
        settingsNavigationJob?.cancel()
        setIntent(intent)
        mode = intent.mode()
        awaitingStep = null
        when (mode) {
            Mode.ACTIVATE -> advanceActivation()
            Mode.SHIZUKU -> continueShizukuSetup()
            Mode.ROOT -> requestRootAuthorization()
        }
    }

    override fun onDestroy() {
        activationResumeJob?.cancel()
        settingsNavigationJob?.cancel()
        Shizuku.removeRequestPermissionResultListener(shizukuPermissionListener)
        super.onDestroy()
    }

    private fun advanceActivation() {
        if (isFinishing || awaitingStep != null) return
        val snapshot = probePhoneControlActivation(this)
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
            PhoneControlActivationStep.ACCESSIBILITY -> launchActivationSettings(
                step,
                accessibilitySettingsIntent(this),
            )
            PhoneControlActivationStep.OVERLAY -> launchActivationSettings(
                step,
                overlaySettingsIntent(this),
            )
            PhoneControlActivationStep.START -> {
                val accepted = PhoneControlService.start(this)
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
                            isAccessibilityEnabled(this@PhoneControlActivity)
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
            val next = nextPhoneControlActivationStep(probePhoneControlActivation(this))
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
                PhoneControlActivationStep.START ->
                    R.string.phone_control_activation_start_failed
            },
            Toast.LENGTH_SHORT,
        ).show()
        finish()
    }

    private fun continueShizukuSetup() {
        val probe = ShizukuCommandBridge.probe(this)
        if (probe.state == CapabilityState.READY) {
            PhoneControlLog.i(TAG, "optional_setup_result provider=shizuku ready=true")
            finish()
            return
        }
        if (probe.state == CapabilityState.NEEDS_USER_STEP && userSteps.shizuku.begin()) {
            if (ShizukuCommandBridge.requestPermission(this, SHIZUKU_PERMISSION_REQUEST)) {
                return
            }
            userSteps.shizuku.finish()
        }
        if (shizukuReturnCount++ > 0) {
            PhoneControlLog.w(TAG, "optional_setup_result provider=shizuku ready=false")
            finish()
            return
        }
        val launch = packageManager.getLaunchIntentForPackage(SHIZUKU_PACKAGE)
            ?: Intent(Intent.ACTION_VIEW, Uri.parse(SHIZUKU_DOWNLOAD_URL))
                .addCategory(Intent.CATEGORY_BROWSABLE)
        launchPlatformStep(userSteps.settings) { settingsLauncher.launch(launch) }
    }

    private fun requestRootAuthorization() {
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
        Mode.SHIZUKU.wireName -> Mode.SHIZUKU
        Mode.ROOT.wireName -> Mode.ROOT
        else -> Mode.ACTIVATE
    }

    private enum class Mode(val wireName: String) {
        ACTIVATE("activate"),
        SHIZUKU("shizuku"),
        ROOT("root"),
    }

    companion object {
        private const val TAG = "SGTPhoneControlActivation"
        private const val EXTRA_MODE = "dev.screengoated.toolbox.mobile.phonecontrol.MODE"
        private const val SHIZUKU_PACKAGE = "moe.shizuku.privileged.api"
        private const val SHIZUKU_DOWNLOAD_URL = "https://shizuku.rikka.app/download/"
        private const val SHIZUKU_PERMISSION_REQUEST = 4082
        private const val ACTIVATION_PROPAGATION_ATTEMPTS = 30
        private const val ACTIVATION_PROPAGATION_POLL_MS = 100L

        internal fun activationIntent(context: Context): Intent = Intent(
            context,
            PhoneControlActivity::class.java,
        ).putExtra(EXTRA_MODE, Mode.ACTIVATE.wireName)

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
                PhoneControlPowerChoice.SHIZUKU -> Mode.SHIZUKU.wireName
                PhoneControlPowerChoice.ROOT -> Mode.ROOT.wireName
            },
        ).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK or Intent.FLAG_ACTIVITY_SINGLE_TOP)
    }
}
