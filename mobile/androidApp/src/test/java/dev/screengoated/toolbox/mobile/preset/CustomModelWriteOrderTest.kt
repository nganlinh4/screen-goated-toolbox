package dev.screengoated.toolbox.mobile.preset

import dev.screengoated.toolbox.mobile.model.CustomModelState
import dev.screengoated.toolbox.mobile.model.CustomModelStatus
import java.nio.file.Files
import java.nio.file.Paths
import java.util.concurrent.Executors
import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CancellationException
import kotlinx.coroutines.asCoroutineDispatcher
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.test.StandardTestDispatcher
import kotlinx.coroutines.test.advanceUntilIdle
import kotlinx.coroutines.test.runCurrent
import kotlinx.coroutines.test.runTest
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.int
import kotlinx.serialization.json.jsonArray
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Assert.assertNotEquals
import org.junit.Test

@OptIn(ExperimentalCoroutinesApi::class)
class CustomModelWriteOrderTest {
    @Test fun deferredPublicationNeverPublishesAnUnacknowledgedEdit() = runTest {
        val previous = PresetCustomModelRegistry.definitions()
        val old = listOf(CustomPresetModelDefinition(id = "old", displayName = "Old", fullName = "old"))
        val fresh = listOf(CustomPresetModelDefinition(id = "fresh", displayName = "Fresh", fullName = "fresh"))
        try {
            for (fail in listOf(false, true)) {
                val release = CompletableDeferred<Unit>()
                val state = CustomModelState({ old }, {
                    release.await()
                    if (fail) error("Uncertain write")
                }, StandardTestDispatcher(testScheduler))
                PresetCustomModelRegistry.set(emptyList())
                assertTrue(state.update(fresh))
                runCurrent()
                state.publish()
                assertEquals(old, PresetCustomModelRegistry.definitions())
                release.complete(Unit)
                advanceUntilIdle()
                state.publish()
                assertEquals(if (fail) old else fresh, PresetCustomModelRegistry.definitions())
                assertEquals(if (fail) CustomModelStatus.WRITE_UNCERTAIN else CustomModelStatus.AVAILABLE,
                    state.status.value)
            }
        } finally { PresetCustomModelRegistry.set(previous) }
    }

    @Test fun settingsWriteRunsOutsideTheSubmittingThread() = runBlocking {
        val previous = PresetCustomModelRegistry.definitions()
        val dispatcher = Executors.newSingleThreadExecutor().asCoroutineDispatcher()
        val worker = CompletableDeferred<Thread>()
        val release = CompletableDeferred<Unit>()
        try {
            val state = CustomModelState({ emptyList() }, {
                worker.complete(Thread.currentThread())
                release.await()
            }, dispatcher)
            state.publish()
            val caller = Thread.currentThread()
            assertTrue(state.update(listOf(CustomPresetModelDefinition(id = "new", displayName = "New", fullName = "new"))))
            assertNotEquals(caller, withTimeout(5_000) { worker.await() })
            assertEquals(CustomModelStatus.SAVING, state.status.value)
            release.complete(Unit)
            withTimeout(5_000) { state.status.first { it == CustomModelStatus.AVAILABLE } }
            Unit
        } finally {
            release.complete(Unit)
            dispatcher.close()
            PresetCustomModelRegistry.set(previous)
        }
    }

    @Test fun sharedTimelinesKeepLatestEditAndFenceUncertainWrites() = runTest {
        val fixture = listOf(Paths.get("..", "parity-fixtures"), Paths.get("..", "..", "parity-fixtures"),
            Paths.get("parity-fixtures")).map { it.resolve("preset-system/custom-model-write-order.json") }.first(Files::exists)
        val contract = Json.parseToJsonElement(Files.readAllBytes(fixture).decodeToString()).jsonObject
        assertEquals(1, contract.getValue("schema_version").jsonPrimitive.int)
        val cases = contract.getValue("cases").jsonArray.map { it.jsonObject }
        val previous = PresetCustomModelRegistry.definitions()
        fun model(id: String) = CustomPresetModelDefinition(id = id, displayName = id, fullName = id)
        val old = listOf(model("old"))
        val first = listOf(model("first"))
        val middle = listOf(model("middle"))
        val latest = listOf(model("latest"))
        fun label(models: List<CustomPresetModelDefinition>) = when (models) {
            old -> "previous"
            latest -> "latest"
            else -> "other"
        }
        try {
            for (case in cases.filter { it.getValue("event").jsonPrimitive.content != "commit_returns_false" }) {
                val event = case.getValue("event").jsonPrimitive.content
                val failFirst = event == "failure_with_newer_edit" || event == "cancelled_with_newer_edit"
                val release = CompletableDeferred<Unit>()
                val writes = mutableListOf<List<CustomPresetModelDefinition>>()
                val state = CustomModelState({ old }, { snapshot ->
                    writes += snapshot
                    if (writes.size == 1) {
                        release.await()
                        if (event == "cancelled_with_newer_edit") throw CancellationException("Write ended unexpectedly")
                        if (failFirst) error("Synthetic uncertain write")
                    }
                }, StandardTestDispatcher(testScheduler))
                state.publish()
                assertTrue(state.update(first))
                runCurrent()
                assertEquals(listOf(first), writes)
                assertEquals(old, PresetCustomModelRegistry.definitions())
                assertEquals(CustomModelStatus.SAVING, state.status.value)
                assertTrue(state.update(middle))
                assertTrue(state.update(latest))
                assertEquals(latest, state.models.value)
                case["published_before_ack"]?.let { assertEquals(it.jsonPrimitive.content, label(PresetCustomModelRegistry.definitions())) }
                case["editor"]?.let { assertEquals(it.jsonPrimitive.content, label(state.models.value)) }
                release.complete(Unit)
                advanceUntilIdle()
                if (failFirst) {
                    assertEquals(old, PresetCustomModelRegistry.definitions())
                    assertEquals(CustomModelStatus.WRITE_UNCERTAIN, state.status.value)
                    assertEquals(case.getValue("published_after_failure").jsonPrimitive.content, label(PresetCustomModelRegistry.definitions()))
                    assertEquals(case.getValue("pending").jsonPrimitive.content, label(state.models.value))
                    assertEquals(listOf(first), writes)
                    assertFalse(state.update(emptyList()))
                    assertTrue(state.retry())
                    advanceUntilIdle()
                }
                assertEquals(listOf(first, latest), writes)
                assertEquals(latest, state.models.value)
                assertEquals(latest, PresetCustomModelRegistry.definitions())
                case["final"]?.let { assertEquals(it.jsonPrimitive.content, label(PresetCustomModelRegistry.definitions())) }
                case["after_retry"]?.let { assertEquals(it.jsonPrimitive.content, label(PresetCustomModelRegistry.definitions())) }
                assertEquals(CustomModelStatus.AVAILABLE, state.status.value)
            }
        } finally { PresetCustomModelRegistry.set(previous) }
    }
}
