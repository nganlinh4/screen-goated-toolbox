package dev.screengoated.toolbox.mobile.service

import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleEffect
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveLifecycleFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveServerFrame
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionException
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.openGeminiLiveConnectedSession
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import java.util.concurrent.atomic.AtomicInteger

internal suspend fun GeminiS2sClient.runLiveTranslateContinuousSession(
    apiKey: String,
    model: String,
    sourceMode: SourceMode,
    player: AudioTrackPlayer,
    audioChunks: kotlinx.coroutines.flow.Flow<ShortArray>,
    settingsProvider: () -> GeminiS2sRuntimeSettings,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    logTag: String,
) = coroutineScope {
    val textState = LiveTranslateTextAccumulator()
    val stats = LiveTranslateStreamStats()
    val playbackEpoch = LiveTranslatePlaybackEpoch()
    val playbackQueue = Channel<LiveTranslatePlaybackChunk>(LIVE_TRANSLATE_PLAYBACK_QUEUE_CAPACITY)
    val pendingAudio = LiveTranslatePendingAudio(
        maxSamples = LIVE_TRANSLATE_PENDING_AUDIO_SAMPLES,
        frameSamples = FRAME_SAMPLES,
    )
    var lastHealthAtMs = SystemClock.elapsedRealtime()
    var userSpeaking = false
    lateinit var lifecycleAdapter: GeminiS2sLiveLifecycleAdapter
    lifecycleAdapter = GeminiS2sLiveLifecycleAdapter(
        clockMs = SystemClock::elapsedRealtime,
        openConnectedSession = {
            openGeminiLiveConnectedSession(httpClient, apiKey)
        },
        setupPayload = {
            val setupPayload = buildGeminiS2sSetupPayload(
                model = model,
                settings = settingsProvider(),
                contextText = "",
            )
            setupPayload
        },
        onEffect = { effect ->
            when (effect) {
                is GeminiLiveLifecycleEffect.OpenSocket -> Log.i(
                    logTag,
                    "continuous connect requested stream=${effect.generation}",
                )
                is GeminiLiveLifecycleEffect.SendSetup -> Log.i(
                    logTag,
                    "continuous socket opened; setup requested stream=${effect.generation} " +
                        "reconnect_attempts=${lifecycleAdapter.state.reconnectAttempt}",
                )
                is GeminiLiveLifecycleEffect.ScheduleReconnect -> Log.i(
                    logTag,
                    "continuous reconnect scheduled reason=${effect.reason.fixtureName} " +
                        "stream=${effect.generation} attempt=${effect.attempt} " +
                        "retry_ms=${effect.delayMs}",
                )
                is GeminiLiveLifecycleEffect.ReportFailure -> Log.w(
                    logTag,
                    "continuous lifecycle failed reason=${effect.reason}",
                )
                else -> Unit
            }
        },
    )
    if (sourceMode == SourceMode.MIC) {
        player.beginCommunicationSession()
    }
    val playbackJob = launch(Dispatchers.IO) {
        for (chunk in playbackQueue) {
            val volumePercent = settingsProvider().realtime.volumePercent
            playbackEpoch.playIfCurrent(chunk) { bytes ->
                player.playNativePcm24k(bytes, volumePercent)
            }
            stats.playbackQueuedChunks.updateAndGet { (it - 1).coerceAtLeast(0) }
        }
    }

    try {
        audioChunks.collect { chunk ->
            if (!currentCoroutineContext().isActive) {
                return@collect
            }
            pendingAudio.append(chunk)
            updateLiveTranslateWorkState(
                lifecycleAdapter = lifecycleAdapter,
                player = player,
                stats = stats,
                bufferedInputCount = pendingAudio.sampleCount.toLong(),
                userSpeaking = userSpeaking,
            )
            lifecycleAdapter.ensureReady() ?: return@collect
            while (pendingAudio.sampleCount > 0) {
                val active = lifecycleAdapter.activeConnection ?: return@collect
                val frame = pendingAudio.takeFirst() ?: break
                userSpeaking = rms(frame) >=
                    MIN_SPEECH_THRESHOLD * SPEECH_THRESHOLD_MULTIPLIER
                if (userSpeaking) {
                    lifecycleAdapter.inputActivity()
                }
                updateLiveTranslateWorkState(
                    lifecycleAdapter = lifecycleAdapter,
                    player = player,
                    stats = stats,
                    bufferedInputCount = pendingAudio.sampleCount.toLong(),
                    userSpeaking = userSpeaking,
                )
                val sendStartedAtMs = SystemClock.elapsedRealtime()
                val sent = active.session.trySend(buildGeminiS2sAudioPayload(frame))
                val sendElapsedMs = SystemClock.elapsedRealtime() - sendStartedAtMs
                stats.maxSendMs = maxOf(stats.maxSendMs, sendElapsedMs)
                if (sendElapsedMs >= LIVE_TRANSLATE_SLOW_SEND_LOG_MS) {
                    stats.slowSendCount++
                    Log.w(
                        logTag,
                        "continuous slow-send stream=${active.generation} elapsed_ms=$sendElapsedMs " +
                            "slow_count=${stats.slowSendCount} sent_chunks=${stats.sentChunks}",
                    )
                }
                if (!sent) {
                    pendingAudio.restoreFirst(frame)
                    val nowMs = SystemClock.elapsedRealtime()
                    val state = lifecycleAdapter.state
                    Log.w(
                        logTag,
                        "continuous send-failed stream=${active.generation} " +
                            "sent_chunks=${stats.sentChunks} " +
                            "socket_age_ms=${elapsedMs(nowMs, state.connectedAtMs)} " +
                            "since_server_ms=${elapsedMs(nowMs, state.lastServerActivityMs)}",
                    )
                    lifecycleAdapter.transportFailed(active.generation)
                    return@collect
                }
                stats.sentChunks++
                lifecycleAdapter.inputSent()
                val drainStartedAtMs = SystemClock.elapsedRealtime()
                val drained = drainLiveTranslateSocket(
                    connection = active,
                    lifecycleAdapter = lifecycleAdapter,
                    player = player,
                    onDisplay = onDisplay,
                    textState = textState,
                    stats = stats,
                    playbackQueue = playbackQueue,
                    playbackEpoch = playbackEpoch,
                    bufferedInputCount = pendingAudio.sampleCount.toLong(),
                    userSpeaking = userSpeaking,
                    logTag = logTag,
                )
                val drainElapsedMs = SystemClock.elapsedRealtime() - drainStartedAtMs
                stats.maxDrainMs = maxOf(stats.maxDrainMs, drainElapsedMs)
                if (drainElapsedMs >= LIVE_TRANSLATE_SLOW_DRAIN_LOG_MS) {
                    stats.slowDrainCount++
                    Log.w(
                        logTag,
                        "continuous slow-drain stream=${active.generation} elapsed_ms=$drainElapsedMs " +
                            "slow_count=${stats.slowDrainCount} sent_chunks=${stats.sentChunks} " +
                            "received_audio_chunks=${stats.receivedAudioChunks}",
                    )
                }
                if (!drained) {
                    return@collect
                }
            }

            val nowMs = SystemClock.elapsedRealtime()
            updateLiveTranslateWorkState(
                lifecycleAdapter = lifecycleAdapter,
                player = player,
                stats = stats,
                bufferedInputCount = pendingAudio.sampleCount.toLong(),
                userSpeaking = userSpeaking,
            )
            lifecycleAdapter.tick()
            if (nowMs - lastHealthAtMs >= 5_000L) {
                val snapshot = player.debugSnapshot()
                val state = lifecycleAdapter.state
                Log.i(
                    logTag,
                    "continuous health stream=${state.generation} sent_chunks=${stats.sentChunks} " +
                        "silent_sent_chunks=${state.inputChunksSinceServerActivity} " +
                        "since_server_ms=${elapsedMs(nowMs, state.lastServerActivityMs)} " +
                        "received_audio_chunks=${stats.receivedAudioChunks} " +
                        "playback_queued_chunks=${stats.playbackQueuedChunks.get()} " +
                        "playback_dropped_chunks=${stats.playbackDroppedChunks} " +
                        "playback_active=${snapshot.active} " +
                        "playback_pending_frames=${snapshot.pendingFrames} " +
                        "playback_output_mode=${snapshot.outputMode} " +
                        "routed_device=${snapshot.routedDevice} " +
                        "communication_device=${snapshot.communicationDevice} " +
                        "socket_age_ms=${elapsedMs(nowMs, state.connectedAtMs)} " +
                        "since_input_ms=${elapsedMs(nowMs, state.lastInputActivityMs)} " +
                        "reconnect_attempts=${state.reconnectAttempt} " +
                        "max_send_ms=${stats.maxSendMs} slow_send_count=${stats.slowSendCount} " +
                        "max_drain_ms=${stats.maxDrainMs} slow_drain_count=${stats.slowDrainCount}",
                )
                stats.maxSendMs = 0L
                stats.maxDrainMs = 0L
                lastHealthAtMs = nowMs
            }
        }
    } finally {
        lifecycleAdapter.cancel()
        playbackQueue.close()
        playbackJob.cancelAndJoin()
        player.drain(1_000)
        if (sourceMode == SourceMode.MIC) {
            player.endCommunicationSession()
        }
    }
}

