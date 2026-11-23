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
    private val outputMode: AudioTrackOutputMode = AudioTrackOutputMode.VOICE_COMMUNICATION,
    private val outputVolumeBoost: Float = 1.0f,
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
        val outputMode: AudioTrackOutputMode,
        val communicationDevice: String?,
        val routedDevice: String?,
        val voiceCallVolume: Int?,
        val voiceCallMaxVolume: Int?,
        val musicVolume: Int?,
        val musicMaxVolume: Int?,
    )

    private val appContext = context.applicationContext
    private val audioManager = appContext.getSystemService(AudioManager::class.java)
    private val outputAudioAttributes = buildOutputAudioAttributes()
    private val focusListener = AudioManager.OnAudioFocusChangeListener { }
    private val focusRequest = AudioFocusRequest.Builder(AudioManager.AUDIOFOCUS_GAIN_TRANSIENT_MAY_DUCK)
        .setAudioAttributes(outputAudioAttributes)
        .setOnAudioFocusChangeListener(focusListener)
        .build()

    private val audioTrack = createAudioTrack()
    private val stretcher = WsolaStretcher(24_000)

    // Snapshots are read per audio frame; the AudioManager binder reads behind
    // them are refreshed at most once per TTL instead of on every call.
    private val routeCache = PlaybackRouteCache(ROUTE_SNAPSHOT_TTL_MS, ::readPlaybackRoute)

    // Generation of the write in progress. The idle auto-stop in debugSnapshot()
    // must not cut a writer that just started and has not written its first slice.
    private var writerGeneration = NO_WRITER
    private var writtenFrames: Long = 0
    private var hasAudioFocus = false
    private var active = false
    private var playbackGeneration = 0L
    private var lastPlayStartedAtMs: Long = 0L
    private var lastWriteCompletedAtMs: Long = 0L
    private var lastStopAtMs: Long = 0L
    private var nativePlaybackChunks: Long = 0L
    private var lastNativePlaybackLogAtMs: Long = 0L
    private var communicationSessionActive = false
    private var previousAudioMode: Int? = null
    private var previousSpeakerphoneOn: Boolean? = null

    fun playPcm24kInterruptibly(
        pcm24k: ByteArray,
        speedPercent: Int,
        volumePercent: Int,
        isCurrent: () -> Boolean,
        dispatch: (() -> Unit) -> Boolean,
    ): PlaybackWriteResult {
        val output = synchronized(this) {
            if (!isCurrent()) { stopInternal(); return PlaybackWriteResult.INTERRUPTED }
            preparePcm24k(pcm24k, speedPercent, volumePercent)
        }
        return writePreparedInterruptibly(output, isCurrent, dispatch)
    }

    fun finishPcm24kInterruptibly(
        volumePercent: Int,
        isCurrent: () -> Boolean,
        dispatch: (() -> Unit) -> Boolean,
    ): PlaybackWriteResult {
        val output = synchronized(this) {
            if (!isCurrent()) { stopInternal(); return PlaybackWriteResult.INTERRUPTED }
            upsampleAndScale(stretcher.finish(), effectiveVolumePercent(volumePercent))
        }
        return writePreparedInterruptibly(output, isCurrent, dispatch)
    }

    fun playNativePcm24kInterruptibly(
        pcm24k: ByteArray,
        volumePercent: Int,
        isCurrent: () -> Boolean,
        dispatch: (() -> Unit) -> Boolean,
    ): PlaybackWriteResult {
        if (!isCurrent()) {
            synchronized(this) { stopInternal() }
            return PlaybackWriteResult.INTERRUPTED
        }
        val output = upsampleAndScale(decodePcm16Le(pcm24k), effectiveVolumePercent(volumePercent))
        return writePreparedInterruptibly(output, isCurrent, dispatch)
    }

    /** Plays one native 24 kHz chunk for a single owner; a stop cuts it between bounded writes. */
    fun playNativePcm24k(
        pcm24k: ByteArray,
        volumePercent: Int,
    ) {
        if (pcm24k.isEmpty()) return
        val result = playNativePcm24kInterruptibly(pcm24k, volumePercent, isCurrent = { true }) { effect ->
            effect()
            true
        }
        if (result == PlaybackWriteResult.COMPLETE) logNativePlaybackHealth(pcm24k.size, volumePercent)
    }

    /**
     * Writes in bounded nonblocking slices. The monitor is held only for each
     * slice, never across a wait, so stopImmediate() and snapshots are never
     * blocked behind a long chunk; a stop advances the generation and the
     * writer notices before its next slice.
     */
    private fun writePreparedInterruptibly(
        output: ByteArray,
        isCurrent: () -> Boolean,
        dispatch: (() -> Unit) -> Boolean,
    ): PlaybackWriteResult {
        if (output.isEmpty()) return PlaybackWriteResult.NO_OUTPUT
        var ownedGeneration = -1L
        if (!dispatch {
            if (isCurrent()) {
                synchronized(this) {
                    requestAudioFocus()
                    ensureStarted()
                    playbackGeneration++
                    ownedGeneration = playbackGeneration
                    writerGeneration = ownedGeneration
                }
            }
        } || ownedGeneration < 0) {
            synchronized(this) { stopInternal() }
            return PlaybackWriteResult.INTERRUPTED
        }
        val owned = { isCurrent() && synchronized(this) { playbackGeneration == ownedGeneration } }
        val result = writeInterruptibly(output.size, owned, dispatch, { offset, count ->
            synchronized(this) {
                if (playbackGeneration != ownedGeneration) return@synchronized 0
                val written = audioTrack.write(output, offset, count, AudioTrack.WRITE_NON_BLOCKING)
                if (written > 0) {
                    writtenFrames += written / 2L
                    lastWriteCompletedAtMs = SystemClock.elapsedRealtime()
                }
                written
            }
        }, { Thread.sleep(WRITE_RETRY_MS) })
        if (result != PlaybackWriteResult.COMPLETE) {
            synchronized(this) { if (playbackGeneration == ownedGeneration) stopInternal() }
        }
        synchronized(this) { if (writerGeneration == ownedGeneration) writerGeneration = NO_WRITER }
        return result
    }

    private fun preparePcm24k(pcm24k: ByteArray, speedPercent: Int, volumePercent: Int): ByteArray {
        // Apply WSOLA time-stretching at 24kHz (pitch-preserving)
        val speedRatio = speedPercent.coerceIn(50, 200) / 100.0
        val stretched = stretcher.stretch(decodePcm16Le(pcm24k), speedRatio)

        // Upsample stretched 24kHz to 48kHz and apply volume
        return upsampleAndScale(stretched, effectiveVolumePercent(volumePercent))
    }

    @Synchronized
    private fun logNativePlaybackHealth(lastBytes: Int, volumePercent: Int) {
        nativePlaybackChunks++
        val nowMs = SystemClock.elapsedRealtime()
        if (nowMs - lastNativePlaybackLogAtMs < 5_000L) return
        lastNativePlaybackLogAtMs = nowMs
        val playedFrames = audioTrack.playbackHeadPosition.toLong() and 0xFFFFFFFFL
        Log.d(
            TAG,
            "playNativePcm24k health chunks=$nativePlaybackChunks last_bytes=${lastBytes * 2} totalWrittenFrames=$writtenFrames playedFrames=$playedFrames pendingFrames=${(writtenFrames - playedFrames).coerceAtLeast(0)} volume=$volumePercent effective_volume=${effectiveVolumePercent(volumePercent)} outputMode=$outputMode",
        )
    }

    @Synchronized
    fun beginCommunicationSession() {
        routeCache.invalidate()
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
                val preferredDevice = manager.preferredCommunicationDevice()
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
        routeCache.invalidate()
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
        val writerOwnsPlayback = writerGeneration == playbackGeneration
        if (active && !writerOwnsPlayback && pending == 0L && ageMs(lastWriteCompletedAtMs, nowMs) >= PLAYBACK_IDLE_STOP_MS) {
            Log.d(
                TAG,
                "AudioTrack playback auto-stopped after drain playState=${audioTrack.playState} trackState=${audioTrack.state} audioSessionId=${audioTrack.audioSessionId} mode=${audioManager?.mode?.debugAudioMode()} device=${audioManager?.currentCommunicationDeviceLabel()}",
            )
            stopInternal()
            played = audioTrack.playbackHeadPosition.toLong() and 0xFFFFFFFFL
            pending = (writtenFrames - played).coerceAtLeast(0L)
        }
        val route = routeCache.get(nowMs)
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
            audioMode = route.audioMode,
            outputMode = outputMode,
            communicationDevice = route.communicationDevice,
            routedDevice = route.routedDevice,
            voiceCallVolume = route.voiceCallVolume,
            voiceCallMaxVolume = route.voiceCallMaxVolume,
            musicVolume = route.musicVolume,
            musicMaxVolume = route.musicMaxVolume,
        )
    }

    private fun readPlaybackRoute(): PlaybackRouteInfo {
        val manager = audioManager
        return PlaybackRouteInfo(
            audioMode = manager?.mode,
            communicationDevice = manager?.currentCommunicationDeviceLabel(),
            routedDevice = if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                audioTrack.routedDevice?.debugLabel()
            } else {
                null
            },
            voiceCallVolume = manager?.getStreamVolume(AudioManager.STREAM_VOICE_CALL),
            voiceCallMaxVolume = manager?.getStreamMaxVolume(AudioManager.STREAM_VOICE_CALL),
            musicVolume = manager?.getStreamVolume(AudioManager.STREAM_MUSIC),
            musicMaxVolume = manager?.getStreamMaxVolume(AudioManager.STREAM_MUSIC),
        )
    }

    fun drain(timeoutMs: Long = 30_000, isCurrent: () -> Boolean = { true }): PlaybackDrainResult {
        val generation = synchronized(this) { playbackGeneration }
        val completed = waitForPlaybackDrain(
            deadlineMs = SystemClock.elapsedRealtime() + timeoutMs,
            now = SystemClock::elapsedRealtime,
            isCurrent = { isCurrent() && synchronized(this) { playbackGeneration == generation } },
            isDrained = { synchronized(this) {
                !active || (audioTrack.playbackHeadPosition.toLong() and 0xFFFFFFFFL) >= writtenFrames
            } },
            wait = { Thread.sleep(20) },
        )
        synchronized(this) { if (playbackGeneration == generation) stopInternal() }
        return if (isCurrent()) completed else PlaybackDrainResult.INTERRUPTED
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
            playbackGeneration++
            audioTrack.flush()
            audioTrack.play()
            writtenFrames = 0
            active = true
            lastPlayStartedAtMs = SystemClock.elapsedRealtime()
            Log.d(
                TAG,
                "AudioTrack playback started outputMode=$outputMode playState=${audioTrack.playState} trackState=${audioTrack.state} audioSessionId=${audioTrack.audioSessionId} mode=${audioManager?.mode?.debugAudioMode()} communicationDevice=${audioManager?.currentCommunicationDeviceLabel()} routedDevice=${if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) audioTrack.routedDevice?.debugLabel() else null} voiceVolume=${audioManager?.streamVolumeLabel(AudioManager.STREAM_VOICE_CALL)} musicVolume=${audioManager?.streamVolumeLabel(AudioManager.STREAM_MUSIC)}",
            )
        }
    }

    private fun upsampleAndScale(
        samples24k: ShortArray,
        volumePercent: Int,
    ): ByteArray {
        val volume = volumePercent.coerceIn(0, MAX_OUTPUT_VOLUME_PERCENT) / 100f
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

    private fun effectiveVolumePercent(volumePercent: Int): Int {
        return (volumePercent.coerceIn(0, MAX_INPUT_VOLUME_PERCENT) * outputVolumeBoost)
            .toInt()
            .coerceIn(0, MAX_OUTPUT_VOLUME_PERCENT)
    }

    private fun stopInternal() {
        playbackGeneration++
        stretcher.reset()
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
        nativePlaybackChunks = 0
        lastNativePlaybackLogAtMs = 0L
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
        val usage = when (outputMode) {
            AudioTrackOutputMode.MEDIA -> AudioAttributes.USAGE_MEDIA
            AudioTrackOutputMode.VOICE_COMMUNICATION -> AudioAttributes.USAGE_VOICE_COMMUNICATION
        }
        return AudioAttributes.Builder()
            .setUsage(usage)
            .setContentType(AudioAttributes.CONTENT_TYPE_SPEECH)
            .setAllowedCapturePolicy(AudioAttributes.ALLOW_CAPTURE_BY_NONE)
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
        private const val MAX_INPUT_VOLUME_PERCENT = 200
        private const val MAX_OUTPUT_VOLUME_PERCENT = 360
        private const val WRITE_RETRY_MS = 10L
        private const val NO_WRITER = -1L
        private const val ROUTE_SNAPSHOT_TTL_MS = 1_000L
    }
}

