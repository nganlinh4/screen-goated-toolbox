package dev.screengoated.toolbox.mobile.ui.icons

import dev.screengoated.toolbox.mobile.preset.PresetExecutionCapability
import dev.screengoated.toolbox.mobile.preset.ResolvedPreset
import dev.screengoated.toolbox.mobile.preset.inputAdapterOverlayContent
import dev.screengoated.toolbox.mobile.service.preset.FavoriteBubbleHtmlBuilder
import dev.screengoated.toolbox.mobile.service.preset.FavoriteBubblePanelSettings
import dev.screengoated.toolbox.mobile.service.preset.buttonCanvasIconsJson
import dev.screengoated.toolbox.mobile.shared.preset.DefaultPresets
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import java.io.File
import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/** AGENTS.md: UI icons are downloaded Material Symbols, never pasted SVG path data. */
class MaterialSymbolPipelineTest {
    @Test
    fun kotlinSourcesCarryNoPastedIconGeometry() {
        val offenders = kotlinSources().flatMap { file ->
            val source = file.readText()
            PASTED_GEOMETRY.filter { it.containsMatchIn(source) }
                .map { "${file.relativeTo(repoRoot()).invariantSeparatorsPath}: ${it.pattern}" }
        }
        assertEquals(emptyList<String>(), offenders)
    }

    @Test
    fun everySymbolNamedBySourceIsInTheCatalog() {
        val names = kotlinSources().flatMap { file ->
            SYMBOL_CALL.findAll(file.readText()).map { it.groupValues[1] }.toList()
        }.toSet()

        assertTrue("No Material Symbol references found", names.isNotEmpty())
        val missing = names.filterNot { File(catalogDir(), "$it.svg").isFile }
        assertEquals(emptyList<String>(), missing.sorted())
    }

    @Test
    fun catalogHoldsOnlyDownloadedCurrentColorSymbols() {
        val symbols = catalogDir().listFiles { file -> file.extension == "svg" }.orEmpty()

        assertTrue("Empty Material Symbol catalog", symbols.isNotEmpty())
        symbols.forEach { file ->
            val svg = file.readText().trim()
            assertTrue(file.name, svg.startsWith(DOWNLOADED_ROOT))
            assertTrue(file.name, svg.contains("""fill="currentColor""""))
            assertEquals(file.name, 1, Regex("<svg").findAll(svg).count())
        }
    }

    @Test
    fun favoriteBubbleUsesCatalogSymbolsForEveryPresetType() {
        val favorites = PresetType.entries.map { type ->
            val preset = DefaultPresets.all.first { it.presetType == type && !it.isUpcoming }
            ResolvedPreset(
                preset = preset.copy(isFavorite = true),
                hasOverride = false,
                isBuiltIn = true,
                executionCapability = PresetExecutionCapability(supported = true),
                placeholderReasons = emptySet(),
            )
        }
        val html = FavoriteBubbleHtmlBuilder(catalog()).build(
            FavoriteBubblePanelSettings(
                favorites = favorites,
                lang = "en",
                isDark = true,
                keepOpenEnabled = false,
                columnCount = 1,
            ),
        )

        assertEquals(PresetType.entries.size, Regex("""width="20" height="20"""").findAll(html).count())
    }

    @Test
    fun buttonCanvasIconsAreSizedCatalogSymbols() {
        val icons = JSONObject(buttonCanvasIconsJson(catalog()))

        assertTrue(icons.length() > 0)
        icons.keys().forEach { key ->
            val svg = icons.getString(key)
            assertTrue(key, svg.contains("""fill="currentColor""""))
            if (key != "filter") assertTrue(key, svg.contains("""width="16" height="16""""))
        }
    }

    @Test
    fun inputAudioPlayerSymbolsResolveFromTheCatalog() {
        val wav = "RIFF\u0000\u0000\u0000\u0000WAVE".toByteArray(Charsets.ISO_8859_1)
        val html = inputAdapterOverlayContent(PresetInput.Audio(wav), "en").orEmpty()
        val slots = Regex("<!--sgt-material-symbol:([a-z0-9_]+)-->").findAll(html)
            .map { it.groupValues[1] }
            .toList()

        assertEquals(listOf("play_arrow", "pause", "download"), slots)
        val resolved = catalog().resolveSlots(html)
        assertFalse(resolved.contains("<!--sgt-material-symbol:"))
        assertEquals(slots.size, Regex("<svg ").findAll(resolved).count())
    }

    @Test
    fun unknownSlotResolvesToNothing() {
        assertEquals("", catalog().resolveSlots(MaterialSymbolSvgs.slot("not_a_downloaded_symbol")))
    }

    private fun catalog() = MaterialSymbolSvgs { fileName -> File(catalogDir(), fileName).readText() }

    private fun catalogDir() = File(repoRoot(), CATALOG_PATH)

    private fun kotlinSources(): List<File> = SOURCE_ROOTS
        .map { File(repoRoot(), it) }
        .filter(File::isDirectory)
        .flatMap { root -> root.walkTopDown().filter { it.isFile && it.extension == "kt" }.toList() }

    private fun repoRoot(): File {
        var directory = File(requireNotNull(System.getProperty("user.dir"))).canonicalFile
        while (!File(directory, ".claude/parity").exists()) {
            directory = requireNotNull(directory.parentFile) { "Repository root not found" }
        }
        return directory
    }

    private companion object {
        const val CATALOG_PATH = "mobile/androidApp/src/main/assets/material-symbols"
        const val DOWNLOADED_ROOT =
            """<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24">"""
        val SOURCE_ROOTS = listOf("main", "full", "play", "debug")
            .map { "mobile/androidApp/src/$it/java" }

        /** SVG geometry or a hand-built Compose vector inside Kotlin source. */
        val PASTED_GEOMETRY = listOf(
            Regex("""<path[\s>/]"""),
            Regex("""viewBox=\\?""""),
            Regex("""\sd=\\?"[Mm]"""),
            Regex("""ImageVector\.Builder\("""),
            Regex("""PathData\s*\{"""),
        )

        /** Literal symbol names passed to the catalog helpers. */
        val SYMBOL_CALL = Regex("""(?:materialSymbol|\.svg|slot)\("([a-z0-9_]+)"""")
    }
}