private suspend fun drainLiveTranslateSocket(
    connection: GeminiS2sLiveConnection,
    lifecycleAdapter: GeminiS2sLiveLifecycleAdapter,
    player: AudioTrackPlayer,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    textState: LiveTranslateTextAccumulator,
    stats: LiveTranslateStreamStats,
    playbackQueue: Channel<LiveTranslatePlaybackChunk>,
    playbackEpoch: LiveTranslatePlaybackEpoch,
    bufferedInputCount: Long,
    userSpeaking: Boolean,
    logTag: String,
): Boolean {
    while (true) {
        when (val result = connection.session.receive(2)) {
            GeminiLiveReceiveResult.TimedOut -> return true
            is GeminiLiveReceiveResult.Frame -> {
                val featureEffects = lifecycleAdapter.observeFrame(
                    result.frame.toLifecycleFrame(connection.generation),
                )
                applyLiveTranslateFeatureEffects(
                    effects = featureEffects,
                    frame = result.frame,
                    player = player,
                    playbackQueue = playbackQueue,
                    playbackEpoch = playbackEpoch,
                    stats = stats,
                    onDisplay = onDisplay,
                    textState = textState,
                    logTag = logTag,
                )
                updateLiveTranslateWorkState(
                    lifecycleAdapter = lifecycleAdapter,
                    player = player,
                    stats = stats,
                    bufferedInputCount = bufferedInputCount,
                    userSpeaking = userSpeaking,
                )
            }
            is GeminiLiveReceiveResult.Unparsed -> Unit
            is GeminiLiveReceiveResult.Failed -> {
                val error = GeminiLiveSessionException(result.failure)
                val server = result.failure as? GeminiLiveSessionFailure.Server
                if (server != null) {
                    lifecycleAdapter.serverError(
                        generation = connection.generation,
                        retryable = server.retryable,
                    )
                    if (!server.retryable) throw error
                    return false
                }
                Log.w(
                    logTag,
                    "continuous socket failure stream=${connection.generation} error=${error.message}",
                    error,
                )
                lifecycleAdapter.transportFailed(connection.generation)
                return false
            }
            is GeminiLiveReceiveResult.Closed -> {
                Log.i(logTag, "continuous socket closed stream=${connection.generation}")
                lifecycleAdapter.transportFailed(connection.generation)
                return false
            }
        }
    }
}

