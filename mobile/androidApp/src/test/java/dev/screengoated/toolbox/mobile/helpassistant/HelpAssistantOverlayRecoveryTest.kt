package dev.screengoated.toolbox.mobile.helpassistant

import java.io.File
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class HelpAssistantOverlayRecoveryTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun pendingLaunchStorePreservesQuestionUntilRetry() {
        HelpAssistantPendingLaunchStore.take()

        HelpAssistantPendingLaunchStore.set("  How do I use the bubble?  ", "en")

        val pending = HelpAssistantPendingLaunchStore.take()
        assertNotNull(pending)
        assertEquals("How do I use the bubble?", pending?.question)
        assertEquals("en", pending?.uiLanguage)
        assertNull(HelpAssistantPendingLaunchStore.take())
    }

    @Test
    fun overlayPermissionRecoveryKeepsQuestionAndRetriesThroughMainActivity() {
        val case = fixtureCase("android_overlay_permission_recovery")
        assertTrue(case.getValue("pending_question_preserved").jsonPrimitive.boolean)
        assertTrue(case.getValue("retry_overlay_after_permission_granted").jsonPrimitive.boolean)
        assertEquals("none", case.getValue("fallback_answer_surface").jsonPrimitive.content)

        val serviceSource = File(repoRoot(), HELP_ASSISTANT_OVERLAY_SERVICE).readText()
        val activitySource = File(repoRoot(), MAIN_ACTIVITY).readText()

        assertTrue(serviceSource.contains("HelpAssistantPendingLaunchStore.set(question, uiLanguage)"))
        assertTrue(serviceSource.contains("Settings.ACTION_MANAGE_OVERLAY_PERMISSION"))
        assertTrue(serviceSource.contains("resultModule?.destroy()"))
        assertTrue(activitySource.contains("maybeRunPendingHelpAssistant()"))
        assertTrue(activitySource.contains("HelpAssistantPendingLaunchStore.take()"))
        assertTrue(activitySource.contains("HelpAssistantOverlayService.start("))
    }

    private fun fixtureCase(name: String) = json
        .parseToJsonElement(File(repoRoot(), FIXTURE_PATH).readText())
        .jsonObject
        .getValue("cases")
        .jsonArray
        .first { it.jsonObject.getValue("name").jsonPrimitive.content == name }
        .jsonObject

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/mobile-shell/help-assistant.json"
        private const val HELP_ASSISTANT_OVERLAY_SERVICE =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/helpassistant/HelpAssistantOverlayService.kt"
        private const val MAIN_ACTIVITY =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/MainActivity.kt"
    }
}
