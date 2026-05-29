package dev.screengoated.toolbox.mobile.live

import dev.screengoated.toolbox.mobile.model.LanguageCatalog
import dev.screengoated.toolbox.mobile.model.RealtimeModelIds
import dev.screengoated.toolbox.mobile.model.RealtimePaneFontSizes
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.service.overlay.RealtimeOverlayModelOptions
import dev.screengoated.toolbox.mobile.shared.live.LiveSessionConfig
import dev.screengoated.toolbox.mobile.shared.live.SourceMode
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class LiveTranslateOverlayBootstrapTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `live translate defaults match overlay bootstrap fixture`() {
        val defaults = loadFixture().defaults
        val config = LiveSessionConfig()
        val fontSizes = RealtimePaneFontSizes()
        val tts = RealtimeTtsSettings()

        assertEquals(defaults.audioSource, config.sourceMode.toFixtureValue())
        assertEquals(defaults.targetLanguage, config.targetLanguage)
        assertEquals(defaults.translationModel, config.translationProvider.id)
        assertEquals(defaults.translationApiModel, config.translationProvider.model)
        assertEquals(defaults.transcriptionModel, config.transcriptionProvider.id.toFixtureTranscriptionModel())
        assertEquals(defaults.fontSize, fontSizes.transcriptionSp)
        assertEquals(defaults.fontSize, fontSizes.translationSp)
        assertEquals(defaults.ttsEnabled, tts.enabled)
        assertEquals(defaults.ttsSpeed, tts.speedPercent)
        assertEquals(defaults.ttsAutoSpeed, tts.autoSpeed)
        assertEquals(defaults.ttsVolume, tts.volumePercent)
    }

    @Test
    fun `live translate model catalogs expose fixture-required providers`() {
        val fixture = loadFixture()
        val controls = fixture.requiredControls
        val requiredModels = fixture.requiredModels
        val translation = RealtimeOverlayModelOptions.translationProviderIds
        val transcription = RealtimeOverlayModelOptions.transcriptionProviderIds

        assertEquals(requiredModels.translationProviders, translation)
        assertEquals(requiredModels.androidTranscriptionProviders, transcription)
        assertEquals(requiredModels.targetLanguages, LanguageCatalog.names)
        assertTrue(translation.contains(defaultTranslationProviderId()))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_GEMINI_S2S))
        assertTrue(transcription.contains(RealtimeModelIds.TRANSCRIPTION_PARAKEET))
        assertTrue(requiredModels.androidUnavailableTranscriptionProviders.contains(RealtimeModelIds.TRANSCRIPTION_PARAKEET))
        assertTrue(controls.translationPane.contains("translation-model-toggle"))
        assertTrue(controls.transcriptionPane.contains("transcription-model-toggle"))
        assertTrue(fixture.requiredVisuals.androidS2sAdaptiveVad)
        assertTrue(fixture.requiredVisuals.androidS2sScaledTimeouts)
        assertTrue(fixture.requiredVisuals.androidS2sStaleOrderedSkip)
        assertTrue(fixture.requiredVisuals.androidS2sFullTranscriptDisplay)
        assertTrue(fixture.requiredVisuals.androidS2sRejectsTranslationModelChanges)
        assertTrue(fixture.requiredVisuals.androidS2sRejectsTtsDisable)
        assertTrue(fixture.requiredVisuals.androidTextLlmUsesProviderAvailability)
        assertTrue(fixture.requiredVisuals.androidRejectedTranslationApplyIsFailure)
        assertTrue(fixture.requiredVisuals.androidRejectedPrimaryApplyCanTryFallback)
        assertTrue(fixture.requiredVisuals.androidForceCommitPrimesTranslationInterval)
        assertTrue(fixture.requiredVisuals.androidSkipsTranslationWhenPaneHidden)
        assertEquals("live-on-ui-language-change", fixture.requiredVisuals.ttsModalLocaleRefresh)
        assertEquals("live-on-ui-language-change", fixture.requiredVisuals.downloadModalLocaleRefresh)
        assertEquals("live-on-ui-language-change", fixture.requiredVisuals.s2sTooltipLocaleRefresh)
        assertEquals("active-ui-language-bundle", fixture.requiredVisuals.nativePickerLocaleSource)
        assertTrue(fixture.requiredVisuals.targetLanguageChangeRestartsS2s)
    }

    @Test
    fun `active Android overlay sources expose fixture-required controls`() {
        val fixture = loadFixture()
        val baseHtml = loadRepoFile(OVERLAY_BASE_HTML_PATH).readText()
        val builderSource = loadRepoFile(OVERLAY_HTML_BUILDER_PATH).readText()
        val styleSource = loadRepoFile(OVERLAY_STYLE_PATH).readText()

        fixture.requiredControls.transcriptionPane.forEach { control ->
            assertTrue("Missing transcription control: $control", activeControlSource(control, baseHtml, builderSource))
        }
        fixture.requiredControls.translationPane.forEach { control ->
            assertTrue("Missing translation control: $control", activeControlSource(control, baseHtml, builderSource))
        }
        assertEquals("pinch", fixture.requiredVisuals.mobileResizeGesture)
        assertTrue(!baseHtml.contains("id=\"resize-hint\""))
        assertTrue(!styleSource.contains("#resize-hint"))
    }

    private fun defaultTranslationProviderId(): String = LiveSessionConfig().translationProvider.id

    private fun SourceMode.toFixtureValue(): String {
        return when (this) {
            SourceMode.DEVICE -> "device"
            SourceMode.MIC -> "mic"
        }
    }

    private fun String.toFixtureTranscriptionModel(): String {
        return when (this) {
            RealtimeModelIds.TRANSCRIPTION_GEMINI_2_5,
            RealtimeModelIds.TRANSCRIPTION_GEMINI_3_1,
            -> "gemini"
            RealtimeModelIds.TRANSCRIPTION_PARAKEET -> "parakeet"
            else -> this
        }
    }

    private fun loadFixture(): OverlayFixture {
        return json.decodeFromString(loadRepoFile(FIXTURE_PATH).readText())
    }

    private fun loadRepoFile(path: String): File {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        return generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.map { root -> File(root, path) }
            .firstOrNull(File::exists)
            ?: error("Could not locate $path from $workingDirectory")
    }

    private fun activeControlSource(
        control: String,
        baseHtml: String,
        builderSource: String,
    ): Boolean {
        val source = "$baseHtml\n$builderSource"
        return when (control) {
            "waveform-canvas" -> source.contains("id=\"volume-canvas\"")
            "audio-source-toggle" -> source.contains("id=\"mic-btn\"") && source.contains("id=\"device-btn\"")
            "transcription-model-toggle" -> source.contains("id=\"transcription-model-btn\"")
            "tts-read" -> source.contains("id=\"speak-btn\"")
            "translation-model-toggle" -> source.contains("id=\"translation-model-btn\"")
            "language-select" -> source.contains("id=\"language-select\"")
            "copy" -> source.contains("id=\"copy-btn\"")
            "font-minus" -> source.contains("id=\"font-decrease\"")
            "font-plus" -> source.contains("id=\"font-increase\"")
            "toggle-transcription" -> source.contains("id=\"toggle-mic\"")
            "toggle-translation" -> source.contains("id=\"toggle-trans\"")
            "collapse-chevron" -> source.contains("id=\"header-toggle\"")
            else -> false
        }
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/live-translate/overlay-bootstrap.json"
        private const val OVERLAY_BASE_HTML_PATH = "mobile/androidApp/src/main/assets/realtime_overlay/base.html"
        private const val OVERLAY_STYLE_PATH = "mobile/androidApp/src/main/assets/realtime_overlay/style.css"
        private const val OVERLAY_HTML_BUILDER_PATH =
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/overlay/RealtimeOverlayHtmlBuilder.kt"
    }
}

