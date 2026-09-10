package dev.screengoated.toolbox.mobile.service

import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveMediaResolution
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
import kotlinx.coroutines.withTimeout
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
    ) {
        val isLiveTranscribe = isLiveTranscribeModel(model)
        val audioBuffer = LinkedBlockingDeque<ShortArray>()
        val silenceBuffer = mutableListOf<Short>()
        var audioMode = AudioMode.NORMAL
        var modeStartMs = android.os.SystemClock.elapsedRealtime()
        var consecutiveEmptyPolls = 0
        val recovery = GeminiTranscriptionRecovery()
        var connectionStartedMs = android.os.SystemClock.elapsedRealtime()
        var vocabulary = GeminiTranscribeVocabulary.snapshot()
        var resumptionHandle: String? = null
        val hybridVad = GeminiTranscribeVad()

        // Connect initial socket
        var session = connectAndSetup(apiKey, model, vocabulary.entries, null)
            ?: throw IOException("Gemini Live initial connection failed.")

        coroutineScope {
        // Collect audio in background, buffer it for the streaming loop
        val collectJob = launch(Dispatchers.IO) {
            audioChunks.collect { chunk ->
                audioBuffer.offer(chunk)
            }
        }

        try {
            while (isActive && !collectJob.isCancelled) {
                val latestVocabulary = GeminiTranscribeVocabulary.snapshot()
                if (isLiveTranscribe &&
                    (android.os.SystemClock.elapsedRealtime() - connectionStartedMs >= ROTATE_AT_MS || latestVocabulary.version != vocabulary.version) &&
                    hybridVad.isSafeGap()
                ) {
                    session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer, resumptionHandle, latestVocabulary.entries)
                        ?: break
                    vocabulary = latestVocabulary
                    connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                    hybridVad.reset()
                    recovery.reset()
                    audioMode = AudioMode.CATCH_UP
                }
                // Audio mode state machine transitions
                if (audioMode == AudioMode.CATCH_UP && silenceBuffer.isEmpty()) {
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
                                silenceBuffer.clear()
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
                            if (silenceBuffer.isEmpty()) {
                                audioMode = AudioMode.NORMAL
                                modeStartMs = android.os.SystemClock.elapsedRealtime()
                            }
                        }
                    }
                }

                // Drain audio buffer
                val realAudio = mutableListOf<Short>()
                while (true) {
                    val chunk = audioBuffer.poll() ?: break
                    for (s in chunk) realAudio.add(s)
                }

                // Send audio based on mode
                val sendOk = when (audioMode) {
                    AudioMode.NORMAL -> {
                        if (realAudio.isNotEmpty()) {
                            val samples = realAudio.toShortArray()
                            sendChunked(session.socket, samples, CHUNK_SIZE).also {
                                if (it) observeActiveAudio(samples, recovery)
                                if (it && isLiveTranscribe && hybridVad.observe(samples, android.os.SystemClock.elapsedRealtime())) {
                                    session.socket.send(AUDIO_STREAM_END_MESSAGE)
                                }
                            }
                        } else {
                            true
                        }
                    }
                    AudioMode.SILENCE -> {
                        silenceBuffer.addAll(realAudio)
                        val silence = ShortArray(SAMPLES_PER_100MS)
                        sendChunked(session.socket, silence, CHUNK_SIZE)
                    }
                    AudioMode.CATCH_UP -> {
                        silenceBuffer.addAll(realAudio)
                        val doubleChunk = SAMPLES_PER_100MS * 2
                        if (silenceBuffer.size >= doubleChunk) {
                            val toSend = ShortArray(doubleChunk) { silenceBuffer.removeAt(0) }
                            sendChunked(session.socket, toSend, CHUNK_SIZE).also {
                                if (it) observeActiveAudio(toSend, recovery)
                                if (it && isLiveTranscribe && hybridVad.observe(toSend, android.os.SystemClock.elapsedRealtime())) {
                                    session.socket.send(AUDIO_STREAM_END_MESSAGE)
                                }
                            }
                        } else if (silenceBuffer.isNotEmpty()) {
                            val toSend = ShortArray(silenceBuffer.size) { silenceBuffer.removeAt(0) }
                            sendChunked(session.socket, toSend, CHUNK_SIZE).also {
                                if (it) observeActiveAudio(toSend, recovery)
                                if (it && isLiveTranscribe && hybridVad.observe(toSend, android.os.SystemClock.elapsedRealtime())) {
                                    session.socket.send(AUDIO_STREAM_END_MESSAGE)
                                }
                            }
                        } else {
                            true
                        }
                    }
                }
                if (sendOk && isLiveTranscribe && realAudio.isEmpty() && hybridVad.pollEnd(android.os.SystemClock.elapsedRealtime())) {
                    session.socket.send(AUDIO_STREAM_END_MESSAGE)
                }

                if (!sendOk) {
                    // Send failed — reconnect
                    session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer, resumptionHandle, vocabulary.entries)
                        ?: break
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
                    val event = session.incomingEvents.poll() ?: break
                    readCount++
                    when (event) {
                        LiveSocketEvent.Progress -> recovery.reset()
                        is LiveSocketEvent.Transcript -> {
                            recovery.reset()
                            consecutiveEmptyPolls = 0
                            onTranscript(event.text, event.isFinal)
                        }
                        is LiveSocketEvent.Resumption -> resumptionHandle = event.handle
                        LiveSocketEvent.GoAway -> {
                            session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer, resumptionHandle, vocabulary.entries)
                                ?: throw IOException("Gemini Live reconnection failed after goAway.")
                            connectionStartedMs = android.os.SystemClock.elapsedRealtime()
                            hybridVad.reset()
                            recovery.reset()
                            audioMode = AudioMode.CATCH_UP
                            break
                        }
                        is LiveSocketEvent.Error -> {
                            throw IOException(event.message)
                        }
                        is LiveSocketEvent.Closed -> {
                            // Server closed — reconnect
                            session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer, resumptionHandle, vocabulary.entries)
                                ?: throw IOException("Gemini Live reconnection failed.")
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
                    session.socket.close(1000, "stalled")
                    session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer, resumptionHandle, vocabulary.entries)
                        ?: throw IOException("Gemini Live reconnection failed after stall.")
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
            session.socket.close(1000, "SGT session finished")
        }
        } // coroutineScope
    }

    private data class LiveSession(
        val socket: WebSocket,
        val incomingEvents: LinkedBlockingDeque<LiveSocketEvent>,
    )

    private sealed class LiveSocketEvent {
        data class Transcript(val text: String, val isFinal: Boolean) : LiveSocketEvent()
        data class Error(val message: String) : LiveSocketEvent()
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
    ): LiveSession? {
        val events = LinkedBlockingDeque<LiveSocketEvent>()
        val setupReady = CompletableDeferred<Unit>()

        val request = geminiLiveWebSocketRequest(apiKey)

        val socket = httpClient.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    webSocket.send(buildSetupPayload(model, vocabulary, resumptionHandle))
                }

                override fun onMessage(webSocket: WebSocket, text: String) {
                    handleMessage(text, events, setupReady)
                }

                override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                    handleMessage(bytes.utf8(), events, setupReady)
                }

                override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                    events.offer(LiveSocketEvent.Closed)
                }

                override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                    events.offer(LiveSocketEvent.Closed)
                }

                override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                    if (!setupReady.isCompleted) {
                        setupReady.completeExceptionally(t)
                    }
                    events.offer(LiveSocketEvent.Closed)
                }
            },
        )

        return try {
            withTimeout(20_000) { setupReady.await() }
            LiveSession(socket, events)
        } catch (e: Throwable) {
            socket.close(1000, "setup failed")
            null
        }
    }

    private fun handleMessage(
        message: String,
        events: LinkedBlockingDeque<LiveSocketEvent>,
        setupReady: CompletableDeferred<Unit>,
    ) {
        val frame = parseGeminiLiveServerFrame(message) ?: return
        frame.error?.let { error ->
            if (!setupReady.isCompleted) {
                setupReady.completeExceptionally(IOException(error))
            }
            events.offer(LiveSocketEvent.Error(error))
            return
        }

        if (frame.setupComplete) {
            if (!setupReady.isCompleted) {
                setupReady.complete(Unit)
            }
            return
        }
        if (frame.contentCount > 0 || frame.responseComplete || frame.interrupted) {
            events.offer(LiveSocketEvent.Progress)
        }
        frame.sessionResumption?.takeIf { it.resumable }?.handle?.let {
            events.offer(LiveSocketEvent.Resumption(it))
        }
        if (frame.goAway) {
            events.offer(LiveSocketEvent.GoAway)
        }

        frame.inputTranscript?.let { transcript ->
            events.offer(LiveSocketEvent.Transcript(transcript, isFinal = true))
            return
        }
        frame.interimInputTranscript?.let { transcript ->
            events.offer(LiveSocketEvent.Transcript(transcript, isFinal = false))
            return
        }
    }

    private suspend fun tryReconnect(
        apiKey: String,
        model: String,
        audioBuffer: LinkedBlockingDeque<ShortArray>,
        silenceBuffer: MutableList<Short>,
        resumptionHandle: String?,
        vocabulary: List<String>,
    ): LiveSession? {
        // Drain pending audio into silence buffer for catchup replay
        while (true) {
            val chunk = audioBuffer.poll() ?: break
            for (s in chunk) silenceBuffer.add(s)
        }

        // Retry indefinitely until success or cancellation
        var useResumption = true
        while (currentCoroutineContext().isActive) {
            // Drain any audio that arrived during reconnection attempt
            while (true) {
                val chunk = audioBuffer.poll() ?: break
                for (s in chunk) silenceBuffer.add(s)
            }

            val session = connectAndSetup(
                apiKey,
                model,
                vocabulary,
                resumptionHandle.takeIf { useResumption },
            )
            if (session != null) {
                // Final drain before resuming
                while (true) {
                    val chunk = audioBuffer.poll() ?: break
                    for (s in chunk) silenceBuffer.add(s)
                }
                return session
            }
            useResumption = false
            delay(1_000)
        }
        return null
    }

    private fun sendChunked(socket: WebSocket, samples: ShortArray, chunkSize: Int): Boolean {
        var offset = 0
        while (offset < samples.size) {
            val end = minOf(offset + chunkSize, samples.size)
            val chunk = samples.copyOfRange(offset, end)
            if (!socket.send(buildGeminiS2sAudioPayload(chunk))) {
                return false
            }
            offset = end
        }
        return true
    }

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
