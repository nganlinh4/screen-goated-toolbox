package dev.screengoated.toolbox.mobile.bilingualrelay

import android.content.Context
import android.os.SystemClock
import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.service.tts.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.tts.WebSocketEvent
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException
import java.util.concurrent.LinkedBlockingDeque

class BilingualRelayRuntime(
    context: Context,
    projectionConsentStore: ProjectionConsentStore,
    private val repository: BilingualRelayRepository,
    private val httpClient: OkHttpClient,
) {
    private val audioCaptureController = AudioCaptureController(context, projectionConsentStore)
    private val audioPlayer = AudioTrackPlayer(context)
    private var sessionJob: Job? = null

    fun start(scope: CoroutineScope) {
        if (sessionJob?.isActive == true) {
            return
        }
        sessionJob = scope.launch {
            runLoop()
        }
    }

    fun restart(scope: CoroutineScope) {
        stop()
        start(scope)
    }

    fun stop() {
        sessionJob?.cancel()
        sessionJob = null
        audioPlayer.stopImmediate()
        repository.finalizeActiveTranscripts(SystemClock.elapsedRealtime())
        repository.markStopped()
    }

    private suspend fun runLoop() {
        val applied = repository.currentAppliedConfig()
        if (!applied.isValid()) {
            repository.markNotConfigured()
            return
        }

        val apiKey = repository.currentApiKey()
        val modelName = repository.currentGeminiModel().ifBlank { MODEL_NAME }
        val voiceName = repository.currentGeminiVoice().ifBlank { DEFAULT_VOICE_NAME }
        if (apiKey.isBlank()) {
            repository.fail(repository.localeText().bilingualRelayApiKeyRequired)
            return
        }

        var attempt = 0
        while (currentCoroutineContext().isActive) {
            repository.markConnecting(reconnecting = attempt > 0)
            try {
                runSession(apiKey, applied, modelName, voiceName)
                break
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                if (!currentCoroutineContext().isActive) {
                    break
                }
                repository.fail(repository.localeText().bilingualRelayConnectionLost)
                delay((1_000L * (attempt + 1)).coerceAtMost(5_000L))
                attempt += 1
            }
        }
    }

    private suspend fun runSession(
        apiKey: String,
        config: BilingualRelayConfig,
        modelName: String,
        voiceName: String,
    ) = withContext(Dispatchers.IO) {
        val request = Request.Builder()
            .url("$BILINGUAL_RELAY_LIVE_WS_ENDPOINT?key=$apiKey")
            .build()

        BlockingWebSocketSession(httpClient, request).use { session ->
            if (!session.awaitOpen(10_000)) {
                throw IOException("Bilingual relay websocket failed to open.")
            }
            if (!session.sendText(buildBilingualRelaySetupPayload(modelName, config.buildSystemInstruction(), voiceName))) {
                throw IOException("Bilingual relay setup payload was rejected.")
            }

            waitForSetup(session)
            repository.markReady()

            coroutineScope {
                val outboundAudio = LinkedBlockingDeque<ShortArray>()
                val captureJob = launch(Dispatchers.IO) {
                    audioCaptureController.open(
                        config = LiveSessionConfig(sourceMode = SourceMode.MIC),
                        onRms = repository::updateVisualizerLevel,
                    ).collect { chunk ->
                        outboundAudio.offer(chunk)
                    }
                }

                try {
                    while (currentCoroutineContext().isActive) {
                        flushOutboundAudio(session, outboundAudio)
                        when (val event = session.poll(50)) {
                            null -> Unit
                            is WebSocketEvent.Text -> handleSocketPayload(event.payload)
                            is WebSocketEvent.Binary -> handleSocketPayload(event.payload.utf8())
                            is WebSocketEvent.Failure -> throw IOException(
                                event.throwable.message ?: "Bilingual relay websocket failed.",
                            )
                            WebSocketEvent.Closed -> throw IOException("Bilingual relay websocket closed.")
                        }
                    }
                } finally {
                    captureJob.cancel()
                    runCatching {
                        session.sendText(buildBilingualRelayAudioStreamEndPayload())
                    }
                    repository.finalizeActiveTranscripts(SystemClock.elapsedRealtime())
                }
            }
        }
    }

    private fun waitForSetup(session: BlockingWebSocketSession) {
        val deadline = SystemClock.elapsedRealtime() + 15_000L
        while (SystemClock.elapsedRealtime() < deadline) {
            when (val event = session.poll(100)) {
                null -> Unit
                is WebSocketEvent.Text -> {
                    val update = parseBilingualRelaySocketUpdate(event.payload)
                    if (update.error != null) {
                        throw IOException(update.error)
                    }
                    if (update.setupComplete) {
                        return
                    }
                }
                is WebSocketEvent.Binary -> {
                    val update = parseBilingualRelaySocketUpdate(event.payload.utf8())
                    if (update.error != null) {
                        throw IOException(update.error)
                    }
                    if (update.setupComplete) {
                        return
                    }
                }
                is WebSocketEvent.Failure -> throw IOException(
                    event.throwable.message ?: "Bilingual relay websocket failed during setup.",
                )
                WebSocketEvent.Closed -> throw IOException("Bilingual relay websocket closed during setup.")
            }
        }
        throw IOException("Bilingual relay setup timed out.")
    }

    private fun flushOutboundAudio(
        session: BlockingWebSocketSession,
        outboundAudio: LinkedBlockingDeque<ShortArray>,
    ) {
        val combined = mutableListOf<Short>()
        while (true) {
            val chunk = outboundAudio.poll() ?: break
            chunk.forEach(combined::add)
        }
        if (combined.isEmpty()) {
            return
        }
        val samples = ShortArray(combined.size)
        combined.forEachIndexed { index, sample -> samples[index] = sample }
        if (!session.sendText(buildBilingualRelayAudioPayload(samples))) {
            throw IOException("Bilingual relay audio payload was rejected.")
        }
    }

    private fun handleSocketPayload(message: String) {
        val update = parseBilingualRelaySocketUpdate(message)
        update.error?.let { throw IOException(it) }
        val nowMs = SystemClock.elapsedRealtime()
        update.inputTranscript?.let {
            repository.upsertTranscript(BilingualRelayTranscriptRole.INPUT, it, update.turnComplete, nowMs)
        }
        update.outputTranscript?.let {
            repository.upsertTranscript(BilingualRelayTranscriptRole.OUTPUT, it, update.turnComplete, nowMs)
        }
        update.audioChunk?.let { pcm24k ->
            audioPlayer.playPcm24k(
                pcm24k = pcm24k,
                speedPercent = 100,
                volumePercent = 100,
            )
        }
        if (update.interrupted) {
            audioPlayer.stopImmediate()
            repository.finalizeActiveTranscripts(nowMs)
        } else if (update.turnComplete) {
            repository.finalizeActiveTranscripts(nowMs)
        }
        if (update.goAway) {
            throw IOException("Server GoAway — reconnecting")
        }
    }

    private companion object {
        private const val MODEL_NAME = "gemini-3.1-flash-live-preview"
        private const val DEFAULT_VOICE_NAME = "Aoede"
    }
}
