package dev.screengoated.toolbox.mobile.preset

internal fun PresetModelProvider.hasTextPresetRuntime(): Boolean = this in setOf(
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GROQ,
    PresetModelProvider.OPENROUTER,
    PresetModelProvider.NVIDIA,
    PresetModelProvider.GOOGLE_GTX,
    PresetModelProvider.GEMINI_LIVE,
    PresetModelProvider.OLLAMA,
    PresetModelProvider.TAALAS,
)

internal fun PresetModelProvider.hasVisionPresetRuntime(): Boolean = this in setOf(
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GROQ,
    PresetModelProvider.OPENROUTER,
    PresetModelProvider.NVIDIA,
    PresetModelProvider.GEMINI_LIVE,
    PresetModelProvider.OLLAMA,
    PresetModelProvider.QRSERVER,
)

internal fun PresetModelProvider.hasAudioPresetRuntime(): Boolean = this in setOf(
    PresetModelProvider.GOOGLE,
    PresetModelProvider.GROQ,
    PresetModelProvider.GEMINI_LIVE,
)
