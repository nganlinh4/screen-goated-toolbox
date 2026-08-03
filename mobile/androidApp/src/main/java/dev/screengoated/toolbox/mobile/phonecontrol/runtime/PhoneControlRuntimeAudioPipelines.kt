package dev.screengoated.toolbox.mobile.phonecontrol.runtime

import dev.screengoated.toolbox.mobile.capture.AudioCaptureController
import dev.screengoated.toolbox.mobile.capture.AudioCaptureReadException
import dev.screengoated.toolbox.mobile.phonecontrol.PhoneControlLog as Log
import dev.screengoated.toolbox.mobile.service.tts.AudioTrackPlayer
import dev.screengoated.toolbox.mobile.shared.live.GenerationPlaybackGate
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.currentCoroutineContext
import kotlinx.coroutines.delay
import kotlinx.coroutines.channels.SendChannel
import kotlinx.coroutines.flow.collect
import kotlinx.coroutines.isActive
import java.util.concurrent.atomic.AtomicInteger

internal class PhoneControlRuntimeAudioPipelines(
    private val audioCapture: AudioCaptureController,
    private val audioPlayer: AudioTrackPlayer,
    private val playbackGate: GenerationPlaybackGate,
    private val audioFrames: SendChannel<ShortArray>,
    private val bufferedAudio: AtomicInteger,
    private val playback: PhoneControlPlaybackQueue,
    private val inputAdmitted: () -> Boolean,
    private val onListeningLevel: (Float) -> Unit,
) {
    suspend fun captureMicrophone() {
        Log.i(TAG, "microphone_capture_starting")
        var firstFrame = true
        var consecutiveFailures = 0
        while (currentCoroutineContext().isActive) {
            try {
                audioCapture.open(
                    config = LiveSessionConfig(sourceMode = SourceMode.MIC),
                    onRms = { level ->
                        onListeningLevel(if (inputAdmitted()) level else 0f)
                    },
                ).collect { samples ->
                    consecutiveFailures = 0
                    if (firstFrame) {
                        firstFrame = false
                        Log.i(TAG, "microphone_capture_started samples_per_frame=${samples.size}")
                    }
                    if (inputAdmitted() && audioFrames.trySend(samples).isSuccess) {
                        bufferedAudio.incrementAndGet()
                    }
                }
                if (currentCoroutineContext().isActive) {
                    error("Microphone capture closed without cancellation.")
                }
            } catch (cancelled: CancellationException) {
                throw cancelled
            } catch (error: Throwable) {
                consecutiveFailures += 1
                if (consecutiveFailures > MAX_CAPTURE_REOPEN_ATTEMPTS) {
                    throw PhoneControlRuntimeFailure(
                        PhoneControlRuntimeCode.MICROPHONE_FAILED,
                        "Phone Control could not keep the microphone open.",
                        error,
                    )
                }
                val code = (error as? AudioCaptureReadException)
                    ?.diagnosticCode
                    ?: "capture_closed"
                onListeningLevel(0f)
                Log.w(
                    TAG,
                    "microphone_capture_retry attempt=$consecutiveFailures code=$code",
                )
                delay(MICROPHONE_REOPEN_DELAY_MS)
            }
        }
    }

    suspend fun playOutput() {
        playback.consume { chunk ->
            playbackGate.playIfCurrent(chunk) { bytes ->
                audioPlayer.playNativePcm24k(bytes, DEFAULT_OUTPUT_VOLUME_PERCENT)
            }
        }
    }

    private companion object {
        const val TAG = "SGTPhoneControl"
        const val DEFAULT_OUTPUT_VOLUME_PERCENT = 100
        const val MAX_CAPTURE_REOPEN_ATTEMPTS = 4
        const val MICROPHONE_REOPEN_DELAY_MS = 500L
    }
}
