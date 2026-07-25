package dev.screengoated.toolbox.mobile.phonecontrol

import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.res.Configuration
import android.os.Handler
import android.os.IBinder
import android.os.Looper
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import androidx.compose.runtime.State
import androidx.compose.runtime.mutableStateOf
import dev.screengoated.toolbox.mobile.R
import dev.screengoated.toolbox.mobile.SgtMobileApplication
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointToken
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlResourceAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authorization.PhoneControlStructuralEditAuthorization
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayController
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayExclusion
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionGrant
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionProvider
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PhoneControlProjectionStartResult
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PROJECTION_DATA_EXTRA
import dev.screengoated.toolbox.mobile.phonecontrol.projection.PROJECTION_RESULT_CODE_EXTRA
import dev.screengoated.toolbox.mobile.phonecontrol.projection.phoneControlProjectionGrant
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.PhoneControlBrowserLifecycle
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntime
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeCode
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeObserver
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimePhase
import dev.screengoated.toolbox.mobile.phonecontrol.runtime.PhoneControlRuntimeSnapshot
import dev.screengoated.toolbox.mobile.phonecontrol.session.PhoneControlContractAssets
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatcher
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerSelectionRoute
import dev.screengoated.toolbox.mobile.phonecontrol.ui.phoneControlPowerSelectionRoute
import dev.screengoated.toolbox.mobile.service.tryStartForegroundService

