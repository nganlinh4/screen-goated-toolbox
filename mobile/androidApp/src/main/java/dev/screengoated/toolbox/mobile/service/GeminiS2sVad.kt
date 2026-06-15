package dev.screengoated.toolbox.mobile.service

import android.content.Context
import android.os.SystemClock
import android.util.Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.service.tts.BlockingWebSocketSession
import dev.screengoated.toolbox.mobile.service.tts.WebSocketEvent
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.Job
import kotlinx.coroutines.cancelAndJoin
import kotlinx.coroutines.channels.Channel
import kotlinx.coroutines.coroutineScope
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext
import okhttp3.OkHttpClient
import okhttp3.Request
import java.io.IOException
import java.util.Locale
import java.util.concurrent.atomic.AtomicInteger
import kotlin.math.sqrt


// VAD + segment-analysis constants and helpers extracted from GeminiS2sClient.
internal const val TAG = "RealtimeS2SAndroid"
internal const val LIVE_WS_ENDPOINT =
    "wss://generativelanguage.googleapis.com/ws/google.ai.generativelanguage.v1beta.GenerativeService.BidiGenerateContent"
internal const val SAMPLE_RATE = 16_000
internal const val FRAME_SAMPLES = 1_600
internal const val FRAME_MS = 100L
internal const val PREROLL_SAMPLES = 4_000
internal const val MIN_SEGMENT_SAMPLES = 16_000
internal const val TARGET_SEGMENT_SAMPLES = 48_000
internal const val MAX_SEGMENT_SAMPLES = 80_000
internal const val END_SILENCE_FRAMES = 3
internal const val HEDGE_ATTEMPTS = 2
internal const val SESSION_COUNT = 3
internal const val FIRST_AUDIO_SILENT_RETRY_MS = 3_800L
internal const val FIRST_AUDIO_ACTIVE_RETRY_MS = 5_200L
internal const val AUDIO_IDLE_FINISH_MS = 1_200L
internal const val S2S_HEDGE_TIMEOUT_MS = 45_000L
internal const val S2S_HEDGE_FINAL_TIMEOUT_MS = 60_000L
internal const val SPEECH_THRESHOLD_MULTIPLIER = 2.2f
internal const val MIN_SPEECH_THRESHOLD = 0.012f
internal const val MAX_SPEECH_THRESHOLD = 0.035f
internal const val ABSOLUTE_SPEECH_RMS = 0.045f
internal const val NOISE_LEARN_MAX_RMS = 0.018f
internal const val NOISE_LEARN_THRESHOLD_RATIO = 0.60f
internal const val MIN_SEGMENT_SPEECH_FRAMES = 4
internal const val MIN_SEGMENT_PEAK_RMS = 0.025f
internal const val MIN_SEGMENT_SPEECH_RATIO = 0.08f
internal const val MIN_SPEECH_LIKE_RATIO = 0.18f
internal const val STRICT_MIN_SPEECH_LIKE_RATIO = 0.32f
internal const val STRICT_MIN_SPEECH_CONFIDENCE = 0.38f
internal const val VAD_HEALTH_INTERVAL_MS = 2_000L

internal fun rms(samples: ShortArray): Float {
    var sum = 0.0
    for (sample in samples) {
        val normalized = sample / 32768.0
        sum += normalized * normalized
    }
    return sqrt(sum / samples.size).toFloat()
}

internal data class SegmentMetrics(
    val meanRms: Float,
    val peakRms: Float,
    val energeticFrames: Int,
    val speechLikeFrames: Int,
)

internal class AdaptiveS2sVadState {
    private var strictness = 0f
    private var consecutiveEmptyNoInput = 0

    @Synchronized
    fun snapshot(backlogMs: Int): AdaptiveS2sVadSnapshot {
        val backlogPressure = (backlogMs.toFloat() / 30_000f).coerceIn(0f, 0.55f)
        return AdaptiveS2sVadSnapshot(strictness = maxOf(strictness, backlogPressure))
    }

    @Synchronized
    fun observe(outcome: AdaptiveS2sVadOutcome, segment: S2sSegment) {
        when (outcome) {
            AdaptiveS2sVadOutcome.HEALTHY -> {
                consecutiveEmptyNoInput = 0
                strictness = (strictness - 0.10f).coerceAtLeast(0f)
            }
            AdaptiveS2sVadOutcome.EMPTY_NO_INPUT -> {
                consecutiveEmptyNoInput += 1
                val highEnergy = segment.meanRms >= 0.025f ||
                    segment.peakRms >= 0.060f ||
                    segmentSpeechRatio(segment) >= 0.60f
                val step = if (highEnergy) 0.22f else 0.12f
                strictness = (strictness + step).coerceAtMost(1f)
            }
            AdaptiveS2sVadOutcome.RETRY_FRESH -> Unit
        }
        Log.i(
            TAG,
            "adaptive-vad outcome=$outcome strictness=${"%.2f".format(Locale.US, strictness)} consecutive_empty=$consecutiveEmptyNoInput segment=${segment.id} confidence=${"%.2f".format(Locale.US, segmentSpeechConfidence(segment))} speech_like_ratio=${"%.2f".format(Locale.US, segmentSpeechLikeRatio(segment))} speech_ratio=${"%.2f".format(Locale.US, segmentSpeechRatio(segment))} mean_rms=${"%.4f".format(Locale.US, segment.meanRms)} peak_rms=${"%.4f".format(Locale.US, segment.peakRms)}",
        )
    }
}

