package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetPlaceholderReason
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
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

internal fun buildInputBootstrap(
    preset: Preset,
    lang: String,
): String {
    return JSONObject().apply {
        put("title", preset.name(lang))
        put(
            "footerHint",
            if (preset.continuousInput) {
                localized(lang, "Enter to submit, Shift+Enter for newline, stays open", "Enter để gửi, Shift+Enter xuống dòng, sẽ giữ mở", "Enter 전송, Shift+Enter 줄바꿈, 창 유지")
            } else {
                localized(lang, "Enter to submit, Shift+Enter for newline", "Enter để gửi, Shift+Enter xuống dòng", "Enter 전송, Shift+Enter 줄바꿈")
            },
        )
        put("submitLabel", localized(lang, "Send", "Gửi", "전송"))
        put(
            "placeholder",
            localized(lang, "Type here...", "Nhập tại đây...", "여기에 입력하세요..."),
        )
    }.toString()
}

internal fun buildResultBootstrap(
    preset: Preset,
    lang: String,
    status: String,
): String {
    return JSONObject().apply {
        put("title", preset.name(lang))
        put("status", status)
    }.toString()
}

internal fun buildResultUpdatePayload(
    preset: Preset,
    html: String,
    status: String,
    streaming: Boolean,
    lang: String,
): String {
    return JSONObject().apply {
        put("title", preset.name(lang))
        put("status", status)
        put("html", html)
        put("streaming", streaming)
    }.toString()
}

internal fun buildCanvasBootstrap(lang: String): String {
    return JSONObject().apply {
        put("copyLabel", localized(lang, "Copy", "Sao chép", "복사"))
        put("closeLabel", localized(lang, "Close", "Đóng", "닫기"))
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
        localized(lang, "Only text graphs supported", "Chỉ hỗ trợ graph văn bản", "텍스트 그래프만 지원")
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
    return when (model.presetType) {
        PresetType.IMAGE -> "image"
        PresetType.TEXT_SELECT -> "select"
        PresetType.TEXT_INPUT -> "text"
        PresetType.MIC -> if (model.audioProcessingMode == "realtime") "realtime" else "mic"
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
        PresetType.MIC -> {
            if (preset.preset.audioProcessingMode == "realtime") {
                "#FF7C7C"
            } else {
                "#FFB35C"
            }
        }
        PresetType.DEVICE_AUDIO -> "#FFB35C"
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
