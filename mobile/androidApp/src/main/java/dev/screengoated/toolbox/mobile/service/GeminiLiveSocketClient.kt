package dev.screengoated.toolbox.mobile.service

import android.util.Log
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
import okhttp3.MediaType.Companion.toMediaType
import okhttp3.OkHttpClient
import okhttp3.Request
import okhttp3.RequestBody.Companion.toRequestBody
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit

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
        onTranscript: (String) -> Unit,
    ) {
        val audioBuffer = LinkedBlockingDeque<ShortArray>()
        var silenceBuffer = mutableListOf<Short>()
        var audioMode = AudioMode.NORMAL
        var modeStartMs = System.currentTimeMillis()
        var lastTranscriptionMs = System.currentTimeMillis()
        var consecutiveEmptyPolls = 0
        var outboundChunks = 0

        // Connect initial socket
        var session = connectAndSetup(apiKey, model)
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
                // Audio mode state machine transitions
                val elapsed = System.currentTimeMillis() - modeStartMs
                when (audioMode) {
                    AudioMode.NORMAL -> {
                        if (elapsed >= NORMAL_DURATION_MS) {
                            audioMode = AudioMode.SILENCE
                            modeStartMs = System.currentTimeMillis()
                            silenceBuffer.clear()
                        }
                    }
                    AudioMode.SILENCE -> {
                        if (elapsed >= SILENCE_DURATION_MS) {
                            audioMode = AudioMode.CATCH_UP
                            modeStartMs = System.currentTimeMillis()
                        }
                    }
                    AudioMode.CATCH_UP -> {
                        if (silenceBuffer.isEmpty()) {
                            audioMode = AudioMode.NORMAL
                            modeStartMs = System.currentTimeMillis()
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
                            sendChunked(session.socket, realAudio.toShortArray(), CHUNK_SIZE).also {
                                outboundChunks++
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
                            sendChunked(session.socket, toSend, CHUNK_SIZE)
                        } else if (silenceBuffer.isNotEmpty()) {
                            val toSend = ShortArray(silenceBuffer.size) { silenceBuffer.removeAt(0) }
                            sendChunked(session.socket, toSend, CHUNK_SIZE)
                        } else {
                            true
                        }
                    }
                }

                if (!sendOk) {
                    // Send failed — reconnect
                    session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer)
                        ?: break
                    audioMode = AudioMode.CATCH_UP
                    modeStartMs = System.currentTimeMillis()
                    lastTranscriptionMs = System.currentTimeMillis()
                    consecutiveEmptyPolls = 0
                    continue
                }

                // Read transcriptions from the incoming queue
                var readCount = 0
                while (readCount < 20) {
                    val event = session.incomingEvents.poll() ?: break
                    readCount++
                    when (event) {
                        is LiveSocketEvent.Transcript -> {
                            lastTranscriptionMs = System.currentTimeMillis()
                            consecutiveEmptyPolls = 0
                            onTranscript(event.text)
                        }
                        is LiveSocketEvent.Error -> {
                            throw IOException(event.message)
                        }
                        is LiveSocketEvent.Closed -> {
                            // Server closed — reconnect
                            session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer)
                                ?: throw IOException("Gemini Live reconnection failed.")
                            audioMode = AudioMode.CATCH_UP
                            modeStartMs = System.currentTimeMillis()
                            lastTranscriptionMs = System.currentTimeMillis()
                            consecutiveEmptyPolls = 0
                            break
                        }
                    }
                }
                if (readCount == 0) {
                    consecutiveEmptyPolls++
                }

                // Degradation detection: stalled connection
                val timeSinceTranscription = System.currentTimeMillis() - lastTranscriptionMs
                if (consecutiveEmptyPolls >= EMPTY_READ_CHECK_COUNT &&
                    timeSinceTranscription > NO_RESULT_THRESHOLD_MS
                ) {
                    session.socket.close(1000, "stalled")
                    session = tryReconnect(apiKey, model, audioBuffer, silenceBuffer)
                        ?: throw IOException("Gemini Live reconnection failed after stall.")
                    audioMode = AudioMode.CATCH_UP
                    modeStartMs = System.currentTimeMillis()
                    lastTranscriptionMs = System.currentTimeMillis()
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
        data class Transcript(val text: String) : LiveSocketEvent()
        data class Error(val message: String) : LiveSocketEvent()
        data object Closed : LiveSocketEvent()
    }

    private suspend fun connectAndSetup(
        apiKey: String,
        model: String,
    ): LiveSession? {
        val events = LinkedBlockingDeque<LiveSocketEvent>()
        val setupReady = CompletableDeferred<Unit>()

        val request = Request.Builder()
            .url("$LIVE_WS_ENDPOINT?key=$apiKey")
            .build()

        val socket = httpClient.newWebSocket(
            request,
            object : WebSocketListener() {
                override fun onOpen(webSocket: WebSocket, response: Response) {
                    webSocket.send(buildSetupPayload(model))
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
        if (message.contains("setupComplete")) {
            if (!setupReady.isCompleted) {
                setupReady.complete(Unit)
            }
            return
        }

        val update = parseGeminiS2sUpdate(message)

        update.error?.let { error ->
            events.offer(LiveSocketEvent.Error(error))
            return
        }

        update.inputText.takeIf(String::isNotBlank)?.let { transcript ->
            events.offer(LiveSocketEvent.Transcript(transcript))
            return
        }
    }

    private suspend fun tryReconnect(
        apiKey: String,
        model: String,
        audioBuffer: LinkedBlockingDeque<ShortArray>,
        silenceBuffer: MutableList<Short>,
    ): LiveSession? {
        // Drain pending audio into silence buffer for catchup replay
        while (true) {
            val chunk = audioBuffer.poll() ?: break
            for (s in chunk) silenceBuffer.add(s)
        }

        // Retry indefinitely until success or cancellation
        while (currentCoroutineContext().isActive) {
            // Drain any audio that arrived during reconnection attempt
            while (true) {
                val chunk = audioBuffer.poll() ?: break
                for (s in chunk) silenceBuffer.add(s)
            }

            val session = connectAndSetup(apiKey, model)
            if (session != null) {
                // Final drain before resuming
                while (true) {
                    val chunk = audioBuffer.poll() ?: break
                    for (s in chunk) silenceBuffer.add(s)
                }
                return session
            }
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

    private fun buildSetupPayload(model: String): String {
        val generationConfig = JSONObject()
            .put("responseModalities", JSONArray().put("AUDIO"))
            .put("mediaResolution", "MEDIA_RESOLUTION_LOW")
            .put("thinkingConfig", JSONObject().put("thinkingBudget", 0))

        val setup = JSONObject()
            .put(
                "setup",
                JSONObject()
                    .put("model", "models/$model")
                    .put("generationConfig", generationConfig)
                    .put("inputAudioTranscription", JSONObject()),
            )

        return setup.toString()
    }

    private companion object {
        private const val NORMAL_DURATION_MS = 20_000L
        private const val SILENCE_DURATION_MS = 2_000L
        private const val SAMPLES_PER_100MS = 1_600
        private const val CHUNK_SIZE = 1_600
        private const val SEND_INTERVAL_MS = 100L
        private const val EMPTY_READ_CHECK_COUNT = 50
        private const val NO_RESULT_THRESHOLD_MS = 8_000L
    }
}