internal data class AdaptiveS2sVadSnapshot(val strictness: Float = 0f)

internal fun analyzeSegmentSamples(samples: ShortArray): SegmentMetrics {
    if (samples.isEmpty()) {
        return SegmentMetrics(0f, 0f, 0, 0)
    }
    var rmsSum = 0f
    var peakRms = 0f
    var energeticFrames = 0
    var speechLikeFrames = 0
    var frameCount = 0
    var offset = 0
    while (offset < samples.size) {
        val end = minOf(offset + FRAME_SAMPLES, samples.size)
        val frame = samples.copyOfRange(offset, end)
        val frameRms = rms(frame)
        frameCount++
        rmsSum += frameRms
        peakRms = maxOf(peakRms, frameRms)
        if (frameRms >= MIN_SPEECH_THRESHOLD) energeticFrames++
        if (isSpeechLikeFrame(frame, frameRms)) speechLikeFrames++
        offset = end
    }
    return SegmentMetrics(
        meanRms = rmsSum / frameCount.coerceAtLeast(1),
        peakRms = peakRms,
        energeticFrames = energeticFrames,
        speechLikeFrames = speechLikeFrames,
    )
}

internal fun isSpeechLikeFrame(frame: ShortArray, frameRms: Float): Boolean {
    if (frame.size < 2 || frameRms < MIN_SPEECH_THRESHOLD) return false
    var peak = 0f
    var zeroCrossings = 0
    for (index in frame.indices) {
        peak = maxOf(peak, kotlin.math.abs(frame[index] / 32768f))
        if (index > 0) {
            val prev = frame[index - 1]
            val current = frame[index]
            if ((prev < 0 && current >= 0) || (prev >= 0 && current < 0)) {
                zeroCrossings++
            }
        }
    }
    val crest = peak / frameRms.coerceAtLeast(0.0001f)
    val zcr = zeroCrossings.toFloat() / (frame.size - 1).toFloat()
    return zcr in 0.015f..0.24f && crest in 1.2f..18.0f
}

internal fun isSegmentWorthSending(segment: S2sSegment, vad: AdaptiveS2sVadSnapshot): Boolean {
    val speechRatio = segmentSpeechRatio(segment)
    val speechLikeRatio = segmentSpeechLikeRatio(segment)
    val confidence = segmentSpeechConfidence(segment)
    val baseline = segment.speechFrames >= MIN_SEGMENT_SPEECH_FRAMES ||
        speechRatio >= MIN_SEGMENT_SPEECH_RATIO ||
        (segment.peakRms >= MIN_SEGMENT_PEAK_RMS && speechLikeRatio >= 0.08f)
    if (!baseline) return false

    if (vad.strictness <= 0f) {
        return confidence >= 0.18f || speechLikeRatio >= 0.08f
    }

    val minSpeechLike = MIN_SPEECH_LIKE_RATIO +
        (STRICT_MIN_SPEECH_LIKE_RATIO - MIN_SPEECH_LIKE_RATIO) * vad.strictness
    val minConfidence = 0.24f +
        (STRICT_MIN_SPEECH_CONFIDENCE - 0.24f) * vad.strictness
    // A high blended confidence alone can come purely from the energy terms
    // (loud flat/tonal/DC noise), so require at least minimal speech-like
    // structure before accepting on confidence (matches Windows s2s/utils.rs).
    return speechLikeRatio >= minSpeechLike ||
        (speechLikeRatio >= 0.08f && confidence >= minConfidence)
}

internal fun segmentSpeechRatio(segment: S2sSegment): Float {
    val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
    return segment.speechFrames.toFloat() / frameCount.toFloat()
}

internal fun segmentSpeechLikeRatio(segment: S2sSegment): Float {
    val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
    return segment.speechLikeFrames.toFloat() / frameCount.toFloat()
}

internal fun segmentEnergeticRatio(segment: S2sSegment): Float {
    val frameCount = ((segment.samples.size + FRAME_SAMPLES - 1) / FRAME_SAMPLES).coerceAtLeast(1)
    return segment.energeticFrames.toFloat() / frameCount.toFloat()
}

internal fun segmentSpeechConfidence(segment: S2sSegment): Float {
    val energyScore = (segment.meanRms / 0.055f).coerceIn(0f, 1f)
    return (segmentSpeechLikeRatio(segment) * 0.45f) +
        (segmentSpeechRatio(segment) * 0.30f) +
        (segmentEnergeticRatio(segment) * 0.15f) +
        (energyScore * 0.10f)
}

internal fun groupedFirstAudioTimeoutMs(
    sourceAudioMs: Long,
    textUpdates: Int,
): Long {
    val base = if (textUpdates == 0) {
        FIRST_AUDIO_SILENT_RETRY_MS
    } else {
        FIRST_AUDIO_ACTIVE_RETRY_MS
    }
    return (base + sourceAudioMs * 2).coerceIn(5_500L, 30_000L)
}

internal fun groupedHardTimeoutMs(
    sourceAudioMs: Long,
    finalAttempt: Boolean,
): Long {
    val base = if (finalAttempt) {
        S2S_HEDGE_FINAL_TIMEOUT_MS
    } else {
        S2S_HEDGE_TIMEOUT_MS
    }
    return (base + sourceAudioMs * 4).coerceAtMost(180_000L)
}

