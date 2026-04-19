package dev.screengoated.toolbox.mobile.translationgummy

import android.media.AudioManager
import android.os.SystemClock
import kotlin.math.sqrt

internal data class TranslationGummyBargeInDecision(
    val shouldInterruptPlayback: Boolean,
    val shouldBufferCandidate: Boolean,
    val likelyEcho: Boolean,
    val micRms: Float,
    val referenceRms: Float,
    val correlation: Float,
    val lagMs: Int,
    val route: String,
)

internal class TranslationGummyBargeInDetector {
    private val recentPlayback16k = ArrayDeque<ShortArray>()
    private var recentPlaybackSamples = 0
    private var lastPlaybackAtMs = 0L

    fun onPlaybackChunk(
        pcm24k: ByteArray,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ) {
        val downsampled = pcm24k.decodePcm24kTo16k()
        if (downsampled.isEmpty()) {
            return
        }
        recentPlayback16k.addLast(downsampled)
        recentPlaybackSamples += downsampled.size
        while (recentPlaybackSamples > MAX_REFERENCE_SAMPLES && recentPlayback16k.isNotEmpty()) {
            recentPlaybackSamples -= recentPlayback16k.removeFirst().size
        }
        lastPlaybackAtMs = nowMs
    }

    fun clear() {
        recentPlayback16k.clear()
        recentPlaybackSamples = 0
        lastPlaybackAtMs = 0L
    }

    fun evaluate(
        micChunk16k: ShortArray,
        route: String?,
        nowMs: Long = SystemClock.elapsedRealtime(),
    ): TranslationGummyBargeInDecision {
        val micRms = micChunk16k.rmsLevel()
        val routeLabel = route ?: "unknown"
        val speakerRoute = routeLabel.contains("speaker")
        val conservativeRoute = speakerRoute || routeLabel == "unknown" || routeLabel == "none"
        val strongSpeechThreshold = if (conservativeRoute) SPEECH_RMS_SPEAKER else SPEECH_RMS_PRIVATE_ROUTE
        val candidateThreshold = if (conservativeRoute) CANDIDATE_RMS_SPEAKER else CANDIDATE_RMS_PRIVATE_ROUTE
        if (micRms < candidateThreshold) {
            return TranslationGummyBargeInDecision(
                shouldInterruptPlayback = false,
                shouldBufferCandidate = false,
                likelyEcho = false,
                micRms = micRms,
                referenceRms = 0f,
                correlation = 0f,
                lagMs = 0,
                route = routeLabel,
            )
        }

        if (ageMs(lastPlaybackAtMs, nowMs) > MAX_REFERENCE_AGE_MS || recentPlaybackSamples < micChunk16k.size) {
            val interrupt = micRms >= strongSpeechThreshold
            return TranslationGummyBargeInDecision(
                shouldInterruptPlayback = interrupt,
                shouldBufferCandidate = !interrupt,
                likelyEcho = false,
                micRms = micRms,
                referenceRms = 0f,
                correlation = 0f,
                lagMs = 0,
                route = routeLabel,
            )
        }

        val reference = snapshotPlaybackReference()
        val metrics = bestReferenceMatch(micChunk16k, reference)
        val echoCorrelationThreshold = if (conservativeRoute) ECHO_CORRELATION_SPEAKER else ECHO_CORRELATION_PRIVATE
        val echoRatioThreshold = if (conservativeRoute) ECHO_RMS_RATIO_SPEAKER else ECHO_RMS_RATIO_PRIVATE
        val interruptCorrelationThreshold = if (conservativeRoute) INTERRUPTION_CORRELATION_SPEAKER else INTERRUPTION_CORRELATION_PRIVATE
        val interruptRatioThreshold = if (conservativeRoute) INTERRUPTION_RMS_RATIO_SPEAKER else INTERRUPTION_RMS_RATIO_PRIVATE
        val echoLike = metrics.correlation >= echoCorrelationThreshold &&
            micRms <= metrics.referenceRms * echoRatioThreshold
        val overpoweringReference = micRms >= metrics.referenceRms * interruptRatioThreshold
        val decorrelatedSpeech = metrics.correlation <= interruptCorrelationThreshold
        val strongSpeech = micRms >= strongSpeechThreshold
        val shouldInterrupt = strongSpeech && !echoLike && (decorrelatedSpeech || overpoweringReference)
        return TranslationGummyBargeInDecision(
            shouldInterruptPlayback = shouldInterrupt,
            shouldBufferCandidate = !shouldInterrupt && !echoLike,
            likelyEcho = echoLike,
            micRms = micRms,
            referenceRms = metrics.referenceRms,
            correlation = metrics.correlation,
            lagMs = metrics.lagMs,
            route = routeLabel,
        )
    }

    private fun snapshotPlaybackReference(): ShortArray {
        val snapshot = ShortArray(recentPlaybackSamples)
        var offset = 0
        recentPlayback16k.forEach { chunk ->
            chunk.copyInto(snapshot, destinationOffset = offset)
            offset += chunk.size
        }
        return snapshot
    }

