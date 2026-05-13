package dev.screengoated.toolbox.mobile.translationgummy

import dev.screengoated.toolbox.mobile.ui.i18n.MobileLocaleText
import java.io.File
import kotlinx.serialization.Serializable
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class TranslationGummyOnboardingContractTest {
    private val strictJson = Json { ignoreUnknownKeys = false }
    private val configJson = Json { ignoreUnknownKeys = true }

    @Test
    fun onboardingFixtureMatchesMobileStateAndLocaleSources() {
        val fixture = loadFixtureDocument()

        assertEquals("guide_seen", fixture.persistedFlag)
        assertEquals(false, fixture.showWhen["guide_seen"]!!.jsonPrimitive.boolean)
        assertEquals("translation_gummy_title", fixture.titleSource)
        assertEquals("translation_gummy_guide", fixture.messageSource)
        assertEquals("translation_gummy_guide_ok", fixture.confirmLabelSource)
        assertTrue(fixture.dismissBehavior.persistGuideSeen)
        assertTrue(fixture.dismissBehavior.suppressFutureAutoShow)

        val locale = MobileLocaleText.forLanguage("en")
        assertEquals("Translation Gummy", locale.translationGummyTitle)
        assertTrue(locale.translationGummyGuide.isNotBlank())
        assertTrue(locale.translationGummyGuideOk.isNotBlank())
        assertFalse(TranslationGummyConfig().guideSeen)
    }

    @Test
    fun guideSeenPersistsWithWindowsSnakeCaseKeyAndReadsLegacyAndroidKey() {
        val encoded = configJson.encodeToString(
            TranslationGummyConfig.serializer(),
            TranslationGummyConfig(guideSeen = true),
        )

        assertTrue(encoded.contains(""""guide_seen":true"""))
        assertFalse(encoded.contains("guideSeen"))

        val windowsStyle = configJson.decodeFromString(
            TranslationGummyConfig.serializer(),
            """{"guide_seen":true}""",
        )
        val legacyAndroidStyle = configJson.decodeFromString(
            TranslationGummyConfig.serializer(),
            """{"guideSeen":true}""",
        )

        assertTrue(windowsStyle.guideSeen)
        assertTrue(legacyAndroidStyle.guideSeen)
    }

    private fun loadFixtureDocument(): OnboardingFixtureDocument {
        val workingDirectory = requireNotNull(System.getProperty("user.dir"))
        val repoRoot = generateSequence(File(workingDirectory).absoluteFile) { current ->
            current.parentFile ?: return@generateSequence null
        }.firstOrNull { root ->
            File(root, FIXTURE_PATH).exists()
        } ?: error("Could not locate $FIXTURE_PATH from $workingDirectory")

        return strictJson.decodeFromString(File(repoRoot, FIXTURE_PATH).readText())
    }

    private companion object {
        private const val FIXTURE_PATH = "parity-fixtures/translation-gummy/onboarding-contract.json"
    }
}

@Serializable
private data class OnboardingFixtureDocument(
    val persistedFlag: String,
    val showWhen: JsonObject,
    val titleSource: String,
    val messageSource: String,
    val confirmLabelSource: String,
    val dismissBehavior: OnboardingDismissBehavior,
)

@Serializable
private data class OnboardingDismissBehavior(
    val persistGuideSeen: Boolean,
    val suppressFutureAutoShow: Boolean,
)
