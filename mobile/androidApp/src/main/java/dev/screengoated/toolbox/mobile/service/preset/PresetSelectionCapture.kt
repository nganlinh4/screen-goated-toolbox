package dev.screengoated.toolbox.mobile.service.preset

import android.os.Handler
import android.os.Looper
import android.widget.Toast
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.ClipboardReaderActivity
import dev.screengoated.toolbox.mobile.service.SgtAccessibilityService

internal fun PresetOverlayController.capturePresetSelection(
    resolved: ResolvedPreset,
    armed: () -> Boolean = { true },
) {
    val generation = ++selectionCaptureGeneration
    selectionCapturePending = true
    fun current() = selectionCaptureGeneration == generation && activePreset === resolved && armed()
    fun finish(text: String?) {
        if (selectionCaptureGeneration != generation) return
        selectionCapturePending = false
        processingIndicator.dismiss()
        if (!current()) return
        if (!text.isNullOrBlank()) executeTextSelectWithCapturedText(resolved, text)
        else Toast.makeText(context, when (uiLanguage()) {
            "vi" -> "Hãy copy text trước, sau đó bấm lại preset này"
            "ko" -> "먼저 텍스트를 복사한 후 이 프리셋을 다시 누르세요"
            else -> "Copy text first, then tap this preset again"
        }, Toast.LENGTH_LONG).show()
        // Each invocation accepts at most one clipboard callback.
        selectionCaptureGeneration++
    }
    val service = SgtAccessibilityService.instance
    val treeText = service?.getSelectedText()
    if (!treeText.isNullOrBlank()) { finish(treeText); return }
    service?.eagerCaptureSelection()
    processingIndicator.show(uiPreferencesProvider().themeMode, PresetStatusAccent.SUCCESS)
    service?.readClipboardAsync { text ->
        if (!current()) { finish(null); return@readClipboardAsync }
        if (!text.isNullOrBlank()) finish(text)
        else ClipboardReaderActivity.launch(context) { fallback -> finish(fallback) }
    }
    Handler(Looper.getMainLooper()).postDelayed({ if (current()) finish(null) }, 5000)
}
