package dev.screengoated.toolbox.mobile.preset.ui

import dev.screengoated.toolbox.mobile.shared.preset.BlockType

// ---------------------------------------------------------------------------
// Localized node-graph labels
// ---------------------------------------------------------------------------

internal fun nodeTypeLabel(
    blockType: BlockType,
    lang: String,
    presetType: dev.screengoated.toolbox.mobile.shared.preset.PresetType =
        dev.screengoated.toolbox.mobile.shared.preset.PresetType.TEXT_SELECT,
): String = when (blockType) {
    BlockType.INPUT_ADAPTER -> {
        val inputSuffix = when (presetType) {
            dev.screengoated.toolbox.mobile.shared.preset.PresetType.IMAGE ->
                nodeGraphLocalized(lang, "Image", "Hình ảnh", "이미지")
            dev.screengoated.toolbox.mobile.shared.preset.PresetType.MIC,
            dev.screengoated.toolbox.mobile.shared.preset.PresetType.DEVICE_AUDIO ->
                nodeGraphLocalized(lang, "Audio", "Âm thanh", "오디오")
            else ->
                nodeGraphLocalized(lang, "Text", "Văn bản", "텍스트")
        }
        val prefix = nodeGraphLocalized(lang, "Input", "Đầu vào", "입력")
        "$prefix: $inputSuffix"
    }
    BlockType.TEXT -> nodeGraphLocalized(lang, "Text -> Text", "Text -> Text", "텍스트 -> 텍스트")
    BlockType.IMAGE -> nodeGraphLocalized(lang, "Image -> Text", "Ảnh -> Text", "이미지 -> 텍스트")
    BlockType.AUDIO -> nodeGraphLocalized(lang, "Audio -> Text", "Audio -> Text", "오디오 -> 텍스트")
}

internal fun nodeGraphLocalized(lang: String, en: String, vi: String, ko: String): String = when (lang) {
    "vi" -> vi
    "ko" -> ko
    else -> en
}

internal fun nodeGraphModelLabel(lang: String): String =
    nodeGraphLocalized(lang, "Model:", "Mô hình:", "모델:")

internal fun nodeGraphPromptLabel(lang: String): String =
    nodeGraphLocalized(lang, "Prompt:", "Lệnh:", "프롬프트:")

internal fun nodeGraphAddLanguageLabel(lang: String): String =
    nodeGraphLocalized(lang, "+ Language", "+ Ngôn ngữ", "+ 언어")

internal fun nodeGraphPromptPlaceholder(lang: String): String =
    nodeGraphLocalized(lang, "Prompt…", "Lệnh…", "프롬프트…")

internal fun nodeGraphLanguageSearchPlaceholder(lang: String): String =
    nodeGraphLocalized(lang, "Search...", "Tìm kiếm...", "검색...")

internal fun nodeGraphStreamLabel(lang: String, isStreaming: Boolean): String = when {
    isStreaming -> nodeGraphLocalized(lang, "Stream", "Stream", "스트림")
    else -> nodeGraphLocalized(lang, "No Stream", "Không stream", "스트림 없음")
}
