package dev.screengoated.toolbox.mobile.service.preset

import dev.screengoated.toolbox.mobile.preset.PresetResultWindowId
import dev.screengoated.toolbox.mobile.preset.PresetResultWindowState
import dev.screengoated.toolbox.mobile.service.OverlayBounds

internal data class ActivePresetResultWindow(
    val id: PresetResultWindowId,
    val runtimeState: PresetResultWindowRuntimeState,
    val windowState: PresetResultWindowState,
    val window: PresetOverlayWindow,
)

internal data class PresetResultWindowRuntimeState(
    val opacityPercent: Int = 100,
    val navDepth: Int = 0,
    val maxNavDepth: Int = 0,
    val historyBaseIndex: Int = 0,
    val isBrowsing: Boolean = false,
    val isRawHtml: Boolean = false,
    val copySuccess: Boolean = false,
    val disabledActions: Set<String> = emptySet(),
)

internal data class PresetCanvasWindowPayload(
    val windowsJson: String,
    val activeWindowId: String?,
    val lingerMs: Int,
)

internal data class PresetResultWindowPlacement(
    val id: PresetResultWindowId,
    val bounds: OverlayBounds,
)
