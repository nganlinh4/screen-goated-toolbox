package dev.screengoated.toolbox.mobile.translationgummy

import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer

// Diagnostic logging helpers extracted from TranslationGummyRuntime.
// These observe playback/capture state for debugging echo/barge-in behavior
// and never affect the audio pipeline itself.

private const val DEBUG_PLAYBACK_WINDOW_MS = 2_500L
private const val STUCK_PLAYBACK_LOG_MS = 2_000L
private const val DEBUG_RMS_THRESHOLD = 0.015f
private const val TAG = "SGTTranslationGummy"

internal fun TranslationGummyRuntime.debugCaptureChunk(
    debugSessionId: String,
    chunk: ShortArray,
) {
    val rms = chunk.rmsLevel()
    if (rms < DEBUG_RMS_THRESHOLD) {
        return
    }
    val snapshot = audioPlayer.debugSnapshot()
    if (snapshot.active || recentlyPlayed(snapshot)) {
        Log.w(
            TAG,
            "micChunkWhilePlayback sessionId=$debugSessionId rms=${"%.4f".format(rms)} samples=${chunk.size} pendingFrames=${snapshot.pendingFrames} active=${snapshot.active} lastPlayStartedAgoMs=${ageMs(snapshot.lastPlayStartedAtMs)} lastWriteAgoMs=${ageMs(snapshot.lastWriteCompletedAtMs)} lastStopAgoMs=${ageMs(snapshot.lastStopAtMs)}",
        )
    }
}

internal fun TranslationGummyRuntime.debugOutboundAudio(
    debugSessionId: String,
    samples: ShortArray,
    sourceChunkCount: Int,
) {
    val rms = samples.rmsLevel()
    val snapshot = audioPlayer.debugSnapshot()
    if (snapshot.active || recentlyPlayed(snapshot) || rms >= DEBUG_RMS_THRESHOLD) {
        Log.d(
            TAG,
            "outboundAudio sessionId=$debugSessionId chunks=$sourceChunkCount samples=${samples.size} rms=${"%.4f".format(rms)} playbackActive=${snapshot.active} pendingFrames=${snapshot.pendingFrames} lastPlayStartedAgoMs=${ageMs(snapshot.lastPlayStartedAtMs)} lastWriteAgoMs=${ageMs(snapshot.lastWriteCompletedAtMs)} lastStopAgoMs=${ageMs(snapshot.lastStopAtMs)}",
        )
    }
}

internal fun TranslationGummyRuntime.debugDroppedOutboundAudio(
    debugSessionId: String,
    samples: ShortArray,
    sourceChunkCount: Int,
    reason: String,
) {
    val rms = samples.rmsLevel()
    val snapshot = audioPlayer.debugSnapshot()
    Log.w(
        TAG,
        "droppingOutboundMic sessionId=$debugSessionId reason=$reason chunks=$sourceChunkCount samples=${samples.size} rms=${"%.4f".format(rms)} playbackActive=${snapshot.active} pendingFrames=${snapshot.pendingFrames} playState=${snapshot.playState} trackState=${snapshot.trackState} audioSessionId=${snapshot.audioSessionId} audioMode=${snapshot.audioMode?.let { it.debugAudioMode() }} communicationDevice=${snapshot.communicationDevice} voiceVolume=${snapshot.voiceCallVolume}/${snapshot.voiceCallMaxVolume} musicVolume=${snapshot.musicVolume}/${snapshot.musicMaxVolume} lastPlayStartedAgoMs=${ageMs(snapshot.lastPlayStartedAtMs)} lastWriteAgoMs=${ageMs(snapshot.lastWriteCompletedAtMs)} lastStopAgoMs=${ageMs(snapshot.lastStopAtMs)}",
    )
    val nowMs = SystemClock.elapsedRealtime()
    if (snapshot.active &&
        snapshot.pendingFrames == 0L &&
        ageMs(snapshot.lastWriteCompletedAtMs, nowMs) >= STUCK_PLAYBACK_LOG_MS &&
        ageMs(lastStuckPlaybackLogAtMs, nowMs) >= STUCK_PLAYBACK_LOG_MS
    ) {
        lastStuckPlaybackLogAtMs = nowMs
        Log.e(
            TAG,
            "stuckPlaybackGate sessionId=$debugSessionId reason=$reason playState=${snapshot.playState} trackState=${snapshot.trackState} audioSessionId=${snapshot.audioSessionId} audioMode=${snapshot.audioMode?.let { it.debugAudioMode() }} communicationDevice=${snapshot.communicationDevice} voiceVolume=${snapshot.voiceCallVolume}/${snapshot.voiceCallMaxVolume} musicVolume=${snapshot.musicVolume}/${snapshot.musicMaxVolume} lastPlayStartedAgoMs=${ageMs(snapshot.lastPlayStartedAtMs, nowMs)} lastWriteAgoMs=${ageMs(snapshot.lastWriteCompletedAtMs, nowMs)} lastStopAgoMs=${ageMs(snapshot.lastStopAtMs, nowMs)}",
        )
    }
}

internal fun TranslationGummyRuntime.debugInputTranscript(
    debugSessionId: String,
    transcript: String,
    isFinal: Boolean,
    nowMs: Long,
) {
    val snapshot = audioPlayer.debugSnapshot()
    val suspicious = snapshot.active || recentlyPlayed(snapshot, nowMs)
    val level = if (suspicious) Log.WARN else Log.DEBUG
    Log.println(
        level,
        TAG,
        "inputTranscript sessionId=$debugSessionId final=$isFinal suspiciousRecapture=$suspicious playbackActive=${snapshot.active} pendingFrames=${snapshot.pendingFrames} lastPlayStartedAgoMs=${ageMs(snapshot.lastPlayStartedAtMs, nowMs)} lastWriteAgoMs=${ageMs(snapshot.lastWriteCompletedAtMs, nowMs)} lastStopAgoMs=${ageMs(snapshot.lastStopAtMs, nowMs)} text=${transcript.debugSnippet()}",
    )
}

internal fun TranslationGummyRuntime.recentlyPlayed(
    snapshot: AudioTrackPlayer.PlaybackDebugSnapshot,
    nowMs: Long = SystemClock.elapsedRealtime(),
): Boolean {
    return ageMs(snapshot.lastPlayStartedAtMs, nowMs) <= DEBUG_PLAYBACK_WINDOW_MS ||
        ageMs(snapshot.lastWriteCompletedAtMs, nowMs) <= DEBUG_PLAYBACK_WINDOW_MS ||
        ageMs(snapshot.lastStopAtMs, nowMs) <= DEBUG_PLAYBACK_WINDOW_MS
}
