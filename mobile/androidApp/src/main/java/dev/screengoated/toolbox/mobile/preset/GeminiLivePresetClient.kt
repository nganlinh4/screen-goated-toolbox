package dev.screengoated.toolbox.mobile.preset

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.util.Base64
import androidx.core.graphics.scale
import dev.screengoated.toolbox.mobile.model.TtsDefaults
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveMediaResolution
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSetupSpec
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveTranscriptionMode
import dev.screengoated.toolbox.mobile.shared.live.buildGeminiLiveSetup
import dev.screengoated.toolbox.mobile.shared.live.geminiLiveWebSocketRequest
import dev.screengoated.toolbox.mobile.shared.live.parseGeminiLiveServerFrame
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.delay
import kotlinx.coroutines.withContext
import kotlinx.coroutines.withTimeout
import okhttp3.OkHttpClient
import okhttp3.Response
import okhttp3.WebSocket
import okhttp3.WebSocketListener
import okio.ByteString
import org.json.JSONArray
import org.json.JSONObject
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque
import java.util.concurrent.TimeUnit

private const val STILL_FRAME_STREAM_COUNT = 4
private const val STILL_FRAME_INTERVAL_MS = 500L
private const val LIVE_IDLE_COMPLETION_MS = 1_200L

internal sealed interface GeminiLivePresetEvent {
    data class Chunk(val text: String) : GeminiLivePresetEvent
    data class Error(val message: String) : GeminiLivePresetEvent
    data object Complete : GeminiLivePresetEvent
    data object Closed : GeminiLivePresetEvent
}

internal suspend fun OkHttpClient.streamGeminiLiveText(
    model: PresetModelDescriptor,
    apiKey: String,
    prompt: String,
    inputText: String,
    onChunk: (String) -> Unit,
): String {
    return streamGeminiLive(
        model = model,
        apiKey = apiKey,
        systemInstruction = prompt,
        image = null,
        inputText = inputText,
        onChunk = onChunk,
    )
}

internal suspend fun OkHttpClient.streamGeminiLiveVision(
    model: PresetModelDescriptor,
    apiKey: String,
    prompt: String,
    imageBytes: ByteArray,
    mimeType: String,
    onChunk: (String) -> Unit,
): String {
    return streamGeminiLive(
        model = model,
        apiKey = apiKey,
        systemInstruction = "",
        image = imageBytes to mimeType,
        inputText = prompt,
        onChunk = onChunk,
    )
}

private suspend fun OkHttpClient.streamGeminiLive(
    model: PresetModelDescriptor,
    apiKey: String,
    systemInstruction: String,
    image: Pair<ByteArray, String>?,
    inputText: String,
    onChunk: (String) -> Unit,
): String = withContext(Dispatchers.IO) {
    if (apiKey.isBlank()) throw IOException("NO_API_KEY:google")

    val events = LinkedBlockingDeque<GeminiLivePresetEvent>()
    val setupReady = CompletableDeferred<Unit>()
    val socket = newWebSocket(
        geminiLiveWebSocketRequest(apiKey),
        object : WebSocketListener() {
            override fun onOpen(webSocket: WebSocket, response: Response) {
                webSocket.send(buildGeminiLivePresetSetup(model.fullName, systemInstruction))
            }

            override fun onMessage(webSocket: WebSocket, text: String) {
                handleGeminiLivePresetMessage(text, setupReady, events)
            }

            override fun onMessage(webSocket: WebSocket, bytes: ByteString) {
                handleGeminiLivePresetMessage(bytes.utf8(), setupReady, events)
            }

            override fun onFailure(webSocket: WebSocket, t: Throwable, response: Response?) {
                if (!setupReady.isCompleted) {
                    setupReady.completeExceptionally(t)
                }
                events.offer(GeminiLivePresetEvent.Error(t.message ?: "Gemini Live websocket failed."))
            }

            override fun onClosing(webSocket: WebSocket, code: Int, reason: String) {
                events.offer(GeminiLivePresetEvent.Closed)
            }

            override fun onClosed(webSocket: WebSocket, code: Int, reason: String) {
                events.offer(GeminiLivePresetEvent.Closed)
            }
        },
    )

    val result = try {
        withTimeout(20_000) { setupReady.await() }
        image?.let { (bytes, mimeType) ->
            for (payload in buildGeminiLiveImagePayloads(bytes, mimeType)) {
                if (!socket.send(payload)) {
                    throw IOException("Gemini Live image payload was rejected.")
                }
                delay(STILL_FRAME_INTERVAL_MS)
            }
        }
        if (!socket.send(buildGeminiLiveTextPayload(inputText))) {
            throw IOException("Gemini Live text payload was rejected.")
        }

        val fullContent = StringBuilder()
        var contentStarted = false
        while (true) {
            val event = events.poll(
                if (contentStarted) LIVE_IDLE_COMPLETION_MS else 20_000L,
                TimeUnit.MILLISECONDS,
            )
            if (event == null) {
                if (contentStarted) {
                    break
                }
                throw IOException("Gemini Live websocket timed out before producing output.")
            }

            when (event) {
                is GeminiLivePresetEvent.Chunk -> {
                    contentStarted = true
                    fullContent.append(event.text)
                    onChunk(event.text)
                }

                is GeminiLivePresetEvent.Error -> throw IOException(event.message)
                GeminiLivePresetEvent.Complete -> break
                GeminiLivePresetEvent.Closed -> {
                    if (fullContent.isNotEmpty()) {
                        break
                    }
                    throw IOException("Gemini Live websocket closed before producing output.")
                }
            }
        }
        fullContent.toString()
    } finally {
        socket.close(1000, "SGT preset request finished")
    }
    result
}

