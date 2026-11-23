package dev.screengoated.toolbox.mobile.service

import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.service.tts.PlaybackWriteResult
import dev.screengoated.toolbox.mobile.service.tts.PlaybackDrainResult
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.InternalCoroutinesApi
import kotlinx.coroutines.Job
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.isActive
import java.util.TreeMap
import java.util.concurrent.atomic.AtomicInteger

@OptIn(InternalCoroutinesApi::class)
internal suspend fun runGeminiS2sPlaybackCoordinator(
    player: AudioTrackPlayer,
    contextMemory: S2sContextMemory,
    events: Channel<S2sEvent>,
    settingsProvider: () -> GeminiS2sRuntimeSettings,
    onDisplay: (S2sDisplaySnapshot) -> Unit,
    logTag: String,
    backlogMs: AtomicInteger,
    visibilityGate: S2sVisibilityGate,
) {
    val tracked = TreeMap<Long, SegmentPlayback>()
    var nextPlay = 0L
    var nextText = 0L
    var sourceCommitted = ""
    var targetCommitted = ""
    val job = requireNotNull(currentCoroutineContext()[Job])
    val ownership = S2sPlaybackOwnership()
    val retireOnCancellation = job.invokeOnCompletion(onCancelling = true, invokeImmediately = true) { ownership.retire() }
    val current = { job.isActive && ownership.isCurrent() }

    fun requireWrite(result: PlaybackWriteResult, epoch: Long?) {
        when (result) {
            PlaybackWriteResult.COMPLETE, PlaybackWriteResult.NO_OUTPUT -> Unit
            PlaybackWriteResult.INTERRUPTED -> if (!current()) throw CancellationException("Playback retired")
            PlaybackWriteResult.PARTIAL -> if (!current()) throw CancellationException("Playback retired") else if (visibilityGate.isCurrent(epoch)) error("Audio playback was partial")
            PlaybackWriteResult.FAILED -> error("Audio playback failed")
        }
    }

    fun finishPlayback(epoch: Long?) {
        if (!visibilityGate.isCurrent(epoch)) return
        val canPlay = { current() && visibilityGate.isCurrent(epoch) }
        val dispatch = { effect: () -> Unit ->
            var allowed = false
            ownership.dispatch { allowed = visibilityGate.dispatch(epoch, effect) } && allowed
        }
        requireWrite(player.finishPcm24kInterruptibly(settingsProvider().realtime.volumePercent, canPlay, dispatch), epoch)
        when (player.drain(5_000, canPlay)) {
            PlaybackDrainResult.COMPLETE -> Unit
            PlaybackDrainResult.INTERRUPTED -> if (!current()) throw CancellationException("Playback retired")
            PlaybackDrainResult.TIMED_OUT -> error("Audio playback did not finish")
        }
    }

    suspend fun publish() {
        onDisplay(
            S2sDisplaySnapshot(
                sourceCommitted = sourceCommitted,
                sourceDraft = "",
                targetCommitted = targetCommitted,
                targetDraft = "",
            ),
        )
    }

    suspend fun drainText() {
        while (true) {
            val id = nextText
            val item = tracked[id] ?: break
            if (!item.done) break
            if (item.sourceText.isNotBlank()) sourceCommitted = mergeGeminiS2sSegmentText(sourceCommitted, item.sourceText)
            if (item.targetText.isNotBlank()) {
                targetCommitted = mergeGeminiS2sSegmentText(targetCommitted, item.targetText)
                contextMemory.push(item.targetText)
            }
            nextText++
            if (id < nextPlay) tracked.remove(id)
            publish()
        }
    }

    fun advancePlayback(playback: SegmentPlayback) {
        val id = nextPlay++
        decrementBacklog(backlogMs, playback.audioMs)
        if (id < nextText) tracked.remove(id)
    }

    suspend fun drainReady() {
        while (currentCoroutineContext().isActive) {
            val playback = tracked[nextPlay] ?: break
            if (!visibilityGate.isCurrent(playback.playbackEpoch)) {
                playback.audioChunks.clear()
                advancePlayback(playback)
                continue
            }
            if (
                playback.audioChunks.isEmpty() &&
                !playback.done &&
                shouldSkipStalePendingSegment(playback)
            ) {
                Log.i(
                    logTag,
                    "skip-play segment=$nextPlay reason=pending-timeout delay_ms=${segmentDelayMs(playback)} source_audio_ms=${playback.audioMs} input_text=${playback.sourceText.isNotBlank()} output_text=${playback.targetText.isNotBlank()} backlog_ms=${backlogMs.get()}",
                )
                advancePlayback(playback)
                continue
            }
            if (playback.audioChunks.isEmpty() && !playback.done) break
            while (playback.audioChunks.isNotEmpty()) {
                val bytes = playback.audioChunks.removeFirst()
                val epoch = playback.playbackEpoch
                if (!visibilityGate.isCurrent(epoch)) continue
                val settings = settingsProvider()
                val backlogMs = tracked.tailMap(nextPlay).values.sumOf { it.audioMs }
                val speed = geminiS2sPlaybackSpeed(settings, backlogMs)
                Log.i(
                    logTag,
                    "play-start segment=$nextPlay bytes=${bytes.size} backlog_ms=$backlogMs speed=$speed",
                )
                val canPlay = { current() && visibilityGate.isCurrent(epoch) }
                val dispatch = { effect: () -> Unit ->
                    var allowed = false
                    ownership.dispatch { allowed = visibilityGate.dispatch(epoch, effect) } && allowed
                }
                requireWrite(player.playPcm24kInterruptibly(bytes, speed, settings.realtime.volumePercent, canPlay, dispatch), epoch)
            }
            if (playback.done) {
                advancePlayback(playback)
                if (tracked[nextPlay]?.audioChunks?.isNotEmpty() != true) {
                    finishPlayback(playback.playbackEpoch)
                }
            } else {
                break
            }
        }
    }

    try {
        for (event in events) {
            if (event.segmentId < nextText) continue
            val playback = tracked.getOrPut(event.segmentId) { SegmentPlayback(audioMs = 0) }
            when (event) {
                is S2sEvent.Queued -> {
                    playback.audioMs = event.audioMs
                    playback.queuedAtMs = event.queuedAtMs
                    playback.playbackEpoch = event.playbackEpoch
                }
                is S2sEvent.SourceText -> playback.sourceText =
                    mergeGeminiS2sSegmentText(playback.sourceText, event.text)
                is S2sEvent.TargetText -> {
                    playback.targetText = mergeGeminiS2sSegmentText(playback.targetText, event.text)
                    onDisplay(
                        S2sDisplaySnapshot(
                            sourceCommitted = sourceCommitted,
                            sourceDraft = playback.sourceText,
                            targetCommitted = targetCommitted,
                            targetDraft = playback.targetText,
                        ),
                    )
                }
                is S2sEvent.Audio -> {
                    if (event.segmentId >= nextPlay && visibilityGate.isCurrent(playback.playbackEpoch)) playback.audioChunks.add(event.bytes)
                    if (event.bytes.isNotEmpty()) {
                        Log.i(logTag, "audio-ready segment=${event.segmentId} bytes=${event.bytes.size}")
                    }
                }
                is S2sEvent.Done -> {
                    playback.done = true
                    playback.empty = event.empty
                }
            }
            drainText()
            drainReady()
        }
    } catch (cancelled: CancellationException) {
        throw cancelled
    } finally {
        ownership.retire()
        retireOnCancellation.dispose()
    }
}

private fun shouldSkipStalePendingSegment(segment: SegmentPlayback): Boolean {
    val delayMs = segmentDelayMs(segment)
    val baseGraceMs = if (segment.sourceText.isNotBlank() || segment.targetText.isNotBlank()) {
        S2S_ORDERED_TRANSCRIPT_PENDING_SKIP_MS
    } else {
        S2S_ORDERED_PENDING_SKIP_MS
    }
    val sourceMultiplier = if (segment.targetText.isNotBlank()) 4 else 2
    val graceMs = baseGraceMs + (segment.audioMs.toLong() * sourceMultiplier)
    return delayMs >= graceMs
}

private fun segmentDelayMs(segment: SegmentPlayback): Long {
    if (segment.queuedAtMs <= 0L) return 0L
    return SystemClock.elapsedRealtime() - segment.queuedAtMs
}

private fun decrementBacklog(backlogMs: AtomicInteger, audioMs: Int) {
    val remaining = backlogMs.addAndGet(-audioMs)
    if (remaining < 0) {
        backlogMs.set(0)
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

private const val S2S_ORDERED_PENDING_SKIP_MS = 8_000L
private const val S2S_ORDERED_TRANSCRIPT_PENDING_SKIP_MS = 28_000L
