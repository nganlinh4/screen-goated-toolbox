package dev.screengoated.toolbox.mobile.service.tts

import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.os.SystemClock

internal class AudioTrackPlayer(
    context: Context,
) {
    private val appContext = context.applicationContext
    private val audioManager = appContext.getSystemService(AudioManager::class.java)
    private val outputAudioAttributes = buildOutputAudioAttributes()
    private val focusListener = AudioManager.OnAudioFocusChangeListener { _ -> Unit }
    private val focusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
        .setAudioAttributes(outputAudioAttributes)
        .setOnAudioFocusChangeListener(focusListener)
        .build()

    private val audioTrack = createAudioTrack()
    private val stretcher = WsolaStretcher(24_000)
    private var writtenFrames: Long = 0
    private var hasAudioFocus = false
    private var active = false

    @Synchronized
    fun playPcm24k(
        pcm24k: ByteArray,
        speedPercent: Int,
        volumePercent: Int,
    ) {
        if (pcm24k.isEmpty()) {
            return
        }
        requestAudioFocus()
        ensureStarted()

        // Decode PCM bytes to 24kHz shorts
        val samples24k = ShortArray(pcm24k.size / 2)
        for (i in samples24k.indices) {
            val byteIndex = i * 2
            samples24k[i] = ((pcm24k[byteIndex + 1].toInt() shl 8) or
                (pcm24k[byteIndex].toInt() and 0xFF)).toShort()
        }

        // Apply WSOLA time-stretching at 24kHz (pitch-preserving)
        val speedRatio = speedPercent.coerceIn(50, 200) / 100.0
        val stretched = stretcher.stretch(samples24k, speedRatio)

        // Upsample stretched 24kHz to 48kHz and apply volume
        val output = upsampleAndScale(stretched, volumePercent)
        val writtenBytes = audioTrack.write(output, 0, output.size, AudioTrack.WRITE_BLOCKING)
        if (writtenBytes > 0) {
            writtenFrames += writtenBytes / 2L
        }
    }

    @Synchronized
    fun drain(timeoutMs: Long = 30_000) {
        if (!active) {
            abandonAudioFocus()
            return
        }
        val deadline = SystemClock.elapsedRealtime() + timeoutMs
        while (SystemClock.elapsedRealtime() < deadline) {
            val played = audioTrack.playbackHeadPosition.toLong() and 0xFFFFFFFFL
            if (played >= writtenFrames) {
                break
            }
            Thread.sleep(20)
        }
        stopInternal()
    }

    @Synchronized
    fun stopImmediate() {
        stopInternal()
    }

    @Synchronized
    fun release() {
        stopInternal()
        audioTrack.release()
    }

    private fun ensureStarted() {
        if (!active) {
            audioTrack.flush()
            audioTrack.play()
            writtenFrames = 0
            active = true
        }
    }

    private fun upsampleAndScale(
        samples24k: ShortArray,
        volumePercent: Int,
    ): ByteArray {
        val volume = volumePercent.coerceIn(0, 200) / 100f
        val output = ByteArray(samples24k.size * 4) // 2x upsample, 2 bytes per sample
        var outputIndex = 0
        for (sample in samples24k) {
            val scaled = (sample * volume).toInt().coerceIn(Short.MIN_VALUE.toInt(), Short.MAX_VALUE.toInt()).toShort()
            repeat(2) {
                output[outputIndex] = (scaled.toInt() and 0xFF).toByte()
                output[outputIndex + 1] = ((scaled.toInt() shr 8) and 0xFF).toByte()
                outputIndex += 2
            }
        }
        return output
    }

    private fun stopInternal() {
        if (active) {
            runCatching { audioTrack.pause() }
            runCatching { audioTrack.flush() }
            runCatching { audioTrack.stop() }
        }
        active = false
        writtenFrames = 0
        abandonAudioFocus()
    }

    private fun requestAudioFocus() {
        if (hasAudioFocus) {
            return
        }
        val result = audioManager?.requestAudioFocus(focusRequest)
        hasAudioFocus = result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
    }

    private fun abandonAudioFocus() {
        if (!hasAudioFocus) {
            return
        }
        audioManager?.abandonAudioFocusRequest(focusRequest)
        hasAudioFocus = false
    }

    private fun createAudioTrack(): AudioTrack {
        val format = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .setSampleRate(48_000)
            .setChannelMask(AudioFormat.CHANNEL_OUT_MONO)
            .build()
        val bufferSize = AudioTrack.getMinBufferSize(
            48_000,
            AudioFormat.CHANNEL_OUT_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
        ).coerceAtLeast(48_000)

        return AudioTrack.Builder()
            .setAudioAttributes(outputAudioAttributes)
            .setAudioFormat(format)
            .setTransferMode(AudioTrack.MODE_STREAM)
            .setBufferSizeInBytes(bufferSize)
            .build()
    }

    private fun buildOutputAudioAttributes(): AudioAttributes {
        return AudioAttributes.Builder()
            .setUsage(AudioAttributes.USAGE_ASSISTANT)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_NONE)
                }
            }
            .build()
    }
}