private fun handleLiveTranslateFrame(
    frame: GeminiLiveServerFrame,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    textState: LiveTranslateTextAccumulator,
    stats: LiveTranslateStreamStats,
    playbackQueue: Channel<LiveTranslatePlaybackChunk>,
    playbackEpoch: LiveTranslatePlaybackEpoch,
    logTag: String,
) {
    val parsed = parseGeminiS2sUpdate(frame)
    var textChanged = false
    if (parsed.inputText.isNotBlank()) {
        textChanged = textState.updateSource(parsed.inputText) || textChanged
    }
    if (parsed.outputText.isNotBlank()) {
        textChanged = textState.updateTarget(parsed.outputText) || textChanged
    }
    if (textChanged) {
        onDisplay(textState.snapshot())
    }
    if (parsed.audioChunks.isNotEmpty()) {
        parsed.audioChunks.forEach { bytes ->
            val queued = playbackQueue.trySend(playbackEpoch.tag(bytes)).isSuccess
            if (queued) {
                stats.playbackQueuedChunks.incrementAndGet()
            } else {
                stats.playbackDroppedChunks++
                if (stats.playbackDroppedChunks <= 3 || stats.playbackDroppedChunks % 20 == 0) {
                    Log.w(
                        logTag,
                        "continuous playback-drop received_audio_chunks=${stats.receivedAudioChunks} playback_queued_chunks=${stats.playbackQueuedChunks.get()} playback_dropped_chunks=${stats.playbackDroppedChunks}",
                    )
                }
            }
        }
        stats.receivedAudioChunks += parsed.audioChunks.size
    }
}

