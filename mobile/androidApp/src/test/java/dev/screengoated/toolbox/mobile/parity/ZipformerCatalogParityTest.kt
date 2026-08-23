package dev.screengoated.toolbox.mobile.parity

import dev.screengoated.toolbox.mobile.service.moonshine.ZipformerLanguage
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.long
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
 * The All-8 displayName is intentionally not asserted because Android adds
 * presentation-only spaces. Runtime model type is part of the shared contract.
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
            check(!lang.downloadBaseUrl.endsWith("/main"))
            check(!lang.downloadBaseUrl.endsWith("/master"))
            assertEquals(
                "hasNativePunctuation ${lang.code}",
                entry["hasNativePunctuation"]!!.jsonPrimitive.boolean,
                lang.hasNativePunctuation,
            )
            assertEquals(
                "sherpaModelType ${lang.code}",
                entry["sherpaModelType"]!!.jsonPrimitive.content,
                lang.sherpaModelType,
            )
            val expectedFiles = entry["modelFiles"]!!.jsonArray.map { fileElement ->
                val file = fileElement.jsonObject
                Triple(
                    file["name"]!!.jsonPrimitive.content,
                    file["byteCount"]!!.jsonPrimitive.long,
                    file["sha256"]!!.jsonPrimitive.content,
                )
            }
            val actualFiles = lang.modelFileContracts.map { file ->
                Triple(file.name, file.byteCount, file.sha256)
            }
            assertEquals("modelFiles ${lang.code}", expectedFiles, actualFiles)
            check(actualFiles.all { (_, byteCount, sha256) ->
                byteCount > 0 && sha256.length == 64 && sha256.all { it.isHexDigit() }
            })
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

private fun Char.isHexDigit(): Boolean = this in '0'..'9' || this in 'a'..'f' || this in 'A'..'F'
