package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import android.content.Context
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.GeneratedPhoneControlContract
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRepository
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.PhoneControlBrowserLifecycle
import dev.screengoated.toolbox.mobile.phonecontrol.session.PhoneControlContractAssets
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlSetupPayload
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatchBoundary
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveClassifiedError
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleAdapter
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleConnection
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackGate
import dev.screengoated.toolbox.mobile.shared.live.openGeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.NonCancellable
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.channels.BufferOverflow
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
internal class PhoneControlRuntime(
    context: Context,
    private val httpClient: OkHttpClient,
    projectionConsentStore: ProjectionConsentStore,
    private val apiKey: String,
    private val voiceName: String,
    private val contractAssets: PhoneControlContractAssets,
    private val capabilityContext: () -> String,
    memoryRepository: PhoneControlMemoryRepository,
    dispatchBoundary: PhoneControlToolDispatchBoundary,
    observer: PhoneControlRuntimeObserver,
    additionalTurnRecorders: List<PhoneControlTurnRecorder> = emptyList(),
    private val onUserInterfaceGoalFinished: (PhoneControlUiGoalCompletion) -> Unit = {},
) {
    private val appContext = context.applicationContext
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val running = AtomicBoolean(false)
    private val stopRequested = AtomicBoolean(false)
    private val resourcesReleased = AtomicBoolean(false)
    private val transportReady = AtomicBoolean(false)
    private val visualEvidence = PhoneControlRuntimeVisualEvidence()
    private val protocolAbortRequested = AtomicBoolean(false)
    private val discardOutboundUntilFreshConnection = AtomicBoolean(false)
    private val screenReconciliationQueued = AtomicBoolean(false)
    private val bufferedAudio = AtomicInteger(0)
    private val audioFramesSent = AtomicLong(0L)
    private val screenFramesSent = AtomicLong(0L)
    private val serverFramesReceived = AtomicLong(0L)
    private var lastServerFrameMs = 0L
    private val protectedCheckpointGoalId = AtomicLong(NO_PROTECTED_CHECKPOINT_GOAL)
    private val userInterfaceGoals = PhoneControlUserInterfaceGoalQueue()
    private val audioCapture = AudioCaptureController(appContext, projectionConsentStore)
    private val audioPlayer = AudioTrackPlayer(appContext)
    private val playbackGate = GenerationPlaybackGate()
    private val outboundSender = PhoneControlOutboundSender()
    private val setupSession = PhoneControlSetupSessionRuntime(
        onBegin = {
            purgeMicrophoneFrames()
            inputActivity.reset()
            playbackGate.interrupt(audioPlayer::stopImmediate)
            playback.discard()
        },
        onResetRequested = { screenRefreshRequests.trySend(Unit) },
        onInputResumed = { source ->
            inputActivity.reset()
            statusPublisher.clearConversation()
            statusPublisher.publishTurnPhase(turnCoordinator.phase)
            screenRefreshRequests.trySend(Unit)
            Log.i(TAG, "setup_session_state state=ready input_admitted=true source=$source")
        },
    )

    private val audioFrames = Channel<ShortArray>(
        capacity = MAX_BUFFERED_AUDIO_FRAMES,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
        onUndeliveredElement = { bufferedAudio.updateAndGet { (it - 1).coerceAtLeast(0) } },
    )
    private val screenFrames = Channel<String>(
        capacity = 1,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    private val screenRefreshRequests = Channel<Unit>(
        capacity = 1,
        onBufferOverflow = BufferOverflow.DROP_OLDEST,
    )
    private val uiGoalSubmission = PhoneControlRuntimeUiGoalSubmission(
        userInterfaceGoals,
        { running.get() && !resourcesReleased.get() },
        { screenRefreshRequests.trySend(Unit) },
    )
    private val controlPayloads = PhoneControlSessionPayloadQueue()
    private val playback = PhoneControlPlaybackQueue(MAX_BUFFERED_PLAYBACK_CHUNKS)
    private val audioPipelines = PhoneControlRuntimeAudioPipelines(
        audioCapture = audioCapture,
        audioPlayer = audioPlayer,
        playbackGate = playbackGate,
        audioFrames = audioFrames,
        bufferedAudio = bufferedAudio,
        playback = playback,
        inputAdmitted = setupSession::inputAdmitted,
        onListeningLevel = { level -> inputActivity.observe(level) },
    )

    private val statusPublisher = PhoneControlRuntimeStatusPublisher(
        observer = observer,
        isTransportReady = transportReady::get,
    )
    private val orbEmotion = PhoneControlOrbEmotionController(
        scope = scope,
        classifier = TaalasPhoneControlEmotionClassifier(httpClient),
        publishIcon = { icon ->
            statusPublisher.updateOrbPresentation(
                GeneratedPhoneControlContract.ORB_STATE_RESPONDING,
                icon,
            )
        },
    )
    @Volatile
    private var resumptionHandle: String? = null
    private val memoryTurnRecorder = PhoneControlMemoryTurnRecorder(memoryRepository)
    private val turnRecorder = PhoneControlPresentationAwareTurnRecorder(
        PhoneControlDiagnosticTurnRecorder(
            CompositePhoneControlTurnRecorder(
                listOf(memoryTurnRecorder) + additionalTurnRecorders,
            ),
        ),
        { userInterfaceGoals.conversationSurfaceSuppressed },
    )
    private val turnCoordinator = PhoneControlTurnCoordinator(
        executor = PhoneControlDispatcherToolExecutor(dispatchBoundary, scope),
        scope = scope,
        sink = PhoneControlRuntimeTurnSink(
            send = { payload ->
                offerPhoneControlPayload(
                    controlPayloads,
                    protocolAbortRequested,
                    payload,
                    PhoneControlOutboundKind.TOOL_RESPONSE,
                )
            },
            sendEvidence = { payload ->
                visualEvidence.offerExact(payload, screenFrames, controlPayloads)
            },
            play = { bytes -> playback.offer(playbackGate.tag(bytes)) },
            interrupt = { playbackGate.interrupt(audioPlayer::stopImmediate) },
            discard = playback::discard,
            inputCaption = { text ->
                if (!userInterfaceGoals.conversationSurfaceSuppressed) statusPublisher.updateCaption(input = text)
            },
            outputCaption = { text ->
                orbEmotion.observeReply(text)
                statusPublisher.updateCaption(output = text)
            },
            assistantContentEnabled = { !userInterfaceGoals.conversationSurfaceSuppressed },
            orbPresentation = { stateLabel, iconOverride ->
                orbEmotion.observePresentation(stateLabel)
                statusPublisher.updateOrbPresentation(
                    stateLabel,
                    iconOverride,
                    preserveCurrentIconOnNull =
                        stateLabel == GeneratedPhoneControlContract.ORB_STATE_RESPONDING,
                )
            },
            phase = statusPublisher::publishTurnPhase,
            refresh = { screenRefreshRequests.trySend(Unit) },
            abortProtocol = { protocolAbortRequested.set(true) },
        ),
        recorder = turnRecorder,
        cleanup = PhoneControlTurnCleanup { turnId ->
            scope.launch(Dispatchers.IO) {
                val receipt = PhoneControlBrowserLifecycle.retireTurn(turnId)
                Log.i(
                    TAG,
                    "browser_turn_cleanup requested_count=${receipt.requested} " +
                        "verified_count=${receipt.verifiedClosed} " +
                        "unresolved_count=${receipt.unresolved}",
                )
            }
        },
    )
    private val inputActivity by lazy {
        PhoneControlRuntimeInputActivity(
            onSpeechStarted = { epoch ->
                Log.i(TAG, "microphone_speech_started epoch=$epoch")
                val output = audioPlayer.debugSnapshot()
                val assistantPlaybackActive = output.active || output.pendingFrames > 0L
                turnCoordinator.userSpeechStarted(assistantPlaybackActive)
                if (!assistantPlaybackActive) screenRefreshRequests.trySend(Unit)
            },
            onSpeechEnded = { epoch, elapsedMs, audioFrames ->
                Log.i(
                    TAG,
                    "microphone_speech_ended epoch=$epoch elapsed_ms=$elapsedMs " +
                        "audio_frames=$audioFrames",
                )
            },
            onLevel = statusPublisher::updateListeningLevel,
        )
    }
    private val runtimeOutbound by lazy {
        PhoneControlRuntimeOutbound(
            visualEvidence = visualEvidence,
            audioFrames = audioFrames,
            bufferedAudio = bufferedAudio,
            controlPayloads = controlPayloads,
            screenFrames = screenFrames,
            screenReconciliationQueued = screenReconciliationQueued,
            sender = outboundSender,
            audioFramesSent = audioFramesSent,
            screenFramesSent = screenFramesSent,
            pendingWorkCount = turnCoordinator::pendingWorkCount,
            turnPhase = turnCoordinator::phase,
            microphoneInput = setupSession.inputGate,
            userSpeaking = {
                setupSession.inputAdmitted &&
                    inputActivity.isActive(SystemClock.elapsedRealtime())
            },
            userInterfaceGoals = userInterfaceGoals,
            onInputSent = lifecycle::inputSent,
            onInputActivity = lifecycle::inputActivity,
            onFreshScreenDelivered = turnCoordinator::freshScreenEvidenceDelivered,
        )
    }
    private val screenStreamer = PhoneControlScreenStreamer(
        running = running,
        transportReady = transportReady,
        visualEvidenceEnabled = visualEvidence.enabled,
        screenFrames = screenFrames,
        refreshRequests = screenRefreshRequests,
        reconciliationFrameQueued = screenReconciliationQueued,
        statusPublisher = statusPublisher,
        currentTurnPhase = { turnCoordinator.phase },
        pendingWorkCount = { turnCoordinator.pendingWorkCount },
    )
    private val lifecycleEffects = PhoneControlRuntimeLifecycleEffects(
        transportReady = transportReady,
        statusPublisher = statusPublisher,
        prepareReconnect = { controlPayloads.prepareReconnect(resumptionHandle) },
        retireTransportInterruptedTurn =
            turnCoordinator::retireTransportInterruptedTurn,
        abandonProtocolSession = turnCoordinator::abandonProtocolSession,
        purgeSessionOutbound = ::purgeSessionOutbound,
        discardUntilFreshConnection = discardOutboundUntilFreshConnection,
    )
    private val lifecycle = GeminiLiveLifecycleAdapter(
        policy = GeminiLiveLifecyclePolicy.agent(),
        clockMs = SystemClock::elapsedRealtime,
        openConnectedSession = {
            val startedMs = SystemClock.elapsedRealtime()
            openGeminiLiveConnectedSession(httpClient = httpClient, apiKey = apiKey.trim())
                .also {
                    Log.i(
                        TAG,
                        "live_session_opened " +
                            "model=${GeneratedLiveModelCatalog.GEMINI_LIVE_API_MODEL_3_1} " +
                            "open_ms=${SystemClock.elapsedRealtime() - startedMs}",
                    )
                }
        },
        setupPayload = {
            buildPhoneControlSetupPayload(
                assets = contractAssets,
                capabilityContext = capabilityContext(),
                voiceName = voiceName,
                resumptionHandle = PhoneControlResumptionPolicy.usableHandle(resumptionHandle),
            )
        },
        onEffect = lifecycleEffects::observe,
    )
    private val sessionBoundary = PhoneControlRuntimeSessionBoundary(
        lifecycle = lifecycle,
        transportReady = transportReady,
        discardOutboundUntilFreshConnection = discardOutboundUntilFreshConnection,
        setupSession = setupSession,
        userInterfaceGoals = userInterfaceGoals,
        turnCoordinator = turnCoordinator,
        statusPublisher = statusPublisher,
        clearResumptionHandle = { resumptionHandle = null },
        purgeSessionOutbound = ::purgeSessionOutbound,
        requestScreenRefresh = { screenRefreshRequests.trySend(Unit) },
    )

    private var sessionJob: Job? = null
    fun start(): Boolean {
        if (apiKey.isBlank()) {
            statusPublisher.publish(
                running = false,
                phase = PhoneControlRuntimePhase.ERROR,
                code = PhoneControlRuntimeCode.API_KEY_REQUIRED,
                message = "Add a Gemini API key before starting Phone Control.",
            )
            releaseResources()
            return false
        }
        if (!running.compareAndSet(false, true)) return true
        stopRequested.set(false)
        statusPublisher.publish(
            phase = PhoneControlRuntimePhase.STARTING,
            code = PhoneControlRuntimeCode.STARTING,
            message = "Starting microphone and agent session…",
        )
        audioPlayer.beginCommunicationSession()
        sessionJob = scope.launch {
            try {
                coroutineScope {
                    launch { audioPipelines.captureMicrophone() }
                    launch { screenStreamer.run() }
                    launch { audioPipelines.playOutput() }
                    runTransportLoop()
                }
            } catch (cancelled: CancellationException) {
                if (!stopRequested.get()) throw cancelled
            } catch (failure: PhoneControlRuntimeFailure) {
                Log.e(TAG, "runtime_failed code=${failure.code.name.lowercase()}", failure.cause)
                statusPublisher.publish(
                    running = false,
                    phase = PhoneControlRuntimePhase.ERROR,
                    code = failure.code,
                    message = failure.message,
                )
            } catch (error: Throwable) {
                Log.e(TAG, "runtime_failed code=transport_failed", error)
                statusPublisher.publish(
                    running = false,
                    phase = PhoneControlRuntimePhase.ERROR,
                    code = PhoneControlRuntimeCode.RUNTIME_FAILED,
                    message = error.message ?: "Phone Control stopped after a runtime failure.",
                )
            } finally {
                withContext(NonCancellable) { lifecycle.cancel() }
                releaseResources()
            }
        }
        return true
    }

    fun stop() {
        stopRequested.set(true)
        running.set(false)
        sessionJob?.cancel()
        sessionJob = null
        releaseResources()
    }

    fun submitUserInterfaceGoal(
        text: String,
        presentation: PhoneControlUiGoalPresentation =
            PhoneControlUiGoalPresentation.CONVERSATIONAL,
    ): Long? = uiGoalSubmission.submit(text, presentation)

    fun submitExternalGoal(text: String): Long? = uiGoalSubmission.submit(
        text = text,
        presentation = PhoneControlUiGoalPresentation.CONVERSATIONAL,
        replacePending = false,
    )

    fun requestProtectedCheckpointBoundary(goalId: Long): Boolean {
        if (goalId <= 0L || !running.get() || resourcesReleased.get()) return false
        while (true) {
            val current = protectedCheckpointGoalId.get()
            if (current == goalId) return true
            if (current != NO_PROTECTED_CHECKPOINT_GOAL) return false
            if (protectedCheckpointGoalId.compareAndSet(current, goalId)) return true
        }
    }

    fun suspendVisualEvidence() {
        visualEvidence.suspend(
            screenFrames,
            screenRefreshRequests,
            controlPayloads,
            screenReconciliationQueued,
        )
    }

    fun resumeVisualEvidence() = visualEvidence.resume(screenRefreshRequests)

    fun beginAuthoritySetupSession() {
        setupSession.begin()
    }

    fun finishAuthoritySetupSession(waitForAnnouncement: Boolean) {
        setupSession.finish(waitForAnnouncement)
    }

    fun authoritySetupAnnouncementFinished() {
        setupSession.observeAnnouncementFinished()
    }

    private suspend fun runTransportLoop() {
        var readyGeneration = 0L
        while (currentCoroutineContext().isActive && running.get()) {
            if (setupSession.consumeResetRequest()) {
                resetAuthoritySetupConversation()
                continue
            }
            turnCoordinator.drainToolCompletions()
            settleProtectedCheckpointBoundary()
            settleUserInterfaceGoalIfReady()
            if (sessionBoundary.abortOverflowedProtocolSession(protocolAbortRequested)) continue
            val connection = lifecycle.ensureReady()
            if (lifecycle.state.phase == GeminiLiveLifecyclePhase.FAILED) break
            if (connection == null) {
                transportReady.set(false)
                delay(TRANSPORT_POLL_MS)
                continue
            }
            sessionBoundary.bindReady(connection, readyGeneration != connection.generation)
            readyGeneration = connection.generation
            val sent = runtimeOutbound.flush(connection.session)
            if (!sent) {
                lifecycle.transportFailed(connection.generation)
                continue
            }
            lifecycle.updateWorkState(
                pendingWorkCount = turnCoordinator.pendingWorkCount.toLong(),
                bufferedInputCount = bufferedAudio.get().coerceAtLeast(0).toLong(),
                userSpeaking = setupSession.inputAdmitted &&
                    inputActivity.isActive(SystemClock.elapsedRealtime()),
            )
            when (val received = connection.session.receive(RECEIVE_POLL_MS)) {
                GeminiLiveReceiveResult.TimedOut -> lifecycle.tick()
                is GeminiLiveReceiveResult.Frame -> observeServerFrame(connection, received.frame)
                is GeminiLiveReceiveResult.Unparsed ->
                    Log.w(TAG, "unparsed_server_frame format=${received.wireFormat}")
                is GeminiLiveReceiveResult.Closed -> {
                    val reason = Log.normalizeDiagnosticField(
                        received.reason.orEmpty().ifBlank { "none" },
                        MAX_TRANSPORT_REASON_CHARS,
                    )
                    val queued = controlPayloads.snapshot()
                    Log.w(
                        TAG,
                        "transport_closed code=${received.code ?: -1} reason=$reason " +
                            "pending=${turnCoordinator.pendingWorkCount} " +
                            "control_count=${queued.count} control_bytes=${queued.utf8Bytes} " +
                            "outbound_tail=${outboundSender.describe()}",
                    )
                    lifecycle.transportFailed(connection.generation)
                }
                is GeminiLiveReceiveResult.Failed -> observeReceiveFailure(connection, received.failure)
            }
        }
        if (!stopRequested.get() && lifecycle.state.phase == GeminiLiveLifecyclePhase.FAILED) {
            throw PhoneControlRuntimeFailure(
                PhoneControlRuntimeCode.TRANSPORT_FAILED,
                "Phone Control could not restore the Gemini Live connection.",
            )
        }
    }

    private suspend fun resetAuthoritySetupConversation() {
        sessionBoundary.resetSetupConversation()
    }

    private fun purgeSessionOutbound() {
        controlPayloads.abandonSession()
        while (screenFrames.tryReceive().isSuccess) {
            // A fresh connection must receive a fresh screen observation.
        }
        purgeMicrophoneFrames()
        while (screenRefreshRequests.tryReceive().isSuccess) {
            // The fresh connection requests its own capture after binding.
        }
        screenReconciliationQueued.set(false)
    }

    private fun purgeMicrophoneFrames() {
        while (audioFrames.tryReceive().isSuccess) {
            bufferedAudio.updateAndGet { (it - 1).coerceAtLeast(0) }
        }
    }

    private suspend fun observeServerFrame(
        connection: GeminiLiveLifecycleConnection,
        frame: GeminiLiveServerFrame,
    ) {
        val received = serverFramesReceived.incrementAndGet()
        val nowMs = SystemClock.elapsedRealtime()
        val previousFrameMs = lastServerFrameMs
        lastServerFrameMs = nowMs
        if (received == 1L) {
            Log.i(
                TAG,
                "server_activity_started content_present=${frame.contentCount > 0} " +
                    "tools=${frame.toolCallIds.isNotEmpty()}",
            )
        }
        // Quantifies how long the stream went quiet between server frames. A long gap here is
        // model/transport time, not app time: every tool dispatch completes in well under a second.
        if (previousFrameMs != 0L && nowMs - previousFrameMs >= SERVER_FRAME_GAP_LOG_MS) {
            Log.w(
                TAG,
                "server_frame_gap gap_ms=${nowMs - previousFrameMs} frame=$received " +
                    "content_present=${frame.contentCount > 0} " +
                    "tools=${frame.toolCallIds.isNotEmpty()}",
            )
        }
        frame.sessionResumption?.let { update ->
            resumptionHandle = update.handle
                ?.takeIf { update.resumable }
                ?.let(PhoneControlResumptionPolicy::usableHandle)
        }
        val effects = lifecycle.observeFrame(
            GeminiLiveLifecycleFrame(
                generation = connection.generation,
                contentCount = frame.contentCount,
                setupComplete = frame.setupComplete,
                turnComplete = frame.turnComplete,
                generationComplete = frame.generationComplete,
                interrupted = frame.interrupted,
                goAwayTimeLeftMs = if (frame.goAway) frame.goAwayTimeLeftMs ?: 0L else null,
                toolCallIds = frame.toolCallIds,
                toolCancellationIds = frame.toolCancellationIds.orEmpty(),
                error = frame.error?.let {
                    GeminiLiveClassifiedError("server", frame.errorRetryable)
                },
            ),
        )
        val interruptedGoal = if (frame.interrupted) {
            userInterfaceGoals.observeTurnBoundary(interrupted = true)
        } else {
            null
        }
        turnCoordinator.handleFrame(frame, effects)
        interruptedGoal?.let(onUserInterfaceGoalFinished)
        if (!frame.interrupted && (frame.turnComplete || frame.generationComplete)) {
            userInterfaceGoals.observeTurnBoundary(frame.interrupted)
                ?.let(onUserInterfaceGoalFinished)
        }
    }

    private fun settleUserInterfaceGoalIfReady() {
        if (!userInterfaceGoals.awaitingSettlement) return
        val pendingFrames = audioPlayer.debugSnapshot().pendingFrames
        userInterfaceGoals.settle(
            phase = turnCoordinator.phase,
            pendingWorkCount = turnCoordinator.pendingWorkCount,
            playbackDrained = playback.isDrained(pendingFrames),
        )?.let(onUserInterfaceGoalFinished)
    }

    private fun settleProtectedCheckpointBoundary() {
        val goalId = protectedCheckpointGoalId.get()
        if (goalId == NO_PROTECTED_CHECKPOINT_GOAL ||
            !turnCoordinator.retireForProtectedCheckpoint()
        ) {
            return
        }
        val completion = userInterfaceGoals.retireForProtectedCheckpoint(goalId)
        protectedCheckpointGoalId.compareAndSet(goalId, NO_PROTECTED_CHECKPOINT_GOAL)
        completion?.let(onUserInterfaceGoalFinished)
    }

    private suspend fun observeReceiveFailure(
        connection: GeminiLiveLifecycleConnection,
        failure: GeminiLiveSessionFailure,
    ) {
        Log.w(TAG, "transport_receive_failed type=${failure::class.simpleName}")
        if (failure is GeminiLiveSessionFailure.Server) {
            lifecycle.serverError(connection.generation, failure.retryable)
        } else {
            lifecycle.transportFailed(connection.generation)
        }
    }

    private fun releaseResources() {
        if (!resourcesReleased.compareAndSet(false, true)) return
        Log.i(
            TAG,
            "runtime_released requested=${stopRequested.get()} " +
                "audio_frames=${audioFramesSent.get()} screen_frames=${screenFramesSent.get()} " +
                "server_frames=${serverFramesReceived.get()} speech=${inputActivity.speechObserved}",
        )
        running.set(false)
        transportReady.set(false)
        userInterfaceGoals.clear()
        protectedCheckpointGoalId.set(NO_PROTECTED_CHECKPOINT_GOAL)
        memoryTurnRecorder.finalizeSession()
        turnCoordinator.stop()
        audioFrames.close()
        screenFrames.close()
        screenRefreshRequests.close()
        controlPayloads.close()
        playback.close()
        playbackGate.interrupt(audioPlayer::stopImmediate)
        audioPlayer.endCommunicationSession()
        audioPlayer.release()
        scope.cancel()
        statusPublisher.clearListeningLevel()
        if (stopRequested.get()) {
            statusPublisher.publishStopped()
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
        const val SERVER_FRAME_GAP_LOG_MS = 3_000L
        const val NO_PROTECTED_CHECKPOINT_GOAL = -1L
    }
}
