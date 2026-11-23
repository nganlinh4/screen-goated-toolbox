package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveMediaResolution
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveBackoffPolicy
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSetupSpec
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveTranscriptionMode
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import dev.screengoated.toolbox.mobile.shared.live.GeminiTranscribeVad
import dev.screengoated.toolbox.mobile.shared.live.buildGeminiLiveSetup
import dev.screengoated.toolbox.mobile.shared.live.geminiLiveWebSocketRequest
import dev.screengoated.toolbox.mobile.shared.live.parseGeminiLiveServerFrame
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeoutOrNull
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.serialization.json.buildJsonArray
import kotlinx.serialization.json.buildJsonObject
import kotlinx.serialization.json.put
import kotlinx.serialization.json.JsonPrimitive
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.atomic.AtomicBoolean

class GeminiLiveSocketClient(
    private val httpClient: OkHttpClient,
) {
    private enum class AudioMode { NORMAL, SILENCE, CATCH_UP }

    /**
     * Runs a long-lived Gemini Live session with automatic WebSocket reconnection,
     * matching the Windows audio streaming implementation:
     * - Normal mode: stream real audio for 20s
     * - Silence mode: send silence for 2s while buffering real audio
     * - CatchUp mode: replay buffered audio at 2x speed
     * - On connection loss/stall: reconnect and enter CatchUp
     */
    suspend fun runSession(
        apiKey: String,
        model: String,
        audioChunks: Flow<ShortArray>,
        onTranscript: (String, Boolean) -> Unit,
        onAudioGap: () -> Unit = {},
    ) {
        val isLiveTranscribe = isLiveTranscribeModel(model)
        val audioBuffer = PendingLiveAudio()
        val gapReported = AtomicBoolean(false)
        fun reportGap() { if (gapReported.compareAndSet(false, true)) onAudioGap() }
        var audioMode = AudioMode.NORMAL
        var modeStartMs = android.os.SystemClock.elapsedRealtime()
        var consecutiveEmptyPolls = 0
        val recovery = GeminiTranscriptionRecovery()
        var connectionStartedMs = android.os.SystemClock.elapsedRealtime()
        var vocabulary = GeminiTranscribeVocabulary.snapshot()
        val resumption = LiveResumption()
        val hybridVad = GeminiTranscribeVad()

        // Connect initial socket
        var session = connectAndSetup(apiKey, model, vocabulary.entries, null)
            ?: throw IOException("Gemini Live initial connection failed.")

        coroutineScope {
        // Collect audio in background, buffer it for the streaming loop
        val collectJob = launch(Dispatchers.IO) {
            audioChunks.collect { chunk ->
                if (audioBuffer.append(chunk) > 0) reportGap()
            }
        }

        suspend fun reconnectSession(entries: List<String>): LiveSession? {
            session.inbox.retire()
            session.socket.cancel()
            return recoverRetiredLiveSession(
                events = session.inbox.events,
                onTranscript = { text, isFinal ->
                    recovery.reset()
                    consecutiveEmptyPolls = 0
                    gapReported.set(false)
                    onTranscript(text, isFinal)
                },
                onResumption = { resumption.handle = it },
                onProgress = { recovery.reset() },
                reconnect = { tryReconnect(apiKey, model, resumption, entries) { collectJob.isCompleted } },
            )
        }

        try {
            sessionLoop@ while (isActive && !collectJob.isCancelled) {
                val latestVocabulary = GeminiTranscribeVocabulary.snapshot()
                if (isLiveTranscribe &&
                    (android.os.SystemClock.elapsedRealtime() - connectionStartedMs >= ROTATE_AT_MS || latestVocabulary.version != vocabulary.version) &&
                    hybridVad.isSafeGap()
                ) {
                    session = reconnectSession(latestVocabulary.entries)
                        ?: break@sessionLoop
                    vocabulary = latestVocabulary
                    connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                    hybridVad.reset()
                    recovery.reset()
                    audioMode = AudioMode.CATCH_UP
                }
                // Audio mode state machine transitions
                if (audioMode == AudioMode.CATCH_UP && audioBuffer.isEmpty()) {
                    audioMode = AudioMode.NORMAL
                    modeStartMs = android.os.SystemClock.elapsedRealtime()
                }
                val elapsed = android.os.SystemClock.elapsedRealtime() - modeStartMs
                if (!isLiveTranscribe) {
                    when (audioMode) {
                        AudioMode.NORMAL -> {
                            if (elapsed >= NORMAL_DURATION_MS) {
                                audioMode = AudioMode.SILENCE
                                modeStartMs = android.os.SystemClock.elapsedRealtime()
                            }
                        }
                        AudioMode.SILENCE -> {
                            if (elapsed >= SILENCE_DURATION_MS) {
                                recovery.inputFlushed(android.os.SystemClock.elapsedRealtime())
                                audioMode = AudioMode.CATCH_UP
                                modeStartMs = android.os.SystemClock.elapsedRealtime()
                            }
                        }
                        AudioMode.CATCH_UP -> {
                            if (audioBuffer.isEmpty()) {
                                audioMode = AudioMode.NORMAL
                                modeStartMs = android.os.SystemClock.elapsedRealtime()
                            }
                        }
                    }
                }

                // Leave later samples queued while silence or catch-up is sent.
                val realAudio = when (audioMode) {
                    AudioMode.NORMAL -> audioBuffer.take(MAX_PENDING_LIVE_SAMPLES)
                    AudioMode.SILENCE -> ShortArray(0)
                    AudioMode.CATCH_UP -> audioBuffer.take(SAMPLES_PER_100MS * 2)
                }
                val payload = if (audioMode == AudioMode.SILENCE) ShortArray(SAMPLES_PER_100MS) else realAudio
                var failedFrameEnd = sendChunked(session.socket, payload, CHUNK_SIZE)
                if (failedFrameEnd == null && realAudio.isNotEmpty()) {
                    observeActiveAudio(realAudio, recovery)
                    if (isLiveTranscribe && hybridVad.observe(realAudio, android.os.SystemClock.elapsedRealtime()) &&
                        !session.socket.send(AUDIO_STREAM_END_MESSAGE)
                    ) failedFrameEnd = realAudio.size
                }
                if (failedFrameEnd == null && isLiveTranscribe && realAudio.isEmpty() &&
                    hybridVad.pollEnd(android.os.SystemClock.elapsedRealtime()) &&
                    !session.socket.send(AUDIO_STREAM_END_MESSAGE)
                ) failedFrameEnd = 0
                val sendOk = failedFrameEnd == null
                if (!sendOk && audioMode != AudioMode.SILENCE) {
                    // The attempted frame has uncertain delivery; only its never-attempted suffix is replayable.
                    val suffix = realAudio.copyOfRange(requireNotNull(failedFrameEnd), realAudio.size)
                    if (audioBuffer.prepend(suffix) > 0) reportGap()
                }

                if (!sendOk) {
                    reportGap()
                    session = reconnectSession(vocabulary.entries)
                        ?: break@sessionLoop
                    audioMode = AudioMode.CATCH_UP
                    connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                    hybridVad.reset()
                    recovery.reset()
                    modeStartMs = android.os.SystemClock.elapsedRealtime()
                    consecutiveEmptyPolls = 0
                    continue
                }

                // Read transcriptions from the incoming queue
                var readCount = 0
                while (readCount < 20) {
                    val event = session.inbox.events.poll() ?: break
                    readCount++
                    when (event) {
                        LiveSocketEvent.Progress -> recovery.reset()
                        is LiveSocketEvent.Transcript -> {
                            recovery.reset()
                            consecutiveEmptyPolls = 0
                            gapReported.set(false)
                            onTranscript(event.text, event.isFinal)
                        }
                        is LiveSocketEvent.Resumption -> resumption.handle = event.handle
                        LiveSocketEvent.GoAway -> {
                            session = reconnectSession(vocabulary.entries)
                                ?: break@sessionLoop
                            connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                            hybridVad.reset()
                            recovery.reset()
                            audioMode = AudioMode.CATCH_UP
                            break
                        }
                        is LiveSocketEvent.Error -> {
                            if (!event.retryable) throw IOException(event.message)
                            reportGap()
                            session = reconnectSession(vocabulary.entries)
                                ?: break@sessionLoop
                            connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                            hybridVad.reset()
                            recovery.reset()
                            audioMode = AudioMode.CATCH_UP
                            modeStartMs = android.os.SystemClock.elapsedRealtime()
                            consecutiveEmptyPolls = 0
                            break
                        }
                        is LiveSocketEvent.Closed -> {
                            // Server closed — reconnect
                            reportGap()
                            session = reconnectSession(vocabulary.entries)
                                ?: break@sessionLoop
                            audioMode = AudioMode.CATCH_UP
                            connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                            hybridVad.reset()
                            recovery.reset()
                            modeStartMs = android.os.SystemClock.elapsedRealtime()
                            consecutiveEmptyPolls = 0
                            break
                        }
                    }
                }
                if (readCount == 0) {
                    consecutiveEmptyPolls++
                }

                // Degradation detection: stalled connection
                if (!isLiveTranscribe &&
                    consecutiveEmptyPolls >= EMPTY_READ_CHECK_COUNT &&
                    recovery.shouldReconnect(android.os.SystemClock.elapsedRealtime())
                ) {
                    reportGap()
                    session = reconnectSession(vocabulary.entries)
                        ?: break@sessionLoop
                    connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                    hybridVad.reset()
                    recovery.reset()
                    audioMode = AudioMode.CATCH_UP
                    modeStartMs = android.os.SystemClock.elapsedRealtime()
                    consecutiveEmptyPolls = 0
                    continue
                }

                // If audio flow completed (mic stopped), exit
                if (collectJob.isCompleted) break

                delay(SEND_INTERVAL_MS)
            }
        } finally {
            collectJob.cancel()
            session.inbox.retire()
            session.socket.close(1000, "SGT session finished")
        }
        } // coroutineScope
    }

    private data class LiveSession(
        val socket: WebSocket,
        val inbox: LiveEventInbox,
    )

    internal sealed class LiveSocketEvent {
        data class Transcript(val text: String, val isFinal: Boolean) : LiveSocketEvent()
        data class Error(val message: String, val retryable: Boolean) : LiveSocketEvent()
        data class Resumption(val handle: String) : LiveSocketEvent()
        data object Progress : LiveSocketEvent()
        data object GoAway : LiveSocketEvent()
        data object Closed : LiveSocketEvent()
    }

    private suspend fun connectAndSetup(
        apiKey: String,
        model: String,
        vocabulary: List<String>,
        resumptionHandle: String?,
        captureEnded: () -> Boolean = { false },
    ): LiveSession? {
        val inbox = LiveEventInbox()
        val events = inbox.events
        val setupReady = CompletableDeferred<Unit>()

        val request = geminiLiveWebSocketRequest(apiKey)

        val socket = httpClient.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    inbox.admit {
                        sendLiveSetup(setupReady) {
                            webSocket.send(buildSetupPayload(model, vocabulary, resumptionHandle))
                        }
                    }
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    inbox.admit { handleMessage(text, events, setupReady) }
                }

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                    inbox.admit { handleMessage(bytes.utf8(), events, setupReady) }
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    inbox.admit { closeLiveSocket(setupReady, events) }
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    inbox.admit { closeLiveSocket(setupReady, events) }
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    inbox.admit {
                    if (!setupReady.isCompleted) {
                        val code = response?.code
                        val fatal = code != null && code in 400..499 && code != 408 && code != 429
                        setupReady.completeExceptionally(if (fatal) LiveSetupFailure("Live setup rejected", false) else t)
                    }
                    events.offer(LiveSocketEvent.Closed)
                    }
                }
            },
        )

        return try {
            if (withTimeoutOrNull(20_000) { awaitLiveSetup(setupReady, captureEnded) } != true) {
                inbox.retire()
                socket.cancel()
                return null
            }
            LiveSession(socket, inbox)
        } catch (cancelled: CancellationException) {
            inbox.retire()
            socket.cancel()
            throw cancelled
        } catch (error: LiveSetupFailure) {
            inbox.retire()
            socket.cancel()
            if (!error.retryable) throw error
            null
        } catch (_: Exception) {
            inbox.retire()
            socket.cancel()
            null
        }
    }

    internal fun handleMessage(
        message: String,
        events: LinkedBlockingDeque<LiveSocketEvent>,
        setupReady: CompletableDeferred<Unit>,
    ) {
        val frame = parseGeminiLiveServerFrame(message) ?: return
        frame.error?.let { error ->
            if (!setupReady.isCompleted) {
                setupReady.completeExceptionally(LiveSetupFailure(error, frame.errorRetryable))
            }
            events.offer(LiveSocketEvent.Error(error, frame.errorRetryable))
            return
        }

        if (frame.contentCount > 0 || frame.responseComplete || frame.interrupted) {
            events.offer(LiveSocketEvent.Progress)
        }
        frame.sessionResumption?.takeIf { it.resumable }?.handle?.let {
            events.offer(LiveSocketEvent.Resumption(it))
        }
        if (frame.inputTranscript != null) {
            val transcript = frame.inputTranscript
            events.offer(LiveSocketEvent.Transcript(transcript, isFinal = true))
        } else if (frame.interimInputTranscript != null) {
            val transcript = frame.interimInputTranscript
            events.offer(LiveSocketEvent.Transcript(transcript, isFinal = false))
        }
        if (frame.goAway) {
            events.offer(LiveSocketEvent.GoAway)
        }
        if (frame.setupComplete && !setupReady.isCompleted) {
            setupReady.complete(Unit)
        }
    }

    private suspend fun tryReconnect(
        apiKey: String,
        model: String,
        resumption: LiveResumption,
        vocabulary: List<String>,
        captureEnded: () -> Boolean,
    ): LiveSession? {
        val backoff = GeminiLiveBackoffPolicy()
        return reconnectLive(resumption, MAX_RECONNECT_ATTEMPTS, captureEnded,
            connect = { handle -> connectAndSetup(apiKey, model, vocabulary, handle, captureEnded) },
            pause = { attempt -> delayWhileCaptureActive(backoff.delayMs(attempt), captureEnded) },
        )
    }

    /** Null means every frame was accepted; otherwise returns the end of the uncertain frame. */
    private fun sendChunked(socket: WebSocket, samples: ShortArray, chunkSize: Int): Int? =
        sendLiveAudioFrames(samples, chunkSize) { socket.send(buildGeminiS2sAudioPayload(it)) }

    internal fun buildSetupPayload(
        model: String,
        vocabulary: List<String> = emptyList(),
        resumptionHandle: String? = null,
    ): String {
        val isLiveTranscribe = isLiveTranscribeModel(model)
        return buildGeminiLiveSetup(
            GeminiLiveSetupSpec(
                apiModel = model,
                responseModalities = if (isLiveTranscribe) listOf("TEXT") else listOf("AUDIO"),
                mediaResolution = if (isLiveTranscribe) null else GeminiLiveMediaResolution.LOW,
                transcriptionMode = GeminiLiveTranscriptionMode.INPUT,
                inputAudioTranscriptionConfig = if (isLiveTranscribe) {
                    buildJsonObject {
                        put("languageCodes", buildJsonArray {})
                        put("mode", "SMART")
                        put("customVocabulary", buildJsonArray {
                            vocabulary.forEach { add(JsonPrimitive(it)) }
                        })
                    }
                } else {
                    buildJsonObject {}
                },
                setupExtensions = if (isLiveTranscribe) {
                    buildJsonObject {
                        put("sessionResumption", buildJsonObject {
                            resumptionHandle?.let { put("handle", it) }
                        })
                    }
                } else {
                    buildJsonObject {}
                },
            ),
        ).toString()
    }

    private fun observeActiveAudio(samples: ShortArray, recovery: GeminiTranscriptionRecovery) {
        val rms = kotlin.math.sqrt(samples.sumOf {
            val normalized = it.toDouble() / 32768.0
            normalized * normalized
        } / samples.size.coerceAtLeast(1))
        if (rms >= 0.004) recovery.audioSent(samples.size.toLong() * 1_000 / 16_000)
    }

    private fun isLiveTranscribeModel(model: String): Boolean =
        GeneratedLiveModelCatalog.endpointProfile(model)
            ?.protocol == "live-transcribe"

    private companion object {
        private const val MAX_RECONNECT_ATTEMPTS = 6
        private const val ROTATE_AT_MS = 540_000L
        private const val AUDIO_STREAM_END_MESSAGE = "{\"realtimeInput\":{\"audioStreamEnd\":true}}"
        private const val NORMAL_DURATION_MS = 20_000L
        private const val SILENCE_DURATION_MS = 2_000L
        private const val SAMPLES_PER_100MS = 1_600
        private const val CHUNK_SIZE = 1_600
        private const val SEND_INTERVAL_MS = 100L
        private const val EMPTY_READ_CHECK_COUNT = 50
    }

}
