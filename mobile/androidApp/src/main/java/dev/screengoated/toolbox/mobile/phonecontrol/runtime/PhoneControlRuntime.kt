package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import android.content.Context
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.phonecontrol.memory.PhoneControlMemoryRepository
import dev.screengoated.toolbox.mobile.phonecontrol.provider.browser.PhoneControlBrowserLifecycle
import dev.screengoated.toolbox.mobile.phonecontrol.session.PhoneControlContractAssets
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlAudioPayload
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlSetupPayload
import dev.screengoated.toolbox.mobile.phonecontrol.tools.PhoneControlToolDispatchBoundary
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveClassifiedError
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleAdapter
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleConnection
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
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
    private val capabilityContext: String,
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
    private val userInterfaceGoals = PhoneControlUserInterfaceGoalQueue()
    private val audioCapture = AudioCaptureController(appContext, projectionConsentStore)
    private val audioPlayer = AudioTrackPlayer(appContext)
    private val playbackGate = GenerationPlaybackGate()
    private val outboundSender = PhoneControlOutboundSender()

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
        onListeningLevel = { level -> inputActivity.observe(level) },
    )

    private val statusPublisher = PhoneControlRuntimeStatusPublisher(
        observer = observer,
        isTransportReady = transportReady::get,
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
            outputCaption = { text -> statusPublisher.updateCaption(output = text) },
            assistantContentEnabled = { !userInterfaceGoals.conversationSurfaceSuppressed },
            orbPresentation = statusPublisher::updateOrbPresentation,
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
                    "browser_turn_cleanup requested=${receipt.requested} " +
                        "verified=${receipt.verifiedClosed} unresolved=${receipt.unresolved}",
                )
            }
        },
    )
    private val inputActivity by lazy {
        PhoneControlRuntimeInputActivity(
            onFirstSpeech = { Log.i(TAG, "microphone_speech_detected") },
            onSpeechActive = {
                val output = audioPlayer.debugSnapshot()
                turnCoordinator.userSpeechStarted(output.active || output.pendingFrames > 0L)
            },
            onLevel = statusPublisher::updateListeningLevel,
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
        abandonProtocolSession = turnCoordinator::abandonProtocolSession,
        purgeSessionOutbound = ::purgeSessionOutbound,
        discardUntilFreshConnection = discardOutboundUntilFreshConnection,
    )
    private val lifecycle = GeminiLiveLifecycleAdapter(
        policy = GeminiLiveLifecyclePolicy.agent(),
        clockMs = SystemClock::elapsedRealtime,
        openConnectedSession = {
            openGeminiLiveConnectedSession(httpClient = httpClient, apiKey = apiKey.trim())
        },
        setupPayload = {
            buildPhoneControlSetupPayload(
                assets = contractAssets,
                capabilityContext = capabilityContext,
                voiceName = voiceName,
                resumptionHandle = PhoneControlResumptionPolicy.usableHandle(resumptionHandle),
            )
        },
        onEffect = lifecycleEffects::observe,
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

    fun suspendVisualEvidence() {
        visualEvidence.suspend(
            screenFrames,
            screenRefreshRequests,
            controlPayloads,
            screenReconciliationQueued,
        )
    }

    fun resumeVisualEvidence() = visualEvidence.resume(screenRefreshRequests)

    private suspend fun runTransportLoop() {
        var readyGeneration = 0L
        while (currentCoroutineContext().isActive && running.get()) {
            turnCoordinator.drainToolCompletions()
            settleUserInterfaceGoalIfReady()
            if (abortOverflowedProtocolSession()) continue
            val connection = lifecycle.ensureReady()
            if (lifecycle.state.phase == GeminiLiveLifecyclePhase.FAILED) break
            if (connection == null) {
                transportReady.set(false)
                delay(TRANSPORT_POLL_MS)
                continue
            }
            bindReadyConnection(connection, readyGeneration != connection.generation)
            readyGeneration = connection.generation
            val sent = flushOutbound(connection.session)
            if (!sent) {
                lifecycle.transportFailed(connection.generation)
                continue
            }
            lifecycle.updateWorkState(
                pendingWorkCount = turnCoordinator.pendingWorkCount.toLong(),
                bufferedInputCount = bufferedAudio.get().coerceAtLeast(0).toLong(),
                userSpeaking = inputActivity.isActive(SystemClock.elapsedRealtime()),
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

    private fun bindReadyConnection(
        connection: GeminiLiveLifecycleConnection,
        becameReady: Boolean,
    ) {
        if (becameReady && discardOutboundUntilFreshConnection.compareAndSet(true, false)) {
            purgeSessionOutbound()
            turnCoordinator.freshProtocolSessionBound()
        }
        transportReady.set(true)
        if (becameReady) {
            Log.i(TAG, "transport_ready generation=${connection.generation}")
            screenRefreshRequests.trySend(Unit)
            statusPublisher.publishTurnPhase(turnCoordinator.phase)
        }
    }

    private suspend fun abortOverflowedProtocolSession(): Boolean {
        if (!protocolAbortRequested.compareAndSet(true, false)) return false
        transportReady.set(false)
        resumptionHandle = null
        discardOutboundUntilFreshConnection.set(true)
        turnCoordinator.abandonProtocolSession()
        purgeSessionOutbound()
        val generation = lifecycle.activeConnection?.generation ?: lifecycle.state.generation
        Log.e(TAG, "protocol_overflow_abandon generation=$generation")
        if (generation > 0L && lifecycle.state.phase != GeminiLiveLifecyclePhase.FAILED) {
            lifecycle.transportFailed(generation)
        }
        return true
    }

    private fun purgeSessionOutbound() {
        controlPayloads.abandonSession()
        while (screenFrames.tryReceive().isSuccess) {
            // A fresh connection must receive a fresh screen observation.
        }
        while (audioFrames.tryReceive().isSuccess) {
            bufferedAudio.updateAndGet { (it - 1).coerceAtLeast(0) }
        }
        while (screenRefreshRequests.tryReceive().isSuccess) {
            // The fresh connection requests its own capture after binding.
        }
        screenReconciliationQueued.set(false)
    }

    private fun flushOutbound(session: GeminiLiveReadySession): Boolean {
        visualEvidence.discardPending(screenFrames, controlPayloads)
        while (true) {
            val queued = controlPayloads.next() ?: break
            if (!outboundSender.send(
                    session = session,
                    payload = queued.payload,
                    kind = queued.kind,
                    pendingWork = turnCoordinator.pendingWorkCount,
                    turnPhase = turnCoordinator.phase,
                    utf8Bytes = queued.utf8Bytes,
                )
            ) {
                return false
            }
            controlPayloads.markSent(queued)
        }
        if (visualEvidence.enabled.get() &&
            canSendAmbientScreen(turnCoordinator.pendingWorkCount)
        ) {
            val screenPayload = screenFrames.tryReceive().getOrNull()
            if (screenPayload != null) {
                if (!outboundSender.send(
                        session = session,
                        payload = screenPayload,
                        kind = PhoneControlOutboundKind.AMBIENT_SCREEN,
                        pendingWork = turnCoordinator.pendingWorkCount,
                        turnPhase = turnCoordinator.phase,
                    )
                ) return false
                val sent = screenFramesSent.incrementAndGet()
                if (sent == 1L) Log.i(TAG, "screen_uplink_started")
                lifecycle.inputSent()
                if (screenReconciliationQueued.compareAndSet(true, false)) {
                    turnCoordinator.freshScreenEvidenceDelivered()
                }
            }
        }
        if (!userInterfaceGoals.flushRuntimeGoal(
                session,
                turnCoordinator.phase,
                turnCoordinator.pendingWorkCount,
                inputActivity.isActive(SystemClock.elapsedRealtime()),
                outboundSender,
            ) {
                lifecycle.inputSent()
                Log.i(TAG, "ui_goal_sent")
            }
        ) return false
        repeat(MAX_AUDIO_FRAMES_PER_FLUSH) {
            val samples = audioFrames.tryReceive().getOrNull() ?: return@repeat
            bufferedAudio.updateAndGet { (it - 1).coerceAtLeast(0) }
            val payload = buildPhoneControlAudioPayload(samples)
            if (!outboundSender.send(
                    session = session,
                    payload = payload,
                    kind = PhoneControlOutboundKind.MICROPHONE_AUDIO,
                    pendingWork = turnCoordinator.pendingWorkCount,
                    turnPhase = turnCoordinator.phase,
                )
            ) return false
            val sent = audioFramesSent.incrementAndGet()
            if (sent == 1L) {
                Log.i(TAG, "audio_uplink_started samples_per_frame=${samples.size}")
            }
            lifecycle.inputSent()
            lifecycle.inputActivity()
        }
        return true
    }

    private suspend fun observeServerFrame(
        connection: GeminiLiveLifecycleConnection,
        frame: GeminiLiveServerFrame,
    ) {
        val received = serverFramesReceived.incrementAndGet()
        if (received == 1L) {
            Log.i(
                TAG,
                "server_activity_started content_present=${frame.contentCount > 0} " +
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
    }
}
