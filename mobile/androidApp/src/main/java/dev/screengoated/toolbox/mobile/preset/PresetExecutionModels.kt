package dev.screengoated.toolbox.mobile.preset

data class PresetResultWindowId(
    val sessionId: String,
    val blockIdx: Int,
)

data class PresetResultWindowState(
    val id: PresetResultWindowId,
    val blockIdx: Int,
    val title: String,
    val markdownText: String = "",
    val isStreaming: Boolean = false,
    val isError: Boolean = false,
    val renderMode: String = "markdown_stream",
    val overlayOrder: Int = 0,
)

data class PresetExecutionState(
    val sessionId: String? = null,
    val isExecuting: Boolean = false,
    val activePresetId: String? = null,
    val resultWindows: List<PresetResultWindowState> = emptyList(),
    val error: String? = null,
    val isComplete: Boolean = false,
)
