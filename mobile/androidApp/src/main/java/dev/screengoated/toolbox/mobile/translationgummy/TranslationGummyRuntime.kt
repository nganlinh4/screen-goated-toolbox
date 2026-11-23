package dev.screengoated.toolbox.mobile.translationgummy

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.model.TtsDefaults
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionException
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEvent
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecyclePhase
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionLifecycle
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSocketConnector
import dev.screengoated.toolbox.mobile.shared.live.OkHttpGeminiLiveSocketConnector
import dev.screengoated.toolbox.mobile.shared.live.geminiLiveWebSocketRequest
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.openGeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import dev.screengoated.toolbox.mobile.shared.live.geminiLiveCloseRetryable
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.CoroutineStart
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import java.io.IOException
import java.util.ArrayDeque
import java.util.concurrent.LinkedBlockingDeque

class TranslationGummyRuntime(
    context: Context,
    projectionConsentStore: ProjectionConsentStore,
    private val repository: TranslationGummyRepository,
    private val httpClient: OkHttpClient,
) {
    private val audioCaptureController = AudioCaptureController(context, projectionConsentStore)
    internal val audioPlayer = AudioTrackPlayer(context)
    private val bargeInDetector = TranslationGummyBargeInDetector()
    private var sessionJob: Job? = null
    private val sessionLock = Any()
    private val audioLease = Mutex()
    private val owner = TranslationGummyOwner()
    private var debugSessionOrdinal: Long = 0L
    internal var lastStuckPlaybackLogAtMs: Long = 0L
    private var speakerTurnActive = false
    private var speakerTurnCompletedAtMs: Long = 0L
    private var speakerTurnLastAudioAtMs: Long = 0L
    private var micResumeAfterMs: Long = 0L
    private var localBargeInPending = false
    private var bargeInCandidateCount = 0
    private var lastBargeInCandidateAtMs: Long = 0L
    private val localInputPreRoll = ArrayDeque<ShortArray>()
    private var localInputTurnActive = false
    private val inputActivity = dev.screengoated.toolbox.mobile.shared.live.SpeechActivity()
    private var lastLocalSpeechAtMs: Long = 0L

    fun start(scope: CoroutineScope) {
        synchronized(sessionLock) {
            if (sessionJob?.isActive == true) return
            val generation = owner.replace()
            val next = scope.launch(start = CoroutineStart.LAZY) {
                audioLease.withLock {
                    if (owner.isCurrent(generation)) runLoop(generation)
                }
            }
            sessionJob = next
            next.start()
        }
    }

    fun restart(scope: CoroutineScope) {
        Log.d(TAG, "restart requested")
        stop()
        start(scope)
    }

    fun stop() {
        Log.d(TAG, "stop requested")
        synchronized(sessionLock) {
            val generation = owner.replace()
            sessionJob?.cancel()
            owner.ifCurrent(generation) {
                repository.finalizeActiveTranscripts(SystemClock.elapsedRealtime())
                repository.markStopped()
            }
        }
    }

    private suspend fun runLoop(ownerId: Long) {
        val applied = repository.currentAppliedConfig()
        if (!applied.isValid()) {
            Log.w(TAG, "runLoop aborted because applied config is invalid")
            owner.ifCurrent(ownerId) { repository.markNotConfigured() }
            return
        }

        debugSessionOrdinal += 1L
        val debugSessionId = "tg-${debugSessionOrdinal}-${SystemClock.elapsedRealtime()}"
        Log.d(
            TAG,
            "runLoop starting sessionId=$debugSessionId first=${applied.first.language}/${applied.first.accent}/${applied.first.tone} second=${applied.second.language}/${applied.second.accent}/${applied.second.tone}",
        )

        val lifecycle = gummyLifecycle()
        lifecycle.reduce(SystemClock.elapsedRealtime(), GeminiLiveLifecycleEvent.Start)
        while (currentCoroutineContext().isActive) {
            if (!owner.isCurrent(ownerId)) return
            if (lifecycle.state.phase == GeminiLiveLifecyclePhase.BACKING_OFF) {
                val remaining = (lifecycle.state.reconnectDeadlineMs ?: SystemClock.elapsedRealtime()) - SystemClock.elapsedRealtime()
                if (remaining > 0) delay(remaining)
                lifecycle.reduce(SystemClock.elapsedRealtime(), GeminiLiveLifecycleEvent.Tick)
                continue
            }
            val apiKey = repository.currentApiKey()
            val modelName = repository.currentGeminiModel()
            val voiceName = repository.currentGeminiVoice().ifBlank { DEFAULT_VOICE_NAME }
            if (apiKey.isBlank()) {
                Log.w(TAG, "runLoop aborting sessionId=$debugSessionId because apiKey is blank")
                owner.ifCurrent(ownerId) { repository.fail(repository.localeText().translationGummyApiKeyRequired) }
                return
            }

            val attempt = lifecycle.state.reconnectAttempt
            owner.ifCurrent(ownerId) { repository.markConnecting(reconnecting = attempt > 0) }
            try {
                Log.d(
                    TAG,
                    "runLoop connecting sessionId=$debugSessionId attempt=$attempt model=$modelName voice=$voiceName",
                )
                runSession(ownerId, lifecycle, debugSessionId, apiKey, applied, modelName, voiceName)
                Log.d(TAG, "runLoop session completed normally sessionId=$debugSessionId")
                break
            } catch (cancelled: CancellationException) {
                Log.d(TAG, "runLoop cancelled sessionId=$debugSessionId")
                throw cancelled
            } catch (error: Throwable) {
                if (!currentCoroutineContext().isActive) {
                    break
                }
                Log.w(
                    TAG,
                    "runLoop failure sessionId=$debugSessionId attempt=$attempt message=${error.message}",
                    error,
                )
                val retryable = when (error) {
                    is GeminiLiveSessionException -> gummyFailureRetryable(error.failure)
                    is TranslationGummyClosed -> geminiLiveCloseRetryable(error.code)
                    else -> gummyTransportRetryable(error)
                }
                lifecycle.reduce(SystemClock.elapsedRealtime(), GeminiLiveLifecycleEvent.TransportFailure(lifecycle.state.generation, retryable))
                if (lifecycle.state.phase == GeminiLiveLifecyclePhase.FAILED) {
                    val detail = ((error as? GeminiLiveSessionException)?.failure as? GeminiLiveSessionFailure.Server)?.message
                    owner.ifCurrent(ownerId) { repository.fail(detail?.take(1024)?.ifBlank { null } ?: repository.localeText().translationGummyConnectionLost) }
                    return
                }
                owner.ifCurrent(ownerId) { repository.markConnecting(reconnecting = true) }
            }
        }
    }

    private suspend fun runSession(
        ownerId: Long,
        lifecycle: GeminiLiveSessionLifecycle,
        debugSessionId: String,
        apiKey: String,
        config: TranslationGummyConfig,
        modelName: String,
        voiceName: String,
    ) = withContext(Dispatchers.IO) {
        val setupPayload = buildTranslationGummySetupPayload(
            model = modelName,
            instruction = config.buildSystemInstruction(),
            voiceName = voiceName,
        )
        val connector = OkHttpGeminiLiveSocketConnector(httpClient, geminiLiveWebSocketRequest(apiKey))
        val ownedConnector = GeminiLiveSocketConnector { listener ->
            owner.ifCurrent(ownerId) { connector.connect(listener) }
                ?: throw CancellationException("Translation Gummy session retired")
        }
        openGeminiLiveConnectedSession(ownedConnector).use { connected ->
          connected.activate(setupPayload, dispatchSetup = { send -> owner.ifCurrent(ownerId) { send() } ?: false }).use { session ->
            val generation = lifecycle.state.generation
            if (!owner.isCurrent(ownerId)) return@withContext
            lifecycle.reduce(SystemClock.elapsedRealtime(), GeminiLiveLifecycleEvent.SocketOpened(generation))
            lifecycle.reduce(SystemClock.elapsedRealtime(), GeminiLiveLifecycleEvent.Frame(GeminiLiveLifecycleFrame(generation, setupComplete = true)))
            owner.ifCurrent(ownerId) { repository.markReady() }
            Log.d(TAG, "setup complete sessionId=$debugSessionId")

            coroutineScope {
                if (!owner.isCurrent(ownerId)) return@coroutineScope
                audioPlayer.beginCommunicationSession()
                if (!owner.isCurrent(ownerId)) {
                    audioPlayer.endCommunicationSession()
                    return@coroutineScope
                }
                val outboundAudio = LinkedBlockingDeque<ShortArray>()
                val captureJob = launch(Dispatchers.IO) {
                    audioCaptureController.open(
                        config = LiveSessionConfig(sourceMode = SourceMode.MIC),
                        onRms = { level -> owner.ifCurrent(ownerId) { repository.updateVisualizerLevel(level) } },
                        startMicrophone = { start ->
                            if (!owner.isCurrent(ownerId)) false else {
                                start()
                                owner.isCurrent(ownerId)
                            }
                        },
                    ).collect { chunk ->
                        debugCaptureChunk(debugSessionId, chunk)
                        owner.ifCurrent(ownerId) { outboundAudio.offer(chunk) }
                    }
                }

                try {
                    while (currentCoroutineContext().isActive) {
                        if (!owner.isCurrent(ownerId)) return@coroutineScope
                        refreshSpeakerTurnState(debugSessionId)
                        flushOutboundAudio(ownerId, debugSessionId, session, outboundAudio)
                        when (val result = session.receive(50)) {
                            GeminiLiveReceiveResult.TimedOut -> Unit
                            is GeminiLiveReceiveResult.Frame -> {
                                if (!owner.isCurrent(ownerId)) return@coroutineScope
                                lifecycle.observeGummyFrame(SystemClock.elapsedRealtime(), generation, result.frame)
                                handleSocketFrame(ownerId, debugSessionId, result.frame)
                            }
                            is GeminiLiveReceiveResult.Unparsed -> Unit
                            is GeminiLiveReceiveResult.Failed -> throw GeminiLiveSessionException(result.failure)
                            is GeminiLiveReceiveResult.Closed -> {
                                throw TranslationGummyClosed(result.code)
                            }
                        }
                    }
                } finally {
                    captureJob.cancel()
                    val streamEndSent = owner.ifCurrent(ownerId) { session.trySend(buildTranslationGummyAudioStreamEndPayload()) } ?: false
                    if (!streamEndSent) {
                        Log.w(TAG, "audio stream end rejected during cleanup sessionId=$debugSessionId")
                    }
                    localBargeInPending = false
                    resetBargeInCandidateState()
                    resetLocalInputTurnState()
                    clearSpeakerTurnState()
                    owner.ifCurrent(ownerId) { repository.finalizeActiveTranscripts(SystemClock.elapsedRealtime()) }
                    audioPlayer.stopImmediate()
                    Log.d(TAG, "audio stream end finalized sessionId=$debugSessionId sent=$streamEndSent")
                    audioPlayer.endCommunicationSession()
                }
            }
          }
        }
    }

    private fun flushOutboundAudio(
        ownerId: Long,
        debugSessionId: String,
        session: GeminiLiveReadySession,
        outboundAudio: LinkedBlockingDeque<ShortArray>,
    ) {
        val combined = ArrayList<ShortArray>()
        while (true) {
            val chunk = outboundAudio.poll() ?: break
            combined.add(chunk)
        }
        if (combined.isEmpty()) {
            return
        }
        val suppressionReason = outboundMicSuppressionReason()
        if (suppressionReason == "speaker_turn_active") {
            combined.forEach { chunk ->
                if (!tryBargeInDuringSpeakerTurn(ownerId, debugSessionId, session, chunk)) {
                    debugDroppedOutboundAudio(debugSessionId, chunk, 1, suppressionReason)
                }
            }
            return
        }
        if (suppressionReason != null) {
            resetLocalInputTurnState()
            combined.forEach { chunk ->
                debugDroppedOutboundAudio(debugSessionId, chunk, 1, suppressionReason)
            }
            return
        }
        for (chunk in combined) {
            processOutboundAudioChunk(ownerId, debugSessionId, session, chunk)
        }
    }

    private suspend fun handleSocketFrame(
        ownerId: Long,
        debugSessionId: String,
        frame: GeminiLiveServerFrame,
    ) {
        val update = parseTranslationGummySocketUpdate(frame)
        val nowMs = SystemClock.elapsedRealtime()
        update.inputTranscript?.let {
            debugInputTranscript(debugSessionId, it, update.turnComplete, nowMs)
            publishTranscript(ownerId, TranslationGummyTranscriptRole.INPUT, it, update.turnComplete, nowMs)
        }
        update.outputTranscript?.let {
            Log.d(
                TAG,
                "outputTranscript sessionId=$debugSessionId final=${update.turnComplete} text=${it.debugSnippet()}",
            )
            publishTranscript(ownerId, TranslationGummyTranscriptRole.OUTPUT, it, update.turnComplete, nowMs)
        }
        if (update.audioChunks.isNotEmpty() && localBargeInPending) {
            val bytes = update.audioChunks.sumOf(ByteArray::size)
            Log.w(TAG, "droppingModelAudioDuringBargeIn sessionId=$debugSessionId bytes=$bytes")
        } else {
            update.audioChunks.forEach { pcm24k ->
                if (!speakerTurnActive) {
                    resetLocalInputTurnState()
                }
                bargeInDetector.onPlaybackChunk(pcm24k, nowMs)
                speakerTurnActive = true
                speakerTurnLastAudioAtMs = nowMs
                speakerTurnCompletedAtMs = 0L
                Log.d(
                    TAG,
                    "audioChunk sessionId=$debugSessionId bytes=${pcm24k.size} transcriptPresent=${update.outputTranscript != null}",
                )
                audioPlayer.playPcm24kInterruptibly(
                    pcm24k = pcm24k,
                    speedPercent = 100,
                    volumePercent = repository.currentOutputVolumePercent(),
                    isCurrent = { owner.isCurrent(ownerId) },
                    dispatch = { action ->
                        if (!owner.isCurrent(ownerId)) false else {
                            action()
                            owner.isCurrent(ownerId)
                        }
                    },
                )
            }
        }
        if (update.interrupted) {
            Log.w(TAG, "server interrupted turn sessionId=$debugSessionId")
            localBargeInPending = false
            resetBargeInCandidateState()
            clearSpeakerTurnState(resumeDelayMs = INTERRUPT_MIC_COOLDOWN_MS, nowMs = nowMs)
            audioPlayer.stopImmediate()
            owner.ifCurrent(ownerId) { repository.finalizeActiveTranscripts(nowMs) }
        } else if (update.turnComplete) {
            Log.d(TAG, "turn complete sessionId=$debugSessionId")
            localBargeInPending = false
            resetBargeInCandidateState()
            speakerTurnCompletedAtMs = nowMs
            owner.ifCurrent(ownerId) { repository.finalizeActiveTranscripts(nowMs) }
        }
        if (update.goAway) {
            throw IOException("Server GoAway — reconnecting")
        }
    }

    private suspend fun publishTranscript(ownerId: Long, role: TranslationGummyTranscriptRole, text: String, final: Boolean, nowMs: Long) {
        while (owner.isCurrent(ownerId)) {
            val prepared = repository.prepareTranscript(role, text, final, nowMs) ?: return
            val item = prepared.updated[prepared.index]
            val language = if (role == TranslationGummyTranscriptRole.OUTPUT && (item.lang.isBlank() || item.isFinal)) {
                TranslationGummyClassifier.detect(item.text, repository::detectTranscriptLanguage, repository::fallbackTranscriptLanguage).ifBlank { item.lang }
            } else if (role == TranslationGummyTranscriptRole.OUTPUT) item.lang else ""
            val committed = owner.ifCurrent(ownerId) { repository.commitTranscript(prepared, language) } ?: return
            if (committed) return
        }
    }

    private fun processOutboundAudioChunk(
        ownerId: Long,
        debugSessionId: String,
        session: GeminiLiveReadySession,
        chunk: ShortArray,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ) {
        val rms = chunk.rmsLevel()
        val isSpeech = inputActivity.observe(rms.toDouble(), nowMs)
        if (isSpeech) {
            if (!localInputTurnActive) {
                val preRollCount = localInputPreRoll.size
                localInputTurnActive = true
                Log.d(
                    TAG,
                    "localInputTurnOpened sessionId=$debugSessionId rms=${"%.4f".format(rms)} preRollChunks=$preRollCount",
                )
                while (localInputPreRoll.isNotEmpty()) {
                    sendOutboundAudioChunk(ownerId, debugSessionId, session, localInputPreRoll.removeFirst())
                }
            }
            lastLocalSpeechAtMs = nowMs
            sendOutboundAudioChunk(ownerId, debugSessionId, session, chunk)
            return
        }

        if (!localInputTurnActive) {
            sendOutboundAudioChunk(ownerId, debugSessionId, session, chunk)
            return
        }

        val silenceMs = ageMs(lastLocalSpeechAtMs, nowMs)
        sendOutboundAudioChunk(ownerId, debugSessionId, session, chunk)
        if (silenceMs <= LOCAL_INPUT_TRAILING_AUDIO_MS) {
            return
        }

        if (silenceMs >= LOCAL_INPUT_END_SILENCE_MS) {
            if (!sendOwned(ownerId, session, buildTranslationGummyAudioStreamEndPayload())) {
                throw IOException("Translation Gummy audio stream end payload was rejected.")
            }
            Log.d(
                TAG,
                "localInputTurnFlushed sessionId=$debugSessionId silenceMs=$silenceMs",
            )
            resetLocalInputTurnState()
        }
    }

    private fun tryBargeInDuringSpeakerTurn(
        ownerId: Long,
        debugSessionId: String,
        session: GeminiLiveReadySession,
        chunk: ShortArray,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ): Boolean {
        val snapshot = audioPlayer.debugSnapshot()
        val decision = bargeInDetector.evaluate(chunk, snapshot.communicationDevice, nowMs)
        if (decision.shouldBufferCandidate) {
            bufferLocalInputPreRoll(chunk)
        }
        if (!decision.shouldInterruptPlayback) {
            if (!decision.shouldBufferCandidate || decision.likelyEcho) {
                resetBargeInCandidateState()
            }
            return false
        }
        val requiresConfirmation = decision.route.contains("speaker")
        if (requiresConfirmation) {
            if (ageMs(lastBargeInCandidateAtMs, nowMs) > BARGE_IN_CONFIRMATION_WINDOW_MS) {
                bargeInCandidateCount = 0
            }
            bargeInCandidateCount += 1
            lastBargeInCandidateAtMs = nowMs
            if (bargeInCandidateCount < SPEAKER_BARGE_IN_CONFIRMATION_CHUNKS) {
                bufferLocalInputPreRoll(chunk)
                Log.d(
                    TAG,
                    "bargeInCandidate sessionId=$debugSessionId route=${decision.route} confirmations=$bargeInCandidateCount/$SPEAKER_BARGE_IN_CONFIRMATION_CHUNKS micRms=${"%.4f".format(decision.micRms)} refRms=${"%.4f".format(decision.referenceRms)} correlation=${"%.3f".format(decision.correlation)} lagMs=${decision.lagMs}",
                )
                return true
            }
        }
        resetBargeInCandidateState()
        Log.w(
            TAG,
            "bargeInDetected sessionId=$debugSessionId route=${decision.route} micRms=${"%.4f".format(decision.micRms)} refRms=${"%.4f".format(decision.referenceRms)} correlation=${"%.3f".format(decision.correlation)} lagMs=${decision.lagMs}",
        )
        localBargeInPending = true
        audioPlayer.stopImmediate()
        clearSpeakerTurnState(resumeDelayMs = 0L, nowMs = nowMs)
        processOutboundAudioChunk(ownerId, debugSessionId, session, chunk, nowMs)
        return true
    }

    private fun sendOutboundAudioChunk(
        ownerId: Long,
        debugSessionId: String,
        session: GeminiLiveReadySession,
        chunk: ShortArray,
    ) {
        debugOutboundAudio(debugSessionId, chunk, 1)
        if (!sendOwned(ownerId, session, buildTranslationGummyAudioPayload(chunk))) {
            throw IOException("Translation Gummy audio payload was rejected.")
        }
    }

    private fun sendOwned(ownerId: Long, session: GeminiLiveReadySession, payload: String): Boolean =
        owner.ifCurrent(ownerId) { session.trySend(payload) } ?: throw CancellationException("Translation Gummy session retired")

    private fun bufferLocalInputPreRoll(chunk: ShortArray) {
        if (localInputPreRoll.size == LOCAL_INPUT_PREROLL_CHUNKS) {
            localInputPreRoll.removeFirst()
        }
        localInputPreRoll.addLast(chunk)
    }

    private fun resetLocalInputTurnState() {
        localInputTurnActive = false
        lastLocalSpeechAtMs = 0L
        localInputPreRoll.clear()
    }

    private fun resetBargeInCandidateState() {
        bargeInCandidateCount = 0
        lastBargeInCandidateAtMs = 0L
    }

    private fun outboundMicSuppressionReason(
        nowMs: Long = SystemClock.elapsedRealtime(),
    ): String? {
        if (speakerTurnActive) {
            return "speaker_turn_active"
        }
        if (nowMs < micResumeAfterMs) {
            return "speaker_turn_cooldown"
        }
        return null
    }

    private fun refreshSpeakerTurnState(
        debugSessionId: String,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ) {
        if (!speakerTurnActive) {
            return
        }
        val snapshot = audioPlayer.debugSnapshot()
        val turnCompleted = speakerTurnCompletedAtMs > 0L
        val idleSinceLastWriteMs = ageMs(snapshot.lastWriteCompletedAtMs, nowMs)
        val idleSinceLastAudioChunkMs = ageMs(speakerTurnLastAudioAtMs, nowMs)
        val shouldCloseTurn = turnCompleted &&
            snapshot.pendingFrames == 0L &&
            idleSinceLastWriteMs >= PLAYBACK_DRAIN_IDLE_MS
        val shouldRecoverStuckPlayback = snapshot.pendingFrames == 0L &&
            idleSinceLastAudioChunkMs >= PLAYBACK_STUCK_RECOVERY_MS

        if (!shouldCloseTurn && !shouldRecoverStuckPlayback) {
            return
        }

        val reason = when {
            shouldCloseTurn -> "turn_complete_drain"
            else -> "stuck_playback_recovery"
        }
        Log.w(
            TAG,
            "closingSpeakerTurn sessionId=$debugSessionId reason=$reason pendingFrames=${snapshot.pendingFrames} active=${snapshot.active} playState=${snapshot.playState} trackState=${snapshot.trackState} lastAudioChunkAgoMs=$idleSinceLastAudioChunkMs lastWriteAgoMs=$idleSinceLastWriteMs",
        )
        audioPlayer.stopImmediate()
        clearSpeakerTurnState(resumeDelayMs = OUTBOUND_MIC_SUPPRESSION_WINDOW_MS, nowMs = nowMs)
    }

    private fun clearSpeakerTurnState(
        resumeDelayMs: Long = OUTBOUND_MIC_SUPPRESSION_WINDOW_MS,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ) {
        speakerTurnActive = false
        speakerTurnCompletedAtMs = 0L
        speakerTurnLastAudioAtMs = 0L
        resetBargeInCandidateState()
        bargeInDetector.clear()
        micResumeAfterMs = nowMs + resumeDelayMs
    }

    internal fun ageMs(
        eventAtMs: Long,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ): Long {
        if (eventAtMs <= 0L) {
            return Long.MAX_VALUE
        }
        return (nowMs - eventAtMs).coerceAtLeast(0L)
    }

    internal fun String.debugSnippet(): String {
        return trim().replace('\n', ' ').take(DEBUG_TEXT_LIMIT)
    }

    internal companion object {
        internal const val TAG = "SGTTranslationGummy"
        private const val DEFAULT_VOICE_NAME = TtsDefaults.DEFAULT_TTS_GEMINI_VOICE
        private const val OUTBOUND_MIC_SUPPRESSION_WINDOW_MS = 600L
        private const val INTERRUPT_MIC_COOLDOWN_MS = 250L
        private const val PLAYBACK_DRAIN_IDLE_MS = 200L
        private const val PLAYBACK_STUCK_RECOVERY_MS = 750L
        private const val DEBUG_TEXT_LIMIT = 160
        // Widened to internal so the cross-platform VAD parity test can assert
        // against the real runtime constants. See .claude/parity/translation-gummy.md
        // and parity-fixtures/translation-gummy/vad-contract.json.
        internal const val LOCAL_INPUT_SPEECH_RMS = 0.015f
        internal const val LOCAL_INPUT_TRAILING_AUDIO_MS = 180L
        internal const val LOCAL_INPUT_END_SILENCE_MS = 420L
        internal const val LOCAL_INPUT_PREROLL_CHUNKS = 2
        private const val SPEAKER_BARGE_IN_CONFIRMATION_CHUNKS = 2
        private const val BARGE_IN_CONFIRMATION_WINDOW_MS = 220L
    }
}