private fun buildGeminiLivePresetSetup(
    model: String,
    systemInstruction: String,
): String {
    val trimmedInstruction = systemInstruction.trim()
    val instruction = trimmedInstruction.takeIf(String::isNotEmpty)?.let {
        "$it IMPORTANT: You must respond as fast as possible. Be concise and direct."
    }
    return buildGeminiLiveSetup(
        GeminiLiveSetupSpec(
            apiModel = model,
            mediaResolution = GeminiLiveMediaResolution.LOW,
            voiceName = TtsDefaults.DEFAULT_TTS_GEMINI_VOICE,
            systemInstruction = instruction,
            transcriptionMode = GeminiLiveTranscriptionMode.OUTPUT,
        ),
    ).toString()
}

private fun buildGeminiLiveTextPayload(text: String): String {
    return JSONObject()
        .put("realtimeInput", JSONObject().put("text", text))
        .toString()
}

private fun buildGeminiLiveImagePayloads(
    imageBytes: ByteArray,
    mimeType: String,
): List<String> {
    val frame = buildGeminiLiveStillFrame(imageBytes, mimeType)
    val payload = JSONObject()
        .put(
            "realtimeInput",
            JSONObject().put(
                "video",
                JSONObject()
                    .put("mimeType", frame.second)
                    .put("data", Base64.encodeToString(frame.first, Base64.NO_WRAP)),
            ),
        )
        .toString()

    return List(STILL_FRAME_STREAM_COUNT) { payload }
}

private fun buildGeminiLiveStillFrame(
    imageBytes: ByteArray,
    mimeType: String,
): Pair<ByteArray, String> {
    val bitmap = BitmapFactory.decodeByteArray(imageBytes, 0, imageBytes.size)
        ?: return imageBytes to mimeType
    val scaled = bitmap.scale(
        (bitmap.width / 4).coerceAtLeast(1),
        (bitmap.height / 4).coerceAtLeast(1),
    )
    val jpegBytes = PreparedImageBytes.encodeJpeg(scaled)
    if (scaled !== bitmap) {
        scaled.recycle()
    }
    bitmap.recycle()
    return jpegBytes to "image/jpeg"
}

internal fun handleGeminiLivePresetMessage(
    message: String,
    setupReady: CompletableDeferred<Unit>,
    events: LinkedBlockingDeque<GeminiLivePresetEvent>,
) {
    parseGeminiLiveServerFrame(message)?.let { frame ->
        if (frame.setupComplete) {
            if (!setupReady.isCompleted) {
                setupReady.complete(Unit)
            }
            return
        }

        frame.error?.let { error ->
            events.offer(GeminiLivePresetEvent.Error(error))
            return
        }

        val outputText = frame.outputTranscript
        if (outputText != null) {
            events.offer(GeminiLivePresetEvent.Chunk(outputText))
            if (frame.responseComplete) {
                events.offer(GeminiLivePresetEvent.Complete)
            }
            return
        }

        frame.visibleTextParts.forEach { text ->
            events.offer(GeminiLivePresetEvent.Chunk(text))
        }
        if (frame.responseComplete) {
            events.offer(GeminiLivePresetEvent.Complete)
        }
    }
}

private object PreparedImageBytes {
    fun encodeJpeg(bitmap: Bitmap): ByteArray {
        return java.io.ByteArrayOutputStream().use { output ->
            bitmap.compress(Bitmap.CompressFormat.JPEG, 90, output)
            output.toByteArray()
        }
    }
}
