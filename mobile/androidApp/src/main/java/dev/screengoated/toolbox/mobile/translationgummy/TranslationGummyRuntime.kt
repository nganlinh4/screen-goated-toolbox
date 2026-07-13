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
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.openGeminiLiveReadySession
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
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
    private var lastLocalSpeechAtMs: Long = 0L

    fun start(scope: CoroutineScope) {
        if (sessionJob?.isActive == true) {
            Log.d(TAG, "start ignored because session is already active")
            return
        }
        sessionJob = scope.launch {
            runLoop()
        }
    }

    fun restart(scope: CoroutineScope) {
        Log.d(TAG, "restart requested")
        stop()
        start(scope)
    }

    fun stop() {
        Log.d(TAG, "stop requested")
        sessionJob?.cancel()
        sessionJob = null
        clearSpeakerTurnState()
        resetLocalInputTurnState()
        localBargeInPending = false
        resetBargeInCandidateState()
        audioPlayer.stopImmediate()
        repository.finalizeActiveTranscripts(SystemClock.elapsedRealtime())
        repository.markStopped()
    }

    private suspend fun runLoop() {
        val applied = repository.currentAppliedConfig()
        if (!applied.isValid()) {
            Log.w(TAG, "runLoop aborted because applied config is invalid")
            repository.markNotConfigured()
            return
        }

        debugSessionOrdinal += 1L
        val debugSessionId = "tg-${debugSessionOrdinal}-${SystemClock.elapsedRealtime()}"
        Log.d(
            TAG,
            "runLoop starting sessionId=$debugSessionId first=${applied.first.language}/${applied.first.accent}/${applied.first.tone} second=${applied.second.language}/${applied.second.accent}/${applied.second.tone}",
        )

        var attempt = 0
        while (currentCoroutineContext().isActive) {
            val apiKey = repository.currentApiKey()
            val modelName = repository.currentGeminiModel()
            val voiceName = repository.currentGeminiVoice().ifBlank { DEFAULT_VOICE_NAME }
            if (apiKey.isBlank()) {
                Log.w(TAG, "runLoop aborting sessionId=$debugSessionId because apiKey is blank")
                repository.fail(repository.localeText().translationGummyApiKeyRequired)
                return
            }

            repository.markConnecting(reconnecting = attempt > 0)
            try {
                Log.d(
                    TAG,
                    "runLoop connecting sessionId=$debugSessionId attempt=$attempt model=$modelName voice=$voiceName",
                )
                runSession(debugSessionId, apiKey, applied, modelName, voiceName)
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
                repository.fail(repository.localeText().translationGummyConnectionLost)
                delay((1_000L * (attempt + 1)).coerceAtMost(5_000L))
                attempt += 1
            }
        }
    }

    private suspend fun runSession(
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
        openGeminiLiveReadySession(httpClient, apiKey, setupPayload).use { session ->
            repository.markReady()
            Log.d(TAG, "setup complete sessionId=$debugSessionId")

            coroutineScope {
                audioPlayer.beginCommunicationSession()
                val outboundAudio = LinkedBlockingDeque<ShortArray>()
                val captureJob = launch(Dispatchers.IO) {
                    audioCaptureController.open(
                        config = LiveSessionConfig(sourceMode = SourceMode.MIC),
                        onRms = repository::updateVisualizerLevel,
                    ).collect { chunk ->
                        debugCaptureChunk(debugSessionId, chunk)
                        outboundAudio.offer(chunk)
                    }
                }

                try {
                    while (currentCoroutineContext().isActive) {
                        refreshSpeakerTurnState(debugSessionId)
                        flushOutboundAudio(debugSessionId, session, outboundAudio)
                        when (val result = session.receive(50)) {
                            GeminiLiveReceiveResult.TimedOut -> Unit
                            is GeminiLiveReceiveResult.Frame -> handleSocketFrame(debugSessionId, result.frame)
                            is GeminiLiveReceiveResult.Unparsed -> Unit
                            is GeminiLiveReceiveResult.Failed -> throw GeminiLiveSessionException(result.failure)
                            is GeminiLiveReceiveResult.Closed -> {
                                throw IOException("Translation Gummy websocket closed.")
                            }
                        }
                    }
                } finally {
                    captureJob.cancel()
                    val streamEndSent = session.trySend(buildTranslationGummyAudioStreamEndPayload())
                    if (!streamEndSent) {
                        Log.w(TAG, "audio stream end rejected during cleanup sessionId=$debugSessionId")
                    }
                    localBargeInPending = false
                    resetBargeInCandidateState()
                    resetLocalInputTurnState()
                    clearSpeakerTurnState()
                    audioPlayer.stopImmediate()
                    Log.d(TAG, "audio stream end finalized sessionId=$debugSessionId sent=$streamEndSent")
                    repository.finalizeActiveTranscripts(SystemClock.elapsedRealtime())
                    audioPlayer.endCommunicationSession()
                }
            }
        }
    }

    private fun flushOutboundAudio(
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
                if (!tryBargeInDuringSpeakerTurn(debugSessionId, session, chunk)) {
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
            processOutboundAudioChunk(debugSessionId, session, chunk)
        }
    }

    private fun handleSocketFrame(
        debugSessionId: String,
        frame: GeminiLiveServerFrame,
    ) {
        val update = parseTranslationGummySocketUpdate(frame)
        val nowMs = SystemClock.elapsedRealtime()
        update.inputTranscript?.let {
            debugInputTranscript(debugSessionId, it, update.turnComplete, nowMs)
            repository.upsertTranscript(TranslationGummyTranscriptRole.INPUT, it, update.turnComplete, nowMs)
        }
        update.outputTranscript?.let {
            Log.d(
                TAG,
                "outputTranscript sessionId=$debugSessionId final=${update.turnComplete} text=${it.debugSnippet()}",
            )
            repository.upsertTranscript(TranslationGummyTranscriptRole.OUTPUT, it, update.turnComplete, nowMs)
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
                audioPlayer.playPcm24k(
                    pcm24k = pcm24k,
                    speedPercent = 100,
                    volumePercent = repository.currentOutputVolumePercent(),
                )
            }
        }
        if (update.interrupted) {
            Log.w(TAG, "server interrupted turn sessionId=$debugSessionId")
            localBargeInPending = false
            resetBargeInCandidateState()
            clearSpeakerTurnState(resumeDelayMs = INTERRUPT_MIC_COOLDOWN_MS, nowMs = nowMs)
            audioPlayer.stopImmediate()
            repository.finalizeActiveTranscripts(nowMs)
        } else if (update.turnComplete) {
            Log.d(TAG, "turn complete sessionId=$debugSessionId")
            localBargeInPending = false
            resetBargeInCandidateState()
            speakerTurnCompletedAtMs = nowMs
            repository.finalizeActiveTranscripts(nowMs)
        }
        if (update.goAway) {
            throw IOException("Server GoAway — reconnecting")
        }
    }

    private fun processOutboundAudioChunk(
        debugSessionId: String,
        session: GeminiLiveReadySession,
        chunk: ShortArray,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ) {
        val rms = chunk.rmsLevel()
        val isSpeech = rms >= LOCAL_INPUT_SPEECH_RMS
        if (isSpeech) {
            if (!localInputTurnActive) {
                val preRollCount = localInputPreRoll.size
                localInputTurnActive = true
                Log.d(
                    TAG,
                    "localInputTurnOpened sessionId=$debugSessionId rms=${"%.4f".format(rms)} preRollChunks=$preRollCount",
                )
                while (localInputPreRoll.isNotEmpty()) {
                    sendOutboundAudioChunk(debugSessionId, session, localInputPreRoll.removeFirst())
                }
            }
            lastLocalSpeechAtMs = nowMs
            sendOutboundAudioChunk(debugSessionId, session, chunk)
            return
        }

        if (!localInputTurnActive) {
            bufferLocalInputPreRoll(chunk)
            return
        }

        val silenceMs = ageMs(lastLocalSpeechAtMs, nowMs)
        if (silenceMs <= LOCAL_INPUT_TRAILING_AUDIO_MS) {
            sendOutboundAudioChunk(debugSessionId, session, chunk)
            return
        }

        if (silenceMs >= LOCAL_INPUT_END_SILENCE_MS) {
            if (!session.trySend(buildTranslationGummyAudioStreamEndPayload())) {
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
        processOutboundAudioChunk(debugSessionId, session, chunk, nowMs)
        return true
    }

    private fun sendOutboundAudioChunk(
        debugSessionId: String,
        session: GeminiLiveReadySession,
        chunk: ShortArray,
    ) {
        debugOutboundAudio(debugSessionId, chunk, 1)
        if (!session.trySend(buildTranslationGummyAudioPayload(chunk))) {
            throw IOException("Translation Gummy audio payload was rejected.")
        }
    }

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
