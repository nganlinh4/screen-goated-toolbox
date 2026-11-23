package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsVoiceConfig
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit
import java.util.concurrent.Executors
import kotlin.concurrent.thread
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.*
import org.junit.Test

class VoicePreviewContractTest {
    private val fixture = listOf(File("../../parity-fixtures/tts-runtime/voice-preview.json"), File("../parity-fixtures/tts-runtime/voice-preview.json"), File("parity-fixtures/tts-runtime/voice-preview.json")).first(File::isFile).readText()
    private val json = Json.parseToJsonElement(fixture).jsonObject
    private val samples = VoicePreviewSamples.parse(fixture)

    @Test fun clickedEdgeRowUsesSharedSampleAndExactUnsavedVoice() {
        for (case in json.getValue("cases").jsonArray.map { it.jsonObject }) {
            if (case.getValue("provider").jsonPrimitive.content != "edge") continue
            val language = case.getValue("language").jsonPrimitive.content
            val voice = case.getValue("voice").jsonPrimitive.content
            val priorVoice = case["earlierVoice"]?.jsonPrimitive?.content ?: "different-saved-voice"
            val settings = MobileGlobalTtsSettings(edgeSettings = MobileEdgeTtsSettings(voiceConfigs = listOf(
                MobileEdgeTtsVoiceConfig(language, "row", priorVoice),
                MobileEdgeTtsVoiceConfig(language, "duplicate", "another-voice"),
            )))
            val request = edgeVoicePreviewRequest(settings, language, voice, samples)
            val expectedLanguage = case.getValue("expectedSampleLanguage").jsonPrimitive.content
            val expected = if (expectedLanguage == "fallback") json.getValue("fallback").jsonPrimitive.content
                else json.getValue("samples").jsonObject.getValue(expectedLanguage).jsonPrimitive.content
            assertEquals(expected, request.text)
            assertEquals(MobileTtsMethod.EDGE_TTS, request.settingsSnapshot.method)
            assertEquals(voice, resolveEdgeVoice(request) { error("preview must not detect language") })
            assertEquals(priorVoice, settings.edgeSettings.voiceConfigs.first().voiceName)
        }
    }

    @Test fun normalSpeechStillUsesDetectedLanguage() {
        val settings = MobileGlobalTtsSettings(edgeSettings = MobileEdgeTtsSettings(voiceConfigs = listOf(
            MobileEdgeTtsVoiceConfig("ja", "Japanese", "ja-JP-selected"),
        )))
        val preview = edgeVoicePreviewRequest(settings, "ja", "temporary", samples)
        assertEquals("ja-JP-selected", resolveEdgeVoice(preview.copy(edgeVoiceOverride = null)) { "JA" })
    }

    @Test fun replacingPreviewRetiresOnlyThatPreview() {
        val admission = SettingsPreviewAdmission()
        val base = edgeVoicePreviewRequest(MobileGlobalTtsSettings(), "ja", "first", samples)
        val unrelated = base.copy(consumer = TtsConsumer.REALTIME, ownerToken = "other")
        val first = admission.admit(base)
        var firstDispatched = false
        assertTrue(first.dispatchIfCurrent { firstDispatched = true })
        assertTrue(firstDispatched)
        // An admission rejection does not call admit, leaving the current preview intact.
        assertTrue(first.canDispatch())
        val second = admission.admit(base.copy(edgeVoiceOverride = "second"))
        assertFalse(first.canDispatch())
        assertFalse(first.dispatchIfCurrent { fail("retired preview dispatched") })
        assertTrue(second.canDispatch())
        assertTrue(unrelated.canDispatch())
    }

    @Test fun aStalledAdmittedEffectDoesNotBlockReplacementAdmission() {
        val admission = SettingsPreviewAdmission()
        val base = edgeVoicePreviewRequest(MobileGlobalTtsSettings(), "ja", "first", samples)
        val first = admission.admit(base)
        val entered = CountDownLatch(1)
        val finish = CountDownLatch(1)
        val completed = CountDownLatch(1)
        val worker = thread {
            first.dispatchIfCurrent {
                entered.countDown()
                check(finish.await(5, TimeUnit.SECONDS))
            }
            completed.countDown()
        }
        try {
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            val second = admission.admit(base.copy(edgeVoiceOverride = "second"))
            assertFalse(first.canDispatch())
            assertTrue(second.canDispatch())
            assertFalse(completed.await(50, TimeUnit.MILLISECONDS))
        } finally {
            finish.countDown()
            worker.join(5_000)
        }
        assertTrue(completed.await(5, TimeUnit.SECONDS))
    }

    @Test fun readingGlobalGenerationDoesNotWaitForAnAdmittedEffect() {
        val gate = TtsDispatchGeneration()
        val entered = CountDownLatch(1)
        val finish = CountDownLatch(1)
        val worker = thread {
            gate.runIfCurrent(0) {
                entered.countDown()
                check(finish.await(5, TimeUnit.SECONDS))
                true
            }
        }
        val reader = Executors.newSingleThreadExecutor()
        try {
            assertTrue(entered.await(5, TimeUnit.SECONDS))
            assertEquals(0L, reader.submit<Long> { gate.current() }.get(1, TimeUnit.SECONDS))
        } finally {
            finish.countDown()
            worker.join(5_000)
            reader.shutdownNow()
        }
    }
}
