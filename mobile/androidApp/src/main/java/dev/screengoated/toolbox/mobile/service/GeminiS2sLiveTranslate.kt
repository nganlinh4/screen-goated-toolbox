package dev.screengoated.toolbox.mobile.service

import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.service.tts.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.tts.WebSocketEvent
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.Request
import java.io.IOException

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
    val playbackQueue = Channel<ByteArray>(LIVE_TRANSLATE_PLAYBACK_QUEUE_CAPACITY)
    var streamId = 0L
    var session: BlockingWebSocketSession? = null
    var socketConnectedAtMs: Long? = null
    var lastHealthAtMs = SystemClock.elapsedRealtime()
    var lastServerActivityAtMs = SystemClock.elapsedRealtime()
    var lastActiveInputAtMs = SystemClock.elapsedRealtime()
    var sentChunksAtLastActivity = 0
    var reconnectAttempts = 0
    var nextConnectAtMs = SystemClock.elapsedRealtime()
    if (sourceMode == SourceMode.MIC) {
        player.beginCommunicationSession()
    }
    val playbackJob = launch(Dispatchers.IO) {
        for (bytes in playbackQueue) {
            stats.playbackQueuedChunks = (stats.playbackQueuedChunks - 1).coerceAtLeast(0)
            player.playNativePcm24k(bytes, settingsProvider().realtime.volumePercent)
        }
    }

    suspend fun connect(): BlockingWebSocketSession? {
        val request = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()
        val opened = BlockingWebSocketSession(httpClient, request)
        if (!withContext(Dispatchers.IO) { opened.awaitOpen(10_000) }) {
            opened.close()
            Log.w(
                logTag,
                "continuous open-failed stream=$streamId attempt=${reconnectAttempts + 1}",
            )
            return null
        }
        val setupPayload = buildGeminiS2sSetupPayload(
            model = model,
            settings = settingsProvider(),
            contextText = "",
        )
        if (!opened.sendText(setupPayload) || !waitForGeminiS2sSetup(opened, logTag)) {
            opened.close()
            Log.w(
                logTag,
                "continuous setup-failed stream=$streamId attempt=${reconnectAttempts + 1}",
            )
            return null
        }
        Log.i(
            logTag,
            "continuous socket connected stream=$streamId reconnect_attempts=$reconnectAttempts",
        )
        socketConnectedAtMs = SystemClock.elapsedRealtime()
        lastServerActivityAtMs = SystemClock.elapsedRealtime()
        sentChunksAtLastActivity = stats.sentChunks
        reconnectAttempts = 0
        return opened
    }

    suspend fun ensureSession(): BlockingWebSocketSession? {
        val current = session
        if (current != null) {
            return current
        }
        val nowMs = SystemClock.elapsedRealtime()
        if (nowMs < nextConnectAtMs) {
            return null
        }
        val connected = connect()
        if (connected == null) {
            val delayMs = liveTranslateReconnectDelayMs(reconnectAttempts, streamId)
            Log.w(
                logTag,
                "continuous reconnect delayed stream=$streamId attempt=${reconnectAttempts + 1} retry_ms=$delayMs",
            )
            reconnectAttempts++
            nextConnectAtMs = SystemClock.elapsedRealtime() + delayMs
        }
        session = connected
        return connected
    }

    fun socketAgeMs(nowMs: Long = SystemClock.elapsedRealtime()): Long {
        return socketConnectedAtMs?.let { nowMs - it } ?: 0L
    }

    fun scheduleReconnect(reason: String) {
        val nowMs = SystemClock.elapsedRealtime()
        val delayMs = liveTranslateReconnectDelayMs(reconnectAttempts, streamId)
        Log.i(
            logTag,
            "continuous reconnect scheduled reason=$reason stream=$streamId attempt=${reconnectAttempts + 1} retry_ms=$delayMs socket_age_ms=${socketAgeMs(nowMs)}",
        )
        session?.close()
        session = null
        socketConnectedAtMs = null
        streamId++
        reconnectAttempts++
        nextConnectAtMs = nowMs + delayMs
    }

    fun maybeRotateQuietSocket() {
        val connectedAtMs = socketConnectedAtMs ?: return
        if (session == null) {
            return
        }
        val nowMs = SystemClock.elapsedRealtime()
        if (nowMs - connectedAtMs < LIVE_TRANSLATE_PROACTIVE_ROTATE_MS ||
            nowMs - lastActiveInputAtMs < LIVE_TRANSLATE_ROTATE_QUIET_MS ||
            nowMs - lastServerActivityAtMs < LIVE_TRANSLATE_ROTATE_QUIET_MS
        ) {
            return
        }
        Log.i(
            logTag,
            "continuous reconnect reason=proactive-rotation stream=$streamId socket_age_ms=${nowMs - connectedAtMs} quiet_input_ms=${nowMs - lastActiveInputAtMs} quiet_server_ms=${nowMs - lastServerActivityAtMs}",
        )
        scheduleReconnect("proactive-rotation")
    }

    try {
        audioChunks.collect { chunk ->
            if (!currentCoroutineContext().isActive) {
                return@collect
            }
            val active = ensureSession() ?: return@collect
            for (offset in chunk.indices step FRAME_SAMPLES) {
                val end = minOf(offset + FRAME_SAMPLES, chunk.size)
                val frame = chunk.copyOfRange(offset, end)
                if (rms(frame) >= MIN_SPEECH_THRESHOLD * SPEECH_THRESHOLD_MULTIPLIER) {
                    lastActiveInputAtMs = SystemClock.elapsedRealtime()
                }
                val sendStartedAtMs = SystemClock.elapsedRealtime()
                val sent = active.sendText(buildGeminiS2sAudioPayload(frame))
                val sendElapsedMs = SystemClock.elapsedRealtime() - sendStartedAtMs
                stats.maxSendMs = maxOf(stats.maxSendMs, sendElapsedMs)
                if (sendElapsedMs >= LIVE_TRANSLATE_SLOW_SEND_LOG_MS) {
                    stats.slowSendCount++
                    Log.w(
                        logTag,
                        "continuous slow-send stream=$streamId elapsed_ms=$sendElapsedMs slow_count=${stats.slowSendCount} sent_chunks=${stats.sentChunks}",
                    )
                }
                if (!sent) {
                    Log.w(
                        logTag,
                        "continuous send-failed stream=$streamId sent_chunks=${stats.sentChunks} socket_age_ms=${socketAgeMs()} since_server_ms=${SystemClock.elapsedRealtime() - lastServerActivityAtMs}",
                    )
                    scheduleReconnect("send-failed")
                    return@collect
                }
                stats.sentChunks++
                val drainStartedAtMs = SystemClock.elapsedRealtime()
                val drained = drainLiveTranslateSocket(
                    session = active,
                    streamId = streamId,
                    onDisplay = onDisplay,
                    textState = textState,
                    stats = stats,
                    playbackQueue = playbackQueue,
                    logTag = logTag,
                )
                val drainElapsedMs = SystemClock.elapsedRealtime() - drainStartedAtMs
                stats.maxDrainMs = maxOf(stats.maxDrainMs, drainElapsedMs)
                if (drainElapsedMs >= LIVE_TRANSLATE_SLOW_DRAIN_LOG_MS) {
                    stats.slowDrainCount++
                    Log.w(
                        logTag,
                        "continuous slow-drain stream=$streamId elapsed_ms=$drainElapsedMs slow_count=${stats.slowDrainCount} sent_chunks=${stats.sentChunks} received_audio_chunks=${stats.receivedAudioChunks}",
                    )
                }
                if (!drained) {
                    scheduleReconnect("socket-drain")
                    return@collect
                }
            }

            val nowMs = SystemClock.elapsedRealtime()
            if (stats.serverActivity) {
                lastServerActivityAtMs = nowMs
                sentChunksAtLastActivity = stats.sentChunks
                stats.serverActivity = false
            }
            if (nowMs - lastHealthAtMs >= 5_000L) {
                val snapshot = player.debugSnapshot()
                val silentSentChunks = stats.sentChunks - sentChunksAtLastActivity
                Log.i(
                    logTag,
                    "continuous health stream=$streamId sent_chunks=${stats.sentChunks} silent_sent_chunks=$silentSentChunks since_server_ms=${nowMs - lastServerActivityAtMs} received_audio_chunks=${stats.receivedAudioChunks} playback_queued_chunks=${stats.playbackQueuedChunks} playback_dropped_chunks=${stats.playbackDroppedChunks} playback_active=${snapshot.active} playback_pending_frames=${snapshot.pendingFrames} playback_output_mode=${snapshot.outputMode} routed_device=${snapshot.routedDevice} communication_device=${snapshot.communicationDevice} socket_age_ms=${socketAgeMs(nowMs)} since_input_ms=${nowMs - lastActiveInputAtMs} reconnect_attempts=$reconnectAttempts max_send_ms=${stats.maxSendMs} slow_send_count=${stats.slowSendCount} max_drain_ms=${stats.maxDrainMs} slow_drain_count=${stats.slowDrainCount}",
                )
                stats.maxSendMs = 0L
                stats.maxDrainMs = 0L
                lastHealthAtMs = nowMs
            }
            val silentSentChunks = stats.sentChunks - sentChunksAtLastActivity
            if (session != null &&
                silentSentChunks >= LIVE_TRANSLATE_SERVER_SILENT_SENT_CHUNKS &&
                nowMs - lastServerActivityAtMs >= LIVE_TRANSLATE_SERVER_SILENT_MS
            ) {
                Log.i(
                    logTag,
                    "continuous reconnect reason=server-silent stream=$streamId silent_ms=${nowMs - lastServerActivityAtMs} silent_sent_chunks=$silentSentChunks received_audio_chunks=${stats.receivedAudioChunks} socket_age_ms=${socketAgeMs(nowMs)}",
                )
                scheduleReconnect("server-silent")
                lastServerActivityAtMs = nowMs
                sentChunksAtLastActivity = stats.sentChunks
            }
            maybeRotateQuietSocket()
        }
    } finally {
        session?.close()
        playbackQueue.close()
        playbackJob.cancelAndJoin()
        player.drain(1_000)
        if (sourceMode == SourceMode.MIC) {
            player.endCommunicationSession()
        }
    }
}

