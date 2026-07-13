package dev.screengoated.toolbox.mobile.service.tts

import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveConnectedSession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveMediaResolution
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReceiveResult
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionException
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSessionFailure
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveSetupSpec
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveWireFormat
import dev.screengoated.toolbox.mobile.shared.live.buildGeminiLiveSetup
import dev.screengoated.toolbox.mobile.shared.live.openGeminiLiveConnectedSession
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.async
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.delay
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.supervisorScope
import okhttp3.OkHttpClient
import org.json.JSONObject
import java.util.Base64
import java.util.concurrent.LinkedBlockingDeque

internal sealed interface ProviderAudioEvent {
    data class PcmData(
        val payload: ByteArray,
    ) : ProviderAudioEvent

    data class Error(
        val message: String,
    ) : ProviderAudioEvent

    data object End : ProviderAudioEvent
}

internal class GeminiTtsProvider(
    private val openConnectedSession: suspend (String) -> GeminiLiveConnectedSession,
    private val detectLanguage: (String) -> String,
    private val elapsedRealtime: () -> Long,
    private val launchBackground: (() -> Unit) -> Unit,
    private val logTiming: (String) -> Unit,
) {
    constructor(
        httpClient: OkHttpClient,
        languageDetector: DeviceLanguageDetector,
    ) : this(
        openConnectedSession = { apiKey -> openGeminiLiveConnectedSession(httpClient, apiKey) },
        detectLanguage = languageDetector::detectIso639_3,
        elapsedRealtime = SystemClock::elapsedRealtime,
        launchBackground = { task -> Thread({ task() }, "Gemini TTS warm-up").start() },
        logTiming = { message -> Log.d("TTS-Timing", message) },
    )

    private var warmSession: GeminiLiveConnectedSession? = null
    private var warmApiKey: String? = null
    private var warmCreatedAt = 0L
    private var warmGeneration = 0L
    private val warmLock = Any()

    /** Opens a transport in the background without sending request-specific setup. */
    fun warmUp(apiKey: String) {
        if (apiKey.isBlank()) {
            return
        }
        val generation = synchronized(warmLock) {
            warmGeneration += 1L
            warmGeneration
        }
        logTiming("warmUp: starting background connect...")
        launchBackground {
            var connected: GeminiLiveConnectedSession? = null
            try {
                val startedAt = elapsedRealtime()
                connected = runBlocking { openConnectedSession(apiKey) }
                val stored = synchronized(warmLock) {
                    if (generation != warmGeneration) {
                        false
                    } else {
                        warmSession?.close()
                        warmSession = connected
                        warmApiKey = apiKey
                        warmCreatedAt = elapsedRealtime()
                        true
                    }
                }
                if (stored) {
                    connected = null
                    logTiming("Warm socket ready in ${elapsedRealtime() - startedAt}ms")
                } else {
                    connected.close()
                    connected = null
                    logTiming("Warm socket superseded by a newer warm-up")
                }
            } catch (cancelled: CancellationException) {
                connected?.close()
                throw cancelled
            } catch (error: Throwable) {
                connected?.close()
                logTiming("Warm-up failed: ${error.message}")
            }
        }
    }

    private fun acquireWarmSession(apiKey: String): GeminiLiveConnectedSession? {
        synchronized(warmLock) {
            val session = warmSession ?: return null
            val wrongKey = warmApiKey != apiKey
            val age = elapsedRealtime() - warmCreatedAt
            val expired = age > WARM_SOCKET_MAX_AGE_MS
            if (wrongKey || expired) {
                session.close()
                warmSession = null
                warmApiKey = null
                return null
            }
            warmSession = null
            warmApiKey = null
            return session
        }
    }

    fun stream(
        apiKey: String,
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        if (apiKey.isBlank()) {
            sink.offer(ProviderAudioEvent.Error("NO_API_KEY:google"))
            return
        }

        try {
            runBlocking {
                streamRequest(apiKey, request, isStale, sink)
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            sink.offer(
                ProviderAudioEvent.Error(
                    error.message ?: "Gemini TTS transport failed.",
                ),
            )
        }
    }

    private suspend fun streamRequest(
        apiKey: String,
        request: TtsRequest,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
    ) {
        val startedAt = elapsedRealtime()
        logTiming("▶ START request: '${request.text.take(30)}...'")
        val setupPayload = buildSetupPayload(request)
        val textPayload = buildTextPayload(request.text)

        var connected = acquireWarmSession(apiKey)
        var usingWarmSession = connected != null
        var warmRetryAvailable = usingWarmSession
        logTiming("  Warm socket check: ${if (usingWarmSession) "AVAILABLE" else "not available"}")

        while (!isStale()) {
            val session = connected ?: openFreshSession(apiKey, sink, startedAt) ?: return
            connected = null
            if (isStale()) {
                session.close()
                return
            }

            val ready = try {
                activateWhileCurrent(session, setupPayload, isStale)
            } catch (error: GeminiLiveSessionException) {
                session.close()
                if (usingWarmSession && warmRetryAvailable && error.failure.isRetryableBeforeContent()) {
                    warmRetryAvailable = false
                    usingWarmSession = false
                    logTiming("  Warm socket stale during setup; retrying one fresh connection")
                    continue
                }
                sink.offer(ProviderAudioEvent.Error(error.failure.toTtsMessage()))
                return
            } catch (cancelled: CancellationException) {
                session.close()
                throw cancelled
            } catch (error: Throwable) {
                session.close()
                throw error
            }
            if (ready == null) {
                session.close()
                return
            }

            val warmLabel = if (usingWarmSession) " (warm)" else ""
            logTiming("  Setup COMPLETE$warmLabel: ${elapsedRealtime() - startedAt}ms")
            if (isStale()) {
                ready.close()
                return
            }

            val textSent = try {
                ready.trySend(textPayload)
            } catch (error: Throwable) {
                ready.close()
                throw error
            }
            if (!textSent) {
                ready.close()
                if (usingWarmSession && warmRetryAvailable && !isStale()) {
                    warmRetryAvailable = false
                    usingWarmSession = false
                    logTiming("  Warm socket rejected text; retrying one fresh connection")
                    continue
                }
                sink.offer(
                    ProviderAudioEvent.Error(
                        GeminiLiveSessionFailure.ActiveSendRejected.toTtsMessage(),
                    ),
                )
                return
            }

            val textSentAt = elapsedRealtime()
            logTiming("  Text payload sent$warmLabel: ${textSentAt - startedAt}ms")
            streamReadySession(
                session = ready,
                isStale = isStale,
                sink = sink,
                startedAt = startedAt,
                textSentAt = textSentAt,
                warmLabel = warmLabel,
            )
            return
        }

        connected?.close()
    }

    private suspend fun activateWhileCurrent(
        session: GeminiLiveConnectedSession,
        setupPayload: String,
        isStale: () -> Boolean,
    ): GeminiLiveReadySession? = supervisorScope {
        val activation = async {
            session.activate(setupPayload, SETUP_TIMEOUT_MS)
        }
        while (!activation.isCompleted && !isStale()) {
            delay(STALE_POLL_MS)
        }
        if (!activation.isCompleted && isStale()) {
            activation.cancelAndJoin()
            null
        } else {
            activation.await()
        }
    }

    private suspend fun openFreshSession(
        apiKey: String,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
        startedAt: Long,
    ): GeminiLiveConnectedSession? {
        val connectStartedAt = elapsedRealtime()
        return try {
            openConnectedSession(apiKey).also {
                val openedAt = elapsedRealtime()
                logTiming(
                    "  WebSocket OPEN: ${openedAt - startedAt}ms " +
                        "(connect=${openedAt - connectStartedAt}ms)",
                )
            }
        } catch (error: GeminiLiveSessionException) {
            sink.offer(ProviderAudioEvent.Error(error.failure.toTtsMessage()))
            null
        }
    }

    private suspend fun streamReadySession(
        session: GeminiLiveReadySession,
        isStale: () -> Boolean,
        sink: LinkedBlockingDeque<ProviderAudioEvent>,
        startedAt: Long,
        textSentAt: Long,
        warmLabel: String,
    ) {
        session.use {
            var firstAudioLogged = false
            while (!isStale()) {
                when (val result = session.receive(timeoutMs = 250L)) {
                    is GeminiLiveReceiveResult.Frame -> {
                        result.frame.audioParts.forEach { inlineData ->
                            val pcmData = runCatching {
                                Base64.getDecoder().decode(inlineData.data)
                            }.getOrNull() ?: return@forEach
                            if (!firstAudioLogged) {
                                firstAudioLogged = true
                                val transport = if (result.wireFormat == GeminiLiveWireFormat.BINARY) {
                                    " (binary)"
                                } else {
                                    ""
                                }
                                logTiming(
                                    "  FIRST AUDIO chunk$warmLabel$transport: " +
                                        "${elapsedRealtime() - startedAt}ms " +
                                        "(since text=${elapsedRealtime() - textSentAt}ms)",
                                )
                            }
                            sink.offer(ProviderAudioEvent.PcmData(pcmData))
                        }
                        if (result.frame.responseComplete) {
                            logTiming("  TURN COMPLETE$warmLabel: ${elapsedRealtime() - startedAt}ms")
                            sink.offer(ProviderAudioEvent.End)
                            return
                        }
                    }

                    is GeminiLiveReceiveResult.Failed -> {
                        sink.offer(ProviderAudioEvent.Error(result.failure.toTtsMessage()))
                        return
                    }

                    is GeminiLiveReceiveResult.Closed -> {
                        sink.offer(ProviderAudioEvent.End)
                        return
                    }

                    is GeminiLiveReceiveResult.Unparsed,
                    GeminiLiveReceiveResult.TimedOut,
                    -> Unit
                }
            }
        }
    }

    private fun buildSetupPayload(request: TtsRequest): String {
        val settings = request.settingsSnapshot
        val speedLabel = if (request.requestMode == TtsRequestMode.REALTIME) {
            "Normal"
        } else {
            settings.speedPreset.toGeminiSpeedLabel()
        }

        var systemText =
            "You are a text-to-speech reader. Your ONLY job is to read the user's text out loud, " +
                "exactly as written, word for word. Do NOT respond conversationally. Do NOT add " +
                "commentary. Do NOT ask questions. "
        systemText += when (speedLabel) {
            "Slow" -> "Speak slowly, clearly, and with deliberate pacing. "
            "Fast" -> "Speak quickly, efficiently, and with a brisk pace. "
            else -> "Simply read the provided text aloud naturally and clearly. "
        }
        languageInstruction(settings.languageConditions, request.text)
            ?.takeIf(String::isNotBlank)
            ?.let { instruction ->
                systemText += " Additional instructions: ${instruction.trim()} "
            }
        systemText += "Start reading immediately."

        return buildGeminiLiveSetup(
            GeminiLiveSetupSpec(
                apiModel = settings.geminiModel,
                mediaResolution = GeminiLiveMediaResolution.LOW,
                voiceName = settings.geminiVoice,
                systemInstruction = systemText,
            ),
        ).toString()
    }

    private fun buildTextPayload(text: String): String {
        val prompt = "[READ ALOUD VERBATIM - START NOW]\n\n$text"
        return JSONObject()
            .put("realtimeInput", JSONObject().put("text", prompt))
            .toString()
    }

    private fun languageInstruction(
        conditions: List<dev.screengoated.toolbox.mobile.model.MobileTtsLanguageCondition>,
        text: String,
    ): String? {
        val detectedCode = detectLanguage(text)
        return conditions.firstOrNull {
            it.languageCode.equals(detectedCode, ignoreCase = true)
        }?.instruction
    }

    private fun MobileTtsSpeedPreset.toGeminiSpeedLabel(): String = when (this) {
        MobileTtsSpeedPreset.SLOW -> "Slow"
        MobileTtsSpeedPreset.FAST -> "Fast"
        MobileTtsSpeedPreset.NORMAL -> "Normal"
    }

    private companion object {
        private const val SETUP_TIMEOUT_MS = 10_000L
        private const val STALE_POLL_MS = 50L
        private const val WARM_SOCKET_MAX_AGE_MS = Long.MAX_VALUE
    }
}

private fun GeminiLiveSessionFailure.isRetryableBeforeContent(): Boolean = when (this) {
    is GeminiLiveSessionFailure.Server -> retryable
    else -> true
}

private fun GeminiLiveSessionFailure.toTtsMessage(): String = when (this) {
    GeminiLiveSessionFailure.OpenTimedOut -> "Gemini TTS websocket failed to open."
    GeminiLiveSessionFailure.SetupTimedOut -> "Gemini TTS setup timed out."
    GeminiLiveSessionFailure.SetupSendRejected -> "Gemini TTS setup payload was rejected."
    GeminiLiveSessionFailure.ActiveSendRejected -> "Gemini TTS text payload was rejected."
    is GeminiLiveSessionFailure.Server -> message
    is GeminiLiveSessionFailure.Transport -> cause.message ?: "Gemini TTS websocket failed."
    is GeminiLiveSessionFailure.ClosedBeforeReady -> "Gemini TTS websocket closed before setup completed."
}

internal fun chunkBytes(
    bytes: ByteArray,
    chunkSize: Int,
): Sequence<ByteArray> {
    return sequence {
        var index = 0
        while (index < bytes.size) {
            val end = (index + chunkSize).coerceAtMost(bytes.size)
            yield(bytes.copyOfRange(index, end))
            index = end
        }
    }
}
