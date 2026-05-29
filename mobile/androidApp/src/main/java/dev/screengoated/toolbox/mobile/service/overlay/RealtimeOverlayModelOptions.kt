package dev.screengoated.toolbox.mobile.service.overlay

import dev.screengoated.toolbox.mobile.model.RealtimeModelIds

internal data class RealtimeOverlayModelOption(
    val id: String,
    val label: String,
    val enabled: Boolean = true,
)

internal object RealtimeOverlayModelOptions {
    val transcriptionProviderIds: List<String> = listOf(
        RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5,
        RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S,
        RealtimeModelIds.TRANSCRIPTION_PARAKEET,
        "moonshine-tiny-streaming",
        "moonshine-small-streaming",
        "moonshine-medium-streaming",
        "zipformer",
    )

    val translationProviderIds: List<String> = listOf(
        RealtimeModelIds.TRANSLATION_LLM,
        RealtimeModelIds.TRANSLATION_GTX,
    )

    fun transcriptionOptions(
        geminiS2sLabel: String,
        unavailableSuffix: String,
    ): List<RealtimeOverlayModelOption> {
        return listOf(
            RealtimeOverlayModelOption(RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5, GEMINI_LIVE_LABEL),
            RealtimeOverlayModelOption(RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S, geminiS2sLabel),
            RealtimeOverlayModelOption(
                id = RealtimeModelIds.TRANSCRIPTION_PARAKEET,
                label = parakeetLabel(unavailableSuffix),
                enabled = false,
            ),
            RealtimeOverlayModelOption("moonshine-tiny-streaming", MOONSHINE_TINY_LABEL),
            RealtimeOverlayModelOption("moonshine-small-streaming", MOONSHINE_SMALL_LABEL),
            RealtimeOverlayModelOption("moonshine-medium-streaming", MOONSHINE_MEDIUM_LABEL),
            RealtimeOverlayModelOption("zipformer", ZIPFORMER_LABEL),
        )
    }

    fun translationOptions(
        llmLabel: String,
        gtxLabel: String,
    ): List<RealtimeOverlayModelOption> {
        return listOf(
            RealtimeOverlayModelOption(RealtimeModelIds.TRANSLATION_LLM, llmLabel),
            RealtimeOverlayModelOption(RealtimeModelIds.TRANSLATION_GTX, gtxLabel),
        )
    }

    fun parakeetLabel(unavailableSuffix: String): String {
        return "Parakeet ($unavailableSuffix)"
    }

    private const val GEMINI_LIVE_LABEL = "Gemini Live | 100+ languages"
    private const val MOONSHINE_TINY_LABEL = "Moonshine Tiny | 1 language"
    private const val MOONSHINE_SMALL_LABEL = "Moonshine Small | 1 language"
    private const val MOONSHINE_MEDIUM_LABEL = "Moonshine Medium | 1 language"
    private const val ZIPFORMER_LABEL = "Zipformer | 8 languages"
}
