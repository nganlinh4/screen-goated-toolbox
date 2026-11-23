package dev.screengoated.toolbox.mobile.model

import dev.screengoated.toolbox.mobile.shared.live.DisplayMode
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionStore
import dev.screengoated.toolbox.mobile.shared.live.PermissionSnapshot
import dev.screengoated.toolbox.mobile.shared.live.ProviderDescriptor
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File

class ResetDefaultsScopeTest {
    @Test
    fun restoredUiPreferencesResetThemeAndOpacityButKeepTheLanguage() {
        val customized = MobileUiPreferences(
            themeMode = MobileThemeMode.LIGHT,
            uiLanguage = "ko",
            overlayOpacityPercent = 40,
        )
        val restored = customized.restoredDefaults()
        assertEquals(MobileThemeMode.SYSTEM, restored.themeMode)
        assertEquals(DEFAULT_RESULT_OVERLAY_OPACITY_PERCENT, restored.overlayOpacityPercent)
        assertEquals("ko", restored.uiLanguage)
    }

    @Test
    fun resetRestoresEveryGlobalTtsFieldThroughOneDefaultValue() {
        val viewModel = File(repoRoot(), "$APP_DIR/MainViewModel.kt").readText()
        val reset = viewModel.substringAfter("fun restoreAppSettingsDefaults() {").substringBefore("\n    }")
        assertTrue(reset.contains("updateGlobalTtsSettings(MobileGlobalTtsSettings())"))
        assertTrue(reset.contains("restoredDefaults()"))
        assertTrue(reset.contains("updatePresetRuntimeSettings(PresetRuntimeSettings())"))

        val shell = File(repoRoot(), "$APP_DIR/ui/SgtMobileApp.kt").readText()
        val onReset = shell.substringAfter("onResetDefaults = {").substringBefore("},")
        assertTrue(onReset.contains("onRestoreAppSettingsDefaults()"))
        // Field-by-field TTS resets left the Gemini model and language conditions customized.
        assertFalse(onReset.contains("onGlobalTts"))
    }

    @Test
    fun resetRestoresTheLiveTranslateSessionConfigButKeepsPlatformDisplaySettings() {
        val defaults = LiveSessionConfig()
        val customized = defaults.copy(
            sourceMode = SourceMode.MIC,
            displayMode = DisplayMode.IN_APP_MIRROR,
            targetLanguage = "Korean",
            transcriptionLanguage = "en",
            transcriptionProvider = ProviderDescriptor("custom-transcription", "custom"),
            customVocabulary = listOf("SGT"),
            translationProvider = ProviderDescriptor("custom-translation", "custom"),
            keepOverlayOnTop = false,
        )
        val store = LiveSessionStore()
        store.hydrate(customized, PermissionSnapshot())

        store.updateConfig(liveTranslateRestorePatch())

        val restored = store.state.value.config
        assertEquals(defaults.sourceMode, restored.sourceMode)
        assertEquals(defaults.targetLanguage, restored.targetLanguage)
        assertEquals(defaults.transcriptionLanguage, restored.transcriptionLanguage)
        assertEquals(defaults.transcriptionProvider, restored.transcriptionProvider)
        assertEquals(defaults.translationProvider, restored.translationProvider)
        assertEquals(emptyList<String>(), restored.customVocabulary)
        assertEquals(DisplayMode.IN_APP_MIRROR, restored.displayMode)
        assertFalse(restored.keepOverlayOnTop)

        val viewModel = File(repoRoot(), "$APP_DIR/MainViewModel.kt").readText()
        val reset = viewModel.substringAfter("fun restoreAppSettingsDefaults() {").substringBefore("\n    }")
        assertTrue(reset.contains("repository.restoreLiveTranslateDefaults()"))
    }

    private fun repoRoot(): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current -> current.parentFile }
            .firstOrNull { File(it, "$APP_DIR/MainViewModel.kt").exists() }
            ?: error("Could not locate $APP_DIR from $workingDirectory")
    }

    private companion object {
        const val APP_DIR = "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile"
    }
}