private fun drainLiveTranslateSocket(
    session: BlockingWebSocketSession,
    streamId: Long,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    textState: LiveTranslateTextAccumulator,
    stats: LiveTranslateStreamStats,
    playbackQueue: Channel<ByteArray>,
    logTag: String,
): Boolean {
    while (true) {
        when (val event = session.poll(2)) {
            null -> return true
            is WebSocketEvent.Text -> {
                handleLiveTranslatePayload(event.payload, onDisplay, textState, stats, playbackQueue, logTag)
            }
            is WebSocketEvent.Binary -> {
                handleLiveTranslatePayload(event.payload.utf8(), onDisplay, textState, stats, playbackQueue, logTag)
            }
            is WebSocketEvent.Failure -> {
                Log.w(logTag, "continuous socket failure stream=$streamId error=${event.throwable.message}", event.throwable)
                return false
            }
            WebSocketEvent.Closed -> {
                Log.i(logTag, "continuous socket closed stream=$streamId")
                return false
            }
        }
    }
}

private fun handleLiveTranslatePayload(
    payload: String,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    textState: LiveTranslateTextAccumulator,
    stats: LiveTranslateStreamStats,
    playbackQueue: Channel<ByteArray>,
    logTag: String,
) {
    val parsed = parseGeminiS2sUpdate(payload)
    parsed.error?.let { throw IOException(it) }
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
            val queued = playbackQueue.trySend(bytes).isSuccess
            if (queued) {
                stats.playbackQueuedChunks++
            } else {
                stats.playbackDroppedChunks++
                if (stats.playbackDroppedChunks <= 3 || stats.playbackDroppedChunks % 20 == 0) {
                    Log.w(
                        logTag,
                        "continuous playback-drop received_audio_chunks=${stats.receivedAudioChunks} playback_queued_chunks=${stats.playbackQueuedChunks} playback_dropped_chunks=${stats.playbackDroppedChunks}",
                    )
                }
            }
        }
        stats.receivedAudioChunks += parsed.audioChunks.size
    }
    if (textChanged || parsed.audioChunks.isNotEmpty() || parsed.turnComplete || parsed.generationComplete) {
        stats.serverActivity = true
    }
}

