package dev.screengoated.toolbox.mobile.service.tts

import android.content.Context
import android.media.AudioDeviceInfo
import android.media.AudioAttributes
import android.media.AudioFocusRequest
import android.media.AudioFormat
import android.media.AudioManager
import android.media.AudioTrack
import android.os.Build
import android.os.SystemClock
import android.util.Log

internal class AudioTrackPlayer(
    context: Context,
) {
    data class PlaybackDebugSnapshot(
        val active: Boolean,
        val writtenFrames: Long,
        val playedFrames: Long,
        val pendingFrames: Long,
        val lastPlayStartedAtMs: Long,
        val lastWriteCompletedAtMs: Long,
        val lastStopAtMs: Long,
        val playState: Int,
        val trackState: Int,
        val audioSessionId: Int,
        val audioMode: Int?,
        val communicationDevice: String?,
        val voiceCallVolume: Int?,
        val voiceCallMaxVolume: Int?,
        val musicVolume: Int?,
        val musicMaxVolume: Int?,
    )

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
    private var lastPlayStartedAtMs: Long = 0L
    private var lastWriteCompletedAtMs: Long = 0L
    private var lastStopAtMs: Long = 0L
    private var communicationSessionActive = false
    private var previousAudioMode: Int? = null
    private var previousSpeakerphoneOn: Boolean? = null

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
            lastWriteCompletedAtMs = SystemClock.elapsedRealtime()
            Log.d(
                TAG,
                "playPcm24k wrote bytes=$writtenBytes frames=${writtenBytes / 2L} totalWrittenFrames=$writtenFrames speed=$speedPercent volume=$volumePercent",
            )
        }
    }

    @Synchronized
    fun beginCommunicationSession() {
        if (communicationSessionActive) {
            Log.d(TAG, "beginCommunicationSession ignored because communication session is already active")
            return
        }
        val manager = audioManager
        if (manager == null) {
            Log.w(TAG, "beginCommunicationSession skipped because AudioManager is unavailable")
            return
        }
        Log.d(
            TAG,
            "beginCommunicationSession requested mode=${manager.mode.debugAudioMode()} currentDevice=${manager.currentCommunicationDeviceLabel()} availableDevices=${manager.availableCommunicationDevicesLabel()} voiceVolume=${manager.streamVolumeLabel(AudioManager.STREAM_VOICE_CALL)} musicVolume=${manager.streamVolumeLabel(AudioManager.STREAM_MUSIC)}",
        )
        previousAudioMode = manager.mode
        previousSpeakerphoneOn = runCatching { manager.readSpeakerphoneOn() }.getOrNull()
        runCatching {
            manager.mode = AudioManager.MODE_IN_COMMUNICATION
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                val preferredDevice = manager.availableCommunicationDevices.firstOrNull {
                    it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER
                } ?: manager.availableCommunicationDevices.firstOrNull {
                    it.type == AudioDeviceInfo.TYPE_BUILTIN_EARPIECE
                }
                if (preferredDevice != null) {
                    val selected = manager.setCommunicationDevice(preferredDevice)
                    Log.d(
                        TAG,
                        "Communication routing started mode=${manager.mode} device=${preferredDevice.debugLabel()} selected=$selected previousMode=$previousAudioMode",
                    )
                } else {
                    Log.d(
                        TAG,
                        "Communication routing started mode=${manager.mode} device=none previousMode=$previousAudioMode",
                    )
                }
            } else {
                runCatching { manager.writeSpeakerphoneOn(true) }
                Log.d(
                    TAG,
                    "Communication routing started mode=${manager.mode} speakerphoneOn=${runCatching { manager.readSpeakerphoneOn() }.getOrDefault(false)} previousMode=$previousAudioMode",
                )
            }
            communicationSessionActive = true
        }.onFailure { error ->
            Log.w(TAG, "Failed to start communication routing: ${error.message}", error)
        }
    }

    @Synchronized
    fun endCommunicationSession() {
        if (!communicationSessionActive) {
            Log.d(TAG, "endCommunicationSession ignored because communication session is not active")
            return
        }
        val manager = audioManager
        runCatching {
            if (manager != null) {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.S) {
                    manager.clearCommunicationDevice()
                } else {
                    previousSpeakerphoneOn?.let { manager.writeSpeakerphoneOn(it) }
                }
                previousAudioMode?.let { manager.mode = it }
                Log.d(
                    TAG,
                    "Communication routing ended restoredMode=${manager.mode} speakerphoneOn=${runCatching { manager.readSpeakerphoneOn() }.getOrDefault(false)}",
                )
            }
        }.onFailure { error ->
            Log.w(TAG, "Failed to end communication routing: ${error.message}", error)
        }
        communicationSessionActive = false
        previousAudioMode = null
        previousSpeakerphoneOn = null
    }

    @Synchronized
    fun debugSnapshot(): PlaybackDebugSnapshot {
        val nowMs = SystemClock.elapsedRealtime()
        var played = audioTrack.playbackHeadPosition.toLong() and 0xFFFFFFFFL
        var pending = (writtenFrames - played).coerceAtLeast(0L)
        if (active && pending == 0L && ageMs(lastWriteCompletedAtMs, nowMs) >= PLAYBACK_IDLE_STOP_MS) {
            Log.d(
                TAG,
                "AudioTrack playback auto-stopped after drain playState=${audioTrack.playState} trackState=${audioTrack.state} audioSessionId=${audioTrack.audioSessionId} mode=${audioManager?.mode?.debugAudioMode()} device=${audioManager?.currentCommunicationDeviceLabel()}",
            )
            stopInternal()
            played = audioTrack.playbackHeadPosition.toLong() and 0xFFFFFFFFL
            pending = (writtenFrames - played).coerceAtLeast(0L)
        }
        val manager = audioManager
        return PlaybackDebugSnapshot(
            active = active,
            writtenFrames = writtenFrames,
            playedFrames = played,
            pendingFrames = pending,
            lastPlayStartedAtMs = lastPlayStartedAtMs,
            lastWriteCompletedAtMs = lastWriteCompletedAtMs,
            lastStopAtMs = lastStopAtMs,
            playState = audioTrack.playState,
            trackState = audioTrack.state,
            audioSessionId = audioTrack.audioSessionId,
            audioMode = manager?.mode,
            communicationDevice = manager?.currentCommunicationDeviceLabel(),
            voiceCallVolume = manager?.getStreamVolume(AudioManager.STREAM_VOICE_CALL),
            voiceCallMaxVolume = manager?.getStreamMaxVolume(AudioManager.STREAM_VOICE_CALL),
            musicVolume = manager?.getStreamVolume(AudioManager.STREAM_MUSIC),
            musicMaxVolume = manager?.getStreamMaxVolume(AudioManager.STREAM_MUSIC),
        )
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
            lastPlayStartedAtMs = SystemClock.elapsedRealtime()
            Log.d(
                TAG,
                "AudioTrack playback started playState=${audioTrack.playState} trackState=${audioTrack.state} audioSessionId=${audioTrack.audioSessionId} mode=${audioManager?.mode?.debugAudioMode()} device=${audioManager?.currentCommunicationDeviceLabel()} voiceVolume=${audioManager?.streamVolumeLabel(AudioManager.STREAM_VOICE_CALL)} musicVolume=${audioManager?.streamVolumeLabel(AudioManager.STREAM_MUSIC)}",
            )
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
            val playedBeforeStop = audioTrack.playbackHeadPosition.toLong() and 0xFFFFFFFFL
            runCatching { audioTrack.pause() }
            runCatching { audioTrack.flush() }
            runCatching { audioTrack.stop() }
            Log.d(
                TAG,
                "AudioTrack playback stopped playedBeforeStop=$playedBeforeStop writtenFrames=$writtenFrames mode=${audioManager?.mode?.debugAudioMode()} device=${audioManager?.currentCommunicationDeviceLabel()}",
            )
        }
        active = false
        writtenFrames = 0
        lastStopAtMs = SystemClock.elapsedRealtime()
        abandonAudioFocus()
    }

    private fun requestAudioFocus() {
        if (hasAudioFocus) {
            return
        }
        val result = audioManager?.requestAudioFocus(focusRequest)
        hasAudioFocus = result == AudioManager.AUDIOFOCUS_REQUEST_GRANTED
        Log.d(
            TAG,
            "Audio focus request result=$result granted=$hasAudioFocus mode=${audioManager?.mode?.debugAudioMode()} device=${audioManager?.currentCommunicationDeviceLabel()}",
        )
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
            .setUsage(AudioAttributes.USAGE_VOICE_COMMUNICATION)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .apply {
                if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
                    setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_NONE)
                }
            }
            .build()
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

    private companion object {
        private const val TAG = "SGTTranslationGummyPlayer"
        private const val PLAYBACK_IDLE_STOP_MS = 200L
    }
}

