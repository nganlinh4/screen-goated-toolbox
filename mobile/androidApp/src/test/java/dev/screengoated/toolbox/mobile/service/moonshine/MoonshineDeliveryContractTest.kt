package dev.screengoated.toolbox.mobile.service.moonshine

import kotlinx.serialization.json.Json
import kotlinx.serialization.json.long
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class MoonshineDeliveryContractTest {
    @Test
    fun runtimeCatalogMatchesPinnedSourceAndBundleContracts() {
        val sourceFile = locate(
            "native/moonshine-models/model-contract.json",
            "../native/moonshine-models/model-contract.json",
            "mobile/native/moonshine-models/model-contract.json",
        )
        val deliveryFile = locate(
            "androidApp/src/main/assets/${MoonshineModelDelivery.ASSET_NAME}",
            "src/main/assets/${MoonshineModelDelivery.ASSET_NAME}",
            "mobile/androidApp/src/main/assets/${MoonshineModelDelivery.ASSET_NAME}",
        )
        val json = Json.parseToJsonElement(sourceFile.readText()).jsonObject
        val delivery = Json.parseToJsonElement(deliveryFile.readText()).jsonObject
        assertEquals(
            delivery["modelContractSha256"]!!.jsonPrimitive.content,
            ManagedModelIntegrity.sha256(sourceFile),
        )
        val sourceVariants = json["variants"]!!.jsonArray.associateBy {
            it.jsonObject["id"]!!.jsonPrimitive.content
        }
        val deliveredVariants = delivery["variants"]!!.jsonArray.associateBy {
            it.jsonObject["id"]!!.jsonPrimitive.content
        }
        val runtimeBundles = MoonshineModelDelivery.parse(deliveryFile.readText())

        MoonshineLanguage.entries.forEach { language ->
            val source = sourceVariants.getValue(language.modelName).jsonObject
            val delivered = deliveredVariants.getValue(language.modelName).jsonObject
            val expectedFiles = source["files"]!!.jsonArray.map { element ->
                val file = element.jsonObject
                Triple(
                    file["path"]!!.jsonPrimitive.content,
                    file["sizeBytes"]!!.jsonPrimitive.long,
                    file["sha256"]!!.jsonPrimitive.content,
                )
            }
            val actualFiles = language.modelFileContracts.map {
                Triple(it.name, it.byteCount, it.sha256)
            }
            assertEquals(expectedFiles, actualFiles)
            assertEquals(
                source["fallbackBaseUrl"]!!.jsonPrimitive.content,
                language.downloadBaseUrl,
            )
            val bundle = runtimeBundles.getValue(language.modelName)
            assertEquals(delivered["asset"]!!.jsonPrimitive.content, bundle.asset)
            assertEquals(delivered["sizeBytes"]!!.jsonPrimitive.long, bundle.byteCount)
            assertEquals(delivered["sha256"]!!.jsonPrimitive.content, bundle.sha256)
            assertEquals(delivered["downloadUrl"]!!.jsonPrimitive.content, bundle.downloadUrl)
            assertTrue(bundle.asset.contains(bundle.sha256.take(16)))
        }
        assertEquals(MoonshineLanguage.entries.size, sourceVariants.size)
        assertEquals(MoonshineLanguage.entries.size, deliveredVariants.size)
    }

    private fun locate(vararg candidates: String): File = candidates
        .asSequence()
        .map(::File)
        .firstOrNull(File::isFile)
        ?: error("Missing Moonshine delivery contract. Tried: ${candidates.toList()}")
}