private fun GeminiLiveServerFrame.toLifecycleFrame(generation: Long): GeminiLiveLifecycleFrame {
    return GeminiLiveLifecycleFrame(
        generation = generation,
        contentCount = contentCount,
        setupComplete = setupComplete,
        turnComplete = turnComplete,
        generationComplete = generationComplete,
        interrupted = interrupted,
        goAwayTimeLeftMs = goAwayTimeLeftMs,
        toolCallIds = toolCallIds,
        toolCancellationIds = toolCancellationIds.orEmpty(),
        error = null,
    )
}

private fun applyLiveTranslateFeatureEffects(
    effects: List<GeminiLiveLifecycleEffect>,
    frame: GeminiLiveServerFrame,
    player: AudioTrackPlayer,
    playbackQueue: Channel<LiveTranslatePlaybackChunk>,
    playbackEpoch: LiveTranslatePlaybackEpoch,
    stats: LiveTranslateStreamStats,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    textState: LiveTranslateTextAccumulator,
    logTag: String,
) {
    executeLiveTranslateFeatureEffects(
        effects = effects,
        deliverContent = { _ ->
            handleLiveTranslateFrame(
                frame,
                onDisplay,
                textState,
                stats,
                playbackQueue,
                playbackEpoch,
                logTag,
            )
        },
        stopPlayback = {
            playbackEpoch.interrupt(player::stopImmediate)
        },
        discardQueuedOutput = {
            var removed = 0
            while (playbackQueue.tryReceive().isSuccess) removed++
            stats.playbackQueuedChunks.updateAndGet {
                (it - removed).coerceAtLeast(0)
            }
        },
    )
}

private fun elapsedMs(nowMs: Long, sinceMs: Long?): Long {
    return sinceMs?.let { (nowMs - it).coerceAtLeast(0) } ?: 0L
}

private fun updateLiveTranslateWorkState(
    lifecycleAdapter: GeminiS2sLiveLifecycleAdapter,
    player: AudioTrackPlayer,
    stats: LiveTranslateStreamStats,
    bufferedInputCount: Long,
    userSpeaking: Boolean,
) {
    val playbackPendingFrames = player.debugSnapshot().pendingFrames
    lifecycleAdapter.updateWorkState(
        pendingWorkCount = stats.playbackQueuedChunks.get().toLong() + playbackPendingFrames,
        bufferedInputCount = bufferedInputCount,
        userSpeaking = userSpeaking,
    )
}

private data class LiveTranslateStreamStats(
    var sentChunks: Int = 0,
    var receivedAudioChunks: Int = 0,
    val playbackQueuedChunks: AtomicInteger = AtomicInteger(),
    var playbackDroppedChunks: Int = 0,
    var maxSendMs: Long = 0L,
    var slowSendCount: Int = 0,
    var maxDrainMs: Long = 0L,
    var slowDrainCount: Int = 0,
)

