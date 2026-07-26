package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import java.util.concurrent.atomic.AtomicBoolean
import kotlinx.coroutines.channels.Channel

internal class PhoneControlRuntimeVisualEvidence {
    val enabled = AtomicBoolean(true)

    fun offerExact(
        payload: String,
        periodicFrames: Channel<String>,
        outbound: PhoneControlSessionPayloadQueue,
    ): Boolean {
        if (!enabled.get()) return true
        while (periodicFrames.tryReceive().isSuccess) {
            // Exact tool evidence supersedes older ambient pixels.
        }
        return outbound.offer(payload, PhoneControlOutboundKind.TOOL_SCREEN_EVIDENCE)
    }

    fun suspend(
        periodicFrames: Channel<String>,
        refreshRequests: Channel<Unit>,
        outbound: PhoneControlSessionPayloadQueue,
        reconciliationFrameQueued: AtomicBoolean,
    ) {
        if (!enabled.compareAndSet(true, false)) return
        var periodicCount = 0
        while (periodicFrames.tryReceive().isSuccess) periodicCount += 1
        var refreshCount = 0
        while (refreshRequests.tryReceive().isSuccess) refreshCount += 1
        val toolCount = outbound.discard(PhoneControlOutboundKind.TOOL_SCREEN_EVIDENCE)
        reconciliationFrameQueued.set(false)
        Log.i(
            TAG,
            "visual_evidence_suspended periodic_frames=$periodicCount " +
                "tool_frames=$toolCount refresh_requests=$refreshCount",
        )
    }

    fun resume(refreshRequests: Channel<Unit>) {
        if (!enabled.compareAndSet(false, true)) return
        refreshRequests.trySend(Unit)
        Log.i(TAG, "visual_evidence_resumed fresh_frame_requested=true")
    }

    fun discardPending(
        periodicFrames: Channel<String>,
        outbound: PhoneControlSessionPayloadQueue,
    ) {
        if (enabled.get()) return
        outbound.discard(PhoneControlOutboundKind.TOOL_SCREEN_EVIDENCE)
        while (periodicFrames.tryReceive().isSuccess) {
            // A protected checkpoint cannot retain model-visible pixels.
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
    }
}

internal fun offerPhoneControlPayload(
    queue: PhoneControlSessionPayloadQueue,
    protocolAbortRequested: AtomicBoolean,
    payload: String,
    kind: PhoneControlOutboundKind,
): Boolean {
    val accepted = queue.offer(payload, kind)
    if (!accepted) protocolAbortRequested.set(true)
    return accepted
}

internal const val TRANSPORT_POLL_MS = 40L
internal const val RECEIVE_POLL_MS = 40L
internal const val MAX_TRANSPORT_REASON_CHARS = 240
internal const val LEVEL_UPDATE_INTERVAL_MS = 80L
internal const val MAX_BUFFERED_AUDIO_FRAMES = 24
internal const val MAX_AUDIO_FRAMES_PER_FLUSH = 8
internal const val MAX_BUFFERED_PLAYBACK_CHUNKS = 32
internal const val SPEECH_RMS_THRESHOLD = 120f / 32768f
internal const val SPEECH_HANGOVER_MS = 500L
internal const val ORB_AUDIO_GAIN = 32768f / 4000f
