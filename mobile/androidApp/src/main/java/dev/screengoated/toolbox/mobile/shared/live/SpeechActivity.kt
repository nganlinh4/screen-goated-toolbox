package dev.screengoated.toolbox.mobile.shared.live

/** Canonical noise-relative level evidence. Does not alter PCM or recognize speech. */
internal class SpeechActivity {
    companion object {
        fun autoStopActivity(rms: Float): Boolean = rms.isFinite() && rms > 0.015f
    }

    private var noise = 0.0003
    private var lastMs: Long? = null

    fun observe(rms: Double, nowMs: Long): Boolean {
        val elapsed = lastMs?.let { (nowMs - it).coerceIn(0, 200).toDouble() / 1000.0 } ?: 0.01
        lastMs = nowMs
        if (!rms.isFinite() || rms < 0.0) return false
        val threshold = (noise * 3.0 + 0.0002).coerceIn(0.001, 0.015)
        val active = rms >= threshold
        if (!active) noise += (rms - noise) * (1.0 - kotlin.math.exp(-elapsed / 0.5))
        return active
    }
}
