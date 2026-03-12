package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.shared.live.SourceMode

data class RealtimeOverlayUiState(
    val sourceMode: SourceMode = SourceMode.MIC,
    val targetLanguage: String = "English",
    val transcript: String = "",
    val committedTranslation: String = "",
    val liveTranslation: String = "",
    val listeningVisible: Boolean = true,
    val translationVisible: Boolean = true,
    val listeningHeaderCollapsed: Boolean = false,
    val translationHeaderCollapsed: Boolean = false,
    val fontSizeSp: Float = 17f,
)
