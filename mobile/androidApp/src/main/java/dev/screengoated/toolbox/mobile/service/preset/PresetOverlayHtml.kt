package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.service.OverlayBounds
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import org.json.JSONArray
import org.json.JSONObject

internal fun buildPanelBootstrap(
    presets: List<ResolvedPreset>,
    lang: String,
): String {
    val payload = JSONObject()
    payload.put("title", localized(lang, "Favorite presets", "Preset yêu thích", "즐겨찾기 프리셋"))
    payload.put(
        "emptyText",
        localized(
            lang,
            "No favorite presets yet. Star presets in the app first.",
            "Chưa có preset yêu thích. Hãy đánh dấu sao trong app trước.",
            "아직 즐겨찾기 프리셋이 없습니다. 먼저 앱에서 별표를 추가하세요.",
        ),
    )
    payload.put("keepOpenLabel", localized(lang, "Keep open", "Giữ mở", "계속 열기"))
    payload.put("bubbleSizeDownLabel", localized(lang, "Smaller", "Nhỏ hơn", "작게"))
    payload.put("bubbleSizeUpLabel", localized(lang, "Larger", "Lớn hơn", "크게"))
    payload.put("keepOpenEnabled", false)
    payload.put("keepOpenSupported", false)
    payload.put("bubbleSizeSupported", false)
    payload.put(
        "items",
        JSONArray().apply {
            presets.forEach { preset ->
                put(
                    JSONObject().apply {
                        put("id", preset.preset.id)
                        put("label", preset.preset.name(lang))
                        put("typeLabel", presetTypeLabel(preset.preset.presetType, lang))
                        put("supported", preset.executionCapability.supported)
                        put("iconKey", panelIconKey(preset))
                        put("accentColor", panelAccentColor(preset))
                        put(
                            "reason",
                            preset.executionCapability.reason?.let { placeholderReasonLabel(it, lang) },
                        )
                    },
                )
            }
        },
    )
    return payload.toString()
}

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

private fun presetTypeLabel(
    presetType: PresetType,
    lang: String,
): String = when (presetType) {
    PresetType.IMAGE -> localized(lang, "Image", "Ảnh", "이미지")
    PresetType.TEXT_SELECT -> localized(lang, "Text select", "Chọn văn bản", "텍스트 선택")
    PresetType.TEXT_INPUT -> localized(lang, "Text input", "Nhập văn bản", "텍스트 입력")
    PresetType.MIC -> localized(lang, "Mic", "Mic", "마이크")
    PresetType.DEVICE_AUDIO -> localized(lang, "Device audio", "Âm thanh thiết bị", "기기 오디오")
}

private fun panelIconKey(preset: ResolvedPreset): String {
    val model = preset.preset
    if (model.audioProcessingMode == "realtime") {
        return "realtime"
    }
    return when (model.presetType) {
        PresetType.IMAGE -> "image"
        PresetType.TEXT_SELECT -> "select"
        PresetType.TEXT_INPUT -> "text"
        PresetType.MIC -> "mic"
        PresetType.DEVICE_AUDIO -> "deviceAudio"
    }
}

private fun panelAccentColor(preset: ResolvedPreset): String {
    if (!preset.executionCapability.supported) {
        return "#8D99AE"
    }

    return when (preset.preset.presetType) {
        PresetType.IMAGE -> "#66C2FF"
        PresetType.TEXT_SELECT,
        PresetType.TEXT_INPUT,
        -> "#67E8A5"
        PresetType.MIC,
        PresetType.DEVICE_AUDIO,
        -> if (preset.preset.audioProcessingMode == "realtime") "#FF7C7C" else "#FFB35C"
    }
}

private fun localized(
    lang: String,
    en: String,
    vi: String,
    ko: String,
): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

internal fun PresetResultWindowId.wireValue(): String = "$sessionId:$blockIdx"
