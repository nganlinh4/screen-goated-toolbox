package dev.screengoated.toolbox.mobile.service.parakeet

import kotlin.math.PI
import kotlin.math.cos
import kotlin.math.ln
import kotlin.math.log10
import kotlin.math.pow
import kotlin.math.sqrt

/**
 * Mel spectrogram feature extraction matching parakeet-rs.
 * Constants match the ParakeetEOU model training configuration.
 */
internal class MelSpectrogram {

    private val window = createHannWindow()
    private val melBasis = createMelFilterbank()

    /**
     * Extract log-mel features from raw audio samples.
     * Returns [1, N_MELS, T] shaped float array (flattened, row-major).
     */
    fun extract(audio: FloatArray): MelFeatures {
        val preemph = applyPreemphasis(audio)
        val spec = stft(preemph)
        val mel = matmul(melBasis, N_MELS, spec.data, spec.freqBins, spec.numFrames)
        val logMel = FloatArray(mel.size) { ln((mel[it].coerceAtLeast(0f) + LOG_ZERO_GUARD)) }
        return MelFeatures(logMel, N_MELS, spec.numFrames)
    }

    private fun applyPreemphasis(audio: FloatArray): FloatArray {
        if (audio.isEmpty()) return floatArrayOf()
        val result = FloatArray(audio.size)
        result[0] = sanitize(audio[0])
        for (i in 1 until audio.size) {
            result[i] = sanitize(audio[i]) - PREEMPH * sanitize(audio[i - 1])
        }
        return result
    }

    private fun stft(audio: FloatArray): SpectrogramResult {
        val padAmount = N_FFT / 2
        val padded = FloatArray(padAmount + audio.size + padAmount)
        audio.copyInto(padded, padAmount)

        val numFrames = 1 + (padded.size - WIN_LENGTH) / HOP_LENGTH
        val freqBins = N_FFT / 2 + 1
        val spec = FloatArray(freqBins * numFrames)

        val realBuf = FloatArray(N_FFT)
        val imagBuf = FloatArray(N_FFT)

        for (frame in 0 until numFrames) {
            val start = frame * HOP_LENGTH
            if (start + WIN_LENGTH > padded.size) break

            realBuf.fill(0f)
            imagBuf.fill(0f)
            for (i in 0 until WIN_LENGTH) {
                realBuf[i] = padded[start + i] * window[i]
            }

            fftInPlace(realBuf, imagBuf, N_FFT)

            for (i in 0 until freqBins) {
                val magSq = realBuf[i] * realBuf[i] + imagBuf[i] * imagBuf[i]
                spec[i * numFrames + frame] = if (magSq.isFinite()) magSq else 0f
            }
        }

        return SpectrogramResult(spec, freqBins, numFrames)
    }

    /** Cooley-Tukey radix-2 FFT in-place. */
    private fun fftInPlace(real: FloatArray, imag: FloatArray, n: Int) {
        // Bit-reversal permutation
        var j = 0
        for (i in 0 until n) {
            if (i < j) {
                val tmpR = real[i]; real[i] = real[j]; real[j] = tmpR
                val tmpI = imag[i]; imag[i] = imag[j]; imag[j] = tmpI
            }
            var m = n shr 1
            while (m >= 1 && j >= m) {
                j -= m
                m = m shr 1
            }
            j += m
        }

        // Butterfly stages
        var step = 2
        while (step <= n) {
            val halfStep = step / 2
            val angleStep = -2.0 * PI / step
            for (k in 0 until halfStep) {
                val angle = angleStep * k
                val wr = cos(angle).toFloat()
                val wi = kotlin.math.sin(angle).toFloat()
                var i = k
                while (i < n) {
                    val jj = i + halfStep
                    val tr = wr * real[jj] - wi * imag[jj]
                    val ti = wr * imag[jj] + wi * real[jj]
                    real[jj] = real[i] - tr
                    imag[jj] = imag[i] - ti
                    real[i] += tr
                    imag[i] += ti
                    i += step
                }
            }
            step = step shl 1
        }
    }

    /** Matrix multiply A[rows x inner] * B[inner x cols] → C[rows x cols]. Row-major. */
    private fun matmul(
        a: FloatArray, aRows: Int,
        b: FloatArray, bRows: Int, bCols: Int,
    ): FloatArray {
        val result = FloatArray(aRows * bCols)
        for (i in 0 until aRows) {
            for (k in 0 until bRows) {
                val aVal = a[i * bRows + k]
                if (aVal == 0f) continue
                for (j in 0 until bCols) {
                    result[i * bCols + j] += aVal * b[k * bCols + j]
                }
            }
        }
        return result
    }

    companion object {
        const val N_FFT = 512
        const val WIN_LENGTH = 400
        const val HOP_LENGTH = 160
        const val N_MELS = 128
        private const val PREEMPH = 0.97f
        private const val LOG_ZERO_GUARD = 5.960_464_5e-8f
        private const val FMAX = 8000f
        const val SAMPLE_RATE = 16000

        private fun sanitize(x: Float): Float = if (x.isFinite()) x else 0f

        private fun createHannWindow(): FloatArray {
            return FloatArray(WIN_LENGTH) { i ->
                (0.5 - 0.5 * cos(2.0 * PI * i / (WIN_LENGTH - 1))).toFloat()
            }
        }

        private fun hzToMel(hz: Float): Float = 2595f * log10(1f + hz / 700f)
        private fun melToHz(mel: Float): Float = 700f * (10f.pow(mel / 2595f) - 1f)

        private fun createMelFilterbank(): FloatArray {
            val numFreqs = N_FFT / 2 + 1
            val melMin = hzToMel(0f)
            val melMax = hzToMel(FMAX)

            val melPoints = FloatArray(N_MELS + 2) { i ->
                melToHz(melMin + (melMax - melMin) * i / (N_MELS + 1))
            }

            val fftFreqs = FloatArray(numFreqs) { i ->
                (SAMPLE_RATE.toFloat() / N_FFT) * i
            }

            val weights = FloatArray(N_MELS * numFreqs)

            for (i in 0 until N_MELS) {
                val left = melPoints[i]
                val center = melPoints[i + 1]
                val right = melPoints[i + 2]
                for (j in 0 until numFreqs) {
                    val freq = fftFreqs[j]
                    val w = when {
                        freq in left..center -> (freq - left) / (center - left)
                        freq > center && freq <= right -> (right - freq) / (right - center)
                        else -> 0f
                    }
                    weights[i * numFreqs + j] = w
                }
                val enorm = 2f / (right - left)
                for (j in 0 until numFreqs) {
                    weights[i * numFreqs + j] *= enorm
                }
            }

            return weights
        }
    }
}

internal data class MelFeatures(
    val data: FloatArray,
    val melBins: Int,
    val numFrames: Int,
)

private data class SpectrogramResult(
    val data: FloatArray,
    val freqBins: Int,
    val numFrames: Int,
)
