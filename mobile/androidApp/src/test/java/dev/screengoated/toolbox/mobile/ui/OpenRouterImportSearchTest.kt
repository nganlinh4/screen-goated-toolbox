package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.preset.PresetModelType
import org.junit.Assert.assertEquals
import org.junit.Test

class OpenRouterImportSearchTest {
    private val scanned = (1..300).map { index ->
        ImportableOpenRouterModel(id = "vendor/model-$index", name = "Model $index", type = PresetModelType.TEXT)
    } + ImportableOpenRouterModel(id = "anthropic/claude-late", name = "Late Entry", type = PresetModelType.VISION)

    @Test
    fun emptyQueryRendersOnlyTheFirstPage() {
        val visible = filterOpenRouterImports(scanned, "")
        assertEquals(OPENROUTER_IMPORT_RENDER_LIMIT, visible.size)
        assertEquals("vendor/model-1", visible.first().id)
    }

    @Test
    fun searchReachesModelsBeyondTheRenderedPageByIdOrName() {
        assertEquals(listOf("anthropic/claude-late"), filterOpenRouterImports(scanned, "CLAUDE").map { it.id })
        assertEquals(listOf("anthropic/claude-late"), filterOpenRouterImports(scanned, "late entry").map { it.id })
        assertEquals(listOf("vendor/model-299"), filterOpenRouterImports(scanned, "model-299").map { it.id })
    }

    @Test
    fun matchesAreCappedInScanOrder() {
        val visible = filterOpenRouterImports(scanned, "model 2", limit = 5)
        assertEquals(listOf("Model 2", "Model 20", "Model 21", "Model 22", "Model 23"), visible.map { it.name })
    }
}