private data class LiveTranslateStreamStats(
    var sentChunks: Int = 0,
    var receivedAudioChunks: Int = 0,
    var playbackQueuedChunks: Int = 0,
    var playbackDroppedChunks: Int = 0,
    var maxSendMs: Long = 0L,
    var slowSendCount: Int = 0,
    var maxDrainMs: Long = 0L,
    var slowDrainCount: Int = 0,
    var serverActivity: Boolean = false,
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

private fun liveTranslateReconnectDelayMs(
    attempt: Int,
    streamId: Long,
): Long {
    val cappedAttempt = attempt.coerceAtMost(5)
    val baseMs = 250L * (1L shl cappedAttempt)
    val jitterMs = ((streamId * 97L + attempt * 53L) % 180L) + 20L
    return (baseMs + jitterMs).coerceAtMost(6_000L)
}

internal const val LIVE_TRANSLATE_PLAYBACK_QUEUE_CAPACITY = 48
internal const val LIVE_TRANSLATE_COMMUNICATION_VOLUME_BOOST = 1.8f
private const val LIVE_TRANSLATE_SLOW_SEND_LOG_MS = 120L
private const val LIVE_TRANSLATE_SLOW_DRAIN_LOG_MS = 120L
private const val LIVE_TRANSLATE_SERVER_SILENT_SENT_CHUNKS = 100
private const val LIVE_TRANSLATE_SERVER_SILENT_MS = 15_000L
private const val LIVE_TRANSLATE_PROACTIVE_ROTATE_MS = 12 * 60 * 1_000L
private const val LIVE_TRANSLATE_ROTATE_QUIET_MS = 3_000L
