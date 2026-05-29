package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.model.withMethod
import dev.screengoated.toolbox.mobile.ui.ttssettings.globalTtsMethodOptions
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.coroutines.delay
import kotlinx.coroutines.flow.MutableSharedFlow
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.SharedFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.runBlocking
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths

class RealtimeTtsCoordinatorTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `realtime skips existing history on first enable`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val committed = "Sentence one. Sentence two. Active sentence tail with more words"

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )

        delay(50)
        assertEquals(1, runtime.enqueuedRealtime.size)
        assertEquals(" Active sentence tail with more words", runtime.enqueuedRealtime.single().text)
    }

    @Test
    fun `interrupted realtime request requeues unread tail`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val committed = "Sentence one. Sentence two. Active sentence tail with more words"

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )
        delay(50)
        val first = runtime.enqueuedRealtime.single()
        runtime.emitPlaybackEvent(
            TtsPlaybackEvent(
                requestId = first.id,
                consumer = TtsConsumer.REALTIME,
                ownerToken = RealtimeTtsCoordinator.OWNER_TOKEN,
                completionStatus = TtsCompletionStatus.INTERRUPTED,
            ),
        )
        delay(50)

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )
        delay(50)

        assertEquals(2, runtime.enqueuedRealtime.size)
        assertEquals(" Active sentence tail with more words", runtime.enqueuedRealtime.last().text)
        assertTrue(runtime.realtimeDepths.last() >= 1)
    }

    @Test
    fun `google method coerces fast speed to normal`() {
        val settings = MobileGlobalTtsSettings(
            method = MobileTtsMethod.GEMINI_LIVE,
            speedPreset = MobileTtsSpeedPreset.FAST,
        )

        val next = settings.withMethod(MobileTtsMethod.GOOGLE_TRANSLATE)

        assertEquals(MobileTtsMethod.GOOGLE_TRANSLATE, next.method)
        assertEquals(MobileTtsSpeedPreset.NORMAL, next.speedPreset)
    }

    @Test
    fun `edge ssml volume is not applied again by player`() {
        val case = fixtureCase("edge_ssml_volume_is_not_applied_again_by_player")
        val edgeVolume = case.getValue("initial_settings")
            .jsonObject
            .getValue("edge_volume")
            .jsonPrimitive
            .int
        val expectedPlaybackVolume = case.getValue("expected")
            .jsonObject
            .getValue("playback_volume_percent")
            .jsonPrimitive
            .int
        val request = TtsRequest(
            text = "Preview",
            consumer = TtsConsumer.SETTINGS_PREVIEW,
            priority = TtsPriority.PREVIEW,
            requestMode = TtsRequestMode.INTERRUPT,
            settingsSnapshot = MobileGlobalTtsSettings(
                method = MobileTtsMethod.EDGE_TTS,
                edgeSettings = MobileEdgeTtsSettings(volume = edgeVolume),
            ).toRuntimeSnapshot(),
            ownerToken = "settings-preview",
        )

        assertEquals(expectedPlaybackVolume, playbackVolumePercent(request))
    }

    @Test
    fun `visible tts methods follow windows selector contract`() {
        val case = fixtureCase("android_visible_tts_methods_follow_windows_selector")
        val expectedMethods = case.getValue("windows_selector_methods")
            .jsonArray
            .map { it.jsonPrimitive.content }
        val hiddenMethods = case.getValue("legacy_hidden_methods")
            .jsonArray
            .map { it.jsonPrimitive.content }

        assertEquals(expectedMethods, globalTtsMethodOptions().map { it.name })
        hiddenMethods.forEach { hidden ->
            assertTrue(hidden !in globalTtsMethodOptions().map { it.name })
        }
    }

    @Test
    fun `open weight tts methods return explicit android unavailable error`() {
        val case = fixtureCase("android_open_weight_methods_are_explicitly_unavailable")
        val methods = case.getValue("methods")
            .jsonArray
            .map { it.jsonPrimitive.content }
        val expectedSuffix = case.getValue("expected_error_suffix").jsonPrimitive.content
        val serviceSource = repoFile(
            "mobile/androidApp/src/main/java/dev/screengoated/toolbox/mobile/service/tts/AndroidTtsRuntimeService.kt"
        )

        methods.forEach { method ->
            MobileTtsMethod.valueOf(method)
            assertTrue("Missing unavailable branch for $method", serviceSource.contains("MobileTtsMethod.$method"))
        }
        assertTrue(serviceSource.contains(expectedSuffix))
    }

    private fun fixtureCase(name: String) = json
        .parseToJsonElement(Files.readAllBytes(fixturePath()).decodeToString())
        .jsonObject
        .getValue("cases")
        .jsonArray
        .map { it.jsonObject }
        .firstOrNull { it.getValue("name").jsonPrimitive.content == name }
        ?: error("Missing fixture case: $name")

    private fun fixturePath(): Path {
        val candidates = listOf(
            Paths.get("..", "parity-fixtures", "tts-runtime", "queue-semantics.json"),
            Paths.get("..", "..", "parity-fixtures", "tts-runtime", "queue-semantics.json"),
            Paths.get("parity-fixtures", "tts-runtime", "queue-semantics.json"),
        )
        return candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing TTS runtime fixture. Tried: $candidates")
    }

    private fun repoFile(path: String): String {
        val candidates = listOf(
            Paths.get("..", path),
            Paths.get("..", "..", path),
            Paths.get(path),
        )
        val file = candidates.firstOrNull { Files.exists(it) }
            ?: error("Missing repo file: $path. Tried: $candidates")
        return Files.readAllBytes(file).decodeToString()
    }
}

private class FakeTtsRuntimeService : TtsRuntimeService {
    private val mutableRuntimeState = MutableStateFlow(TtsRuntimeState())
    private val mutablePlaybackEvents = MutableSharedFlow<TtsPlaybackEvent>(extraBufferCapacity = 16)
    private val mutableEdgeCatalogState = MutableStateFlow(EdgeVoiceCatalogState())
    private var nextRequestId = 1L

    data class RecordedRequest(
        val id: Long,
        val text: String,
    )

    val enqueuedRealtime = mutableListOf<RecordedRequest>()
    val realtimeDepths = mutableListOf<Int>()

    override val runtimeState: StateFlow<TtsRuntimeState> = mutableRuntimeState
    override val playbackEvents: SharedFlow<TtsPlaybackEvent> = mutablePlaybackEvents
    override val edgeVoiceCatalogState: StateFlow<EdgeVoiceCatalogState> = mutableEdgeCatalogState

    override fun ensureEdgeVoiceCatalog(force: Boolean) = Unit

    override fun enqueue(request: TtsRequest): Long {
        return nextRequestId++
    }

    override fun enqueueRealtime(request: TtsRequest): Long {
        val requestId = nextRequestId++
        enqueuedRealtime += RecordedRequest(requestId, request.text)
        return requestId
    }

    override fun interruptAndSpeak(request: TtsRequest): Long {
        return nextRequestId++
    }

    override fun stop() = Unit

    override fun stopIfActive(requestId: Long) = Unit

    override fun hasPendingAudio(): Boolean = false

    override fun setRealtimeQueueDepth(depth: Int) {
        realtimeDepths += depth
    }

    fun emitPlaybackEvent(event: TtsPlaybackEvent) {
        mutablePlaybackEvents.tryEmit(event)
    }
}
