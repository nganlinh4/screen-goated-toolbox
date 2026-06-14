package dev.screengoated.toolbox.mobile.service

import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.tts.WebSocketEvent
import kotlinx.coroutines.CancellationException
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
import java.util.Locale
import java.util.concurrent.atomic.AtomicInteger

internal suspend fun GeminiS2sClient.runSegmentWithRetry(
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

private suspend fun GeminiS2sClient.runHedgedSegment(
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

private suspend fun GeminiS2sClient.runAttempt(
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

internal suspend fun collectSegments(
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
