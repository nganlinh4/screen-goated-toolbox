package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.triLang
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import org.json.JSONArray
import org.json.JSONObject

internal fun emptyFavoritesMessage(lang: String): String = localized(
    lang,
    "No favorite presets yet. Star presets in the app first.",
    "Chưa có preset yêu thích. Hãy đánh dấu sao trong app trước.",
    "아직 즐겨찾기 프리셋이 없습니다. 먼저 앱에서 별표를 추가하세요.",
)

internal fun buildResultStatePayload(
    windowId: PresetResultWindowId,
    html: String,
    windowState: PresetResultWindowState,
): String {
    return JSONObject().apply {
        put("windowId", windowId.wireValue())
        put("html", html)
        put("loading", windowState.isLoading)
        put("loadingStatusText", windowState.loadingStatusText ?: JSONObject.NULL)
        put("streaming", windowState.isStreaming)
        put("sourceTextLen", windowState.markdownText.length)
        put("sourceTrimmedLen", windowState.markdownText.trim().length)
    }.toString()
}

internal fun buildCanvasPayload(
    window: ActivePresetResultWindow,
    vertical: Boolean,
    lingerMs: Int,
): String {
    return JSONObject().apply {
        put(
            "window",
            JSONObject().apply {
                put("id", window.id.wireValue())
                put("vertical", vertical)
                put(
                    "state",
                    JSONObject().apply {
                        put("copySuccess", window.runtimeState.copySuccess)
                        put("opacityPercent", window.runtimeState.opacityPercent)
                        put("navDepth", window.runtimeState.navDepth)
                        put("maxNavDepth", window.runtimeState.maxNavDepth)
                        put("isBrowsing", window.runtimeState.isBrowsing)
                        put("isMarkdown", true)
                        put("isEditing", window.runtimeState.isEditing)
                        put("hasUndo", window.runtimeState.textHistory.isNotEmpty())
                        put("hasRedo", window.runtimeState.redoHistory.isNotEmpty())
                        put("ttsLoading", window.runtimeState.ttsLoading)
                        put("ttsSpeaking", window.runtimeState.ttsRequestId != 0L && !window.runtimeState.ttsLoading)
                        put(
                            "disabledActions",
                            JSONArray(window.runtimeState.disabledActions.toList()),
                        )
                    },
                )
            },
        )
        put("activeWindowId", window.id.wireValue())
        put("lingerMs", lingerMs)
    }.toString()
}

internal fun placeholderReasonLabel(
    reason: PresetPlaceholderReason,
    lang: String,
): String = when (reason) {
    PresetPlaceholderReason.IMAGE_CAPTURE_NOT_READY ->
        localized(lang, "Image capture not ready", "Chưa hỗ trợ chụp ảnh", "이미지 캡처 미지원")
    PresetPlaceholderReason.TEXT_SELECTION_NOT_READY ->
        localized(lang, "Text selection not ready", "Chưa hỗ trợ chọn văn bản", "텍스트 선택 미지원")
    PresetPlaceholderReason.TEXT_INPUT_OVERLAY_NOT_READY ->
        localized(lang, "Input overlay not ready", "Overlay nhập chưa sẵn sàng", "입력 오버레이 미지원")
    PresetPlaceholderReason.MODEL_PROVIDER_NOT_READY ->
        localized(lang, "Model/provider runtime not ready", "Runtime model/provider chưa sẵn sàng", "모델/제공자 런타임 미지원")
    PresetPlaceholderReason.AUDIO_CAPTURE_NOT_READY ->
        localized(lang, "Audio capture not ready", "Chưa hỗ trợ âm thanh", "오디오 캡처 미지원")
    PresetPlaceholderReason.REALTIME_AUDIO_NOT_READY ->
        localized(lang, "Realtime audio not ready", "Âm thanh thời gian thực chưa sẵn sàng", "실시간 오디오 미지원")
    PresetPlaceholderReason.HTML_RESULT_NOT_READY ->
        localized(lang, "HTML result not ready", "Kết quả HTML chưa hỗ trợ", "HTML 결과 미지원")
    PresetPlaceholderReason.CONTROLLER_MODE_NOT_READY ->
        localized(lang, "Controller mode not ready", "Controller chưa hỗ trợ", "컨트롤러 미지원")
    PresetPlaceholderReason.AUTO_PASTE_NOT_READY ->
        localized(lang, "Auto-paste not ready", "Tự dán chưa hỗ trợ", "자동 붙여넣기 미지원")
    PresetPlaceholderReason.HOTKEYS_NOT_READY ->
        localized(lang, "Hotkeys not ready", "Phím tắt chưa hỗ trợ", "단축키 미지원")
    PresetPlaceholderReason.GRAPH_EDITING_NOT_READY ->
        localized(lang, "Graph editing placeholder", "Chỉnh sửa graph hiện chỉ là placeholder", "그래프 편집 플레이스홀더")
    PresetPlaceholderReason.NON_TEXT_GRAPH_NOT_READY ->
        localized(
            lang,
            "This graph still uses Android-unsupported block types",
            "Graph này vẫn dùng các block Android chưa hỗ trợ",
            "이 그래프는 아직 Android에서 지원되지 않는 블록을 사용합니다",
        )
}

private fun localized(
    lang: String,
    en: String,
    vi: String,
    ko: String,
): String = triLang(lang, en, vi, ko)

internal fun PresetResultWindowId.wireValue(): String = "$sessionId:$blockIdx"
