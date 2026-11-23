package dev.screengoated.toolbox.mobile.phonecontrol.ui

import android.Manifest
import android.content.Context
import android.content.Intent
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.result.ActivityResultLauncher
import androidx.activity.result.contract.ActivityResultContracts
import androidx.activity.viewModels
import androidx.lifecycle.lifecycleScope
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlProjectionDispatches
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlService
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlSetupNotification
import dev.screengoated.toolbox.mobile.phonecontrol.reservePhoneControlProjectionConsent
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

private typealias Mode = PhoneControlCoordinatorMode

/**
 * Transparent coordinator for Android-owned Phone Control setup steps.
 * Product UI stays on the Apps card and orb; this activity never renders a setup page.
 */
class PhoneControlActivity : ComponentActivity() {
    private var startupBlocked = false
    internal val userSteps by viewModels<PhoneControlUserStepState>()
    private var mode = Mode.ACTIVATE
    internal var awaitingStep: PhoneControlActivationStep? = null
    private var requestedNotification = false
    internal var projectionGrant: PhoneControlProjectionGrant?
        get() = userSteps.projectionGrant
        set(value) {
            userSteps.projectionGrant = value
            if (value == null) userSteps.projectionGrantGeneration = null
        }
    private var sgtAdbSetup: PhoneControlSgtAdbSetupCoordinator? = null
    private var shizukuSetup: PhoneControlShizukuSetupCoordinator? = null
    internal var activationResumeJob: Job? = null
    internal var projectionResultJob: Job? = null
    internal var settingsNavigationJob: Job? = null
    private val permissionLauncher = registerForActivityResult(
        ActivityResultContracts.RequestMultiplePermissions(),
    ) {
        if (startupBlocked || (application as SgtMobileApplication).appContainerOrNull == null) {
            finish()
            return@registerForActivityResult
        }
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
    internal val settingsLauncher = registerForActivityResult(
        ActivityResultContracts.StartActivityForResult(),
    ) {
        if (startupBlocked || (application as SgtMobileApplication).appContainerOrNull == null) {
            finish()
            return@registerForActivityResult
        }
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
    private lateinit var projectionLauncher: ActivityResultLauncher<Intent>

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        if ((application as SgtMobileApplication).appContainerOrNull == null) {
            startupBlocked = true
            finish()
            return
        }
        // Keep the key across configuration changes, but create a new one
        // after process death so an old result cannot reach a new request.
        projectionLauncher = activityResultRegistry.register(
            userSteps.projectionLauncherKey,
            this,
            ActivityResultContracts.StartActivityForResult(),
            { result -> handleProjectionResult(result) },
        )
        mode = intent.mode()
        PhoneControlCoordinatorReentryLauncher.acknowledge(intent, mode.wireName)
        if (!acceptOrSupersedeOrphanReentry(intent, freshIntent = savedInstanceState == null)) {
            finish()
            return
        }
        val outstanding = PhoneControlProjectionDispatches.current()
        if (outstanding != null && userSteps.adoptProjectionDispatch(outstanding)) {
            userSteps.deferredReentry = Intent(intent)
            if (PhoneControlProjectionDispatches.stageReentry(
                    outstanding, intent,
                    replaceExisting = !PhoneControlCoordinatorReentryLauncher.isCoordinatorIntent(intent),
                )
            ) {
                userSteps.deferredReentry = null
                if (mode == Mode.CANCEL_SETUP) {
                    PhoneControlProjectionDispatches.retireIfPending(outstanding)
                }
                finishAndReplayProjectionReentry()
                return
            }
            userSteps.deferredReentry = null
            userSteps.projectionDispatch = null
            userSteps.projectionSettling = false
        }
        // Android can restore an outstanding ActivityResult after process death.
        // Its opaque request owner was intentionally not persisted; do not
        // launch another request whose owner could receive that old result.
        if (retireUnownedRestoredProjection(
                savedInstanceState?.getBoolean(STATE_PROJECTION_IN_FLIGHT) == true,
                userSteps.hasProjectionOwnership,
            )
        ) {
            PhoneControlLog.w(TAG, "projection_request_retired reason=owner_not_restored")
            finish()
            return
        }
        if (userSteps.projectionRequest is PhoneControlProjectionRequest.Activation) {
            awaitingStep = PhoneControlActivationStep.MEDIA_PROJECTION
        }
        PhoneControlLog.i(TAG, intent.phoneControlCoordinatorEvent("coordinator_open", mode.wireName))
        if (userSteps.pendingProjectionResult != null) {
            resumeProjectionResultAfterTeardown()
            return
        }
        if (userSteps.projectionSettling && userSteps.projectionDispatch != null) {
            finishAndReplayProjectionReentry(userSteps.settlingProjectionOwner)
            return
        }
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
        if (startupBlocked || (application as SgtMobileApplication).appContainerOrNull == null) {
            finish()
            return
        }
        PhoneControlCoordinatorReentryLauncher.acknowledge(intent, intent.mode().wireName)
        if (!acceptOrSupersedeOrphanReentry(intent)) {
            return
        }
        val incoming = when (intent.mode()) {
            Mode.ACTIVATE -> ProjectionReentryKind.ACTIVATE
            Mode.RESUME_CAPTURE -> ProjectionReentryKind.RESUME
            Mode.CANCEL_SETUP -> ProjectionReentryKind.CANCEL
            else -> ProjectionReentryKind.OTHER
        }
        val current = userSteps.projectionRequest
        when (projectionReentryDisposition(
            request = current,
            settling = userSteps.projectionSettling,
            incoming = incoming,
            ownerStillCurrent = when (current) {
                is PhoneControlProjectionRequest.Activation ->
                    PhoneControlProjectionDispatches.ownsConsent(current.generation)
                is PhoneControlProjectionRequest.Protected ->
                    PhoneControlProtectedCheckpointRegistry.ownsFreshProjection(current.owner) &&
                        PhoneControlProjectionDispatches.ownsConsent(current.generation)
                null -> false
            },
        )) {
            ProjectionReentryDisposition.CANCEL -> {
                val dispatch = userSteps.projectionDispatch
                if (dispatch != null) {
                    userSteps.deferredReentry = Intent(intent)
                    if (PhoneControlProjectionDispatches.stageReentry(
                            dispatch, intent,
                            replaceExisting = !PhoneControlCoordinatorReentryLauncher.isCoordinatorIntent(intent),
                        )
                    ) {
                        userSteps.deferredReentry = null
                        PhoneControlProjectionDispatches.retireIfPending(dispatch)
                        finishAndReplayProjectionReentry(userSteps.settlingProjectionOwner)
                        return
                    }
                }
                userSteps.releaseProjectionConsent()
                userSteps.projectionRequest = null
                userSteps.pendingProjectionResult = null
                userSteps.projectionSettling = false
                userSteps.settlingProjectionOwner = null
                userSteps.projectionDispatch = null
                userSteps.deferredReentry = null
                userSteps.projection.finish()
            }
            ProjectionReentryDisposition.COALESCE -> {
                return
            }
            ProjectionReentryDisposition.DEFER -> {
                if (userSteps.deferredReentry == null ||
                    !PhoneControlCoordinatorReentryLauncher.isCoordinatorIntent(intent)
                ) userSteps.deferredReentry = Intent(intent)
                val deferred = requireNotNull(userSteps.deferredReentry)
                val dispatch = userSteps.projectionDispatch
                if (dispatch == null || PhoneControlProjectionDispatches.stageReentry(
                        dispatch, deferred,
                        replaceExisting = !PhoneControlCoordinatorReentryLauncher.isCoordinatorIntent(deferred),
                    )
                ) {
                    if (dispatch != null) userSteps.deferredReentry = null
                    return
                }
                userSteps.deferredReentry = null
                userSteps.projectionDispatch = null
                userSteps.projectionSettling = false
            }
            ProjectionReentryDisposition.PROCEED -> Unit
        }
        activationResumeJob?.cancel()
        settingsNavigationJob?.cancel()
        sgtAdbSetup?.close()
        sgtAdbSetup = null
        shizukuSetup?.close()
        shizukuSetup = null
        userSteps.settings.finish()
        setIntent(intent)
        mode = intent.mode()
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
        outState.putBoolean(STATE_PROJECTION_IN_FLIGHT, userSteps.hasProjectionOwnership)
        shizukuSetup?.save(outState)
    }

    internal fun advanceActivation() {
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
                    val generation = reservePhoneControlProjectionConsent(this)
                    if (generation == null) {
                        userSteps.projection.finish()
                        awaitingStep = null
                        showProjectionBusy(activationIntent(this))
                        abortActivation(step)
                        return@launchPlatformStep
                    }
                    userSteps.projectionRequest = PhoneControlProjectionRequest.Activation(generation)
                    try {
                        projectionLauncher.launch(consentIntent)
                    } catch (error: RuntimeException) {
                        PhoneControlProjectionDispatches.releaseConsent(generation)
                        userSteps.projectionRequest = null
                        throw error
                    }
                }
            }
            PhoneControlActivationStep.START -> {
                val grant = projectionGrant
                val generation = userSteps.projectionGrantGeneration
                if (grant == null || generation == null) {
                    abortActivation(PhoneControlActivationStep.MEDIA_PROJECTION)
                    return
                }
                val dispatch = PhoneControlService.start(this, grant, generation)
                userSteps.projectionDispatch = dispatch
                val accepted = dispatch != null
                if (!accepted) PhoneControlProjectionDispatches.releaseConsent(generation)
                userSteps.projectionGrantGeneration = null
                projectionGrant = null
                PhoneControlLog.i(TAG, "activation_service_start accepted=$accepted")
                if (!accepted) {
                    showPhoneControlToast(R.string.phone_control_activation_start_failed_toast)
                    PhoneControlSetupNotification.show(
                        this,
                        phoneControlString(R.string.phone_control_activation_projection_needed),
                        activationIntent(this),
                    )
                }
                finishAndReplayProjectionReentry()
            }
        }
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
        if (userSteps.projection.active &&
            userSteps.projectionRequest is PhoneControlProjectionRequest.Protected
        ) return
        if (!PhoneControlService.captureSuspended) {
            PhoneControlLog.w(TAG, "capture_resume_skipped reason=no_suspended_runtime")
            finish()
            return
        }
        val owner = PhoneControlProtectedCheckpointRegistry.freshProjectionOwner()
        if (owner == null) {
            PhoneControlLog.w(TAG, "capture_resume_skipped reason=no_fresh_checkpoint")
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
        val generation = reservePhoneControlProjectionConsent(this)
        if (generation == null) {
            userSteps.projection.finish()
            showProjectionBusy(resumeCaptureIntent(this))
            finishAndReplayProjectionReentry()
            return
        }
        userSteps.projectionRequest = PhoneControlProjectionRequest.Protected(owner, generation)
        awaitingStep = PhoneControlActivationStep.MEDIA_PROJECTION
        PhoneControlLog.i(
            TAG,
            "capture_resume_launcher_dispatched surface=system_capture_dialog",
        )
        try {
            projectionLauncher.launch(consentIntent)
        } catch (error: RuntimeException) {
            PhoneControlProjectionDispatches.releaseConsent(generation)
            userSteps.projectionRequest = null
            userSteps.projection.finish()
            awaitingStep = null
            PhoneControlLog.w(TAG, "capture_resume_launcher_dispatched accepted=false")
            PhoneControlSetupNotification.show(
                this,
                phoneControlString(R.string.phone_control_activation_projection_needed),
                resumeCaptureIntent(this),
            )
            finishAndReplayProjectionReentry()
        }
    }

    internal fun shouldSkipProjectionReentry(intent: Intent): Boolean =
        intent.mode() == Mode.ACTIVATE && PhoneControlService.state.value.running

    private fun cancelAuthoritySetup() {
        PhoneControlPowerPreferences.save(this, PhoneControlPowerChoice.STANDARD)
        PhoneControlService.clearAuthoritySetup(this)
        PhoneControlLog.i(TAG, "authority_setup_result pending=false reason=user_cancelled")
        finish()
    }

    internal inline fun launchPlatformStep(
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

    private fun Intent.mode(): Mode = phoneControlCoordinatorMode(getStringExtra(EXTRA_MODE))

    companion object {
        private const val TAG = "SGTPhoneControlActivation"
        private const val EXTRA_MODE = "dev.screengoated.toolbox.mobile.phonecontrol.MODE"
        private const val STATE_PROJECTION_IN_FLIGHT = "projection_in_flight"
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
