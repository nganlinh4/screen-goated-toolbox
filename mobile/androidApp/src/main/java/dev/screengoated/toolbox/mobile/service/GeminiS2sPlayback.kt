package dev.screengoated.toolbox.mobile.service

import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.service.tts.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.tts.WebSocketEvent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import java.io.IOException
import java.util.TreeMap

internal fun waitForGeminiS2sSetup(
    session: BlockingWebSocketSession,
    logTag: String,
): Boolean {
    val deadline = SystemClock.elapsedRealtime() + 15_000
    while (SystemClock.elapsedRealtime() < deadline) {
        when (val event = session.poll(50)) {
            null -> Unit
            is WebSocketEvent.Text -> {
                if (event.payload.contains("setupComplete")) return true
                parseGeminiS2sUpdate(event.payload).error?.let { throw IOException(it) }
            }
            is WebSocketEvent.Binary -> {
                val payload = event.payload.utf8()
                if (payload.contains("setupComplete")) return true
                parseGeminiS2sUpdate(payload).error?.let { throw IOException(it) }
                Log.w(logTag, "setup-unexpected-binary bytes=${event.payload.size}")
            }
            is WebSocketEvent.Failure -> {
                Log.w(logTag, "setup-websocket-failure error=${event.throwable.message}", event.throwable)
                throw event.throwable
            }
            WebSocketEvent.Closed -> {
                Log.w(logTag, "setup-websocket-closed")
                return false
            }
        }
    }
    Log.w(logTag, "setup-timeout")
    return false
}

internal suspend fun runGeminiS2sPlaybackCoordinator(
    player: AudioTrackPlayer,
    contextMemory: S2sContextMemory,
    events: Channel<S2sEvent>,
    settingsProvider: () -> GeminiS2sRuntimeSettings,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    logTag: String,
) {
    val tracked = TreeMap<Long, SegmentPlayback>()
    var nextPlay = 0L
    var sourceCommitted = ""
    var targetCommitted = ""

    suspend fun publish() {
        onDisplay(
            S2sDisplaySnapshot(
                sourceCommitted = recentGeminiS2sWindow(sourceCommitted),
                sourceDraft = "",
                targetCommitted = recentGeminiS2sWindow(targetCommitted),
                targetDraft = "",
            ),
        )
    }

    suspend fun drainReady() {
        while (currentCoroutineContext().isActive) {
            val playback = tracked[nextPlay] ?: break
            if (playback.audioChunks.isEmpty() && !playback.done) break
            while (playback.audioChunks.isNotEmpty()) {
                val bytes = playback.audioChunks.removeFirst()
                val settings = settingsProvider()
                val backlogMs = tracked.tailMap(nextPlay).values.sumOf { it.audioMs }
                val speed = geminiS2sPlaybackSpeed(settings, backlogMs)
                Log.i(
                    logTag,
                    "play-start segment=$nextPlay bytes=${bytes.size} backlog_ms=$backlogMs speed=$speed",
                )
                player.playPcm24k(bytes, speed, settings.realtime.volumePercent)
            }
            if (playback.done) {
                if (playback.sourceText.isNotBlank()) {
                    sourceCommitted = mergeGeminiS2sSegmentText(sourceCommitted, playback.sourceText)
                }
                if (playback.targetText.isNotBlank()) {
                    targetCommitted = mergeGeminiS2sSegmentText(targetCommitted, playback.targetText)
                    contextMemory.push(playback.targetText)
                }
                tracked.remove(nextPlay)
                nextPlay++
                publish()
                if (!tracked.containsKey(nextPlay)) {
                    player.drain(5_000)
                }
            } else {
                break
            }
        }
    }

    try {
        for (event in events) {
            val playback = tracked.getOrPut(event.segmentId) { SegmentPlayback(audioMs = 0) }
            when (event) {
                is S2sEvent.Queued -> playback.audioMs = event.audioMs
                is S2sEvent.SourceText -> playback.sourceText =
                    mergeGeminiS2sSegmentText(playback.sourceText, event.text)
                is S2sEvent.TargetText -> {
                    playback.targetText = mergeGeminiS2sSegmentText(playback.targetText, event.text)
                    onDisplay(
                        S2sDisplaySnapshot(
                            sourceCommitted = recentGeminiS2sWindow(sourceCommitted),
                            sourceDraft = playback.sourceText,
                            targetCommitted = recentGeminiS2sWindow(targetCommitted),
                            targetDraft = playback.targetText,
                        ),
                    )
                }
                is S2sEvent.Audio -> {
                    playback.audioChunks.add(event.bytes)
                    if (event.bytes.isNotEmpty()) {
                        Log.i(logTag, "audio-ready segment=${event.segmentId} bytes=${event.bytes.size}")
                    }
                }
                is S2sEvent.Done -> {
                    playback.done = true
                    playback.empty = event.empty
                }
            }
            drainReady()
        }
    } catch (cancelled: CancellationException) {
        throw cancelled
    } finally {
        player.drain(1_000)
    }
}

private fun geminiS2sPlaybackSpeed(settings: GeminiS2sRuntimeSettings, backlogMs: Int): Int {
    val base = settings.realtime.speedPercent.coerceIn(50, 200)
    if (!settings.realtime.autoSpeed) return base
    return when {
        backlogMs >= 8_000 -> (base + 35).coerceAtMost(180)
        backlogMs >= 5_000 -> (base + 15).coerceAtMost(170)
        else -> base
    }
}

private fun recentGeminiS2sWindow(text: String): String {
    val trimmed = text.trim()
    if (trimmed.length <= RECENT_DISPLAY_CHARS) return trimmed
    val start = trimmed.length - RECENT_DISPLAY_CHARS
    val boundary = trimmed.indexOf(' ', start).takeIf { it >= 0 } ?: start
    return trimmed.substring(boundary).trimStart()
}

private const val RECENT_DISPLAY_CHARS = 1_200
