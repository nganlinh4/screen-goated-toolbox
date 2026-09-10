package dev.screengoated.toolbox.mobile.shared.live

/** Canonical speech-end reducer shared by continuous presets and Live Translate. */
internal class GeminiTranscribeVad {
    private var active = false
    private var lastSpeechMs = 0L
    private var endSent = false

    fun observe(samples: ShortArray, nowMs: Long): Boolean {
        val rms = kotlin.math.sqrt(samples.sumOf {
            val normalized = it.toDouble() / 32768.0
            normalized * normalized
        } / samples.size.coerceAtLeast(1))
        return observeRms(rms, nowMs)
    }

    fun observeRms(rms: Double, nowMs: Long): Boolean {
        if (rms >= SPEECH_RMS) {
            active = true
            endSent = false
            lastSpeechMs = nowMs
            return false
        }
        return pollEnd(nowMs)
    }

    fun pollEnd(nowMs: Long): Boolean {
        if (!active || endSent) return false
        val silenceMs = (nowMs - lastSpeechMs).coerceAtLeast(0)
        if (silenceMs <= TRAILING_AUDIO_MS || silenceMs < END_SILENCE_MS) return false
        active = false
        endSent = true
        return true
    }

    fun reset() {
        active = false
        endSent = false
        lastSpeechMs = 0L
    }

    fun isSafeGap(): Boolean = !active

    companion object {
        const val SPEECH_RMS = 0.015
        const val TRAILING_AUDIO_MS = 180L
        const val END_SILENCE_MS = 420L
    }
}