@Serializable
private data class OverlayFixture(
    val defaults: OverlayDefaults,
    val requiredModels: RequiredModels,
    val requiredControls: RequiredControls,
    val requiredVisuals: RequiredVisuals,
)

@Serializable
private data class OverlayDefaults(
    val audioSource: String,
    val targetLanguage: String,
    val translationModel: String,
    val translationApiModel: String,
    val transcriptionModel: String,
    val fontSize: Int,
    val ttsEnabled: Boolean,
    val ttsSpeed: Int,
    val ttsAutoSpeed: Boolean,
    val ttsVolume: Int,
)

@Serializable
private data class RequiredModels(
    val translationProviders: List<String>,
    val windowsTranscriptionProviders: List<String>,
    val androidTranscriptionProviders: List<String>,
    val androidUnavailableTranscriptionProviders: List<String>,
    val targetLanguages: List<String>,
)

@Serializable
private data class RequiredControls(
    val transcriptionPane: List<String>,
    val translationPane: List<String>,
)

@Serializable
private data class RequiredVisuals(
    val mobileResizeGesture: String,
    val androidS2sAdaptiveVad: Boolean,
    val androidS2sScaledTimeouts: Boolean,
    val androidS2sStaleOrderedSkip: Boolean,
    val androidS2sFullTranscriptDisplay: Boolean,
    val androidS2sRejectsTranslationModelChanges: Boolean,
    val androidS2sRejectsTtsDisable: Boolean,
    val androidTextLlmUsesProviderAvailability: Boolean,
    val androidRejectedTranslationApplyIsFailure: Boolean,
    val androidRejectedPrimaryApplyCanTryFallback: Boolean,
    val androidForceCommitPrimesTranslationInterval: Boolean,
    val androidSkipsTranslationWhenPaneHidden: Boolean,
    val ttsModalLocaleRefresh: String,
    val downloadModalLocaleRefresh: String,
    val s2sTooltipLocaleRefresh: String,
    val nativePickerLocaleSource: String,
    val targetLanguageChangeRestartsS2s: Boolean,
)
