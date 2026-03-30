package dev.screengoated.toolbox.mobile.capture

import android.annotation.SuppressLint
import android.content.Context
import android.media.AudioAttributes
import android.media.AudioFormat
import android.media.AudioPlaybackCaptureConfiguration
import android.media.AudioRecord
import android.media.MediaRecorder
import android.media.projection.MediaProjectionManager
import android.util.Log
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import dev.screengoated.toolbox.mobile.storage.ProjectionConsentStore
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.channels.awaitClose
import kotlinx.coroutines.flow.Flow
import kotlinx.coroutines.flow.callbackFlow
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

class AudioCaptureController(
    private val context: Context,
    private val projectionConsentStore: ProjectionConsentStore,
) {
    /** When true, the next playback flow close keeps MediaProjection alive for reuse. */
    @Volatile var preserveConsentOnClose = false
    private var cachedProjection: android.media.projection.MediaProjection? = null
    @SuppressLint("MissingPermission")
    fun open(
        config: LiveSessionConfig,
        onRms: (Float) -> Unit,
    ): Flow<ShortArray> {
        return when (config.sourceMode) {
            SourceMode.MIC -> microphoneFlow(onRms)
            SourceMode.DEVICE -> playbackFlow(onRms)
        }
    }

    @SuppressLint("MissingPermission")
    private fun microphoneFlow(onRms: (Float) -> Unit): Flow<ShortArray> = callbackFlow {
        val format = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .setSampleRate(SAMPLE_RATE_HZ)
            .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
            .build()

        val minBufferSize = AudioRecord.getMinBufferSize(
            SAMPLE_RATE_HZ,
            AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        check(minBufferSize > 0) { "Unable to allocate microphone capture buffer." }

        val audioRecord = AudioRecord.Builder()
            .setAudioSource(MediaRecorder.AudioSource.VOICE_COMMUNICATION)
            .setAudioFormat(format)
            .setBufferSizeInBytes(minBufferSize * 2)
            .build()
        check(audioRecord.state == AudioRecord.STATE_INITIALIZED) {
            "Microphone AudioRecord failed to initialize."
        }

        // Attach acoustic echo canceller to suppress speaker→mic feedback
        val aec = if (android.media.audiofx.AcousticEchoCanceler.isAvailable()) {
            runCatching {
                android.media.audiofx.AcousticEchoCanceler.create(audioRecord.audioSessionId)?.also {
                    it.enabled = true
                    Log.d(TAG, "AcousticEchoCanceler attached (session=${audioRecord.audioSessionId})")
                }
            }.getOrNull()
        } else {
            Log.w(TAG, "AcousticEchoCanceler not available on this device")
            null
        }
        // Also attach noise suppressor
        val ns = if (android.media.audiofx.NoiseSuppressor.isAvailable()) {
            runCatching {
                android.media.audiofx.NoiseSuppressor.create(audioRecord.audioSessionId)?.also {
                    it.enabled = true
                }
            }.getOrNull()
        } else null

        audioRecord.startRecording()
        Log.d(TAG, "Mic capture started with VOICE_COMMUNICATION + AEC, bufferBytes=${minBufferSize * 2}")

        val reader = launch(Dispatchers.IO) {
            val buffer = ShortArray(minBufferSize / 2)
            var chunkCount = 0
            var lastLogAtMs = System.currentTimeMillis()
            while (isActive) {
                val count = audioRecord.read(buffer, 0, buffer.size, AudioRecord.READ_BLOCKING)
                if (count > 0) {
                    chunkCount += 1
                    val now = System.currentTimeMillis()
                    if (now - lastLogAtMs >= 2000) {
                        Log.d(TAG, "Mic capture active: chunks=$chunkCount lastSize=$count")
                        lastLogAtMs = now
                    }
                    val chunk = buffer.copyOf(count)
                    onRms(chunk.rmsLevel())
                    trySend(chunk)
                } else if (count < 0) {
                    Log.w(TAG, "Mic capture read returned error=$count")
                }
            }
        }

        awaitClose {
            reader.cancel()
            runCatching { audioRecord.stop() }
            runCatching { aec?.release() }
            runCatching { ns?.release() }
            audioRecord.release()
        }
    }

    @SuppressLint("MissingPermission")
    private fun playbackFlow(
        onRms: (Float) -> Unit,
    ): Flow<ShortArray> = callbackFlow {
        val mediaProjection = cachedProjection ?: run {
            val projectionManager = context.getSystemService(MediaProjectionManager::class.java)
                ?: error("MediaProjectionManager unavailable on this device.")
            projectionConsentStore.createMediaProjection(projectionManager)
                ?: throw ProjectionConsentInvalidException("Playback capture consent is missing.")
        }
        cachedProjection = null // consumed — will be re-cached in awaitClose if preserveConsentOnClose

        val format = AudioFormat.Builder()
            .setEncoding(AudioFormat.ENCODING_PCM_16BIT)
            .setSampleRate(SAMPLE_RATE_HZ)
            .setChannelMask(AudioFormat.CHANNEL_IN_MONO)
            .build()

        val minBufferSize = AudioRecord.getMinBufferSize(
            SAMPLE_RATE_HZ,
            AudioFormat.CHANNEL_IN_MONO,
            AudioFormat.ENCODING_PCM_16BIT,
        )
        check(minBufferSize > 0) { "Unable to allocate playback capture buffer." }

        val playbackBuilder = AudioPlaybackCaptureConfiguration.Builder(mediaProjection)
            .addMatchingUsage(AudioAttributes.USAGE_MEDIA)
            .addMatchingUsage(AudioAttributes.USAGE_GAME)
            .addMatchingUsage(AudioAttributes.USAGE_UNKNOWN)
        val playbackConfig = playbackBuilder.build()

        val audioRecord = AudioRecord.Builder()
            .setAudioFormat(format)
            .setAudioPlaybackCaptureConfig(playbackConfig)
            .setBufferSizeInBytes(minBufferSize * 2)
            .build()
        check(audioRecord.state == AudioRecord.STATE_INITIALIZED) {
            "Playback AudioRecord failed to initialize."
        }

        audioRecord.startRecording()
        Log.d(TAG, "Playback capture started with bufferBytes=${minBufferSize * 2}")

        val reader = launch(Dispatchers.IO) {
            val buffer = ShortArray(minBufferSize / 2)
            var chunkCount = 0
            var lastLogAtMs = System.currentTimeMillis()
            while (isActive) {
                val count = audioRecord.read(buffer, 0, buffer.size, AudioRecord.READ_BLOCKING)
                if (count > 0) {
                    chunkCount += 1
                    val now = System.currentTimeMillis()
                    if (now - lastLogAtMs >= 2000) {
                        Log.d(TAG, "Playback capture active: chunks=$chunkCount lastSize=$count")
                        lastLogAtMs = now
                    }
                    val chunk = buffer.copyOf(count)
                    onRms(chunk.rmsLevel())
                    trySend(chunk)
                } else if (count < 0) {
                    Log.w(TAG, "Playback capture read returned error=$count")
                }
            }
        }

        awaitClose {
            reader.cancel()
            runCatching { audioRecord.stop() }
            audioRecord.release()
            if (preserveConsentOnClose) {
                preserveConsentOnClose = false
                cachedProjection = mediaProjection // keep alive for next open()
            } else {
                mediaProjection.stop()
                projectionConsentStore.clear()
            }
        }
    }

    private companion object {
        private const val TAG = "SGTAudioCapture"
        private const val SAMPLE_RATE_HZ = 16_000
    }
}

internal class ProjectionConsentInvalidException(
    message: String,
) : IllegalStateException(message)

private fun ShortArray.rmsLevel(): Float {
    if (isEmpty()) {
        return 0f
    }
    var sumSquares = 0.0
    for (sample in this) {
        val normalized = sample / 32768.0
        sumSquares += normalized * normalized
    }
    return kotlin.math.sqrt(sumSquares / size).toFloat().coerceIn(0f, 1f)
}
