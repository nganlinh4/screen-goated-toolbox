package dev.screengoated.toolbox.mobile.service.overlay

import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.shared.live.GeneratedLiveModelCatalog

internal data class RealtimeOverlayModelOption(
    val id: String,
    val label: String,
    val enabled: Boolean = true,
)

internal object RealtimeOverlayModelOptions {
    val transcriptionProviderIds: List<String> =
        GeneratedLiveModelCatalog.realtimeTranscriptionOptions.map { it.id }

    val translationProviderIds: List<String> = listOf(
        RealtimeModelIds.TRANSLATION_LLM,
        RealtimeModelIds.TRANSLATION_GTX,
    )

    fun transcriptionOptions(
        geminiS2sLabel: String,
        unavailableSuffix: String,
    ): List<RealtimeOverlayModelOption> {
        return GeneratedLiveModelCatalog.realtimeTranscriptionOptions.map { option ->
            when (option.id) {
                RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S -> RealtimeOverlayModelOption(
                    option.id,
                    geminiS2sLabel,
                )

                RealtimeModelIds.TRANSCRIPTION_PARAKEET -> RealtimeOverlayModelOption(
                    id = option.id,
                    label = parakeetLabel(unavailableSuffix),
                    enabled = false,
                )

                else -> RealtimeOverlayModelOption(option.id, option.label)
            }
        }
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

}
