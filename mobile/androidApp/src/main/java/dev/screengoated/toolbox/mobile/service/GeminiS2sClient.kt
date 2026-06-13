package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackOutputMode
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.service.tts.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.tts.WebSocketEvent
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException
import java.util.Locale
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.sqrt

class GeminiS2sClient(
    context: Context,
    private val httpClient: OkHttpClient,
) {
    private val mediaPlayer = AudioTrackPlayer(context, outputMode = AudioTrackOutputMode.MEDIA)
    private val communicationPlayer = AudioTrackPlayer(
        context,
        outputMode = AudioTrackOutputMode.VOICE_COMMUNICATION,
        outputVolumeBoost = LIVE_TRANSLATE_COMMUNICATION_VOLUME_BOOST,
    )

    private fun liveTranslatePlayer(sourceMode: SourceMode): AudioTrackPlayer {
        return if (sourceMode == SourceMode.MIC) {
            communicationPlayer
        } else {
            mediaPlayer
        }
    }

    suspend fun runSession(
        apiKey: String,
        model: String,
        sourceMode: SourceMode,
        audioChunks: kotlinx.coroutines.flow.Flow<ShortArray>,
        settingsProvider: () -> GeminiS2sRuntimeSettings,
        onDisplay: (S2sDisplaySnapshot) -> Unit,
    ) = coroutineScope {
        val startMs = SystemClock.elapsedRealtime()
        val logTag = geminiLiveAudioLogTag(model)
        val initialSettings = settingsProvider()
        Log.i(
            logTag,
            "session-start model=$model target=${initialSettings.targetLanguage} " +
                "voice=${initialSettings.globalTts.voice} speed=${initialSettings.realtime.speedPercent} " +
                "volume=${initialSettings.realtime.volumePercent}",
        )
        if (isGeminiLiveTranslateApiModel(model)) {
            val player = liveTranslatePlayer(sourceMode)
            try {
                runLiveTranslateContinuousSession(
                    apiKey = apiKey,
                    model = model,
                    sourceMode = sourceMode,
                    player = player,
                    audioChunks = audioChunks,
                    settingsProvider = settingsProvider,
                    onDisplay = onDisplay,
                    logTag = logTag,
                )
            } finally {
                Log.i(logTag, "session-exit uptime_ms=${SystemClock.elapsedRealtime() - startMs}")
                player.stopImmediate()
            }
            return@coroutineScope
        }

        val player = mediaPlayer
        val contextMemory = S2sContextMemory()
        val adaptiveVad = AdaptiveS2sVadState()
        val backlogMs = AtomicInteger(0)
        val attempts = Channel<S2sEvent>(Channel.UNLIMITED)
        val workerChannels = List(SESSION_COUNT) { Channel<S2sSegment>(Channel.UNLIMITED) }
        val coordinator = launch(Dispatchers.IO) {
            runGeminiS2sPlaybackCoordinator(
                player = player,
                contextMemory = contextMemory,
                events = attempts,
                settingsProvider = settingsProvider,
                onDisplay = onDisplay,
                logTag = logTag,
                backlogMs = backlogMs,
            )
        }
        val workers = workerChannels.mapIndexed { workerIndex, channel ->
            launch(Dispatchers.IO) {
                for (segment in channel) {
                    val contextText = contextMemory.snapshot()
                    runSegmentWithRetry(
                        apiKey = apiKey,
                        model = model,
                        segment = segment,
                        workerIndex = workerIndex,
                        contextText = contextText,
                        settingsProvider = settingsProvider,
                        output = attempts,
                        adaptiveVad = adaptiveVad,
                        logTag = logTag,
                    )
                }
            }
        }

        try {
            collectSegments(audioChunks, adaptiveVad, backlogMs, logTag) { segment ->
                workerChannels[(segment.id % SESSION_COUNT).toInt()].send(segment)
            }
        } finally {
            Log.i(logTag, "session-exit uptime_ms=${SystemClock.elapsedRealtime() - startMs}")
            workerChannels.forEach { it.close() }
            workers.forEach { it.cancelAndJoin() }
            attempts.close()
            coordinator.cancelAndJoin()
            player.stopImmediate()
        }
    }

    private suspend fun runLiveTranslateContinuousSession(
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

    private suspend fun runSegmentWithRetry(
        apiKey: String,
        model: String,
        segment: S2sSegment,
        workerIndex: Int,
        contextText: String,
        settingsProvider: () -> GeminiS2sRuntimeSettings,
        output: Channel<S2sEvent>,
        adaptiveVad: AdaptiveS2sVadState,
        logTag: String,
    ) {
        output.send(
            S2sEvent.Queued(
                segmentId = segment.id,
                generation = segment.generation,
                audioMs = segment.audioMs,
                queuedAtMs = SystemClock.elapsedRealtime(),
            ),
        )
        val first = runHedgedSegment(
            apiKey = apiKey,
            model = model,
            segment = segment,
            workerIndex = workerIndex,
            baseGeneration = segment.generation,
            finalAttempt = false,
            contextText = contextText,
            settingsProvider = settingsProvider,
            output = output,
            logTag = logTag,
        )
        if (first == SegmentResult.OK) {
            adaptiveVad.observe(AdaptiveS2sVadOutcome.HEALTHY, segment)
        } else if (first == SegmentResult.RETRY_FRESH && currentCoroutineContext().isActive) {
            Log.i(logTag, "retry segment=${segment.id} worker=$workerIndex gen=${segment.generation}")
            val second = runHedgedSegment(
                apiKey = apiKey,
                model = model,
                segment = segment.copy(generation = segment.generation + 1_000_000),
                workerIndex = workerIndex,
                baseGeneration = segment.generation + 1_000_000,
                finalAttempt = true,
                contextText = contextText,
                settingsProvider = settingsProvider,
                output = output,
                logTag = logTag,
            )
            if (second == SegmentResult.OK) {
                adaptiveVad.observe(AdaptiveS2sVadOutcome.HEALTHY, segment)
            } else if (second == SegmentResult.EMPTY_FINAL) {
                adaptiveVad.observe(AdaptiveS2sVadOutcome.EMPTY_NO_INPUT, segment)
                output.send(S2sEvent.Done(segment.id, segment.generation + 1_000_000, empty = true))
            }
        } else if (first == SegmentResult.EMPTY_FINAL) {
            adaptiveVad.observe(AdaptiveS2sVadOutcome.EMPTY_NO_INPUT, segment)
        }
    }

    private suspend fun runHedgedSegment(
        apiKey: String,
        model: String,
        segment: S2sSegment,
        workerIndex: Int,
        baseGeneration: Long,
        finalAttempt: Boolean,
        contextText: String,
        settingsProvider: () -> GeminiS2sRuntimeSettings,
        output: Channel<S2sEvent>,
        logTag: String,
    ): SegmentResult = coroutineScope {
        Log.i(logTag, "hedge segment=${segment.id} worker=$workerIndex gen=$baseGeneration attempts=$HEDGE_ATTEMPTS")
        val race = Channel<S2sRaceEvent>(Channel.UNLIMITED)
        val jobs = (0 until HEDGE_ATTEMPTS).map { attempt ->
            launch(Dispatchers.IO) {
                val generation = baseGeneration + (attempt * 100_000L)
                try {
                    runAttempt(
                        apiKey = apiKey,
                        model = model,
                        segment = segment.copy(generation = generation),
                        attempt = attempt,
                        finalAttempt = finalAttempt,
                        contextText = contextText,
                        settings = settingsProvider(),
                        output = race,
                        logTag = logTag,
                    )
                } catch (error: Throwable) {
                    if (error is CancellationException) {
                        throw error
                    }
                    Log.w(
                        logTag,
                        "attempt-error segment=${segment.id} gen=$generation attempt=$attempt error=${error.message}",
                        error,
                    )
                    race.send(S2sRaceEvent.Retry(segment.id, generation, attempt))
                }
            }
        }
        val buffered = mutableMapOf<Int, MutableList<S2sRaceEvent>>()
        var winner: Int? = null
        var completed = 0
        var sawWinnerAudio = false
        var result = SegmentResult.RETRY_FRESH
        try {
            while (completed < HEDGE_ATTEMPTS && currentCoroutineContext().isActive) {
                when (val event = race.receive()) {
                    is S2sRaceEvent.Audio -> {
                        val currentWinner = winner
                        if (currentWinner == null) {
                            winner = event.attempt
                            val generation = baseGeneration + (event.attempt * 100_000L)
                            Log.i(
                                logTag,
                                "hedge-winner segment=${segment.id} worker=$workerIndex gen=$generation attempt=${event.attempt} buffered_events=${buffered[event.attempt]?.size ?: 0}",
                            )
                            buffered[event.attempt]?.forEach { forwardWinnerEvent(it, output) }
                            jobs.forEachIndexed { index, job ->
                                if (index != event.attempt) {
                                    job.cancel()
                                }
                            }
                        }
                        if (winner == event.attempt) {
                            sawWinnerAudio = true
                            output.send(S2sEvent.Audio(segment.id, event.generation, event.bytes))
                        } else {
                            buffered.getOrPut(event.attempt) { mutableListOf() }.add(event)
                        }
                    }
                    is S2sRaceEvent.SourceText,
                    is S2sRaceEvent.TargetText -> {
                        if (winner == event.attempt) {
                            forwardWinnerEvent(event, output)
                        } else if (winner == null) {
                            buffered.getOrPut(event.attempt) { mutableListOf() }.add(event)
                        }
                    }
                    is S2sRaceEvent.Done -> {
                        completed++
                        if (winner == event.attempt) {
                            output.send(S2sEvent.Done(segment.id, event.generation, empty = !sawWinnerAudio))
                            result = if (sawWinnerAudio) SegmentResult.OK else if (finalAttempt) SegmentResult.EMPTY_FINAL else SegmentResult.RETRY_FRESH
                            break
                        }
                    }
                    is S2sRaceEvent.Retry -> {
                        completed++
                        if (winner == event.attempt) {
                            result = if (finalAttempt) SegmentResult.EMPTY_FINAL else SegmentResult.RETRY_FRESH
                            break
                        }
                    }
                }
            }
            if (winner == null) {
                Log.i(logTag, "hedge-empty segment=${segment.id} worker=$workerIndex gen=$baseGeneration attempts=$HEDGE_ATTEMPTS")
                result = if (finalAttempt) SegmentResult.EMPTY_FINAL else SegmentResult.RETRY_FRESH
            }
        } finally {
            jobs.forEach { it.cancelAndJoin() }
            race.close()
        }
        result
    }

    private suspend fun forwardWinnerEvent(
        event: S2sRaceEvent,
        output: Channel<S2sEvent>,
    ) {
        when (event) {
            is S2sRaceEvent.SourceText -> output.send(S2sEvent.SourceText(event.segmentId, event.generation, event.text))
            is S2sRaceEvent.TargetText -> output.send(S2sEvent.TargetText(event.segmentId, event.generation, event.text))
            is S2sRaceEvent.Audio -> output.send(S2sEvent.Audio(event.segmentId, event.generation, event.bytes))
            is S2sRaceEvent.Done,
            is S2sRaceEvent.Retry -> Unit
        }
    }

    private suspend fun runAttempt(
        apiKey: String,
        model: String,
        segment: S2sSegment,
        attempt: Int,
        finalAttempt: Boolean,
        contextText: String,
        settings: GeminiS2sRuntimeSettings,
        output: Channel<S2sRaceEvent>,
        logTag: String,
    ) {
        val startedAtMs = SystemClock.elapsedRealtime()
        val request = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()
        BlockingWebSocketSession(httpClient, request).use { session ->
            if (!withContext(Dispatchers.IO) { session.awaitOpen(10_000) }) {
                Log.w(logTag, "open-failed segment=${segment.id} gen=${segment.generation} attempt=$attempt")
                output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                return
            }
            Log.i(logTag, "open-ok segment=${segment.id} gen=${segment.generation} attempt=$attempt")
            val setupPayload = buildGeminiS2sSetupPayload(
                model = model,
                settings = settings,
                contextText = contextText,
            )
            if (!session.sendText(setupPayload) || !waitForGeminiS2sSetup(session, logTag)) {
                Log.w(logTag, "setup-failed segment=${segment.id} gen=${segment.generation} attempt=$attempt")
                output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                return
            }
            Log.i(
                logTag,
                "start segment=${segment.id} gen=${segment.generation} attempt=$attempt audio_ms=${segment.audioMs} context_chars=${contextText.length}",
            )
            for (offset in segment.samples.indices step FRAME_SAMPLES) {
                val end = minOf(offset + FRAME_SAMPLES, segment.samples.size)
                if (!session.sendText(buildGeminiS2sAudioPayload(segment.samples.copyOfRange(offset, end)))) {
                    output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                    return
                }
            }
            if (shouldSendAudioStreamEnd(model)) {
                session.sendText(buildGeminiS2sAudioStreamEndPayload())
            } else {
                Log.i(logTag, "stream-end-skipped segment=${segment.id} gen=${segment.generation} attempt=$attempt")
            }

            var firstAudioAtMs = 0L
            var lastAudioAtMs = 0L
            var audioChunks = 0
            var textUpdates = 0
            var emptyReads = 0
            val hardTimeoutMs = groupedHardTimeoutMs(segment.audioMs.toLong(), finalAttempt)
            while (currentCoroutineContext().isActive) {
                val now = SystemClock.elapsedRealtime()
                val retryThreshold = groupedFirstAudioTimeoutMs(segment.audioMs.toLong(), textUpdates)
                if (firstAudioAtMs == 0L && now - startedAtMs >= retryThreshold) {
                    Log.i(
                        logTag,
                        "done segment=${segment.id} gen=${segment.generation} attempt=$attempt reason=no_first_audio_retry retry_ms=$retryThreshold source_audio_ms=${segment.audioMs} text_updates=$textUpdates chunks=$audioChunks first_audio_ms=none",
                    )
                    output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                    return
                }
                if (firstAudioAtMs != 0L && now - lastAudioAtMs >= AUDIO_IDLE_FINISH_MS) {
                    Log.i(
                        logTag,
                        "done segment=${segment.id} gen=${segment.generation} attempt=$attempt reason=audio_idle chunks=$audioChunks first_audio_ms=${firstAudioAtMs - startedAtMs}",
                    )
                    output.send(S2sRaceEvent.Done(segment.id, segment.generation, attempt))
                    return
                }
                if (now - startedAtMs > hardTimeoutMs) {
                    Log.i(
                        logTag,
                        "done segment=${segment.id} gen=${segment.generation} attempt=$attempt reason=timeout timeout_ms=$hardTimeoutMs source_audio_ms=${segment.audioMs} chunks=$audioChunks first_audio_ms=${if (firstAudioAtMs == 0L) "none" else firstAudioAtMs - startedAtMs}",
                    )
                    output.send(
                        if (audioChunks > 0) {
                            S2sRaceEvent.Done(segment.id, segment.generation, attempt)
                        } else {
                            S2sRaceEvent.Retry(segment.id, segment.generation, attempt)
                        },
                    )
                    return
                }

                fun handlePayload(payload: String): Boolean {
                    val parsed = parseGeminiS2sUpdate(payload)
                    parsed.error?.let { throw IOException(it) }
                    if (parsed.inputText.isNotBlank()) {
                        textUpdates++
                        output.trySend(S2sRaceEvent.SourceText(segment.id, segment.generation, attempt, parsed.inputText))
                    }
                    if (parsed.outputText.isNotBlank()) {
                        textUpdates++
                        output.trySend(S2sRaceEvent.TargetText(segment.id, segment.generation, attempt, parsed.outputText))
                    }
                    parsed.audioChunks.forEach { bytes ->
                        if (firstAudioAtMs == 0L) {
                            firstAudioAtMs = SystemClock.elapsedRealtime()
                            Log.i(
                                logTag,
                                "first-audio segment=${segment.id} gen=${segment.generation} attempt=$attempt elapsed_ms=${firstAudioAtMs - startedAtMs}",
                            )
                        }
                        lastAudioAtMs = SystemClock.elapsedRealtime()
                        audioChunks++
                        output.trySend(S2sRaceEvent.Audio(segment.id, segment.generation, attempt, bytes))
                    }
                    if ((parsed.turnComplete || parsed.generationComplete) && audioChunks > 0) {
                        Log.i(
                            logTag,
                            "done segment=${segment.id} gen=${segment.generation} attempt=$attempt reason=turn_complete chunks=$audioChunks first_audio_ms=${firstAudioAtMs - startedAtMs}",
                        )
                        output.trySend(S2sRaceEvent.Done(segment.id, segment.generation, attempt))
                        return true
                    }
                    return false
                }

                when (val event = withContext(Dispatchers.IO) { session.poll(50) }) {
                    null -> {
                        emptyReads++
                        if (emptyReads % 60 == 0) {
                            Log.d(
                                logTag,
                                "wait segment=${segment.id} gen=${segment.generation} elapsed_ms=${now - startedAtMs} no_audio_yet=${firstAudioAtMs == 0L} audio_chunks=$audioChunks text_updates=$textUpdates final_attempt=$finalAttempt",
                            )
                        }
                    }
                    is WebSocketEvent.Text -> {
                        if (handlePayload(event.payload)) {
                            return
                        }
                    }
                    is WebSocketEvent.Binary -> {
                        if (handlePayload(event.payload.utf8())) {
                            return
                        }
                    }
                    is WebSocketEvent.Failure -> throw event.throwable
                    WebSocketEvent.Closed -> {
                        output.send(
                            if (audioChunks > 0) {
                                S2sRaceEvent.Done(segment.id, segment.generation, attempt)
                            } else {
                                S2sRaceEvent.Retry(segment.id, segment.generation, attempt)
                            },
                        )
                        return
                    }
                }
            }
        }
    }

    private suspend fun collectSegments(
        audioChunks: kotlinx.coroutines.flow.Flow<ShortArray>,
        adaptiveVad: AdaptiveS2sVadState,
        backlogMs: AtomicInteger,
        logTag: String,
        emitSegment: suspend (S2sSegment) -> Unit,
    ) {
        var nextSegmentId = 0L
        var generation = LongArray(SESSION_COUNT)
        val pending = ArrayList<Short>(TARGET_SEGMENT_SAMPLES + PREROLL_SAMPLES)
        val preroll = ArrayDeque<Short>(PREROLL_SAMPLES)
        val frame = ShortArray(FRAME_SAMPLES)
        var frameFill = 0
        var active = false
        var activeMs = 0L
        var silenceFrames = 0
        var speechFrames = 0
        var peakRms = 0f
        var noiseFloor = 0.003f
        var totalFrames = 0L
        var totalChunks = 0L
        var lastHealthAtMs = SystemClock.elapsedRealtime()
        var windowFrames = 0
        var windowSpeechFrames = 0
        var windowPeakRms = 0f

        suspend fun flush(reason: String) {
            if (pending.size < MIN_SEGMENT_SAMPLES) {
                pending.clear()
                active = false
                activeMs = 0
                silenceFrames = 0
                speechFrames = 0
                peakRms = 0f
                return
            }
            val samples = ShortArray(pending.size)
            for (i in pending.indices) samples[i] = pending[i]
            val metrics = analyzeSegmentSamples(samples)
            val segment = S2sSegment(
                id = nextSegmentId,
                generation = 0L,
                samples = samples,
                speechFrames = speechFrames,
                peakRms = maxOf(peakRms, metrics.peakRms),
                meanRms = metrics.meanRms,
                energeticFrames = metrics.energeticFrames,
                speechLikeFrames = metrics.speechLikeFrames,
                activeMs = activeMs,
            )
            val vadSnapshot = adaptiveVad.snapshot(backlogMs.get())
            val shouldEmit = isSegmentWorthSending(segment, vadSnapshot)
            if (shouldEmit) {
                val id = nextSegmentId++
                val worker = (id % SESSION_COUNT).toInt()
                generation[worker] += 1
                val audioMs = samples.size * 1000 / SAMPLE_RATE
                backlogMs.addAndGet(audioMs)
                Log.i(
                    logTag,
                    "queued segment=$id worker=$worker audio_ms=$audioMs reason=$reason speech_frames=$speechFrames speech_ratio=${"%.2f".format(Locale.US, segmentSpeechRatio(segment))} speech_like_ratio=${"%.2f".format(Locale.US, segmentSpeechLikeRatio(segment))} confidence=${"%.2f".format(Locale.US, segmentSpeechConfidence(segment))} strictness=${"%.2f".format(Locale.US, vadSnapshot.strictness)} mean_rms=${"%.4f".format(Locale.US, segment.meanRms)} peak_rms=${"%.4f".format(Locale.US, segment.peakRms)} backlog_ms=${backlogMs.get()}",
                )
                emitSegment(
                    segment.copy(
                        id = id,
                        generation = generation[worker],
                    ),
                )
            } else {
                Log.i(
                    logTag,
                    "vad-skip segment=${nextSegmentId} strictness=${"%.2f".format(Locale.US, vadSnapshot.strictness)} confidence=${"%.2f".format(Locale.US, segmentSpeechConfidence(segment))} speech_like_ratio=${"%.2f".format(Locale.US, segmentSpeechLikeRatio(segment))} speech_ratio=${"%.2f".format(Locale.US, segmentSpeechRatio(segment))} mean_rms=${"%.4f".format(Locale.US, segment.meanRms)} peak_rms=${"%.4f".format(Locale.US, segment.peakRms)}",
                )
            }
            pending.clear()
            active = false
            activeMs = 0
            silenceFrames = 0
            speechFrames = 0
            peakRms = 0f
        }

        fun addPreroll(sample: Short) {
            if (preroll.size >= PREROLL_SAMPLES) {
                preroll.removeFirst()
            }
            preroll.addLast(sample)
        }

        Log.i(
            logTag,
            "vad-start sample_rate=$SAMPLE_RATE frame_ms=$FRAME_MS min_segment_ms=${MIN_SEGMENT_SAMPLES * 1000 / SAMPLE_RATE}",
        )
        audioChunks.collect { chunk ->
            totalChunks++
            for (sample in chunk) {
                frame[frameFill++] = sample
                if (frameFill < FRAME_SAMPLES) {
                    continue
                }
                val rms = rms(frame)
                totalFrames++
                windowFrames++
                windowPeakRms = maxOf(windowPeakRms, rms)
                peakRms = maxOf(peakRms, rms)
                val threshold = (noiseFloor * SPEECH_THRESHOLD_MULTIPLIER)
                    .coerceIn(MIN_SPEECH_THRESHOLD, MAX_SPEECH_THRESHOLD)
                val isSpeech = rms >= threshold || rms >= ABSOLUTE_SPEECH_RMS
                if (isSpeech) {
                    windowSpeechFrames++
                }
                if (!isSpeech && rms <= noiseFloor * NOISE_LEARN_THRESHOLD_RATIO && rms <= NOISE_LEARN_MAX_RMS) {
                    noiseFloor = (noiseFloor * 0.95f) + (rms * 0.05f)
                }
                if (isSpeech) {
                    if (!active) {
                        active = true
                        pending.addAll(preroll)
                        preroll.clear()
                    }
                    speechFrames++
                    activeMs += FRAME_MS
                    silenceFrames = 0
                } else if (active) {
                    silenceFrames++
                }
                if (active) {
                    for (value in frame) pending.add(value)
                    if (pending.size >= MAX_SEGMENT_SAMPLES) {
                        flush("max")
                    } else if (pending.size >= MIN_SEGMENT_SAMPLES && silenceFrames >= END_SILENCE_FRAMES) {
                        flush("silence")
                    }
                } else {
                    for (value in frame) addPreroll(value)
                }
                frameFill = 0
                val nowMs = SystemClock.elapsedRealtime()
                if (nowMs - lastHealthAtMs >= VAD_HEALTH_INTERVAL_MS) {
                    Log.i(
                        logTag,
                        "vad frames=$windowFrames speech_frames=$windowSpeechFrames " +
                            "peak_rms=${"%.4f".format(Locale.US, windowPeakRms)} " +
                            "noise_floor=${"%.4f".format(Locale.US, noiseFloor)} " +
                            "threshold=${"%.4f".format(Locale.US, threshold)} " +
                            "pending_ms=${pending.size * 1000 / SAMPLE_RATE} active_ms=$activeMs " +
                            "next_segment=$nextSegmentId chunks=$totalChunks total_frames=$totalFrames",
                    )
                    lastHealthAtMs = nowMs
                    windowFrames = 0
                    windowSpeechFrames = 0
                    windowPeakRms = 0f
                }
            }
        }
        Log.i(
            logTag,
            "vad-exit final_segment=$nextSegmentId pending_ms=${pending.size * 1000 / SAMPLE_RATE} " +
                "chunks=$totalChunks total_frames=$totalFrames",
        )
        if (pending.isNotEmpty()) {
            flush("stop")
        }
    }
}

private fun geminiLiveAudioLogTag(model: String): String {
    return if (isGeminiLiveTranslateApiModel(model)) "RealtimeLiveTranslateAndroid" else TAG
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

private fun shouldSendAudioStreamEnd(model: String): Boolean {
    return !isGeminiLiveTranslateApiModel(model)
}

private fun isGeminiLiveTranslateApiModel(model: String): Boolean {
    return model == RealtimeModelIds.GEMINI_LIVE_TRANSLATE_API_MODEL || model.contains("live-translate")
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

private const val LIVE_TRANSLATE_PLAYBACK_QUEUE_CAPACITY = 48
private const val LIVE_TRANSLATE_COMMUNICATION_VOLUME_BOOST = 1.8f
private const val LIVE_TRANSLATE_SLOW_SEND_LOG_MS = 120L
private const val LIVE_TRANSLATE_SLOW_DRAIN_LOG_MS = 120L
private const val LIVE_TRANSLATE_SERVER_SILENT_SENT_CHUNKS = 100
private const val LIVE_TRANSLATE_SERVER_SILENT_MS = 15_000L
private const val LIVE_TRANSLATE_PROACTIVE_ROTATE_MS = 12 * 60 * 1_000L
private const val LIVE_TRANSLATE_ROTATE_QUIET_MS = 3_000L
