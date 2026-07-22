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
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.RootCommandBridge
import dev.screengoated.toolbox.mobile.phonecontrol.provider.privileged.ShizukuBridgeCondition
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
    private var shizukuLastAttempt: PhoneControlShizukuSetupAttempt? = null
    private var shizukuExternalStepActive = false
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
        shizukuExternalStepActive = false
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
                continueShizukuSetup(trigger = "permission_result")
            }
        }

    private val shizukuBinderReceivedListener = Shizuku.OnBinderReceivedListener {
        runOnUiThread {
            if (mode != Mode.SHIZUKU || isFinishing) return@runOnUiThread
            PhoneControlLog.i(
                TAG,
                "optional_setup_event provider=shizuku binder=received " +
                    "external_step_active=$shizukuExternalStepActive",
            )
            if (shizukuExternalStepActive) {
                userSteps.settings.finish()
                shizukuExternalStepActive = false
            }
            continueShizukuSetup(trigger = "binder_received")
        }
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        mode = intent.mode()
        shizukuLastAttempt = savedInstanceState?.shizukuAttempt()
        shizukuExternalStepActive = savedInstanceState?.getBoolean(
            STATE_SHIZUKU_EXTERNAL_ACTIVE,
            false,
        ) ?: false
        Shizuku.addRequestPermissionResultListener(shizukuPermissionListener)
        Shizuku.addBinderReceivedListener(shizukuBinderReceivedListener)
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
        shizukuLastAttempt = null
        shizukuExternalStepActive = false
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
        Shizuku.removeBinderReceivedListener(shizukuBinderReceivedListener)
        super.onDestroy()
    }

    override fun onSaveInstanceState(outState: Bundle) {
        super.onSaveInstanceState(outState)
        shizukuLastAttempt?.let { attempt ->
            outState.putString(STATE_SHIZUKU_CONDITION, attempt.condition.wireName)
            outState.putString(STATE_SHIZUKU_ACTION, attempt.action.wireName)
        }
        outState.putBoolean(STATE_SHIZUKU_EXTERNAL_ACTIVE, shizukuExternalStepActive)
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

    private fun continueShizukuSetup(trigger: String = "direct") {
        val probe = ShizukuCommandBridge.probe(this)
        val action = nextPhoneControlShizukuSetupAction(probe)
        val attempt = PhoneControlShizukuSetupAttempt(probe.condition, action)
        PhoneControlLog.i(
            TAG,
            "optional_setup_step provider=shizuku trigger=$trigger " +
                "condition=${probe.condition.wireName} action=${action.wireName}",
        )
        if (action == PhoneControlShizukuSetupAction.COMPLETE) {
            Toast.makeText(this, R.string.phone_control_shizuku_ready, Toast.LENGTH_SHORT).show()
            PhoneControlLog.i(TAG, "optional_setup_result provider=shizuku ready=true")
            finish()
            return
        }
        if (shizukuLastAttempt == attempt) {
            Toast.makeText(
                this,
                R.string.phone_control_shizuku_still_needs_user_step,
                Toast.LENGTH_LONG,
            ).show()
            PhoneControlLog.w(
                TAG,
                "optional_setup_result provider=shizuku ready=false unchanged=true " +
                    "condition=${probe.condition.wireName}",
            )
            finish()
            return
        }
        shizukuLastAttempt = attempt
        when (action) {
            PhoneControlShizukuSetupAction.REQUEST_PERMISSION -> {
                Toast.makeText(
                    this,
                    R.string.phone_control_shizuku_request_permission,
                    Toast.LENGTH_LONG,
                ).show()
                if (userSteps.shizuku.begin() &&
                    ShizukuCommandBridge.requestPermission(this, SHIZUKU_PERMISSION_REQUEST)
                ) {
                    return
                }
                userSteps.shizuku.finish()
                PhoneControlLog.w(TAG, "optional_setup_dispatch provider=shizuku accepted=false")
                finish()
            }
            PhoneControlShizukuSetupAction.OPEN_MANAGER,
            PhoneControlShizukuSetupAction.OPEN_STORE,
            -> launchShizukuExternalStep(probe.condition, action)
            PhoneControlShizukuSetupAction.COMPLETE -> error("handled above")
        }
    }

    private fun launchShizukuExternalStep(
        condition: ShizukuBridgeCondition,
        action: PhoneControlShizukuSetupAction,
    ) {
        val message = when (condition) {
            ShizukuBridgeCondition.SERVICE_STOPPED -> R.string.phone_control_shizuku_start_service
            ShizukuBridgeCondition.PERMISSION_REVOKED ->
                R.string.phone_control_shizuku_restore_permission
            ShizukuBridgeCondition.API_UNSUPPORTED -> R.string.phone_control_shizuku_update
            ShizukuBridgeCondition.PACKAGE_MISSING -> R.string.phone_control_shizuku_install
            ShizukuBridgeCondition.READY,
            ShizukuBridgeCondition.PERMISSION_REQUESTABLE,
            -> error("condition does not own an external Shizuku step")
        }
        Toast.makeText(this, message, Toast.LENGTH_LONG).show()
        val launch = when (action) {
            PhoneControlShizukuSetupAction.OPEN_MANAGER -> shizukuManagerIntent()
            PhoneControlShizukuSetupAction.OPEN_STORE -> shizukuStoreIntent()
            else -> error("action does not own an external Shizuku step")
        }
        shizukuExternalStepActive = true
        try {
            launchPlatformStep(userSteps.settings) { settingsLauncher.launch(launch) }
            PhoneControlLog.i(
                TAG,
                "optional_setup_dispatch provider=shizuku accepted=true action=${action.wireName}",
            )
        } catch (error: RuntimeException) {
            shizukuExternalStepActive = false
            PhoneControlLog.w(
                TAG,
                "optional_setup_dispatch provider=shizuku accepted=false action=${action.wireName}",
            )
            throw error
        }
    }

    private fun shizukuManagerIntent(): Intent =
        packageManager.getLaunchIntentForPackage(SHIZUKU_PACKAGE) ?: shizukuStoreIntent()

    private fun shizukuStoreIntent(): Intent {
        val store = Intent(
            Intent.ACTION_VIEW,
            Uri.parse("market://details?id=$SHIZUKU_PACKAGE"),
        ).setPackage(PLAY_STORE_PACKAGE)
        return store.takeIf { it.resolveActivity(packageManager) != null }
            ?: Intent(Intent.ACTION_VIEW, Uri.parse(SHIZUKU_DOWNLOAD_URL))
                .addCategory(Intent.CATEGORY_BROWSABLE)
    }

    private fun Bundle.shizukuAttempt(): PhoneControlShizukuSetupAttempt? {
        val condition = getString(STATE_SHIZUKU_CONDITION)?.let { wireName ->
            ShizukuBridgeCondition.entries.firstOrNull { it.wireName == wireName }
        } ?: return null
        val action = getString(STATE_SHIZUKU_ACTION)?.let { wireName ->
            PhoneControlShizukuSetupAction.entries.firstOrNull { it.wireName == wireName }
        } ?: return null
        return PhoneControlShizukuSetupAttempt(condition, action)
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
        private const val PLAY_STORE_PACKAGE = "com.android.vending"
        private const val SHIZUKU_DOWNLOAD_URL = "https://shizuku.rikka.app/download/"
        private const val SHIZUKU_PERMISSION_REQUEST = 4082
        private const val ACTIVATION_PROPAGATION_ATTEMPTS = 30
        private const val ACTIVATION_PROPAGATION_POLL_MS = 100L
        private const val STATE_SHIZUKU_CONDITION = "shizuku_condition"
        private const val STATE_SHIZUKU_ACTION = "shizuku_action"
        private const val STATE_SHIZUKU_EXTERNAL_ACTIVE = "shizuku_external_active"

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