class PhoneControlService : Service() {
    private val mainHandler = Handler(Looper.getMainLooper())
    private lateinit var overlayController: PhoneControlOverlayController
    private lateinit var sessionNotification: PhoneControlSessionNotification
    private lateinit var authoritySetup: PhoneControlAuthoritySetupController
    private lateinit var protectedSetup: PhoneControlProtectedSetupCoordinator
    private val protectedCheckpoint = PhoneControlProtectedCheckpointController()
    private var runtime: PhoneControlRuntime? = null
    private var preserveFailureOnDestroy = false
    private var projectionActive = false
    private var stopReason = "system_destroy"
    private var loggedRuntimeState: Triple<Boolean, PhoneControlRuntimePhase, PhoneControlRuntimeCode>? = null
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        Log.i(TAG, "service_created")
        authoritySetup = PhoneControlAuthoritySetupController(
            context = this,
            runtime = { runtime },
            publishGuidance = { guidance ->
                publish(mutableState.value.copy(authorityGuidance = guidance))
            },
            enterProtectedCheckpoint = ::enterProtectedCheckpoint,
        )
        protectedSetup = PhoneControlProtectedSetupCoordinator(
            this,
            authoritySetup::replaceGuidance,
            ::restoreRetainedProjection,
        )
        overlayController = PhoneControlOverlayController(
            context = this,
            onDismiss = { stopRequested(source = "orb_dismiss") },
            onPowerChoiceSelected = ::selectPowerChoice,
        )
        PhoneControlOverlayExclusion.register(overlayController)
        sessionNotification = PhoneControlSessionNotification(
            service = this,
            stopIntent = Intent(this, PhoneControlService::class.java)
                .setAction(ACTION_STOP)
                .putExtra(EXTRA_STOP_SOURCE, "notification"),
        )
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                val source = intent.getStringExtra(EXTRA_STOP_SOURCE)
                    .orEmpty()
                    .ifBlank { "unknown" }
                Log.i(TAG, "service_command action=stop source=$source start_id=$startId")
                stopRequested(source)
            }
            ACTION_ATTACH_PROJECTION -> attachProjection(intent)
            ACTION_AUTHORITY_SETUP_PROGRESS -> authoritySetup.update(intent)
            ACTION_AUTHORITY_SETUP_CLEAR ->
                if (intent.getStringExtra(EXTRA_AUTHORITY_PROVIDER_ID).isNullOrBlank()) {
                    selectPowerChoice(PhoneControlPowerChoice.STANDARD)
                } else authoritySetup.clear(intent.getStringExtra(EXTRA_AUTHORITY_PROVIDER_ID))
            else -> {
                if (runtime == null) {
                    stopReason = "runtime_terminal"
                    Log.i(TAG, "service_command action=start start_id=$startId")
                    startWithProjection(intent)
                } else {
                    Log.i(TAG, "service_command action=duplicate_start start_id=$startId")
                }
            }
        }
        return START_NOT_STICKY
    }

    override fun onDestroy() {
        Log.i(TAG, "service_destroyed reason=$stopReason")
        runtime?.stop()
        runtime = null
        PhoneControlBrowserLifecycle.close()
        releaseProjection()
        protectedCheckpoint.close()
        protectedSetup.close()
        if (!preserveFailureOnDestroy) publish(stoppedState())
        PhoneControlOverlayExclusion.unregister(overlayController)
        overlayController.destroy()
        super.onDestroy()
    }

    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        overlayController.onConfigurationChanged()
    }

    private fun startRuntime() {
        try {
            val container = (application as SgtMobileApplication).appContainer
            val assets = PhoneControlContractAssets.load(this, container.json)
            val providerEvidence = PhoneControlProviderRegistry.probe(this)
            val apiKey = container.repository.currentApiKey()
            if (apiKey.isBlank()) {
                stopReason = "api_key_required"
                preserveFailureOnDestroy = true
                publish(
                    PhoneControlServiceState(
                        running = false,
                        phase = PhoneControlRuntimePhase.ERROR,
                        code = PhoneControlRuntimeCode.API_KEY_REQUIRED,
                        userMessage = phoneControlString(R.string.phone_control_status_api_key_required),
                    ),
                )
                stopSelf()
                return
            }
            lateinit var candidate: PhoneControlRuntime
            val structuralAuthorization = PhoneControlStructuralEditAuthorization(this)
            val resourceAuthorization = PhoneControlResourceAuthorization(this)
            candidate = PhoneControlRuntime(
                context = this,
                httpClient = container.httpClient,
                projectionConsentStore = container.projectionConsentStore,
                apiKey = apiKey,
                voiceName = container.repository.currentGlobalTtsSettings().voice,
                contractAssets = assets,
                capabilityContext = providerEvidence.modelContext(),
                memoryRepository = container.phoneControlMemoryRepository,
                dispatchBoundary = PhoneControlToolDispatcher(
                    this,
                    structuralAuthorization,
                    resourceAuthorization,
                ),
                observer = PhoneControlRuntimeObserver { snapshot ->
                    mainHandler.post {
                        if (runtime === candidate) publishRuntimeSnapshot(snapshot)
                    }
                },
                onUserInterfaceGoalFinished = { completion ->
                    mainHandler.post {
                        authoritySetup.onUserInterfaceGoalFinished(completion)
                    }
                },
                additionalTurnRecorders = listOf(
                    structuralAuthorization,
                    resourceAuthorization,
                ),
            )
            runtime = candidate
            if (!candidate.start()) {
                stopReason = "runtime_start_rejected"
                preserveFailureOnDestroy = true
                runtime = null
                stopSelf()
            } else {
                resumeSelectedAuthoritySetup()
            }
        } catch (error: Throwable) {
            Log.e(TAG, "service_start_failed code=configuration_failed", error)
            stopReason = "configuration_failed"
            preserveFailureOnDestroy = true
            publish(
                PhoneControlServiceState(
                    running = false,
                    phase = PhoneControlRuntimePhase.ERROR,
                    code = PhoneControlRuntimeCode.CONFIGURATION_FAILED,
                    userMessage = phoneControlString(R.string.phone_control_status_configuration_failed),
                ),
            )
            stopSelf()
        }
    }

    private fun startWithProjection(intent: Intent?) {
        val grant = intent?.phoneControlProjectionGrant()
        if (grant == null) {
            projectionFailure("projection_grant_missing")
            return
        }
        enterForeground()
        when (
            val started = PhoneControlProjectionProvider.start(
                context = this,
                grant = grant,
                onProjectionStopped = {
                    mainHandler.post(::projectionStoppedByPlatform)
                },
            )
        ) {
            is PhoneControlProjectionStartResult.Ready -> {
                projectionActive = true
                PhoneControlSetupNotification.clear(this)
                publish(
                    PhoneControlServiceState(
                        running = true,
                        phase = PhoneControlRuntimePhase.STARTING,
                        code = PhoneControlRuntimeCode.STARTING,
                        userMessage = phoneControlString(R.string.phone_control_status_starting),
                    ),
                )
                startRuntime()
            }
            is PhoneControlProjectionStartResult.Failure -> projectionFailure(started.code)
        }
    }

    private fun projectionStoppedByPlatform() {
        if (!projectionActive) return
        projectionActive = false
        Log.w(TAG, "projection_terminal reason=platform")
        stopReason = "projection_revoked"
        preserveFailureOnDestroy = true
        runtime?.stop()
        runtime = null
        publish(
            PhoneControlServiceState(
                running = false,
                phase = PhoneControlRuntimePhase.ERROR,
                code = PhoneControlRuntimeCode.SCREEN_SHARE_REQUIRED,
                userMessage = phoneControlString(R.string.phone_control_status_projection_required),
            ),
        )
        stopSelf()
    }

    private fun projectionFailure(code: String) {
        Log.w(TAG, "projection_start_failed code=$code")
        stopReason = code
        preserveFailureOnDestroy = true
        publish(
            PhoneControlServiceState(
                running = false,
                phase = PhoneControlRuntimePhase.ERROR,
                code = PhoneControlRuntimeCode.SCREEN_SHARE_REQUIRED,
                userMessage = phoneControlString(R.string.phone_control_status_projection_required),
            ),
        )
        stopSelf()
    }

    private fun releaseProjection() {
        projectionActive = false
        PhoneControlProjectionProvider.stop()
    }

    private fun enterProtectedCheckpoint(providerId: String): Boolean {
        val candidate = runtime ?: return false
        val policy = protectedSetup.capturePolicy(providerId)
        if (!projectionActive || protectedCheckpoint.active || policy == null) {
            Log.w(TAG, "protected_checkpoint_enter accepted=false reason=state_or_adapter")
            return false
        }
        val token = protectedCheckpoint.begin(
            providerId,
            policy,
            candidate,
            ::releaseProjection,
        ) ?: return false
        protectedSetup.start(providerId, token)
        return true
    }

    private fun attachProjection(intent: Intent) {
        val candidate = runtime
        val token = protectedCheckpoint.activeToken
        val grant = intent.phoneControlProjectionGrant()
        if (candidate == null || token == null || grant == null) {
            Log.w(
                TAG,
                "projection_attach accepted=false reason=invalid_runtime_or_grant",
            )
            if (candidate == null) stopSelf()
            return
        }
        when (
            val started = PhoneControlProjectionProvider.start(
                context = this,
                grant = grant,
                onProjectionStopped = {
                    mainHandler.post(::projectionStoppedByPlatform)
                },
            )
        ) {
            is PhoneControlProjectionStartResult.Ready -> {
                projectionActive = true
                if (!protectedCheckpoint.attachFresh(token, candidate)) {
                    releaseProjection()
                    Log.e(TAG, "projection_attach accepted=false reason=checkpoint_owner_lost")
                    return
                }
                Log.i(
                    TAG,
                    "projection_attach accepted=true runtime_reused=true " +
                        "visual_evidence=true",
                )
                protectedSetup.onProjectionAttached(authoritySetup) {
                    resumeSelectedAuthoritySetup()
                }
            }
            is PhoneControlProjectionStartResult.Failure -> {
                Log.w(TAG, "projection_attach accepted=false code=${started.code}")
            }
        }
    }

    private fun selectPowerChoice(choice: PhoneControlPowerChoice) {
        authoritySetup.onPowerChoiceSelected(choice)
        val freshProjectionRequired = protectedCheckpoint.freshProjectionRequired
        if (protectedCheckpoint.active) {
            protectedSetup.cancel(
                resumeSelectedSetupAfterCapture =
                    freshProjectionRequired && choice.elevatedProviderId != null,
            )
            if (!freshProjectionRequired) {
                runtime?.let(protectedCheckpoint::cancelRetained)
            }
        }
        val route = phoneControlPowerSelectionRoute(choice, freshProjectionRequired)
        val intent = when (route) {
            PhoneControlPowerSelectionRoute.RESUME_CAPTURE ->
                PhoneControlActivity.resumeCaptureIntent(this)
            PhoneControlPowerSelectionRoute.SETUP ->
                PhoneControlActivity.optionalPowerIntent(this, choice)
            PhoneControlPowerSelectionRoute.NONE -> null
        }
        Log.i(TAG, "power_choice_route choice=${choice.wireName} route=${route.name.lowercase()}")
        if (intent != null) runCatching { startActivity(intent) }
    }

    private fun restoreRetainedProjection(
        providerId: String,
        token: PhoneControlProtectedCheckpointToken,
        resumeSelectedSetup: Boolean,
    ) {
        val candidate = runtime ?: return
        if (!projectionActive || !protectedCheckpoint.restoreRetained(token, candidate)) {
            Log.e(TAG, "protected_checkpoint_exit accepted=false reason=state_or_owner")
            return
        }
        if (!resumeSelectedSetup ||
            PhoneControlPowerPreferences.current(this)?.elevatedProviderId != providerId
        ) {
            return
        }
        authoritySetup.clear(reason = "retained_projection_restored")
        resumeSelectedAuthoritySetup(announceReady = true)
    }
    private fun resumeSelectedAuthoritySetup(announceReady: Boolean = false) {
        authoritySetup.resumeSelectedAuthoritySetup(announceReady) { choice ->
            mainHandler.postDelayed({
                if (runtime == null ||
                    PhoneControlPowerPreferences.current(this) != choice
                ) {
                    return@postDelayed
                }
                runCatching {
                    startActivity(
                        PhoneControlActivity.optionalPowerIntent(
                            this,
                            choice,
                        ),
                    )
                }.onSuccess {
                    Log.i(
                        TAG,
                        "authority_setup_resume provider=${choice.elevatedProviderId} accepted=true",
                    )
                }.onFailure {
                    Log.w(
                        TAG,
                        "authority_setup_resume provider=${choice.elevatedProviderId} accepted=false",
                    )
                }
            }, AUTHORITY_SETUP_RESUME_DELAY_MS)
        }
    }

    private fun stopRequested(source: String) {
        stopReason = "requested:$source"
        preserveFailureOnDestroy = false
        authoritySetup.clear(reason = "service_stop")
        runtime?.stop()
        runtime = null
        releaseProjection()
        publish(stoppedState())
        stopSelf()
    }

    private fun publishRuntimeSnapshot(snapshot: PhoneControlRuntimeSnapshot) {
        val identity = Triple(snapshot.running, snapshot.phase, snapshot.code)
        if (identity != loggedRuntimeState) {
            loggedRuntimeState = identity
            Log.i(
                TAG,
                "runtime_state running=${snapshot.running} phase=${snapshot.phase.name.lowercase()} " +
                    "code=${snapshot.code.name.lowercase()}",
            )
        }
        val state = PhoneControlServiceState(
            running = snapshot.running,
            phase = snapshot.phase,
            code = snapshot.code,
            userMessage = localizedRuntimeMessage(snapshot.code),
            inputCaption = snapshot.inputCaption,
            outputCaption = snapshot.outputCaption,
            listeningLevel = snapshot.listeningLevel,
            orbStateLabel = snapshot.orbStateLabel,
            orbIconOverride = snapshot.orbIconOverride,
            authorityGuidance = authoritySetup.guidance,
        )
        publish(state)
        if (!snapshot.running && snapshot.phase == PhoneControlRuntimePhase.ERROR) {
            stopReason = "runtime_error:${snapshot.code.name.lowercase()}"
            preserveFailureOnDestroy = true
            runtime = null
            stopSelf()
        }
    }

    private fun publish(next: PhoneControlServiceState) {
        val previousMessage = mutableState.value.notificationMessage()
        val nextMessage = next.notificationMessage()
        mutableState.value = next
        runCatching { overlayController.onState(next) }
            .onFailure { Log.e(TAG, "overlay_state_sink_failed", it) }
        if (previousMessage != nextMessage) {
            sessionNotification.update(nextMessage)
        }
    }

    private fun enterForeground() {
        sessionNotification.enterForeground(phoneControlString(R.string.phone_control_status_starting))
    }

    private fun localizedRuntimeMessage(code: PhoneControlRuntimeCode): String = phoneControlString(
        when (code) {
            PhoneControlRuntimeCode.STOPPED -> R.string.phone_control_status_stopped
            PhoneControlRuntimeCode.STARTING -> R.string.phone_control_status_starting
            PhoneControlRuntimeCode.CONNECTING -> R.string.phone_control_status_connecting
            PhoneControlRuntimeCode.READY -> R.string.phone_control_status_ready
            PhoneControlRuntimeCode.WORKING -> R.string.phone_control_status_working
            PhoneControlRuntimeCode.FINALIZING -> R.string.phone_control_status_finalizing
            PhoneControlRuntimeCode.RECONNECTING -> R.string.phone_control_status_reconnecting
            PhoneControlRuntimeCode.ACCESSIBILITY_UNAVAILABLE ->
                R.string.phone_control_status_accessibility_unavailable
            PhoneControlRuntimeCode.SCREEN_CAPTURE_FAILED ->
                R.string.phone_control_status_capture_failed
            PhoneControlRuntimeCode.SCREEN_SHARE_REQUIRED ->
                R.string.phone_control_status_projection_required
            PhoneControlRuntimeCode.API_KEY_REQUIRED ->
                R.string.phone_control_status_api_key_required
            PhoneControlRuntimeCode.CONFIGURATION_FAILED ->
                R.string.phone_control_status_configuration_failed
            PhoneControlRuntimeCode.MICROPHONE_FAILED ->
                R.string.phone_control_status_microphone_failed
            PhoneControlRuntimeCode.TRANSPORT_FAILED ->
                R.string.phone_control_status_transport_failed
            PhoneControlRuntimeCode.RUNTIME_FAILED -> R.string.phone_control_status_runtime_failed
        },
    )

    private fun stoppedState() = PhoneControlServiceState(
        running = false,
        phase = PhoneControlRuntimePhase.STOPPED,
        code = PhoneControlRuntimeCode.STOPPED,
        userMessage = phoneControlString(R.string.phone_control_status_stopped),
    )

    companion object {
        private const val TAG = "SGTPhoneControlService"
        private const val AUTHORITY_SETUP_RESUME_DELAY_MS = 750L
        private const val ACTION_START = "dev.screengoated.toolbox.mobile.phonecontrol.START"
        private const val ACTION_STOP = "dev.screengoated.toolbox.mobile.phonecontrol.STOP"
        private const val ACTION_ATTACH_PROJECTION =
            "dev.screengoated.toolbox.mobile.phonecontrol.ATTACH_PROJECTION"
        private const val ACTION_AUTHORITY_SETUP_PROGRESS =
            "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_SETUP_PROGRESS"
        private const val ACTION_AUTHORITY_SETUP_CLEAR =
            "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_SETUP_CLEAR"
        private const val EXTRA_STOP_SOURCE = "dev.screengoated.toolbox.mobile.phonecontrol.STOP_SOURCE"
        private val mutableState = mutableStateOf(stoppedPhoneControlServiceState())
        internal val state: State<PhoneControlServiceState> = mutableState

        internal fun start(
            context: Context,
            grant: PhoneControlProjectionGrant,
        ): Boolean = tryStartForegroundService(
            context,
            Intent(context, PhoneControlService::class.java)
                .setAction(ACTION_START)
                .putExtra(PROJECTION_RESULT_CODE_EXTRA, grant.resultCode)
                .putExtra(PROJECTION_DATA_EXTRA, Intent(grant.data)),
            "PhoneControlService",
        )

        internal fun attachProjection(
            context: Context,
            grant: PhoneControlProjectionGrant,
        ): Boolean = runCatching {
            context.startService(
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_ATTACH_PROJECTION)
                    .putExtra(PROJECTION_RESULT_CODE_EXTRA, grant.resultCode)
                    .putExtra(PROJECTION_DATA_EXTRA, Intent(grant.data)),
            )
            true
        }.getOrDefault(false)

        internal val captureSuspended: Boolean
            get() = state.value.running &&
                PhoneControlProtectedCheckpointRegistry.hasActiveCheckpoint()

        fun stop(context: Context) {
            dispatchStop(context, source = "app")
        }

        internal fun reportAuthoritySetup(
            context: Context,
            providerId: String,
            guidance: String,
            requestAutomation: Boolean,
            captureHandoffAfterAutomation: Boolean,
        ) {
            if (!state.value.running) return
            context.startService(
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_AUTHORITY_SETUP_PROGRESS)
                    .putExtra(EXTRA_AUTHORITY_PROVIDER_ID, providerId)
                    .putExtra(EXTRA_AUTHORITY_GUIDANCE, guidance)
                    .putExtra(EXTRA_AUTHORITY_AUTOMATION_REQUESTED, requestAutomation)
                    .putExtra(
                        EXTRA_CAPTURE_HANDOFF_AFTER_AUTOMATION,
                        captureHandoffAfterAutomation,
                    ),
            )
        }

        internal fun clearAuthoritySetup(context: Context, providerId: String? = null) {
            PhoneControlSetupNotification.clear(context)
            if (!state.value.running) return
            context.startService(
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_AUTHORITY_SETUP_CLEAR)
                    .putExtra(EXTRA_AUTHORITY_PROVIDER_ID, providerId.orEmpty()),
            )
        }

        private fun dispatchStop(context: Context, source: String) {
            context.startService(
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_STOP)
                    .putExtra(EXTRA_STOP_SOURCE, source),
            )
        }

    }
}
