package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import android.os.SystemClock
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

internal class PhoneControlRuntimeInputActivity(
    private val onSpeechStarted: (epoch: Long) -> Unit,
    private val onSpeechEnded: (epoch: Long, elapsedMs: Long, audioFrames: Long) -> Unit,
    private val onLevel: (Float) -> Unit,
) {
    private val voiceActivity = VoiceActivityHangover(
        SPEECH_RMS_THRESHOLD,
        SPEECH_HANGOVER_MS,
    )
    private val firstSpeechObserved = AtomicBoolean(false)
    private val lastLevelUpdateMs = AtomicLong(0L)
    private val epoch = AtomicLong(0L)
    private val burstLock = Any()
    private var activeEpoch = 0L
    private var activeSinceMs = 0L
    private var activeFrames = 0L

    val speechObserved: Boolean
        get() = firstSpeechObserved.get()

    fun isActive(nowMs: Long): Boolean = voiceActivity.isActive(nowMs)

    fun observe(level: Float) {
        observe(level, SystemClock.elapsedRealtime())
    }

    internal fun observe(level: Float, nowMs: Long) {
        val started = voiceActivity.observe(level, nowMs)
        var ended: SpeechBurstEvidence? = null
        val startedEpoch = synchronized(burstLock) {
            if (started) {
                if (activeEpoch != 0L) {
                    ended = SpeechBurstEvidence(
                        epoch = activeEpoch,
                        elapsedMs = (nowMs - activeSinceMs).coerceAtLeast(0L),
                        audioFrames = activeFrames,
                    )
                }
                val nextEpoch = epoch.incrementAndGet()
                activeEpoch = nextEpoch
                activeSinceMs = nowMs
                activeFrames = 1L
                nextEpoch
            } else {
                if (activeEpoch != 0L) {
                    activeFrames += 1L
                    if (!voiceActivity.isActive(nowMs)) {
                        ended = SpeechBurstEvidence(
                            epoch = activeEpoch,
                            elapsedMs = (nowMs - activeSinceMs).coerceAtLeast(0L),
                            audioFrames = activeFrames,
                        )
                        activeEpoch = 0L
                        activeSinceMs = 0L
                        activeFrames = 0L
                    }
                }
                null
            }
        }
        ended?.let { onSpeechEnded(it.epoch, it.elapsedMs, it.audioFrames) }
        if (startedEpoch != null) {
            firstSpeechObserved.set(true)
            onSpeechStarted(startedEpoch)
        }
        val previous = lastLevelUpdateMs.get()
        if (nowMs - previous < LEVEL_UPDATE_INTERVAL_MS ||
            !lastLevelUpdateMs.compareAndSet(previous, nowMs)
        ) {
            return
        }
        onLevel(phoneControlOrbAudioLevel(level))
    }
}

private data class SpeechBurstEvidence(
    val epoch: Long,
    val elapsedMs: Long,
    val audioFrames: Long,
)

internal fun phoneControlOrbAudioLevel(normalizedRms: Float): Float {
    if (!normalizedRms.isFinite() || normalizedRms < SPEECH_RMS_THRESHOLD) return 0f
    return (normalizedRms * ORB_AUDIO_GAIN).coerceIn(0f, 1f)
}