private fun AudioDeviceInfo.debugLabel(): String {
    return when (type) {
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "builtin_speaker"
        AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> "builtin_earpiece"
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "bluetooth_sco"
        AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> "bluetooth_a2dp"
        AudioDeviceInfo.TYPE_WIRED_HEADSET -> "wired_headset"
        AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> "wired_headphones"
        else -> "type_$type"
    }
}

private fun AudioManager.currentCommunicationDeviceLabel(): String {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
        return if (readSpeakerphoneOn()) "legacy_speakerphone" else "legacy_default"
    }
    return communicationDevice?.debugLabel() ?: "none"
}

private fun AudioManager.availableCommunicationDevicesLabel(): String {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
        return "legacy"
    }
    return availableCommunicationDevices.joinToString(prefix = "[", postfix = "]") { it.debugLabel() }
}

private fun AudioManager.streamVolumeLabel(streamType: Int): String {
    return "${getStreamVolume(streamType)}/${getStreamMaxVolume(streamType)}"
}

private fun Int.debugAudioMode(): String {
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

@Suppress("DEPRECATION")
private fun AudioManager.readSpeakerphoneOn(): Boolean {
    return isSpeakerphoneOn
}

@Suppress("DEPRECATION")
private fun AudioManager.writeSpeakerphoneOn(value: Boolean) {
    isSpeakerphoneOn = value
}
