package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import android.os.SystemClock
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicLong

internal class PhoneControlRuntimeInputActivity(
    private val onFirstSpeech: () -> Unit,
    private val onSpeechActive: () -> Unit,
    private val onLevel: (Float) -> Unit,
) {
    private val voiceActivity = VoiceActivityHangover(
        SPEECH_RMS_THRESHOLD,
        SPEECH_HANGOVER_MS,
    )
    private val firstSpeechObserved = AtomicBoolean(false)
    private val lastLevelUpdateMs = AtomicLong(0L)

    val speechObserved: Boolean
        get() = firstSpeechObserved.get()

    fun isActive(nowMs: Long): Boolean = voiceActivity.isActive(nowMs)

    fun observe(level: Float) {
        val now = SystemClock.elapsedRealtime()
        if (voiceActivity.observe(level, now)) {
            if (firstSpeechObserved.compareAndSet(false, true)) onFirstSpeech()
            onSpeechActive()
        }
        val previous = lastLevelUpdateMs.get()
        if (now - previous < LEVEL_UPDATE_INTERVAL_MS ||
            !lastLevelUpdateMs.compareAndSet(previous, now)
        ) {
            return
        }
        onLevel(level)
    }
}
