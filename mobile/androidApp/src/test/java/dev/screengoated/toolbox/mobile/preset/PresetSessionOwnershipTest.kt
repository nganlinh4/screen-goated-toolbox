package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.AppToastBus
import dev.screengoated.toolbox.mobile.shared.preset.Preset
import dev.screengoated.toolbox.mobile.shared.preset.PresetInput
import dev.screengoated.toolbox.mobile.shared.preset.PresetType
import dev.screengoated.toolbox.mobile.shared.preset.ProcessingBlock
import dev.screengoated.toolbox.mobile.shared.preset.textBlock
import dev.screengoated.toolbox.mobile.shared.preset.inputAdapter
import dev.screengoated.toolbox.mobile.service.preset.retireUnsupportedPresetLaunch
import dev.screengoated.toolbox.mobile.service.tts.TtsConsumer
import dev.screengoated.toolbox.mobile.service.tts.TtsPriority
import dev.screengoated.toolbox.mobile.service.tts.TtsRequest
import dev.screengoated.toolbox.mobile.service.tts.TtsRequestMode
import dev.screengoated.toolbox.mobile.service.tts.canDispatch
import dev.screengoated.toolbox.mobile.service.tts.dispatchIfCurrent
import dev.screengoated.toolbox.mobile.service.tts.toRuntimeSnapshot
import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.boolean
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import okhttp3.OkHttpClient
import okhttp3.Protocol
import okhttp3.Response
import okhttp3.ResponseBody
import okhttp3.ResponseBody.Companion.toResponseBody
import okio.ForwardingSource
import okio.buffer
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test
import java.io.File
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

@OptIn(ExperimentalCoroutinesApi::class)
class PresetSessionOwnershipTest {
    @Test
    fun sharedWindowCasesRejectOldAndCancelledSessions() {
        val cases = fixture().getValue("cases").jsonArray.associateBy { it.jsonObject.getValue("event").jsonPrimitive.content }
        val window = PresetResultWindowState(PresetResultWindowId("old", 0), 0, "Old")
        val replacement = PresetExecutionState(sessionId = "new", isExecuting = true)
        val current = PresetExecutionState(sessionId = "old", isExecuting = true)
        assertEquals(cases.getValue("old_stream_chunk_after_replacement").jsonObject.getValue("apply").jsonPrimitive.boolean,
            replacement.withWindowState(window).resultWindows.isNotEmpty())
        assertEquals(cases.getValue("current_stream_chunk").jsonObject.getValue("apply").jsonPrimitive.boolean,
            current.withWindowState(window).resultWindows.isNotEmpty())
        assertTrue(current.copy(isExecuting = false).withWindowState(window).resultWindows.isEmpty())
        assertEquals(cases.getValue("old_completion_after_replacement").jsonObject.getValue("apply").jsonPrimitive.boolean,
            replacement.finishSession("old") != replacement)
        assertEquals(cases.getValue("old_cancel_after_replacement").jsonObject.getValue("apply").jsonPrimitive.boolean,
            replacement.finishSession("old", "old failure") != replacement)
        assertTrue(current.finishSession("old").isComplete)
    }

    @Test
    fun retiredNetworkJobCannotChangeReplacementOrRunPostProcessing() = runTest {
        for (replacement in listOf("supported", "unsupported", "unsupported_launch", "cancel", "reset")) {
            clearPresetModelCircuitsForTest()
            val started = CountDownLatch(1)
            val release = CountDownLatch(1)
            val returned = CountDownLatch(1)
            val client = OkHttpClient.Builder().addInterceptor { chain ->
                started.countDown()
                check(release.await(10, TimeUnit.SECONDS))
                val body = "data: {\"choices\":[{\"delta\":{\"content\":\"late\"}}]}\n\ndata: [DONE]\n\n".toResponseBody()
                val tracked = object : ResponseBody() {
                    override fun contentType() = body.contentType()
                    override fun contentLength() = body.contentLength()
                    override fun source() = object : ForwardingSource(body.source()) {
                        override fun close() { try { super.close() } finally { returned.countDown() } }
                    }.buffer()
                }
                Response.Builder().request(chain.request()).protocol(Protocol.HTTP_1_1).code(200).message("OK")
                    .body(tracked).build()
            }.build()
            val actions = RecordingActions()
            val repo = PresetRepository(
                textApiClient = TextApiClient(client), visionApiClient = VisionApiClient(client),
                apiKeys = { ApiKeys(groqKey = "test") }, runtimeSettings = { PresetRuntimeSettings() },
                uiLanguage = { "en" }, overrideStore = MemoryStore(), toastBus = AppToastBus(),
                mainDispatcher = StandardTestDispatcher(testScheduler),
            )
            repo.postProcessActions = actions
            val original = preset("old", listOf(textBlock("groq-qwen-3-8-27b-text", "Answer")
                .copy(showOverlay = true, renderMode = "markdown_stream", autoCopy = true)), autoPaste = true)
            repo.executePreset(original, PresetInput.Text("question"))
            runCurrent()
            assertTrue("request did not start", started.await(10, TimeUnit.SECONDS))
            when (replacement) {
                "supported" -> repo.executePreset(preset("new", listOf(inputAdapter().copy(showOverlay = true))), PresetInput.Text("new"))
                "unsupported" -> repo.executePreset(preset("unsupported", listOf(textBlock("missing-model", ""))), PresetInput.Text("new"))
                "unsupported_launch" -> assertTrue(retireUnsupportedPresetLaunch(
                    ResolvedPreset(preset("unsupported", emptyList()), false, false,
                        PresetExecutionCapability(false, PresetPlaceholderReason.MODEL_UNAVAILABLE), emptySet()),
                    repo::cancelExecution,
                ))
                "cancel" -> repo.cancelExecution()
                else -> repo.resetState()
            }
            runCurrent()
            val expected = repo.executionState.value
            release.countDown()
            assertTrue("request did not return", returned.await(10, TimeUnit.SECONDS))
            // The provider runs on IO; let its cancellation propagate to the test dispatcher.
            kotlinx.coroutines.yield()
            runCurrent()
            assertEquals(replacement, expected, repo.executionState.value)
            assertFalse(replacement, repo.executionState.value.isExecuting)
            assertEquals(replacement, 0, actions.count)
            if (replacement == "supported") assertTrue(expected.isComplete)
            if (replacement == "unsupported") assertTrue(expected.error != null)
        }
    }

