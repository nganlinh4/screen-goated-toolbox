package dev.screengoated.toolbox.mobile.service.tts

internal enum class PlaybackWriteResult { COMPLETE, NO_OUTPUT, INTERRUPTED, PARTIAL, FAILED }
internal enum class PlaybackDrainResult { COMPLETE, INTERRUPTED, TIMED_OUT }

internal fun finishTtsPlayback(
    hasAudio: Boolean,
    drain: () -> PlaybackDrainResult,
    stop: () -> Unit,
    isCurrent: () -> Boolean = { true },
): TtsCompletionStatus {
    if (!hasAudio) {
        stop()
        return if (isCurrent()) TtsCompletionStatus.FAILED else TtsCompletionStatus.INTERRUPTED
    }
    return when (drain()) {
        PlaybackDrainResult.COMPLETE -> TtsCompletionStatus.COMPLETED
        PlaybackDrainResult.INTERRUPTED -> TtsCompletionStatus.PARTIAL
        PlaybackDrainResult.TIMED_OUT -> TtsCompletionStatus.PARTIAL
    }
}

/** Dispatch each bounded nonblocking write while owned; waits never hold the lease. */
internal fun writeInterruptibly(
    byteCount: Int,
    isCurrent: () -> Boolean,
    dispatch: (() -> Unit) -> Boolean,
    write: (offset: Int, count: Int) -> Int,
    wait: () -> Unit,
): PlaybackWriteResult {
    var offset = 0
    while (offset < byteCount) {
        if (!isCurrent()) return if (offset > 0) PlaybackWriteResult.PARTIAL else PlaybackWriteResult.INTERRUPTED
        val requested = minOf(byteCount - offset, 1_920)
        var count = 0
        val dispatched = try {
            dispatch { if (isCurrent()) count = write(offset, requested) }
        } catch (_: Exception) {
            // A driver exception does not prove that no bytes were accepted.
            return PlaybackWriteResult.PARTIAL
        }
        if (count > requested) return PlaybackWriteResult.PARTIAL
        if (count in 1..requested) offset += count
        if (!dispatched || !isCurrent()) return if (offset > 0) PlaybackWriteResult.PARTIAL else PlaybackWriteResult.INTERRUPTED
        if (count < 0) return if (offset > 0) PlaybackWriteResult.PARTIAL else PlaybackWriteResult.FAILED
        if (count == 0) wait()
    }
    return if (isCurrent()) PlaybackWriteResult.COMPLETE else PlaybackWriteResult.PARTIAL
}

internal fun waitForPlaybackDrain(
    deadlineMs: Long,
    now: () -> Long,
    isCurrent: () -> Boolean,
    isDrained: () -> Boolean,
    wait: () -> Unit,
): PlaybackDrainResult {
    while (isCurrent() && now() < deadlineMs) {
        if (isDrained()) return if (isCurrent()) PlaybackDrainResult.COMPLETE else PlaybackDrainResult.INTERRUPTED
        wait()
    }
    return if (isCurrent()) PlaybackDrainResult.TIMED_OUT else PlaybackDrainResult.INTERRUPTED
}
