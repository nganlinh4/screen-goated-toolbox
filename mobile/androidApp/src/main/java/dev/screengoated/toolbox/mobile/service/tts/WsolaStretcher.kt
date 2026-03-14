package dev.screengoated.toolbox.mobile.service.tts

import kotlin.math.PI
import kotlin.math.abs
import kotlin.math.cos
import kotlin.math.min
import kotlin.math.roundToInt

/**
 * OLA (Overlap-Add) time stretcher for pitch-preserving tempo change.
 * Uses Hann window for perfect reconstruction at 50% overlap.
 * Port of the Windows Rust WSOLA implementation.
 */
internal class WsolaStretcher(sampleRate: Int) {
    private val frameSize = sampleRate * 20 / 1000 // 20ms frame
    private val hopSize = frameSize / 2            // 50% overlap
    private val searchRange = hopSize / 2

    private val window = FloatArray(frameSize) { i ->
        val t = i.toFloat() / frameSize
        0.5f * (1f - cos(2f * PI.toFloat() * t))
    }

    private val inputBuffer = ArrayList<Float>(8192)
    private val outputOverlap = ArrayList<Float>(frameSize)
    private var lastSpeed = 1.0

    fun stretch(input: ShortArray, speedRatio: Double): ShortArray {
        if (abs(speedRatio - 1.0) < 0.05 || input.isEmpty()) {
            if (outputOverlap.isNotEmpty()) {
                val flushed = ShortArray(outputOverlap.size + input.size)
                for (i in outputOverlap.indices) {
                    flushed[i] = outputOverlap[i].coerceIn(-32768f, 32767f).toInt().toShort()
                }
                outputOverlap.clear()
                input.copyInto(flushed, outputOverlap.size)
                return flushed
            }
            return input
        }

        if (abs(speedRatio - lastSpeed) > 0.15) {
            inputBuffer.clear()
            outputOverlap.clear()
        }
        lastSpeed = speedRatio

        inputBuffer.ensureCapacity(inputBuffer.size + input.size)
        for (s in input) {
            inputBuffer.add(s.toFloat())
        }

        if (inputBuffer.size < frameSize + searchRange) {
            return ShortArray(0)
        }

        val targetAnalysisHop = (hopSize * speedRatio).roundToInt().coerceAtLeast(1)
        val synthesisHop = hopSize

        val estimatedFrames = inputBuffer.size / targetAnalysisHop
        var output = FloatArray(estimatedFrames * synthesisHop + frameSize)

        for (i in outputOverlap.indices) {
            if (i < output.size) output[i] = outputOverlap[i]
        }

        var inputPos = 0
        var outputPos = 0

        while (true) {
            if (inputPos + frameSize + searchRange + targetAnalysisHop > inputBuffer.size) break
            if (outputPos + frameSize > output.size) {
                output = output.copyOf(outputPos + frameSize * 2)
            }

            val actualHop = findBestOffset(inputPos, targetAnalysisHop)
            inputPos += actualHop

            for (i in 0 until frameSize) {
                output[outputPos + i] += inputBuffer[inputPos + i] * window[i]
            }
            outputPos += synthesisHop
        }

        val completeLen = min(outputPos, output.size)

        outputOverlap.clear()
        if (completeLen < output.size) {
            for (i in completeLen until output.size) {
                outputOverlap.add(output[i])
            }
        }

        val consumed = min(inputPos, inputBuffer.size)
        if (consumed > 0) {
            inputBuffer.subList(0, consumed).clear()
        }

        return ShortArray(completeLen) { i ->
            output[i].coerceIn(-32768f, 32767f).toInt().toShort()
        }
    }

    private fun findBestOffset(inputPos: Int, targetHop: Int): Int {
        val start = (targetHop - searchRange).coerceAtLeast(0)
        val maxEnd = inputBuffer.size - frameSize - inputPos - 1
        val end = min(targetHop + searchRange, maxEnd.coerceAtLeast(0))

        if (start >= end) return targetHop

        val compareLen = searchRange
        val refPos = inputPos + hopSize
        if (refPos + compareLen > inputBuffer.size) return targetHop

        var bestOffset = targetHop
        var maxCorr = -1f

        for (k in start until end) {
            val candidatePos = inputPos + k
            if (candidatePos + compareLen > inputBuffer.size) continue

            var corr = 0f
            for (i in 0 until compareLen) {
                corr += inputBuffer[refPos + i] * inputBuffer[candidatePos + i]
            }
            if (corr > maxCorr) {
                maxCorr = corr
                bestOffset = k
            }
        }
        return bestOffset
    }
}
