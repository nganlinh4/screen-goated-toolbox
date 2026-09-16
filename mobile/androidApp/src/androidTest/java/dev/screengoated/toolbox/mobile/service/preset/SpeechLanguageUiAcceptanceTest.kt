package dev.screengoated.toolbox.mobile.service.preset

import androidx.activity.compose.setContent
import androidx.compose.material3.MaterialTheme
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.test.hasSetTextAction
import androidx.compose.ui.test.junit4.v2.createEmptyComposeRule
import androidx.compose.ui.test.onNodeWithText
import androidx.compose.ui.test.performClick
import androidx.compose.ui.test.performTextInput
import androidx.test.core.app.ActivityScenario
import androidx.test.ext.junit.runners.AndroidJUnit4
import dev.screengoated.toolbox.mobile.MainActivity
import dev.screengoated.toolbox.mobile.preset.PresetModelCatalog
import dev.screengoated.toolbox.mobile.preset.PresetPersistence
import dev.screengoated.toolbox.mobile.preset.ui.SpeechLanguagePicker
import dev.screengoated.toolbox.mobile.shared.preset.BlockType
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Rule
import org.junit.Test
import org.junit.runner.RunWith

@RunWith(AndroidJUnit4::class)
class SpeechLanguageUiAcceptanceTest {
    @get:Rule val compose = createEmptyComposeRule()

    @Test fun languageSearchSelectionAndAutomaticChoicePersist() = PresetUiAcceptanceSupport().use { ui ->
        val id = ui.preset(PresetType.MIC)
        val model = PresetModelCatalog.dialogModels().first { it.inputLanguageSet == "whisper" }
        val initial = ProcessingBlock("audio", BlockType.AUDIO, model.id)
        ui.main { ui.repository.updateBuiltInOverride(id) { it.copy(blocks = listOf(initial)) } }
        ActivityScenario.launch(MainActivity::class.java).use { scenario ->
            scenario.onActivity { activity ->
                activity.setContent {
                    var block by remember { mutableStateOf(initial) }
                    MaterialTheme { SpeechLanguagePicker(block, "en") { updated ->
                        block = updated
                        ui.repository.updateBuiltInOverride(id) { it.copy(blocks = listOf(updated)) }
                    } }
                }
            }
            compose.onNodeWithText("Input language: Auto").performClick()
            compose.onNode(hasSetTextAction()).performTextInput("Vietnam")
            compose.onNodeWithText("Vietnamese").performClick()
            compose.onNodeWithText("Input language: Vietnamese").assertExists()
            fun persisted() = PresetPersistence(ui.context, Json { ignoreUnknownKeys = true })
                .load().customPresets.getValue(id).blocks.single().languageVars["input_language"]
            assertEquals("vi", persisted())
            compose.onNodeWithText("Input language: Vietnamese").performClick()
            compose.onNodeWithText("Auto", substring = false).performClick()
            compose.onNodeWithText("Input language: Auto").assertExists()
            assertEquals("auto", persisted())
        }
    }
}
