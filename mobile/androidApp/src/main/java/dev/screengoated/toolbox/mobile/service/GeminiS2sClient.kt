package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.service.tts.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.tts.WebSocketEvent
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
    private val player = AudioTrackPlayer(context)

    suspend fun runSession(
        apiKey: String,
        model: String,
        audioChunks: kotlinx.coroutines.flow.Flow<ShortArray>,
        settingsProvider: () -> GeminiS2sRuntimeSettings,
        onDisplay: (S2sDisplaySnapshot) -> Unit,
    ) = coroutineScope {
        val startMs = SystemClock.elapsedRealtime()
        val initialSettings = settingsProvider()
        Log.i(
            TAG,
            "session-start model=$model target=${initialSettings.targetLanguage} " +
                "voice=${initialSettings.globalTts.voice} speed=${initialSettings.realtime.speedPercent} " +
                "volume=${initialSettings.realtime.volumePercent}",
        )
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
                logTag = TAG,
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
                    )
                }
            }
        }

        try {
            collectSegments(audioChunks, adaptiveVad, backlogMs) { segment ->
                workerChannels[(segment.id % SESSION_COUNT).toInt()].send(segment)
            }
        } finally {
            Log.i(TAG, "session-exit uptime_ms=${SystemClock.elapsedRealtime() - startMs}")
            workerChannels.forEach { it.close() }
            workers.forEach { it.cancelAndJoin() }
            attempts.close()
            coordinator.cancelAndJoin()
            player.stopImmediate()
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
        )
        if (first == SegmentResult.OK) {
            adaptiveVad.observe(AdaptiveS2sVadOutcome.HEALTHY, segment)
        } else if (first == SegmentResult.RETRY_FRESH && currentCoroutineContext().isActive) {
            Log.i(TAG, "retry segment=${segment.id} worker=$workerIndex gen=${segment.generation}")
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
    ): SegmentResult = coroutineScope {
        Log.i(TAG, "hedge segment=${segment.id} worker=$workerIndex gen=$baseGeneration attempts=$HEDGE_ATTEMPTS")
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
                    )
                } catch (error: Throwable) {
                    if (error is CancellationException) {
                        throw error
                    }
                    Log.w(
                        TAG,
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
                                TAG,
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
                Log.i(TAG, "hedge-empty segment=${segment.id} worker=$workerIndex gen=$baseGeneration attempts=$HEDGE_ATTEMPTS")
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
    ) {
        val startedAtMs = SystemClock.elapsedRealtime()
        val request = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()
        BlockingWebSocketSession(httpClient, request).use { session ->
            if (!withContext(Dispatchers.IO) { session.awaitOpen(10_000) }) {
                Log.w(TAG, "open-failed segment=${segment.id} gen=${segment.generation} attempt=$attempt")
                output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                return
            }
            Log.i(TAG, "open-ok segment=${segment.id} gen=${segment.generation} attempt=$attempt")
            val setupPayload = buildGeminiS2sSetupPayload(
                model = model,
                settings = settings,
                contextText = contextText,
            )
            if (!session.sendText(setupPayload) || !waitForGeminiS2sSetup(session, TAG)) {
                Log.w(TAG, "setup-failed segment=${segment.id} gen=${segment.generation} attempt=$attempt")
                output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                return
            }
            Log.i(
                TAG,
                "start segment=${segment.id} gen=${segment.generation} attempt=$attempt audio_ms=${segment.audioMs} context_chars=${contextText.length}",
            )
            for (offset in segment.samples.indices step FRAME_SAMPLES) {
                val end = minOf(offset + FRAME_SAMPLES, segment.samples.size)
                if (!session.sendText(buildGeminiS2sAudioPayload(segment.samples.copyOfRange(offset, end)))) {
                    output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                    return
                }
            }
            session.sendText(buildGeminiS2sAudioStreamEndPayload())

            var firstAudioAtMs = 0L
            var lastAudioAtMs = 0L
            var audioChunks = 0
            var textUpdates = 0
            var emptyReads = 0
            val retryThreshold = if (segment.activeMs > 0) FIRST_AUDIO_ACTIVE_RETRY_MS else FIRST_AUDIO_SILENT_RETRY_MS
            while (currentCoroutineContext().isActive) {
                val now = SystemClock.elapsedRealtime()
                if (firstAudioAtMs == 0L && now - startedAtMs >= retryThreshold) {
                    Log.i(
                        TAG,
                        "done segment=${segment.id} gen=${segment.generation} attempt=$attempt reason=no_first_audio_retry retry_ms=$retryThreshold text_updates=$textUpdates chunks=$audioChunks first_audio_ms=none",
                    )
                    output.send(S2sRaceEvent.Retry(segment.id, segment.generation, attempt))
                    return
                }
                if (firstAudioAtMs != 0L && now - lastAudioAtMs >= AUDIO_IDLE_FINISH_MS) {
                    Log.i(
                        TAG,
                        "done segment=${segment.id} gen=${segment.generation} attempt=$attempt reason=audio_idle chunks=$audioChunks first_audio_ms=${firstAudioAtMs - startedAtMs}",
                    )
                    output.send(S2sRaceEvent.Done(segment.id, segment.generation, attempt))
                    return
                }
                if (now - startedAtMs > SEGMENT_ATTEMPT_TIMEOUT_MS) {
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
                                TAG,
                                "first-audio segment=${segment.id} gen=${segment.generation} attempt=$attempt elapsed_ms=${firstAudioAtMs - startedAtMs}",
                            )
                        }
                        lastAudioAtMs = SystemClock.elapsedRealtime()
                        audioChunks++
                        output.trySend(S2sRaceEvent.Audio(segment.id, segment.generation, attempt, bytes))
                    }
                    if ((parsed.turnComplete || parsed.generationComplete) && audioChunks > 0) {
                        Log.i(
                            TAG,
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
                                TAG,
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
                    TAG,
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
                    TAG,
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
            TAG,
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
                        TAG,
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
            TAG,
            "vad-exit final_segment=$nextSegmentId pending_ms=${pending.size * 1000 / SAMPLE_RATE} " +
                "chunks=$totalChunks total_frames=$totalFrames",
        )
        if (pending.isNotEmpty()) {
            flush("stop")
        }
    }

    private fun rms(samples: ShortArray): Float {
        var sum = 0.0
        for (sample in samples) {
            val normalized = sample / 32768.0
            sum += normalized * normalized
        }
        return sqrt(sum / samples.size).toFloat()
    }

    private data class SegmentMetrics(
        val meanRms: Float,
        val peakRms: Float,
        val energeticFrames: Int,
        val speechLikeFrames: Int,
    )

    private class AdaptiveS2sVadState {
        private var strictness = 0f
        private var consecutiveEmptyNoInput = 0

        @Synchronized
        fun snapshot(backlogMs: Int): AdaptiveS2sVadSnapshot {
            val backlogPressure = (backlogMs.toFloat() / 30_000f).coerceIn(0f, 0.55f)
            return AdaptiveS2sVadSnapshot(strictness = maxOf(strictness, backlogPressure))
        }

        @Synchronized
        fun observe(outcome: AdaptiveS2sVadOutcome, segment: S2sSegment) {
            when (outcome) {
                AdaptiveS2sVadOutcome.HEALTHY -> {
                    consecutiveEmptyNoInput = 0
                    strictness = (strictness - 0.10f).coerceAtLeast(0f)
                }
                AdaptiveS2sVadOutcome.EMPTY_NO_INPUT -> {
                    consecutiveEmptyNoInput += 1
                    val highEnergy = segment.meanRms >= 0.025f ||
                        segment.peakRms >= 0.060f ||
                        speechRatio(segment) >= 0.60f
                    val step = if (highEnergy) 0.22f else 0.12f
                    strictness = (strictness + step).coerceAtMost(1f)
                }
                AdaptiveS2sVadOutcome.RETRY_FRESH -> Unit
            }
            Log.i(
                TAG,
                "adaptive-vad outcome=$outcome strictness=${"%.2f".format(Locale.US, strictness)} consecutive_empty=$consecutiveEmptyNoInput segment=${segment.id} confidence=${"%.2f".format(Locale.US, speechConfidence(segment))} speech_like_ratio=${"%.2f".format(Locale.US, speechLikeRatio(segment))} speech_ratio=${"%.2f".format(Locale.US, speechRatio(segment))} mean_rms=${"%.4f".format(Locale.US, segment.meanRms)} peak_rms=${"%.4f".format(Locale.US, segment.peakRms)}",
            )
        }

        private fun speechRatio(segment: S2sSegment): Float {
            val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
            return segment.speechFrames.toFloat() / frameCount.toFloat()
        }

        private fun speechLikeRatio(segment: S2sSegment): Float {
            val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
            return segment.speechLikeFrames.toFloat() / frameCount.toFloat()
        }

        private fun energeticRatio(segment: S2sSegment): Float {
            val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
            return segment.energeticFrames.toFloat() / frameCount.toFloat()
        }

        private fun speechConfidence(segment: S2sSegment): Float {
            val energyScore = (segment.meanRms / 0.055f).coerceIn(0f, 1f)
            return (speechLikeRatio(segment) * 0.45f) +
                (speechRatio(segment) * 0.30f) +
                (energeticRatio(segment) * 0.15f) +
                (energyScore * 0.10f)
        }
    }

    private data class AdaptiveS2sVadSnapshot(val strictness: Float = 0f)

    private fun analyzeSegmentSamples(samples: ShortArray): SegmentMetrics {
        if (samples.isEmpty()) {
            return SegmentMetrics(0f, 0f, 0, 0)
        }
        var rmsSum = 0f
        var peakRms = 0f
        var energeticFrames = 0
        var speechLikeFrames = 0
        var frameCount = 0
        var offset = 0
        while (offset < samples.size) {
            val end = minOf(offset + FRAME_SAMPLES, samples.size)
            val frame = samples.copyOfRange(offset, end)
            val frameRms = rms(frame)
            frameCount++
            rmsSum += frameRms
            peakRms = maxOf(peakRms, frameRms)
            if (frameRms >= MIN_SPEECH_THRESHOLD) energeticFrames++
            if (isSpeechLikeFrame(frame, frameRms)) speechLikeFrames++
            offset = end
        }
        return SegmentMetrics(
            meanRms = rmsSum / frameCount.coerceAtLeast(1),
            peakRms = peakRms,
            energeticFrames = energeticFrames,
            speechLikeFrames = speechLikeFrames,
        )
    }

    private fun isSpeechLikeFrame(frame: ShortArray, frameRms: Float): Boolean {
        if (frame.size < 2 || frameRms < MIN_SPEECH_THRESHOLD) return false
        var peak = 0f
        var zeroCrossings = 0
        for (index in frame.indices) {
            peak = maxOf(peak, kotlin.math.abs(frame[index] / 32768f))
            if (index > 0) {
                val prev = frame[index - 1]
                val current = frame[index]
                if ((prev < 0 && current >= 0) || (prev >= 0 && current < 0)) {
                    zeroCrossings++
                }
            }
        }
        val crest = peak / frameRms.coerceAtLeast(0.0001f)
        val zcr = zeroCrossings.toFloat() / (frame.size - 1).toFloat()
        return zcr in 0.015f..0.24f && crest in 1.2f..18.0f
    }

    private fun isSegmentWorthSending(segment: S2sSegment, vad: AdaptiveS2sVadSnapshot): Boolean {
        val speechRatio = segmentSpeechRatio(segment)
        val speechLikeRatio = segmentSpeechLikeRatio(segment)
        val confidence = segmentSpeechConfidence(segment)
        val baseline = segment.speechFrames >= MIN_SEGMENT_SPEECH_FRAMES ||
            speechRatio >= MIN_SEGMENT_SPEECH_RATIO ||
            (segment.peakRms >= MIN_SEGMENT_PEAK_RMS && speechLikeRatio >= 0.08f)
        if (!baseline) return false

        if (vad.strictness <= 0f) {
            return confidence >= 0.18f || speechLikeRatio >= 0.08f
        }

        val minSpeechLike = MIN_SPEECH_LIKE_RATIO +
            (STRICT_MIN_SPEECH_LIKE_RATIO - MIN_SPEECH_LIKE_RATIO) * vad.strictness
        val minConfidence = 0.24f +
            (STRICT_MIN_SPEECH_CONFIDENCE - 0.24f) * vad.strictness
        return speechLikeRatio >= minSpeechLike || confidence >= minConfidence
    }

    private fun segmentSpeechRatio(segment: S2sSegment): Float {
        val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
        return segment.speechFrames.toFloat() / frameCount.toFloat()
    }

    private fun segmentSpeechLikeRatio(segment: S2sSegment): Float {
        val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
        return segment.speechLikeFrames.toFloat() / frameCount.toFloat()
    }

    private fun segmentEnergeticRatio(segment: S2sSegment): Float {
        val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
        return segment.energeticFrames.toFloat() / frameCount.toFloat()
    }

    private fun segmentSpeechConfidence(segment: S2sSegment): Float {
        val energyScore = (segment.meanRms / 0.055f).coerceIn(0f, 1f)
        return (segmentSpeechLikeRatio(segment) * 0.45f) +
            (segmentSpeechRatio(segment) * 0.30f) +
            (segmentEnergeticRatio(segment) * 0.15f) +
            (energyScore * 0.10f)
    }

    private companion object {
        private const val TAG = "RealtimeS2SAndroid"
        private const val LIVE_WS_ENDPOINT =
            "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
        private const val SAMPLE_RATE = 16_000
        private const val FRAME_SAMPLES = 1_600
        private const val FRAME_MS = 100L
        private const val PREROLL_SAMPLES = 4_000
        private const val MIN_SEGMENT_SAMPLES = 16_000
        private const val TARGET_SEGMENT_SAMPLES = 48_000
        private const val MAX_SEGMENT_SAMPLES = 80_000
        private const val END_SILENCE_FRAMES = 3
        private const val HEDGE_ATTEMPTS = 2
        private const val SESSION_COUNT = 3
        private const val FIRST_AUDIO_SILENT_RETRY_MS = 3_800L
        private const val FIRST_AUDIO_ACTIVE_RETRY_MS = 5_200L
        private const val AUDIO_IDLE_FINISH_MS = 1_200L
        private const val SEGMENT_ATTEMPT_TIMEOUT_MS = 30_000L
        private const val SPEECH_THRESHOLD_MULTIPLIER = 2.2f
        private const val MIN_SPEECH_THRESHOLD = 0.012f
        private const val MAX_SPEECH_THRESHOLD = 0.035f
        private const val ABSOLUTE_SPEECH_RMS = 0.045f
        private const val NOISE_LEARN_MAX_RMS = 0.018f
        private const val NOISE_LEARN_THRESHOLD_RATIO = 0.60f
        private const val MIN_SEGMENT_SPEECH_FRAMES = 4
        private const val MIN_SEGMENT_PEAK_RMS = 0.025f
        private const val MIN_SEGMENT_SPEECH_RATIO = 0.08f
        private const val MIN_SPEECH_LIKE_RATIO = 0.18f
        private const val STRICT_MIN_SPEECH_LIKE_RATIO = 0.32f
        private const val STRICT_MIN_SPEECH_CONFIDENCE = 0.38f
        private const val VAD_HEALTH_INTERVAL_MS = 2_000L
    }
}
