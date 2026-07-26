package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.phonecontrol.lifecycle.PhoneControlTurnPhase
import dev.screengoated.toolbox.mobile.phonecontrol.session.buildPhoneControlAudioPayload
import dev.screengoated.toolbox.mobile.shared.live.GeminiLiveReadySession
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicInteger
import java.util.concurrent.atomic.AtomicLong
import kotlinx.coroutines.channels.Channel

/** One outbound cycle ordered for live speech latency and protocol ownership. */
internal class PhoneControlRuntimeOutbound(
    private val visualEvidence: PhoneControlRuntimeVisualEvidence,
    private val audioFrames: Channel<ShortArray>,
    private val bufferedAudio: AtomicInteger,
    private val controlPayloads: PhoneControlSessionPayloadQueue,
    private val screenFrames: Channel<String>,
    private val screenReconciliationQueued: AtomicBoolean,
    private val sender: PhoneControlOutboundSender,
    private val audioFramesSent: AtomicLong,
    private val screenFramesSent: AtomicLong,
    private val pendingWorkCount: () -> Int,
    private val turnPhase: () -> PhoneControlTurnPhase,
    private val userSpeaking: () -> Boolean,
    private val userInterfaceGoals: PhoneControlUserInterfaceGoalQueue,
    private val onInputSent: () -> Unit,
    private val onInputActivity: () -> Unit,
    private val onFreshScreenDelivered: () -> Unit,
) {
    fun flush(session: GeminiLiveReadySession): Boolean {
        visualEvidence.discardPending(screenFrames, controlPayloads)
        if (!flushMicrophone(session)) return false
        if (!flushControl(session)) return false
        if (!flushAmbientScreen(session)) return false
        return flushUserInterfaceGoal(session)
    }

    private fun flushMicrophone(session: GeminiLiveReadySession): Boolean {
        repeat(MAX_AUDIO_FRAMES_PER_FLUSH) {
            val samples = audioFrames.tryReceive().getOrNull() ?: return@repeat
            bufferedAudio.updateAndGet { (it - 1).coerceAtLeast(0) }
            if (!sender.send(
                    session = session,
                    payload = buildPhoneControlAudioPayload(samples),
                    kind = PhoneControlOutboundKind.MICROPHONE_AUDIO,
                    pendingWork = pendingWorkCount(),
                    turnPhase = turnPhase(),
                )
            ) return false
            if (audioFramesSent.incrementAndGet() == 1L) {
                Log.i(TAG, "audio_uplink_started samples_per_frame=${samples.size}")
            }
            onInputSent()
            onInputActivity()
        }
        return true
    }

    private fun flushControl(session: GeminiLiveReadySession): Boolean {
        while (true) {
            val queued = controlPayloads.next() ?: return true
            if (!sender.send(
                    session = session,
                    payload = queued.payload,
                    kind = queued.kind,
                    pendingWork = pendingWorkCount(),
                    turnPhase = turnPhase(),
                    utf8Bytes = queued.utf8Bytes,
                )
            ) return false
            controlPayloads.markSent(queued)
        }
    }

    private fun flushAmbientScreen(session: GeminiLiveReadySession): Boolean {
        if (!visualEvidence.enabled.get() || !canSendAmbientScreen(pendingWorkCount())) return true
        val payload = screenFrames.tryReceive().getOrNull() ?: return true
        if (!sender.send(
                session = session,
                payload = payload,
                kind = PhoneControlOutboundKind.AMBIENT_SCREEN,
                pendingWork = pendingWorkCount(),
                turnPhase = turnPhase(),
            )
        ) return false
        if (screenFramesSent.incrementAndGet() == 1L) Log.i(TAG, "screen_uplink_started")
        onInputSent()
        if (screenReconciliationQueued.compareAndSet(true, false)) {
            onFreshScreenDelivered()
        }
        return true
    }

    private fun flushUserInterfaceGoal(session: GeminiLiveReadySession): Boolean =
        userInterfaceGoals.flushRuntimeGoal(
            session = session,
            phase = turnPhase(),
            pendingWorkCount = pendingWorkCount(),
            userSpeaking = userSpeaking(),
            sender = sender,
        ) {
            onInputSent()
            Log.i(TAG, "ui_goal_sent")
        }

    private companion object {
        const val TAG = "SGTPhoneControl"
    }
}
