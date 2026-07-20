package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackChunk
import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackGate
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.channels.ReceiveChannel
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.flow.collect
import java.util.concurrent.atomic.AtomicInteger

internal class PhoneControlRuntimeAudioPipelines(
    private val audioCapture: AudioCaptureController,
    private val audioPlayer: AudioTrackPlayer,
    private val playbackGate: GenerationPlaybackGate,
    private val audioFrames: SendChannel<ShortArray>,
    private val bufferedAudio: AtomicInteger,
    private val playback: ReceiveChannel<GenerationPlaybackChunk>,
    private val onListeningLevel: (Float) -> Unit,
) {
    suspend fun captureMicrophone() {
        try {
            Log.i(TAG, "microphone_capture_starting")
            var firstFrame = true
            audioCapture.open(
                config = LiveSessionConfig(sourceMode = SourceMode.MIC),
                onRms = onListeningLevel,
            ).collect { samples ->
                if (firstFrame) {
                    firstFrame = false
                    Log.i(TAG, "microphone_capture_started samples_per_frame=${samples.size}")
                }
                if (audioFrames.trySend(samples).isSuccess) {
                    bufferedAudio.incrementAndGet()
                }
            }
        } catch (cancelled: CancellationException) {
            throw cancelled
        } catch (error: Throwable) {
            throw PhoneControlRuntimeFailure(
                PhoneControlRuntimeCode.MICROPHONE_FAILED,
                "Phone Control could not keep the microphone open.",
                error,
            )
        }
    }

    suspend fun playOutput() {
        for (chunk in playback) {
            playbackGate.playIfCurrent(chunk) { bytes ->
                audioPlayer.playNativePcm24k(bytes, DEFAULT_OUTPUT_VOLUME_PERCENT)
            }
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
        const val DEFAULT_OUTPUT_VOLUME_PERCENT = 100
    }
}
