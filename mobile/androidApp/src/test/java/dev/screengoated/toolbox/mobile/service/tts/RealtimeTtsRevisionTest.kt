package dev.screengoated.toolbox.mobile.service.tts

import dev.screengoated.toolbox.mobile.model.MobileGlobalTtsSettings
import dev.screengoated.toolbox.mobile.model.RealtimeTtsSettings
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.async
import kotlinx.coroutines.runBlocking
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Test
import java.nio.file.Files
import java.nio.file.Paths
import java.util.concurrent.CountDownLatch
import java.util.concurrent.TimeUnit

class RealtimeTtsRevisionTest {
    @Test
    fun `committed read cursor matches shared contract`() = runBlocking {
        val paths = listOf(
            Paths.get("..", "parity-fixtures", "live-translate", "read-cursor.json"),
            Paths.get("..", "..", "parity-fixtures", "live-translate", "read-cursor.json"),
            Paths.get("parity-fixtures", "live-translate", "read-cursor.json"),
        )
        val fixture = Json.parseToJsonElement(Files.readAllBytes(paths.first { Files.exists(it) }).decodeToString()).jsonObject
        fixture.getValue("cases").jsonArray.forEach { value ->
            val case = value.jsonObject
            val runtime = FakeTtsRuntimeService()
            val coordinator = RealtimeTtsCoordinator(runtime)
            runtime.awaitPlaybackSubscriber()
            var completed = 0
            case.getValue("updates").jsonArray.forEachIndexed { index, update ->
                val revision = case["revisions"]?.jsonArray?.get(index)?.jsonPrimitive?.content?.toLong() ?: 0L
                coordinator.update(update.jsonPrimitive.content, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true, revision)
                if (runtime.enqueuedRealtime.size > completed) {
                    val request = runtime.enqueuedRealtime.last()
                    runtime.emitPlaybackEvent(TtsPlaybackEvent(request.id, TtsConsumer.REALTIME, RealtimeTtsCoordinator.OWNER_TOKEN, TtsCompletionStatus.COMPLETED))
                    runtime.awaitRealtimeDepth(0)
                    completed = runtime.enqueuedRealtime.size
                }
            }
            assertEquals(case.getValue("name").jsonPrimitive.content,
                case.getValue("segments").jsonArray.map { it.jsonPrimitive.content },
                runtime.enqueuedRealtime.map { it.text })
            coordinator.close()
        }
    }

    @Test
    fun coalescedResetRetiresPriorSpeechEvenWithSamePrefix() {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        fun update(text: String, revision: Long, visible: Boolean = true, enabled: Boolean = true) = coordinator.update(
            text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = enabled), visible, revision,
        )
        update("First", 0)
        val previous = runtime.enqueuedRequests.single()
        update("First again", 1)
        assertFalse(previous.canDispatch())
        assertEquals(0, runtime.stopCount.get())
        assertEquals(listOf("First", "First again"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun hiddenResetIsReconciledOnlyWhenVisibleAgain() {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        fun update(text: String, revision: Long, visible: Boolean) = coordinator.update(
            text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), visible, revision,
        )
        update("First", 0, true)
        update("First again", 1, false)
        assertEquals(listOf("First"), runtime.enqueuedRealtime.map { it.text })
        update("First again", 1, true)
        assertEquals(listOf("First", "First again"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun hiddenReplacementInSameRevisionIsReconciledOnResume() {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        fun update(text: String, visible: Boolean) = coordinator.update(
            text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), visible, 0,
        )
        update("First", true)
        update("Other", false)
        assertEquals(listOf("First"), runtime.enqueuedRealtime.map { it.text })
        update("Other", true)
        assertEquals(listOf("First", "Other"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun disabledReadStartsFreshWhenReenabled() {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        fun update(text: String, revision: Long, enabled: Boolean) = coordinator.update(
            text, "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = enabled), true, revision,
        )
        update("First", 0, true)
        update("First again", 1, false)
        assertEquals(listOf("First"), runtime.enqueuedRealtime.map { it.text })
        update("First again", 1, true)
        assertEquals(listOf("First", "First again"), runtime.enqueuedRealtime.map { it.text })
        coordinator.close()
    }

    @Test
    fun stopRetiresRealtimeWithoutStoppingOtherOwners() {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        coordinator.update("First", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        val previous = runtime.enqueuedRequests.single()
        coordinator.stop()
        assertFalse(previous.canDispatch())
        assertEquals(0, runtime.stopCount.get())
        assertEquals(listOf(runtime.enqueuedRealtime.single().id to RealtimeTtsCoordinator.OWNER_TOKEN), runtime.retiredRealtime)
        coordinator.close()
    }

    @Test
    fun stopWaitsForPendingAdmissionAndRetiresItsRequest() = runBlocking {
        val runtime = FakeTtsRuntimeService()
        val coordinator = RealtimeTtsCoordinator(runtime)
        val entered = CountDownLatch(1)
        val release = CountDownLatch(1)
        runtime.beforeRealtimeReturn = { _, _ -> entered.countDown(); check(release.await(5, TimeUnit.SECONDS)) }
        val update = async(Dispatchers.IO) {
            coordinator.update("First", "English", MobileGlobalTtsSettings(), RealtimeTtsSettings(enabled = true), true)
        }
        check(entered.await(5, TimeUnit.SECONDS))
        val stop = async(Dispatchers.IO) { coordinator.stopAndReset() }
        release.countDown()
        update.await()
        stop.await()
        assertFalse(runtime.enqueuedRequests.single().canDispatch())
        assertEquals(0, runtime.stopCount.get())
        assertEquals(listOf(runtime.enqueuedRealtime.single().id to RealtimeTtsCoordinator.OWNER_TOKEN), runtime.retiredRealtime)
        coordinator.close()
    }
}
