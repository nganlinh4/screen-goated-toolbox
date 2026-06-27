package dev.screengoated.toolbox.mobile.ui

import dev.screengoated.toolbox.mobile.preset.CustomPresetModelDefinition
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetModelDescriptor
import dev.screengoated.toolbox.mobile.preset.PresetModelProvider
import dev.screengoated.toolbox.mobile.preset.PresetModelSource
import dev.screengoated.toolbox.mobile.preset.PresetModelType
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class CustomModelsParityTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `editable providers match windows custom models dialog order`() {
        val fixture = fixture()
        val expected = fixture.getValue("editable_provider_order")
            .jsonArray
            .map { PresetModelProvider.valueOf(it.jsonPrimitive.content) }

        assertEquals(expected, CUSTOM_MODELS_EDITABLE_PROVIDERS)
    }

    @Test
    fun `custom ids use windows compatible provider slug and numeric suffix`() {
        val existing = listOf(
            CustomPresetModelDefinition(
                id = "custom-openrouter-meta-llama",
                provider = PresetModelProvider.OPENROUTER,
                displayName = "Llama",
                fullName = "meta/llama",
            ),
        )

        val duplicate = uniqueCustomId("openrouter", "meta/llama", existing)
        val fresh = newCustomModel(PresetModelProvider.GROQ, emptyList())

        assertEquals("custom-openrouter-meta-llama-2", duplicate)
        assertTrue(fresh.id.startsWith("custom-groq-"))
        assertEquals(PresetModelProvider.GROQ, fresh.provider)
    }

    @Test
    fun `locked preset rows display localized catalog names`() {
        val descriptor = PresetModelDescriptor(
            id = "catalog-model",
            provider = PresetModelProvider.GOOGLE,
            fullName = "provider/model",
            modelType = PresetModelType.TEXT,
            displayName = "English Name",
            nameVi = "Tên tiếng Việt",
            nameKo = "한국어 이름",
        )

        assertEquals("Tên tiếng Việt", lockedModelDisplayName(descriptor, "vi"))
        assertEquals("한국어 이름", lockedModelDisplayName(descriptor, "ko"))
        assertEquals("English Name", lockedModelDisplayName(descriptor, "en"))
    }

    @Test
    fun `editable provider catalog models have complete localized names`() {
        val localizedModels = PresetModelCatalog.dialogModels()
            .filter { it.source == PresetModelSource.BUILT_IN }
            .filter { it.provider in CUSTOM_MODELS_EDITABLE_PROVIDERS }

        assertTrue(localizedModels.isNotEmpty())
        localizedModels.forEach { model ->
            assertTrue("${model.id} missing English name", model.displayName.isNotBlank())
            assertTrue("${model.id} missing Vietnamese name", model.nameVi.isNotBlank())
            assertTrue("${model.id} missing Korean name", model.nameKo.isNotBlank())
        }
    }

    private fun fixture() = json
        .parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
        .jsonObject

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "preset-system", "custom-models-dialog.json"),
            Paths.get("..", "..", "parity-fixtures", "preset-system", "custom-models-dialog.json"),
            Paths.get("parity-fixtures", "preset-system", "custom-models-dialog.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing custom models dialog parity fixture.")
    }
}