    private fun bestReferenceMatch(
        micChunk16k: ShortArray,
        reference16k: ShortArray,
    ): ReferenceMatch {
        val maxLagSamples = minOf(MAX_LAG_SAMPLES, (reference16k.size - micChunk16k.size).coerceAtLeast(0))
        var bestCorrelation = 0f
        var bestReferenceRms = 0f
        var bestLagSamples = 0
        var lagSamples = 0
        while (lagSamples <= maxLagSamples) {
            val start = reference16k.size - micChunk16k.size - lagSamples
            if (start >= 0) {
                val correlation = micChunk16k.normalizedCorrelation(reference16k, start)
                if (correlation > bestCorrelation) {
                    bestCorrelation = correlation
                    bestReferenceRms = reference16k.windowRms(start, micChunk16k.size)
                    bestLagSamples = lagSamples
                }
            }
            lagSamples += LAG_STEP_SAMPLES
        }
        return ReferenceMatch(
            correlation = bestCorrelation,
            referenceRms = bestReferenceRms,
            lagMs = bestLagSamples * 1_000 / MIC_SAMPLE_RATE_HZ,
        )
    }

    private data class ReferenceMatch(
        val correlation: Float,
        val referenceRms: Float,
        val lagMs: Int,
    )

    private companion object {
        private const val MIC_SAMPLE_RATE_HZ = 16_000
        private const val MAX_REFERENCE_SAMPLES = MIC_SAMPLE_RATE_HZ * 3
        private const val MAX_REFERENCE_AGE_MS = 1_800L
        private const val MAX_LAG_SAMPLES = MIC_SAMPLE_RATE_HZ / 4
        private const val LAG_STEP_SAMPLES = MIC_SAMPLE_RATE_HZ / 100
        private const val CANDIDATE_RMS_SPEAKER = 0.020f
        private const val CANDIDATE_RMS_PRIVATE_ROUTE = 0.014f
        private const val SPEECH_RMS_SPEAKER = 0.050f
        private const val SPEECH_RMS_PRIVATE_ROUTE = 0.030f
        private const val ECHO_CORRELATION_SPEAKER = 0.92f
        private const val ECHO_CORRELATION_PRIVATE = 0.96f
        private const val INTERRUPTION_CORRELATION_SPEAKER = 0.78f
        private const val INTERRUPTION_CORRELATION_PRIVATE = 0.88f
        private const val ECHO_RMS_RATIO_SPEAKER = 1.25f
        private const val ECHO_RMS_RATIO_PRIVATE = 1.08f
        private const val INTERRUPTION_RMS_RATIO_SPEAKER = 1.45f
        private const val INTERRUPTION_RMS_RATIO_PRIVATE = 1.12f
    }
}

internal fun Int.debugAudioMode(): String {
    return when (this) {
        AudioManager.MODE_NORMAL -> "MODE_NORMAL"
        AudioManager.MODE_RINGTONE -> "MODE_RINGTONE"
        AudioManager.MODE_IN_CALL -> "MODE_IN_CALL"
        AudioManager.MODE_IN_COMMUNICATION -> "MODE_IN_COMMUNICATION"
        AudioManager.MODE_CALL_SCREENING -> "MODE_CALL_SCREENING"
        AudioManager.MODE_CALL_REDIRECT -> "MODE_CALL_REDIRECT"
        AudioManager.MODE_COMMUNICATION_REDIRECT -> "MODE_COMMUNICATION_REDIRECT"
        else -> "mode_$this"
    }
}

internal fun ShortArray.rmsLevel(): Float {
    if (isEmpty()) {
        return 0f
    }
    var sumSquares = 0.0
    for (sample in this) {
        val normalized = sample / 32768.0
        sumSquares += normalized * normalized
    }
    return sqrt(sumSquares / size).toFloat().coerceIn(0f, 1f)
}

private fun ShortArray.windowRms(
    start: Int,
    count: Int,
): Float {
    if (count <= 0) {
        return 0f
    }
    var sumSquares = 0.0
    for (index in 0 until count) {
        val normalized = this[start + index] / 32768.0
        sumSquares += normalized * normalized
    }
    return sqrt(sumSquares / count).toFloat().coerceIn(0f, 1f)
}

private fun ShortArray.normalizedCorrelation(
    reference: ShortArray,
    referenceStart: Int,
): Float {
    var dot = 0.0
    var micEnergy = 0.0
    var refEnergy = 0.0
    for (index in indices) {
        val mic = this[index] / 32768.0
        val ref = reference[referenceStart + index] / 32768.0
        dot += mic * ref
        micEnergy += mic * mic
        refEnergy += ref * ref
    }
    if (micEnergy <= 1e-9 || refEnergy <= 1e-9) {
        return 0f
    }
    return (dot / sqrt(micEnergy * refEnergy)).toFloat().coerceIn(0f, 1f)
}

private fun ByteArray.decodePcm24kTo16k(): ShortArray {
    if (isEmpty()) {
        return ShortArray(0)
    }
    val inputSamples = ShortArray(size / 2)
    for (index in inputSamples.indices) {
        val byteIndex = index * 2
        inputSamples[index] = ((this[byteIndex + 1].toInt() shl 8) or
            (this[byteIndex].toInt() and 0xFF)).toShort()
    }
    val outputSize = (inputSamples.size * 2) / 3
    val output = ShortArray(outputSize)
    for (index in output.indices) {
        val sourceIndex = ((index.toLong() * 3L) / 2L).toInt().coerceAtMost(inputSamples.lastIndex)
        output[index] = inputSamples[sourceIndex]
    }
    return output
}

private fun ageMs(
    eventAtMs: Long,
    nowMs: Long = SystemClock.elapsedRealtime(),
): Long {
    if (eventAtMs <= 0L) {
        return Long.MAX_VALUE
    }
    return (nowMs - eventAtMs).coerceAtLeast(0L)
}
