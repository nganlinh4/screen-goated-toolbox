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
import dev.screengoated.toolbox.mobile.phonecontrol.capability.PhoneControlProviderRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.authority.PhoneControlProtectedCheckpointRegistry
import dev.screengoated.toolbox.mobile.phonecontrol.overlay.PhoneControlOverlayController
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
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlActivity
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerChoice
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerPreferences
import dev.screengoated.toolbox.mobile.phonecontrol.ui.PhoneControlPowerSelectionRoute
import dev.screengoated.toolbox.mobile.phonecontrol.ui.phoneControlPowerSelectionRoute
import dev.screengoated.toolbox.mobile.service.tryStartForegroundService
import dev.screengoated.toolbox.mobile.service.stopForUnavailableSecureStore

class PhoneControlService : Service() {
    internal val mainHandler = Handler(Looper.getMainLooper())
    private lateinit var overlayController: PhoneControlOverlayController
    private lateinit var sessionNotification: PhoneControlSessionNotification
    internal lateinit var authoritySetup: PhoneControlAuthoritySetupController
    private lateinit var authoritySetupAnnouncer: PhoneControlAuthoritySetupAnnouncer
    internal lateinit var protectedSetup: PhoneControlProtectedSetupCoordinator
    private lateinit var protectedHandoff: PhoneControlProtectedCheckpointHandoff
    internal val protectedCheckpoint = PhoneControlProtectedCheckpointController(onExit = ::submitPendingExternalGoal)
    internal var runtime: PhoneControlRuntime? = null
    internal var preserveFailureOnDestroy = false
    internal var projectionActive = false
    internal var stopReason = "system_destroy"
    private var teardownRequested = false
    private var retainedTeardownConsent: String? = null
    private val deferredStop = DeferredPhoneControlStop()
    private var loggedRuntimeState: Triple<Boolean, PhoneControlRuntimePhase, PhoneControlRuntimeCode>? = null
    override fun onBind(intent: Intent?): IBinder? = null

