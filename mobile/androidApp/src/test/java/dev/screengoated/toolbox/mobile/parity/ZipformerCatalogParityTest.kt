package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

/**
 * Asserts the Android [ZipformerLanguage] catalog matches the Windows-canonical
 * streaming-Zipformer catalog via the shared fixture
 * (`parity-fixtures/zipformer-catalog/catalog.json`), which the Rust side asserts
 * too. If codes / model dirs / download URLs / file lists / native-punctuation
 * drift between the platforms, one suite goes red.
 *
 * Two fields are intentionally NOT asserted (see .claude/parity/zipformer-catalog.md):
 * the All-8 displayName (Android adds spaces) and sherpaModelType (Windows sets
 * "zipformer2" for the Kroko models, Android auto-detects — pending on-device check).
 */
class ZipformerCatalogParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun matchesSharedCatalogFixture() {
        val doc =
            json.parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString()).jsonObject
        val byCode =
            doc["languages"]!!.jsonArray.associateBy { it.jsonObject["code"]!!.jsonPrimitive.content }

        assertEquals(byCode.size, ZipformerLanguage.entries.size)
        for (lang in ZipformerLanguage.entries) {
            val entry = byCode[lang.code]?.jsonObject ?: error("no fixture entry for code ${lang.code}")
            assertEquals(
                "modelName ${lang.code}",
                entry["modelName"]!!.jsonPrimitive.content,
                lang.modelName,
            )
            assertEquals(
                "downloadBaseUrl ${lang.code}",
                entry["downloadBaseUrl"]!!.jsonPrimitive.content,
                lang.downloadBaseUrl,
            )
            assertEquals(
                "hasNativePunctuation ${lang.code}",
                entry["hasNativePunctuation"]!!.jsonPrimitive.boolean,
                lang.hasNativePunctuation,
            )
            val expectedFiles = entry["modelFiles"]!!.jsonArray.map { it.jsonPrimitive.content }
            assertEquals("modelFiles ${lang.code}", expectedFiles, lang.modelFiles)
        }
    }

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "zipformer-catalog", "catalog.json"),
            Paths.get("..", "..", "parity-fixtures", "zipformer-catalog", "catalog.json"),
            Paths.get("parity-fixtures", "zipformer-catalog", "catalog.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing zipformer-catalog fixture. Tried: $candidates")
    }
}
