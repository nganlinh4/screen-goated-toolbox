package dev.screengoated.toolbox.mobile.service.preset

import android.os.Handler
import android.os.Looper
import android.util.Log
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.preset.sniffAudioMimeType

internal fun inputAudioForDownload(state: PresetResultWindowState): ByteArray? =
    state.inputAudioBytes?.takeIf { state.inputAudioHtml?.get() === state.markdownText }

internal fun PresetOverlayResultModule.downloadInputAudioSupport(active: ActivePresetResultWindow): Boolean {
    val bytes = inputAudioForDownload(active.windowState) ?: return false
    if (!audioDownloadsInFlight.add(active.id)) return true
    val mime = sniffAudioMimeType(bytes)
    val filename = if (mime == "audio/ogg") "recording.ogg" else "recording.wav"
    val success = overlayLocalized(uiLanguage(), "Downloaded", "Đã tải xuống", "다운로드됨")
    val failure = overlayLocalized(uiLanguage(), "Could not download", "Không thể tải xuống", "다운로드할 수 없음")
    val appContext = context.applicationContext
    val started = runCatching {
        Thread {
            val saved = runCatching { saveMediaToDownloads(appContext, filename, mime, bytes) }
                .onFailure { Log.e(PresetOverlayResultModule.TAG, "Audio download failed", it) }
                .isSuccess
            audioDownloadsInFlight.remove(active.id)
            Handler(Looper.getMainLooper()).post {
                Toast.makeText(appContext, if (saved) success else failure, Toast.LENGTH_SHORT).show()
            }
        }.start()
    }.onFailure { Log.e(PresetOverlayResultModule.TAG, "Audio download could not start", it) }.isSuccess
    if (!started) {
        audioDownloadsInFlight.remove(active.id)
        Toast.makeText(appContext, failure, Toast.LENGTH_SHORT).show()
    }
    return true
}