internal enum class AudioTrackOutputMode {
    MEDIA,
    VOICE_COMMUNICATION,
}

private fun AudioDeviceInfo.debugLabel(): String {
    return when (type) {
        AudioDeviceInfo.TYPE_BUILTIN_SPEAKER -> "builtin_speaker"
        AudioDeviceInfo.TYPE_BUILTIN_EARPIECE -> "builtin_earpiece"
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO -> "bluetooth_sco"
        AudioDeviceInfo.TYPE_BLUETOOTH_A2DP -> "bluetooth_a2dp"
        AudioDeviceInfo.TYPE_WIRED_HEADSET -> "wired_headset"
        AudioDeviceInfo.TYPE_WIRED_HEADPHONES -> "wired_headphones"
        AudioDeviceInfo.TYPE_USB_HEADSET -> "usb_headset"
        AudioDeviceInfo.TYPE_HEARING_AID -> "hearing_aid"
        AudioDeviceInfo.TYPE_BLE_HEADSET -> "ble_headset"
        AudioDeviceInfo.TYPE_BLE_SPEAKER -> "ble_speaker"
        AudioDeviceInfo.TYPE_BLE_BROADCAST -> "ble_broadcast"
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

private fun AudioManager.preferredCommunicationDevice(): AudioDeviceInfo? {
    if (Build.VERSION.SDK_INT < Build.VERSION_CODES.S) {
        return null
    }
    val devices = availableCommunicationDevices
    return devices.firstOrNull { it.isExternalCommunicationDevice() }
        ?: devices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_SPEAKER }
        ?: devices.firstOrNull { it.type == AudioDeviceInfo.TYPE_BUILTIN_EARPIECE }
}

private fun AudioDeviceInfo.isExternalCommunicationDevice(): Boolean {
    return when (type) {
        AudioDeviceInfo.TYPE_BLUETOOTH_SCO,
        AudioDeviceInfo.TYPE_BLUETOOTH_A2DP,
        AudioDeviceInfo.TYPE_WIRED_HEADSET,
        AudioDeviceInfo.TYPE_WIRED_HEADPHONES,
        AudioDeviceInfo.TYPE_USB_HEADSET,
        AudioDeviceInfo.TYPE_HEARING_AID,
        -> true
        AudioDeviceInfo.TYPE_BLE_HEADSET,
        AudioDeviceInfo.TYPE_BLE_SPEAKER,
        AudioDeviceInfo.TYPE_BLE_BROADCAST,
        -> Build.VERSION.SDK_INT >= Build.VERSION_CODES.S
        else -> false
    }
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