    @Test
    fun queuedEffectsSurviveCompletionButNotRetirement() = runTest {
        val cases = fixture().getValue("cases").jsonArray.associateBy { it.jsonObject.getValue("event").jsonPrimitive.content }
        for (retirement in listOf("none", "replace", "cancel", "reset")) {
            val queue = QueuedActions()
            val client = OkHttpClient()
            val repo = PresetRepository(
                textApiClient = TextApiClient(client), visionApiClient = VisionApiClient(client),
                apiKeys = { ApiKeys() }, runtimeSettings = { PresetRuntimeSettings() }, uiLanguage = { "en" },
                overrideStore = MemoryStore(), toastBus = AppToastBus(), mainDispatcher = StandardTestDispatcher(testScheduler),
            )
            repo.postProcessActions = queue
            val block = inputAdapter().copy(showOverlay = true, autoCopy = true, autoSpeak = true)
            repo.executePreset(preset("first", listOf(block), autoPaste = true), PresetInput.Text("hello"))
            runCurrent()
            assertTrue(repo.executionState.value.isComplete)
            assertEquals(3, queue.tasks.size)
            val lease = requireNotNull(queue.speechLease)
            val speech = TtsRequest("hello", TtsConsumer.AUTO_SPEAK, TtsPriority.USER, TtsRequestMode.NORMAL,
                MobileGlobalTtsSettings().toRuntimeSnapshot(), "speech", lease::isCurrent, lease::runIfCurrent)
            when (retirement) {
                "replace" -> repo.executePreset(preset("next", listOf(inputAdapter().copy(showOverlay = true))), PresetInput.Text("next"))
                "cancel" -> repo.cancelExecution()
                "reset" -> repo.resetState()
            }
            queue.tasks.forEach { it() }
            val fixtureEvent = if (retirement == "none") "completed_session_deferred_effect" else "retired_session_deferred_effect"
            val shouldApply = cases.getValue(fixtureEvent).jsonObject.getValue("apply").jsonPrimitive.boolean
            assertEquals(retirement, if (shouldApply) 3 else 0, queue.applied)
            assertEquals(retirement, shouldApply, speech.canDispatch())
            if (retirement != "none") {
                assertEquals(cases.getValue("retired_session_speech_retry").jsonObject.getValue("apply").jsonPrimitive.boolean,
                    speech.canDispatch())
            }
            var speechDispatched = false
            assertEquals(retirement, shouldApply, speech.dispatchIfCurrent { speechDispatched = true })
            assertEquals(retirement, shouldApply, speechDispatched)
        }
    }

    private fun fixture() = Json.parseToJsonElement(
        generateSequence(File(requireNotNull(System.getProperty("user.dir"))).absoluteFile) { it.parentFile }
            .map { File(it, "parity-fixtures/preset-system/session-ownership.json") }.first(File::isFile).readText(),
    ).jsonObject

    private fun preset(id: String, blocks: List<ProcessingBlock>, autoPaste: Boolean = false) = Preset(
        id = id, nameEn = id, nameVi = id, nameKo = id, presetType = PresetType.TEXT_INPUT,
        blocks = blocks, autoPaste = autoPaste,
    )

    private class MemoryStore : PresetOverrideStore {
        override fun load() = StoredPresetOverrides()
        override fun save(overrides: StoredPresetOverrides) = Unit
    }

    private class RecordingActions : PresetPostProcessActions {
        var count = 0
        override fun handleAutoCopy(block: ProcessingBlock, resultText: String, lease: PresetEffectLease) { count++ }
        override fun handleAutoCopyImage(block: ProcessingBlock, pngBytes: ByteArray, lease: PresetEffectLease) { count++ }
        override fun handleAutoSpeak(block: ProcessingBlock, resultText: String, blockIdx: Int, lease: PresetEffectLease) { count++ }
        override fun handleAutoPaste(lease: PresetEffectLease) { count++ }
    }

    private class QueuedActions : PresetPostProcessActions {
        val tasks = mutableListOf<() -> Unit>()
        var applied = 0
        var speechLease: PresetEffectLease? = null
        private fun enqueue(lease: PresetEffectLease) { tasks.add { lease.runIfCurrent { applied++ } } }
        override fun handleAutoCopy(block: ProcessingBlock, resultText: String, lease: PresetEffectLease) = enqueue(lease)
        override fun handleAutoCopyImage(block: ProcessingBlock, pngBytes: ByteArray, lease: PresetEffectLease) = enqueue(lease)
        override fun handleAutoSpeak(block: ProcessingBlock, resultText: String, blockIdx: Int, lease: PresetEffectLease) {
            speechLease = lease
            enqueue(lease)
        }
        override fun handleAutoPaste(lease: PresetEffectLease) = enqueue(lease)
    }
}
