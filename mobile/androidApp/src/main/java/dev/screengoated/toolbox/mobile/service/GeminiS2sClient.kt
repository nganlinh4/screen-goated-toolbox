package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackOutputMode
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import okhttp3.OkHttpClient
import java.util.concurrent.atomic.AtomicInteger

class GeminiS2sClient(
    context: Context,
    internal val httpClient: OkHttpClient,
) {
    private val mediaPlayer = AudioTrackPlayer(context, outputMode = AudioTrackOutputMode.MEDIA)
    private val communicationPlayer = AudioTrackPlayer(
        context,
        outputMode = AudioTrackOutputMode.VOICE_COMMUNICATION,
        outputVolumeBoost = LIVE_TRANSLATE_COMMUNICATION_VOLUME_BOOST,
    )
    private val visibility = S2sVisibilityController {
        mediaPlayer.stopImmediate()
        communicationPlayer.stopImmediate()
    }
    internal val visibilityGate: S2sVisibilityGate get() = visibility.gate
    private val sessionMutex = Mutex()

    fun setPlaybackVisible(visible: Boolean) = visibility.setVisible(visible)

    fun stopPlayback() = visibility.stopActive()

    private fun stopPlaybackSession(id: Long) = visibility.stop(id)

    private fun liveTranslatePlayer(sourceMode: SourceMode): AudioTrackPlayer {
        return if (sourceMode == SourceMode.MIC) {
            communicationPlayer
        } else {
            mediaPlayer
        }
    }

    internal suspend fun runSession(
        apiKey: String,
        model: String,
        sourceMode: SourceMode,
        audioChunks: kotlinx.coroutines.flow.Flow<S2sCapturedAudio>,
        settingsProvider: () -> GeminiS2sRuntimeSettings,
        onDisplay: (S2sDisplaySnapshot) -> Unit,
    ) = sessionMutex.withLock { coroutineScope {
        val sessionId = visibility.start()
        try {
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
                stopPlaybackSession(sessionId)
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
                visibilityGate = visibilityGate,
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
            stopPlaybackSession(sessionId)
        }
        } finally {
            stopPlaybackSession(sessionId)
        }
    } }
}

internal fun geminiLiveAudioLogTag(model: String): String {
    return if (isGeminiLiveTranslateApiModel(model)) "RealtimeLiveTranslateAndroid" else TAG
}

internal fun usesManualSpeechBoundaries(model: String): Boolean {
    return !isGeminiLiveTranslateApiModel(model)
}

internal fun isGeminiLiveTranslateApiModel(model: String): Boolean {
    return GeneratedLiveModelCatalog.endpointProfile(model)?.protocol == "live-translate"
}
