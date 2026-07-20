package dev.screengoated.toolbox.mobile.phonecontrol.tools

import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.PhoneControlVisualProvider
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualFrame
import dev.screengoated.toolbox.mobile.phonecontrol.provider.visual.VisualProviderResult

/** Tool seam keeps screenshot pixels and Android objects out of handler tests. */
internal interface VisualToolBackend {
    val observationGeneration: Long

    suspend fun resetView(): VisualProviderResult<VisualFrame>

    suspend fun seeWholeScreen(): VisualProviderResult<VisualFrame>

    suspend fun zoom(cell: Int): VisualProviderResult<VisualFrame>

    suspend fun look(): VisualProviderResult<VisualFrame>
}

internal object AndroidVisualToolBackend : VisualToolBackend {
    override val observationGeneration: Long
        get() = PhoneControlVisualProvider.observationGeneration

    override suspend fun resetView(): VisualProviderResult<VisualFrame> =
        PhoneControlVisualProvider.resetView()

    override suspend fun seeWholeScreen(): VisualProviderResult<VisualFrame> =
        PhoneControlVisualProvider.seeWholeScreen()

    override suspend fun zoom(cell: Int): VisualProviderResult<VisualFrame> =
        PhoneControlVisualProvider.zoom(cell)

    override suspend fun look(): VisualProviderResult<VisualFrame> =
        PhoneControlVisualProvider.look()
}