    override fun onCreate() {
        super.onCreate()
        if ((application as SgtMobileApplication).appContainerOrNull == null) {
            stopForUnavailableSecureStore()
            return
        }
        Log.i(TAG, "service_created")
        protectedSetup = PhoneControlProtectedSetupCoordinator(
            this,
            { providerId, guidance -> authoritySetup.replaceGuidance(providerId, guidance) },
            ::restoreRetainedProjection,
            ::reconcileRejectedProtectedGuidance,
        )
        protectedHandoff = PhoneControlProtectedCheckpointHandoff(
            context = this,
            setup = protectedSetup,
            checkpoint = protectedCheckpoint,
            runtime = { runtime },
            projectionActive = { projectionActive },
            releaseProjection = this::releaseProjection,
            continueNavigation = { providerId, previousGoalId, reason ->
                authoritySetup.continueProtectedNavigation(
                    providerId,
                    previousGoalId,
                    reason,
                )
            },
            navigationExhausted = { providerId, reason ->
                authoritySetup.onProtectedNavigationExhausted(providerId, reason)
            },
        )
        authoritySetupAnnouncer = PhoneControlAuthoritySetupAnnouncer(this) { runtime }
        authoritySetup = PhoneControlAuthoritySetupController(
            context = this,
            runtime = { runtime },
            publishGuidance = { guidance ->
                publish(mutableState.value.copy(authorityGuidance = guidance))
            },
            navigationContract = protectedSetup::navigationContract,
            protectedHandoff = protectedHandoff,
            onSetupSessionEvent = authoritySetupAnnouncer::onSetupSessionEvent,
        )
        overlayController = PhoneControlOverlayController(
            context = this,
            onDismiss = {
                stopRequested("orb_dismiss", PhoneControlProjectionDispatches.stopBoundary())
            },
            onPowerChoiceSelected = ::selectPowerChoice,
        )
        registerPhoneControlServiceClients(overlayController)
        sessionNotification = PhoneControlSessionNotification(service = this)
    }
    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        if (!::sessionNotification.isInitialized) {
            stopForUnavailableSecureStore(startId)
            return START_NOT_STICKY
        }
        if (teardownRequested) {
            deferredStop.defer(intent)
            stopSelf()
            return START_NOT_STICKY
        }
        when (intent?.action) {
            ACTION_PHONE_CONTROL_STOP -> {
                if (runtime == null &&
                    !promoteStopOnlyService(this, sessionNotification, intent, startId)) {
                    return START_NOT_STICKY
                }
                val source = intent.getStringExtra(EXTRA_STOP_SOURCE)
                    .orEmpty()
                    .ifBlank { "unknown" }
                Log.i(TAG, "service_command action=stop source=$source start_id=$startId")
                stopRequested(source, intent.getStringExtra(EXTRA_STOP_BOUNDARY),
                    intent.getLongExtra(EXTRA_STOP_SEQUENCE, PhoneControlProjectionDispatches.currentIntentSequence()))
            }
            ACTION_ATTACH_PROJECTION -> dispatchProjection(intent) { attachProjection(intent) }
            ACTION_EXTERNAL_GOAL -> submitPendingExternalGoal()
            ACTION_AUTHORITY_SETUP_PROGRESS -> authoritySetup.update(intent)
            ACTION_AUTHORITY_SETUP_CLEAR ->
                if (intent.getStringExtra(EXTRA_AUTHORITY_PROVIDER_ID).isNullOrBlank()) {
                    selectPowerChoice(PhoneControlPowerChoice.STANDARD)
                } else {
                    val providerId = intent.getStringExtra(EXTRA_AUTHORITY_PROVIDER_ID).orEmpty()
                    if (intent.getBooleanExtra(EXTRA_AUTHORITY_SETUP_VERIFIED_READY, false)) {
                        authoritySetup.completeIfVerifiedReady(providerId)
                    } else {
                        authoritySetup.clear(providerId)
                    }
                }
            else -> dispatchProjection(intent) {
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
    private inline fun dispatchProjection(intent: Intent?, effect: () -> Unit) {
        val accepted = runProjectionDispatch(
            intent?.getStringExtra(EXTRA_PROJECTION_DISPATCH),
            effect,
        )
        if (!accepted) {
            if (runtime == null && !PhoneControlProjectionDispatches.hasUnsettled()) {
                requestServiceTeardown()
            }
        }
    }
    override fun onDestroy() {
        if (!::sessionNotification.isInitialized) {
            super.onDestroy()
            return
        }
        PhoneControlProjectionDispatches.continueTeardown(retainedTeardownConsent)
        try {
            Log.i(TAG, "service_destroyed reason=$stopReason")
            protectedHandoff.close()
            authoritySetupAnnouncer.close()
            runtime?.stop()
            runtime = null
            PhoneControlBrowserLifecycle.close()
            releaseProjection()
            protectedCheckpoint.close()
            protectedSetup.close()
            if (!preserveFailureOnDestroy) publish(stoppedState())
            unregisterPhoneControlServiceClients(overlayController)
            overlayController.destroy()
        } finally {
            try {
                super.onDestroy()
            } finally {
                PhoneControlProjectionDispatches.endStop()
                deferredStop.replay(applicationContext)
            }
        }
    }
    internal fun requestServiceTeardown(startId: Int? = null, failedStopBoundary: String? = null): String? {
        val stopTeardown = failedStopBoundary?.let(PhoneControlProjectionDispatches::beginTeardownForStop)
        if (stopTeardown == null) PhoneControlProjectionDispatches.beginTeardown()
        retainedTeardownConsent = stopTeardown?.retainedConsent
        teardownRequested = true
        if (startId == null) stopSelf() else stopSelf(startId)
        return stopTeardown?.retryBoundary
    }
    override fun onConfigurationChanged(newConfig: Configuration) {
        super.onConfigurationChanged(newConfig)
        if (::overlayController.isInitialized) overlayController.onConfigurationChanged()
    }

    internal fun startRuntime() {
        try {
            val container = (application as SgtMobileApplication).appContainer
            val assets = PhoneControlContractAssets.load(this, container.json)
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
                requestServiceTeardown()
                return
            }
            lateinit var candidate: PhoneControlRuntime
            val checkpoints = PhoneControlSessionCheckpoints(this, assets)
            candidate = PhoneControlRuntime(
                context = this,
                httpClient = container.httpClient,
                projectionConsentStore = container.projectionConsentStore,
                apiKey = apiKey,
                voiceName = container.repository.currentGlobalTtsSettings().voice,
                contractAssets = assets,
                capabilityContext = {
                    PhoneControlProviderRegistry.probe(this).let { evidence ->
                        Log.i(TAG, evidence.diagnosticEvent())
                        evidence.modelContext()
                    }
                },
                memoryRepository = container.phoneControlMemoryRepository,
                dispatchBoundary = checkpoints.dispatcher,
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
                additionalTurnRecorders = checkpoints.recorders,
            )
            runtime = candidate
            if (!candidate.start()) {
                stopReason = "runtime_start_rejected"
                preserveFailureOnDestroy = true
                runtime = null
                requestServiceTeardown()
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
            requestServiceTeardown()
        }
    }

    private fun selectPowerChoice(choice: PhoneControlPowerChoice) {
        protectedHandoff.cancelIfAuthorityChanged(choice.elevatedProviderId)
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
        authoritySetup.clearForReadyProbe(reason = "retained_projection_restored")
        resumeSelectedAuthoritySetup(announceReady = true)
    }

    private fun reconcileRejectedProtectedGuidance(
        token: PhoneControlProtectedCheckpointToken,
    ) {
        if (protectedCheckpoint.activeToken != token) return
        val freshProjectionRequired = protectedCheckpoint.freshProjectionRequired
        // Reconcile against the user's current choice. A released projection
        // remains sealed until the existing resume route obtains a fresh grant.
        selectPowerChoice(
            PhoneControlPowerPreferences.current(this) ?: PhoneControlPowerChoice.STANDARD,
        )
        if (freshProjectionRequired && protectedCheckpoint.activeToken == token) {
            // A background activity launch may be suppressed. Leave an explicit
            // route to the fresh-capture prompt while visual evidence is sealed.
            PhoneControlSetupNotification.show(
                this,
                phoneControlString(R.string.phone_control_private_setup_pending),
                PhoneControlActivity.resumeCaptureIntent(this),
            )
        }
    }
    internal fun resumeSelectedAuthoritySetup(announceReady: Boolean = false) {
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
    private fun stopRequested(source: String, boundary: String?, sequence: Long = PhoneControlProjectionDispatches.currentIntentSequence()) {
        val hasLaterAction = sequence != PhoneControlProjectionDispatches.currentIntentSequence()
        when (val admission = PhoneControlProjectionDispatches.beginStop(boundary, sequence)) {
            PhoneControlStopAdmission.Stale -> {
                if (runtime == null && PhoneControlProjectionDispatches.current() == null) {
                    requestServiceTeardown(failedStopBoundary = boundary)
                }
                return
            }
            is PhoneControlStopAdmission.Wait -> {
                admission.dispatch.settled.invokeOnCompletion {
                    mainHandler.post { stopRequested(source, boundary, sequence) }
                }
                return
            }
            PhoneControlStopAdmission.Accepted -> Unit
        }
        try {
            if (!hasLaterAction) {
                PhoneControlSetupNotification.clearOrphanedReentry(this)
                PhoneControlSetupNotification.clearStopRetry(this)
            }
            PhoneControlSetupNotification.clearProjectionBusy(this)
            stopReason = "requested:$source"
            preserveFailureOnDestroy = false
            authoritySetup.clear(reason = "service_stop")
            PhoneControlExternalGoalRegistry.clear()
            runtime?.stop()
            runtime = null
            releaseProjection()
            publish(stoppedState())
        } finally {
            requestServiceTeardown()
        }
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
            userMessage = phoneControlRuntimeMessage(snapshot.code),
            inputCaption = snapshot.inputCaption,
            outputCaption = snapshot.outputCaption,
            listeningLevel = snapshot.listeningLevel,
            orbStateLabel = snapshot.orbStateLabel,
            orbIconOverride = snapshot.orbIconOverride,
            authorityGuidance = authoritySetup.guidance,
        )
        publish(state)
        if (snapshot.code == PhoneControlRuntimeCode.READY) {
            submitPendingExternalGoal()
        }
        if (!snapshot.running && snapshot.phase == PhoneControlRuntimePhase.ERROR) {
            stopReason = "runtime_error:${snapshot.code.name.lowercase()}"
            preserveFailureOnDestroy = true
            PhoneControlExternalGoalRegistry.clear()
            runtime = null
            requestServiceTeardown()
        }
    }

    internal fun publish(next: PhoneControlServiceState) {
        val previousMessage = mutableState.value.notificationMessage()
        val nextMessage = next.notificationMessage()
        mutableState.value = next
        runCatching { overlayController.onState(next) }
            .onFailure { Log.e(TAG, "overlay_state_sink_failed", it) }
        if (previousMessage != nextMessage) {
            sessionNotification.update(nextMessage)
        }
    }

    internal fun enterForeground() {
        sessionNotification.enterForeground(phoneControlString(R.string.phone_control_status_starting))
    }

    private fun submitPendingExternalGoal() {
        val candidate = runtime ?: return
        val result = PhoneControlExternalGoalRegistry.submitWhen(!protectedCheckpoint.active) { goal ->
            candidate.submitExternalGoal(goal) != null
        } ?: return
        Log.i(TAG, "external_goal_submit accepted=${result.accepted} chars=${result.chars}")
    }

    private fun stoppedState() = stoppedPhoneControlServiceState().copy(
        userMessage = phoneControlString(R.string.phone_control_status_stopped),
    )

    companion object {
        private const val TAG = "SGTPhoneControlService"
        private const val AUTHORITY_SETUP_RESUME_DELAY_MS = 750L
        private const val ACTION_START = "dev.screengoated.toolbox.mobile.phonecontrol.START"
        private const val ACTION_EXTERNAL_GOAL =
            "dev.screengoated.toolbox.mobile.phonecontrol.EXTERNAL_GOAL"
        private const val ACTION_ATTACH_PROJECTION =
            "dev.screengoated.toolbox.mobile.phonecontrol.ATTACH_PROJECTION"
        private const val EXTRA_PROJECTION_DISPATCH =
            "dev.screengoated.toolbox.mobile.phonecontrol.PROJECTION_DISPATCH"
        internal const val EXTRA_PROJECTION_CHECKPOINT_OWNER =
            "dev.screengoated.toolbox.mobile.phonecontrol.PROJECTION_CHECKPOINT_OWNER"
        private const val ACTION_AUTHORITY_SETUP_PROGRESS =
            "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_SETUP_PROGRESS"
        private const val ACTION_AUTHORITY_SETUP_CLEAR =
            "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_SETUP_CLEAR"
        private const val EXTRA_AUTHORITY_SETUP_VERIFIED_READY =
            "dev.screengoated.toolbox.mobile.phonecontrol.AUTHORITY_SETUP_VERIFIED_READY"
        private val mutableState = mutableStateOf(stoppedPhoneControlServiceState())
        internal val state: State<PhoneControlServiceState> = mutableState

        internal fun start(
            context: Context,
            grant: PhoneControlProjectionGrant,
            consentGeneration: String,
        ): PhoneControlProjectionDispatch? {
            val dispatch = newPhoneControlProjectionDispatch(context, consentGeneration)
                ?: return null
            val started = tryStartForegroundService(
                context,
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_START)
                    .putExtra(EXTRA_PROJECTION_DISPATCH, dispatch.id)
                    .putExtra(PROJECTION_RESULT_CODE_EXTRA, grant.resultCode)
                    .putExtra(PROJECTION_DATA_EXTRA, Intent(grant.data)),
                "PhoneControlService",
            )
            if (!started && PhoneControlProjectionDispatches.retireIfPending(dispatch)) {
                PhoneControlProjectionDispatches.releaseIfNoReentry(dispatch)
            }
            return dispatch.takeIf { started }
        }

        internal fun attachProjection(
            context: Context,
            grant: PhoneControlProjectionGrant,
            checkpointOwner: String,
            consentGeneration: String,
        ): PhoneControlProjectionDispatch? {
            val dispatch = newPhoneControlProjectionDispatch(context, consentGeneration)
                ?: return null
            val started = runCatching {
                context.startService(
                    Intent(context, PhoneControlService::class.java)
                        .setAction(ACTION_ATTACH_PROJECTION)
                        .putExtra(EXTRA_PROJECTION_DISPATCH, dispatch.id)
                        .putExtra(EXTRA_PROJECTION_CHECKPOINT_OWNER, checkpointOwner)
                        .putExtra(PROJECTION_RESULT_CODE_EXTRA, grant.resultCode)
                        .putExtra(PROJECTION_DATA_EXTRA, Intent(grant.data)),
                )
            }.isSuccess
            if (!started && PhoneControlProjectionDispatches.retireIfPending(dispatch)) {
                PhoneControlProjectionDispatches.releaseIfNoReentry(dispatch)
            }
            return dispatch.takeIf { started }
        }

        internal val captureSuspended: Boolean
            get() = state.value.running &&
                PhoneControlProtectedCheckpointRegistry.hasActiveCheckpoint()

        fun stop(context: Context) {
            dispatchStop(context, source = "app")
        }

        internal fun submitExternalGoal(context: Context) {
            if (!state.value.running) return
            context.startService(
                Intent(context, PhoneControlService::class.java).setAction(ACTION_EXTERNAL_GOAL),
            )
        }

        internal fun reportAuthoritySetup(
            context: Context,
            providerId: String,
            guidance: String,
            requestAutomation: Boolean,
            monitorProtectedCheckpoint: Boolean,
        ) {
            if (!state.value.running) return
            context.startService(
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_AUTHORITY_SETUP_PROGRESS)
                    .putExtra(EXTRA_AUTHORITY_PROVIDER_ID, providerId)
                    .putExtra(EXTRA_AUTHORITY_GUIDANCE, guidance)
                    .putExtra(EXTRA_AUTHORITY_AUTOMATION_REQUESTED, requestAutomation)
                    .putExtra(
                        EXTRA_PROTECTED_CHECKPOINT_MONITORING,
                        monitorProtectedCheckpoint,
                    ),
            )
        }

        internal fun clearAuthoritySetup(
            context: Context,
            providerId: String? = null,
            verifiedReady: Boolean = false,
        ) {
            PhoneControlSetupNotification.clear(context)
            if (!state.value.running) return
            context.startService(
                Intent(context, PhoneControlService::class.java)
                    .setAction(ACTION_AUTHORITY_SETUP_CLEAR)
                    .putExtra(EXTRA_AUTHORITY_PROVIDER_ID, providerId.orEmpty())
                    .putExtra(EXTRA_AUTHORITY_SETUP_VERIFIED_READY, verifiedReady),
            )
        }

        private fun dispatchStop(context: Context, source: String) {
            context.startService(phoneControlStopIntent(context, source))
        }

    }
}
