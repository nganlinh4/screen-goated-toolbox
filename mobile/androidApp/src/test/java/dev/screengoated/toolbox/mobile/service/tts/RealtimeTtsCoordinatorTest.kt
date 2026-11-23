package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileEdgeTtsSettings
import dev.screengoated.toolbox.mobile.model.MobileTtsMethod
import dev.screengoated.toolbox.mobile.model.MobileTtsSpeedPreset
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import dev.screengoated.toolbox.mobile.model.normalizedForWindowsParity
import dev.screengoated.toolbox.mobile.model.withMethod
import dev.screengoated.toolbox.mobile.ui.ttssettings.globalTtsMethodOptions
import kotlinx.serialization.decodeFromString
import kotlinx.serialization.encodeToString
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import kotlinx.coroutines.delay
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Path
import java.nio.file.Paths
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class RealtimeTtsCoordinatorTest {
    private val json = Json { ignoreUnknownKeys = true }

    @Test
    fun `realtime request carries current voice language and playback settings`() {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        coordinator.update(
            "Words", "Vietnamese", MobileGlobalTtsSettings(voice = "Kore"),
            RealtimeTtsSettings(enabled = true, speedPercent = 125, volumePercent = 80), true,
        )
        val settings = runtime.enqueuedRequests.single().settingsSnapshot
        assertEquals("Kore", settings.geminiVoice)
        assertEquals("Vietnamese", settings.targetLanguage)
        assertEquals(125, settings.realtimeSpeedPercent)
        assertEquals(80, settings.realtimeVolumePercent)
    }

    @Test
    fun `concurrent updates enqueue a committed segment once`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        runtime.beforeRealtimeReturn = { _, _ -> entered.countDown(); check(release.await(5, TimeUnit.SECONDS)) }
        fun update() = coordinator.update("New words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val first = async(Dispatchers.IO) { update() }
        assertTrue(entered.await(5, TimeUnit.SECONDS))
        val second = async(Dispatchers.IO) { update() }
        delay(200)
        release.countDown()
        first.await(); second.await()
        assertEquals(listOf("New words"), runtime.enqueuedRealtime.map { it.text })
    }

    @Test
    fun `completion during enqueue is recorded after registration`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        runtime.beforeRealtimeReturn = { id, _ ->
            runtime.emitPlaybackEvent(TtsPlaybackEvent(id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.COMPLETED))
            entered.countDown()
            check(release.await(5, TimeUnit.SECONDS))
        }
        val first = async(Dispatchers.IO) {
            coordinator.update("New words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        }
        assertTrue(entered.await(5, TimeUnit.SECONDS))
        delay(200)
        release.countDown()
        first.await()
        runtime.awaitRealtimeDepth(0)
        coordinator.update("New words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        assertEquals(1, runtime.enqueuedRealtime.size)
    }

    @Test
    fun `failed first segment is retried before later committed text`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update("First")
        update("First second")
        assertEquals(listOf("First"), runtime.enqueuedRealtime.map { it.text })
        val first = runtime.enqueuedRealtime.single()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.FAILED))
        runtime.awaitRealtimeDepth(0)
        update("First second")
        assertEquals(listOf("First", "First"), runtime.enqueuedRealtime.map { it.text })
        val retry = runtime.enqueuedRealtime.last()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(retry.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.COMPLETED))
        withTimeout(5_000) { while (runtime.enqueuedRealtime.size < 3) delay(5) }
        assertEquals(listOf("First", "First", " second"), runtime.enqueuedRealtime.map { it.text })
    }

    @Test
    fun `completed segment schedules text accumulated during playback`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update("First")
        update("First second")
        val first = runtime.enqueuedRealtime.single()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.COMPLETED))
        withTimeout(5_000) { while (runtime.enqueuedRealtime.size < 2) delay(5) }
        assertEquals(listOf("First", " second"), runtime.enqueuedRealtime.map { it.text })
    }

    @Test
    fun `replacement request retries interrupted final text without another update`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val first = runtime.enqueuedRealtime.single()
        val preview = runtime.interruptAndSpeak(replacementRequest(TtsConsumer.RESULT_OVERLAY))
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.INTERRUPTED))
        withTimeout(5_000) { while (runtime.enqueuedRealtime.size < 2) delay(5) }
        assertEquals(listOf("Final words", "Final words"), runtime.enqueuedRealtime.map { it.text })
        runtime.emitPlaybackEvent(TtsPlaybackEvent(preview, TtsConsumer.RESULT_OVERLAY, "replacement", TtsCompletionStatus.COMPLETED))
        delay(50)
        assertEquals(2, runtime.enqueuedRealtime.size)
        coordinator.close()
    }

    @Test
    fun `replacement terminal before interrupted realtime still retries final text once`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val first = runtime.enqueuedRealtime.single()
        val preview = runtime.interruptAndSpeak(replacementRequest(TtsConsumer.RESULT_OVERLAY))
        runtime.emitPlaybackEvent(TtsPlaybackEvent(preview, TtsConsumer.RESULT_OVERLAY, "replacement", TtsCompletionStatus.INTERRUPTED))
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.INTERRUPTED))
        withTimeout(5_000) { while (runtime.enqueuedRealtime.size < 2) delay(5) }
        assertEquals(listOf("Final words", "Final words"), runtime.enqueuedRealtime.map { it.text })
        delay(50)
        assertEquals(2, runtime.enqueuedRealtime.size)
        coordinator.close()
    }

    @Test
    fun `standalone stop and unrelated preview do not resume interrupted request`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val first = runtime.enqueuedRealtime.single()
        runtime.stop()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.INTERRUPTED))
        runtime.awaitRealtimeDepth(0)
        assertEquals(1, runtime.enqueuedRealtime.size)
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id + 1, TtsConsumer.SETTINGS_PREVIEW, "preview", TtsCompletionStatus.COMPLETED))
        delay(50)
        assertEquals(1, runtime.enqueuedRealtime.size)
        coordinator.close()
    }

    @Test
    fun `phone control announcement terminal before realtime interruption still resumes`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val first = runtime.enqueuedRealtime.single()
        val announcement = runtime.interruptAndSpeak(replacementRequest(TtsConsumer.AUTO_SPEAK))
        runtime.emitPlaybackEvent(TtsPlaybackEvent(announcement, TtsConsumer.AUTO_SPEAK, "replacement", TtsCompletionStatus.COMPLETED))
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.INTERRUPTED))
        withTimeout(5_000) { while (runtime.enqueuedRealtime.size < 2) delay(5) }
        assertEquals(listOf("Final words", "Final words"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun `stop after replacement suppresses delayed automatic retry`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val first = runtime.enqueuedRealtime.single()
        runtime.interruptAndSpeak(replacementRequest(TtsConsumer.AUTO_SPEAK))
        runtime.stop()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.INTERRUPTED))
        withTimeout(5_000) { while (runtime.cancelledAdmissions.get() == 0) delay(5) }
        delay(100)
        assertEquals(1, runtime.enqueuedRealtime.size)
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        coordinator.update("Final words plus more", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        assertEquals(listOf("Final words", " plus more"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun `new suffix after stop is spoken even before old terminal arrives`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update("First")
        val first = runtime.enqueuedRealtime.single()
        runtime.stop()
        update("First second")
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.INTERRUPTED))
        assertEquals(listOf("First", " second"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun `changed transcript starts a fresh generation after stop`() {
        for (replacement in listOf("Other", "Different words")) {
            val runtime = FakeTtsRuntimeService()
            val coordinator = RealtimeTtsCoordinator(runtime)
            fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
            update("First")
            runtime.stop()
            update(replacement)
            assertEquals(listOf("First", replacement), runtime.enqueuedRealtime.map { it.text })
            coordinator.close()
        }
    }

    @Test
    fun `partial stopped playback does not schedule waiting or later transcript`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update("First")
        update("First second")
        val first = runtime.enqueuedRealtime.single()
        runtime.stop()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.PARTIAL))
        withTimeout(5_000) { while (runtime.cancelledAdmissions.get() == 0) delay(5) }
        update("First second third")
        assertEquals(listOf("First", " third"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun `hiding an in flight segment does not replay its uncertain audio on resume`() {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        fun update(text: String, visible: Boolean = true) = coordinator.update(
            text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), visible,
        )
        update("First")
        update("First second")
        update("First second", visible = false)
        update("First second")
        assertEquals(listOf("First", " second"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun `completed terminal delivered after stop does not schedule waiting transcript`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update("First")
        update("First second")
        val first = runtime.enqueuedRealtime.single()
        val completion = runtime.terminalEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.COMPLETED))
        runtime.stop()
        runtime.emitRecordedEvent(completion)
        withTimeout(5_000) { while (runtime.cancelledAdmissions.get() == 0) delay(5) }
        update("First second third")
        assertEquals(listOf("First", " third"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    private fun replacementRequest(consumer: TtsConsumer) = TtsRequest(
        text = "replacement", consumer = consumer, priority = TtsPriority.USER,
        requestMode = TtsRequestMode.INTERRUPT, settingsSnapshot = MobileGlobalTtsSettings().toRuntimeSnapshot(),
        ownerToken = "replacement",
    )

    @Test
    fun `preview completion does not replay a partially played segment`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val first = runtime.enqueuedRealtime.single()
        runtime.interruptAndSpeak(replacementRequest(TtsConsumer.AUTO_SPEAK))
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.PARTIAL))
        runtime.awaitRealtimeDepth(0)
        runtime.emitPlaybackEvent(TtsPlaybackEvent(999, TtsConsumer.SETTINGS_PREVIEW, "preview", TtsCompletionStatus.COMPLETED))
        delay(50)
        assertEquals(1, runtime.enqueuedRealtime.size)
        coordinator.close()
    }

    @Test
    fun `capacity rejection retries final text with bounded delay`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        runtime.rejectionsRemaining.set(1)
        val coordinator = RealtimeTtsCoordinator(runtime)
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        assertTrue(runtime.enqueuedRealtime.isEmpty())
        withTimeout(5_000) { while (runtime.enqueuedRealtime.isEmpty()) delay(10) }
        assertEquals(listOf("Final words"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun `stop prevents a capacity delayed first request from restarting`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        runtime.rejectionsRemaining.set(1)
        val coordinator = RealtimeTtsCoordinator(runtime)
        coordinator.update("Final words", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        runtime.stop()
        withTimeout(5_000) { while (runtime.cancelledAdmissions.get() == 0) delay(10) }
        assertTrue(runtime.enqueuedRealtime.isEmpty())
        coordinator.update("Final words more", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        assertEquals(listOf(" more"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun `service disposal unsubscribes and rejects later updates`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        coordinator.close()
        runtime.awaitNoPlaybackSubscriber()
        coordinator.update("Late", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        assertTrue(runtime.enqueuedRealtime.isEmpty())
        coordinator.close()
    }

    @Test
    fun `failed segment is not treated as old history when transcript grows`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update("First.")
        val first = runtime.enqueuedRealtime.single()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.FAILED))
        runtime.awaitRealtimeDepth(0)
        val grown = "First. " + "following words ".repeat(5)
        update(grown)
        assertEquals("First.", runtime.enqueuedRealtime.last().text)
    }

    @Test
    fun `shorter transcript retires prior request without stopping unrelated playback`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        runtime.awaitPlaybackSubscriber()
        fun update(text: String) = coordinator.update(text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update("Previous transcript")
        val previous = runtime.enqueuedRequests.single()
        assertTrue(previous.canDispatch())
        update("New")
        assertTrue(!previous.canDispatch())
        assertEquals(0, runtime.stopCount.get())
        assertEquals(listOf("Previous transcript", "New"), runtime.enqueuedRealtime.map { it.text })
    }

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

        assertEquals(1, runtime.enqueuedRealtime.size)
        assertEquals(" Active sentence tail with more words", runtime.enqueuedRealtime.single().text)
    }

    @Test
    fun `interrupted realtime request requeues unread tail`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val committed = "Sentence one. Sentence two. Active sentence tail with more words"
        runtime.awaitPlaybackSubscriber()

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )
        val first = runtime.enqueuedRealtime.single()
        runtime.emitPlaybackEvent(
            TtsPlaybackEvent(
                requestId = first.id,
                consumer = TtsConsumer.REALTIME,
                ownerToken = RealtimeTtsCoordinator.OWNER_TOKEN,
                completionStatus = TtsCompletionStatus.INTERRUPTED,
            ),
        )
        runtime.awaitRealtimeDepth(0)

        coordinator.update(
            committedText = committed,
            targetLanguage = "Vietnamese",
            globalSettings = MobileGlobalTtsSettings(),
            realtimeSettings = RealtimeTtsSettings(enabled = true),
            translationVisible = true,
        )
        assertEquals(2, runtime.enqueuedRealtime.size)
        assertEquals(" Active sentence tail with more words", runtime.enqueuedRealtime.last().text)
        assertTrue(runtime.realtimeDepths.last() >= 1)
    }

    @Test
    fun `partially played realtime request does not replay the same text`() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val committed = "Sentence one. Sentence two. Active sentence tail with more words"
        runtime.awaitPlaybackSubscriber()
        fun update() = coordinator.update(committed, "Vietnamese", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        update()
        val first = runtime.enqueuedRealtime.single()
        runtime.emitPlaybackEvent(TtsPlaybackEvent(first.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.PARTIAL))
        runtime.awaitRealtimeDepth(0)
        update()
        assertEquals(1, runtime.enqueuedRealtime.size)
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
        val expectedMethods = case.getValue("android_selector_methods")
            .jsonArray
            .map { it.jsonPrimitive.content }
        val hiddenMethods = case.getValue("android_excluded_windows_local_methods")
            .jsonArray
            .map { it.jsonPrimitive.content }

        assertEquals(expectedMethods, MobileTtsMethod.values().map { it.name })
        assertEquals(expectedMethods, globalTtsMethodOptions().map { it.name })
        hiddenMethods.forEach { hidden ->
            assertTrue(hidden !in globalTtsMethodOptions().map { it.name })
        }
    }

    @Test
    fun `legacy voxtral tts setting coerces to supported android method`() {
        val case = fixtureCase("legacy_voxtral_tts_coerces_to_android_supported_method")
        val initialMethod = case.getValue("initial_settings")
            .jsonObject
            .getValue("method")
            .jsonPrimitive
            .content
        val expectedMethod = case.getValue("expected")
            .jsonObject
            .getValue("method")
            .jsonPrimitive
            .content

        val normalized = json.decodeFromString<MobileGlobalTtsSettings>(
            """{"method":"$initialMethod"}""",
        ).normalizedForWindowsParity()

        assertEquals(MobileTtsMethod.valueOf(expectedMethod), normalized.method)
    }

    @Test
    fun `open weight tts methods are excluded from android selector and normalized away`() {
        val case = fixtureCase("android_open_weight_methods_are_feature_excluded")
        val methods = case.getValue("methods")
            .jsonArray
            .map { it.jsonPrimitive.content }
        val expectedMethod = MobileTtsMethod.valueOf(
            case.getValue("expected_normalized_method").jsonPrimitive.content,
        )

        methods.forEach { method ->
            assertTrue(
                "$method should not be an Android enum constant",
                method !in MobileTtsMethod.values().map { it.name },
            )
            val parsed = json.decodeFromString<MobileGlobalTtsSettings>(
                """{"method":"$method"}""",
            )
            assertEquals(
                "$method should normalize to a supported Android runtime",
                expectedMethod,
                parsed.normalizedForWindowsParity().method,
            )
        }
    }

    @Test
    fun `open weight tts settings are not serialized on Android`() {
        val case = fixtureCase("android_open_weight_settings_are_not_serialized")
        val forbiddenFields = case.getValue("forbidden_settings_fields")
            .jsonArray
            .map { it.jsonPrimitive.content }
        val payload = json.encodeToString(MobileGlobalTtsSettings())

        forbiddenFields.forEach { field ->
            assertTrue("$field should not be part of Android TTS settings", "\"$field\"" !in payload)
        }
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

}