private class LiveTranslateTextAccumulator {
    private var sourceCommitted: String = ""
    private var sourceDraft: String = ""
    private var targetCommitted: String = ""
    private var targetDraft: String = ""

    fun updateSource(incoming: String): Boolean {
        val beforeCommitted = sourceCommitted
        val beforeDraft = sourceDraft
        val updated = updateLiveTextPair(sourceCommitted, sourceDraft, incoming)
        sourceCommitted = updated.first
        sourceDraft = updated.second
        return beforeCommitted != sourceCommitted || beforeDraft != sourceDraft
    }

    fun updateTarget(incoming: String): Boolean {
        val beforeCommitted = targetCommitted
        val beforeDraft = targetDraft
        val updated = updateLiveTextPair(targetCommitted, targetDraft, incoming)
        targetCommitted = updated.first
        targetDraft = updated.second
        return beforeCommitted != targetCommitted || beforeDraft != targetDraft
    }

    fun snapshot(): S2sDisplaySnapshot {
        return S2sDisplaySnapshot(
            sourceCommitted = sourceCommitted,
            sourceDraft = sourceDraft,
            targetCommitted = targetCommitted,
            targetDraft = targetDraft,
        )
    }
}

private fun updateLiveTextPair(
    committedInput: String,
    draftInput: String,
    incomingInput: String,
): Pair<String, String> {
    val incoming = incomingInput.trim()
    if (incoming.isEmpty()) {
        return committedInput to draftInput
    }
    var committed = committedInput
    var draft = draftInput
    if (draft.isEmpty()) {
        draft = incoming
        return maybeCommitLiveDraft(committed, draft)
    }
    val trimmedDraft = draft.trim()
    if (incoming == trimmedDraft || trimmedDraft.startsWith(incoming)) {
        return committed to draft
    }
    if (incoming.startsWith(trimmedDraft)) {
        draft = incoming
        return maybeCommitLiveDraft(committed, draft)
    }

    val overlap = largestSuffixPrefixOverlap(trimmedDraft.trimEnd(), incoming)
    if (overlap > 0) {
        val suffix = incoming.substring(overlap).trimStart()
        draft = if (suffix.isBlank()) draft.trimEnd() else "${draft.trimEnd()} $suffix"
        return maybeCommitLiveDraft(committed, draft)
    }

    val committedPair = commitLiveDraft(committed, draft)
    committed = committedPair.first
    draft = incoming
    return maybeCommitLiveDraft(committed, draft)
}

private fun maybeCommitLiveDraft(
    committed: String,
    draft: String,
): Pair<String, String> {
    val trimmed = draft.trim()
    val wordCount = trimmed.split(Regex("\\s+")).count { it.isNotBlank() }
    val endsSentence = trimmed.lastOrNull()?.let { it == '.' || it == '?' || it == '!' || it == '。' || it == '？' || it == '！' } == true
    return if (endsSentence || wordCount >= 18) {
        commitLiveDraft(committed, draft)
    } else {
        committed to draft
    }
}

private fun commitLiveDraft(
    committed: String,
    draft: String,
): Pair<String, String> {
    val trimmed = draft.trim()
    if (trimmed.isEmpty()) {
        return committed to ""
    }
    val nextCommitted = if (committed.isEmpty()) trimmed else "$committed $trimmed"
    return nextCommitted to ""
}

private fun largestSuffixPrefixOverlap(
    left: String,
    right: String,
): Int {
    val max = minOf(left.length, right.length)
    for (size in max downTo 3) {
        if (left.takeLast(size).equals(right.take(size), ignoreCase = true)) {
            return size
        }
    }
    return 0
}

internal const val LIVE_TRANSLATE_PLAYBACK_QUEUE_CAPACITY = 48
internal const val LIVE_TRANSLATE_COMMUNICATION_VOLUME_BOOST = 1.8f
private const val LIVE_TRANSLATE_PENDING_AUDIO_SAMPLES = FRAME_SAMPLES * 10
private const val LIVE_TRANSLATE_SLOW_SEND_LOG_MS = 120L
private const val LIVE_TRANSLATE_SLOW_DRAIN_LOG_MS = 120L
