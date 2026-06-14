package dev.screengoated.toolbox.mobile.service

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertTrue
import org.junit.Assert.assertEquals
import org.junit.Test

class LiveSessionRuntimeApiKeyNoticeTest {
    private val json = Json

    @Test
    fun `live translate missing key uses global toast bus`() {
        val case = loadFixtureCase("live_translate_missing_key")
        assertEquals("NO_API_KEY:google", case.getValue("raw_error").jsonPrimitive.content)
        assertEquals("live_translate_start", case.getValue("surface").jsonPrimitive.content)

        val runtimeSource = File(repoRoot(), RUNTIME_SOURCE).readText()
        val serviceSource = File(repoRoot(), SERVICE_SOURCE).readText()
        val activitySource = File(repoRoot(), MAIN_ACTIVITY_SOURCE).readText()

        assertTrue(runtimeSource.contains("internal val toastBus: AppToastBus"))
        assertTrue(runtimeSource.contains("apiKeyErrorToastText(\"NO_API_KEY:google\", repository.currentUiPreferences().uiLanguage)"))
        assertTrue(runtimeSource.contains("?.let(toastBus::show)"))
        assertTrue(serviceSource.contains("toastBus = container.toastBus"))
        assertTrue(activitySource.contains("apiKeyErrorToastText(\"NO_API_KEY:google\", appContainer.repository.currentUiPreferences().uiLanguage)"))
        assertTrue(activitySource.contains("?.let(appContainer.toastBus::show)"))
    }

    private fun loadFixtureCase(name: String) =
        json.parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText())
            .jsonObject
            .getValue("cases")
            .jsonArray
            .map { it.jsonObject }
            .firstOrNull { it.getValue("name").jsonPrimitive.content == name }
            ?: error("Missing API-key notification fixture case: $name")

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/api-key-notifications/triggers.json"
        private const val RUNTIME_SOURCE =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveSessionRuntime.kt"
        private const val SERVICE_SOURCE =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/LiveTranslateService.kt"
        private const val MAIN_ACTIVITY_SOURCE =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/MainActivity.kt"
    }
}
